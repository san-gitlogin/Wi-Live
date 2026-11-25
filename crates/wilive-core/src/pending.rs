//! Pairing requests waiting for the owner (docs/DESIGN.md "Trust").
//!
//!   start  -> the viewer's key and commitment; this computer answers with its own
//!   reveal -> the viewer's nonce; both sides now show the same code
//!   allow / deny -> the owner, on this computer, after comparing the codes
//!   status -> the viewer polls: waiting, allowed (with its device id), denied
//!
//! Bounded and short-lived: at most [`MAX_PENDING`] requests, [`MAX_PER_ADDR`] from
//! one address, each gone [`TTL_MS`] after it started. Time is passed in (ms), so
//! every rule is testable without waiting.

use crate::pair::{DeviceKey, Nonce, Responder};
use std::net::IpAddr;

pub const MAX_PENDING: usize = 4;
pub const MAX_PER_ADDR: usize = 2;
pub const TTL_MS: u64 = 120_000;
/// An answer (allowed or denied) stays readable this long, then is gone.
pub const ANSWER_TTL_MS: u64 = 30_000;
pub const NAME_MAX: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refuse {
    /// Too many requests waiting (in all, or from this address).
    Busy,
    /// No such request, or it expired.
    Unknown,
    /// The step does not fit the request's state (reveal twice, allow before reveal).
    Order,
    /// The reveal did not match the commitment, or the key was unusable.
    Proof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Not yet revealed: no code to compare.
    Started,
    /// The code is on both screens; the owner has not answered.
    Waiting,
    Allowed {
        device: String,
    },
    Denied,
}

/// What the owner's window shows for one request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct View {
    pub req: String,
    pub name: String,
    pub addr: String,
    pub agent: String,
    /// "123 456" once revealed, else empty.
    pub code: String,
    pub age_s: u64,
}

struct Req {
    id: String,
    name: String,
    addr: IpAddr,
    agent: String,
    v_pub: [u8; 32],
    commit: [u8; 32],
    responder: Responder,
    started: u64,
    code: Option<u32>,
    key: Option<DeviceKey>,
    answer: Option<(Status, u64)>,
}

/// A request the owner allowed: what the device store keeps.
pub struct Allowed {
    pub device: String,
    pub name: String,
    pub agent: String,
    pub addr: IpAddr,
    pub key: DeviceKey,
}

#[derive(Default)]
pub struct Pending {
    reqs: Vec<Req>,
}

/// A display name the owner can trust to be what it looks like: printable, no
/// control or bidi characters, trimmed, at most [`NAME_MAX`] characters.
pub fn clean_name(s: &str) -> String {
    let t: String = s
        .chars()
        .filter(|c| !c.is_control() && !matches!(*c as u32, 0x200B..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069 | 0xFEFF))
        .take(NAME_MAX)
        .collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        "A device".to_string()
    } else {
        t
    }
}

impl Pending {
    fn expire(&mut self, now: u64) {
        self.reqs.retain(|r| match &r.answer {
            Some((_, at)) => now.saturating_sub(*at) < ANSWER_TTL_MS,
            None => now.saturating_sub(r.started) < TTL_MS,
        });
    }

    fn find(&mut self, req: &str, now: u64) -> Result<&mut Req, Refuse> {
        self.expire(now);
        self.reqs
            .iter_mut()
            .find(|r| r.id == req)
            .ok_or(Refuse::Unknown)
    }

    /// Returns `(req, a_pub, n_a)`.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        name: &str,
        agent: &str,
        addr: IpAddr,
        v_pub: [u8; 32],
        commit: [u8; 32],
        responder: Responder,
        req_id: String,
        now: u64,
    ) -> Result<(String, [u8; 32], Nonce), Refuse> {
        self.expire(now);
        let open = |r: &&Req| r.answer.is_none();
        if self.reqs.iter().filter(open).count() >= MAX_PENDING
            || self
                .reqs
                .iter()
                .filter(open)
                .filter(|r| r.addr == addr)
                .count()
                >= MAX_PER_ADDR
        {
            return Err(Refuse::Busy);
        }
        let out = (req_id.clone(), responder.a_pub, responder.n_a);
        self.reqs.push(Req {
            id: req_id,
            name: clean_name(name),
            addr,
            agent: clean_name(agent),
            v_pub,
            commit,
            responder,
            started: now,
            code: None,
            key: None,
            answer: None,
        });
        Ok(out)
    }

    /// The viewer's nonce: checks the commitment and returns the code to show.
    /// A wrong reveal ends the request (a second guess would help a grinder).
    pub fn reveal(&mut self, req: &str, n_v: &Nonce, now: u64) -> Result<u32, Refuse> {
        let r = self.find(req, now)?;
        if r.code.is_some() || r.answer.is_some() {
            return Err(Refuse::Order);
        }
        match r.responder.finish(&r.v_pub, &r.commit, n_v) {
            Ok((code, key)) => {
                r.code = Some(code);
                r.key = Some(key);
                Ok(code)
            }
            Err(_) => {
                let id = r.id.clone();
                self.reqs.retain(|x| x.id != id);
                Err(Refuse::Proof)
            }
        }
    }

    /// The owner allowed it: the device is made here, once.
    pub fn allow(&mut self, req: &str, device_id: String, now: u64) -> Result<Allowed, Refuse> {
        let r = self.find(req, now)?;
        if r.answer.is_some() {
            return Err(Refuse::Order);
        }
        let key = r.key.take().ok_or(Refuse::Order)?;
        r.answer = Some((
            Status::Allowed {
                device: device_id.clone(),
            },
            now,
        ));
        Ok(Allowed {
            device: device_id,
            name: r.name.clone(),
            agent: r.agent.clone(),
            addr: r.addr,
            key,
        })
    }

    pub fn deny(&mut self, req: &str, now: u64) -> Result<(), Refuse> {
        let r = self.find(req, now)?;
        if r.answer.is_some() {
            return Err(Refuse::Order);
        }
        r.key = None;
        r.answer = Some((Status::Denied, now));
        Ok(())
    }

    /// What the viewer polls. Only the address that started it may ask, so a
    /// neighbour cannot learn a device id by guessing request ids.
    pub fn status(&mut self, req: &str, addr: IpAddr, now: u64) -> Result<Status, Refuse> {
        let r = self.find(req, now)?;
        if r.addr != addr {
            return Err(Refuse::Unknown);
        }
        Ok(match (&r.answer, r.code) {
            (Some((s, _)), _) => s.clone(),
            (None, Some(_)) => Status::Waiting,
            (None, None) => Status::Started,
        })
    }

    /// Requests the owner can answer now (revealed, unanswered), oldest first.
    pub fn views(&mut self, now: u64) -> Vec<View> {
        self.expire(now);
        self.reqs
            .iter()
            .filter(|r| r.answer.is_none() && r.code.is_some())
            .map(|r| View {
                req: r.id.clone(),
                name: r.name.clone(),
                addr: r.addr.to_string(),
                agent: r.agent.clone(),
                code: r.code.map(crate::pair::format_code).unwrap_or_default(),
                age_s: now.saturating_sub(r.started) / 1000,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pair::{commit, derive_key, sas};
    use x25519_dalek::{PublicKey, StaticSecret};

    const A: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 0, 20));
    const B: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 0, 21));

    struct V {
        s: StaticSecret,
        p: [u8; 32],
        n: Nonce,
    }
    fn v(seed: u8) -> V {
        let s = StaticSecret::from([seed; 32]);
        let p = PublicKey::from(&s).to_bytes();
        V {
            s,
            p,
            n: [seed ^ 0x55; 16],
        }
    }
    fn start(
        p: &mut Pending,
        x: &V,
        addr: IpAddr,
        id: &str,
        now: u64,
    ) -> Result<(String, [u8; 32], Nonce), Refuse> {
        p.start(
            "Pixel 7",
            "Chrome on Android",
            addr,
            x.p,
            commit(&x.p, &x.n),
            Responder::from_parts([9; 32], [8; 16]),
            id.into(),
            now,
        )
    }

    #[test]
    fn a_whole_pairing_ends_with_the_same_key_on_both_sides() {
        let mut p = Pending::default();
        let x = v(1);
        let (req, a_pub, n_a) = start(&mut p, &x, A, "r1", 0).unwrap();
        assert_eq!(p.status(&req, A, 10).unwrap(), Status::Started);
        assert!(
            p.views(10).is_empty(),
            "nothing for the owner before the code exists"
        );
        let code = p.reveal(&req, &x.n, 20).unwrap();
        assert_eq!(code, sas(&x.p, &a_pub, &x.n, &n_a));
        assert_eq!(p.status(&req, A, 30).unwrap(), Status::Waiting);
        let views = p.views(30);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].code, crate::pair::format_code(code));
        let ok = p.allow(&req, "dev1".into(), 40).unwrap();
        let shared = x.s.diffie_hellman(&PublicKey::from(a_pub));
        assert_eq!(
            ok.key,
            derive_key(shared.as_bytes(), &x.p, &a_pub, &x.n, &n_a)
        );
        assert_eq!(
            p.status(&req, A, 50).unwrap(),
            Status::Allowed {
                device: "dev1".into()
            }
        );
        assert!(p.views(50).is_empty());
    }

    #[test]
    fn the_owner_cannot_allow_before_the_code_is_shown_or_twice() {
        let mut p = Pending::default();
        let x = v(1);
        let (req, ..) = start(&mut p, &x, A, "r1", 0).unwrap();
        assert_eq!(p.allow(&req, "d".into(), 1).err(), Some(Refuse::Order));
        p.reveal(&req, &x.n, 2).unwrap();
        p.allow(&req, "d".into(), 3).unwrap();
        assert_eq!(p.allow(&req, "d".into(), 4).err(), Some(Refuse::Order));
        assert_eq!(p.deny(&req, 4).err(), Some(Refuse::Order));
    }

    #[test]
    fn a_wrong_reveal_ends_the_request() {
        let mut p = Pending::default();
        let x = v(1);
        let (req, ..) = start(&mut p, &x, A, "r1", 0).unwrap();
        assert_eq!(p.reveal(&req, &[0; 16], 1).err(), Some(Refuse::Proof));
        assert_eq!(
            p.reveal(&req, &x.n, 2).err(),
            Some(Refuse::Unknown),
            "no second guess"
        );
    }

    #[test]
    fn denied_is_told_once_then_forgotten() {
        let mut p = Pending::default();
        let x = v(1);
        let (req, ..) = start(&mut p, &x, A, "r1", 0).unwrap();
        p.reveal(&req, &x.n, 1).unwrap();
        p.deny(&req, 2).unwrap();
        assert_eq!(p.status(&req, A, 3).unwrap(), Status::Denied);
        assert_eq!(
            p.status(&req, A, 2 + ANSWER_TTL_MS).err(),
            Some(Refuse::Unknown)
        );
    }

    #[test]
    fn requests_are_bounded_in_all_and_per_address() {
        let mut p = Pending::default();
        let x = v(1);
        start(&mut p, &x, A, "a1", 0).unwrap();
        start(&mut p, &x, A, "a2", 0).unwrap();
        assert_eq!(
            start(&mut p, &x, A, "a3", 0).err(),
            Some(Refuse::Busy),
            "two per address"
        );
        start(&mut p, &x, B, "b1", 0).unwrap();
        start(&mut p, &x, B, "b2", 0).unwrap();
        let c: IpAddr = "10.0.0.9".parse().unwrap();
        assert_eq!(
            start(&mut p, &x, c, "c1", 0).err(),
            Some(Refuse::Busy),
            "four in all"
        );
        // answered requests do not hold a place
        p.reveal("a1", &x.n, 1).unwrap();
        p.deny("a1", 1).unwrap();
        start(&mut p, &x, c, "c1", 2).unwrap();
    }

    #[test]
    fn a_request_expires_after_two_minutes() {
        let mut p = Pending::default();
        let x = v(1);
        let (req, ..) = start(&mut p, &x, A, "r1", 0).unwrap();
        assert!(p.status(&req, A, TTL_MS - 1).is_ok());
        assert_eq!(p.reveal(&req, &x.n, TTL_MS).err(), Some(Refuse::Unknown));
    }

    #[test]
    fn only_the_address_that_asked_can_read_the_answer() {
        let mut p = Pending::default();
        let x = v(1);
        let (req, ..) = start(&mut p, &x, A, "r1", 0).unwrap();
        p.reveal(&req, &x.n, 1).unwrap();
        p.allow(&req, "dev1".into(), 2).unwrap();
        assert_eq!(p.status(&req, B, 3).err(), Some(Refuse::Unknown));
    }

    #[test]
    fn names_are_cleaned_of_what_could_fool_the_owner() {
        assert_eq!(clean_name("  Pixel\u{202E}7  \n phone "), "Pixel7 phone");
        assert_eq!(clean_name("\u{200B}\u{0007}"), "A device");
        assert_eq!(clean_name(&"x".repeat(100)).chars().count(), NAME_MAX);
    }
}
