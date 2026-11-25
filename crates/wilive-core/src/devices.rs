//! The devices allowed to watch, and the gate a session's proof goes through.
//!
//! Stored as one JSON file, owner-only (the app writes it; see `wilive::store`).
//! Keys are hex in that file and nowhere else: never in a log, a reply or a test's
//! output.

use crate::pair::{hello_ok, DeviceKey};
use serde::{Deserialize, Serialize};

pub const MAX_DEVICES: usize = 32;

#[derive(Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub agent: String,
    /// Unix seconds.
    pub added: u64,
    pub last_seen: u64,
    pub last_addr: String,
    key: String,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Device {
    pub fn new(
        id: String,
        name: String,
        agent: String,
        addr: String,
        key: &DeviceKey,
        now: u64,
    ) -> Self {
        Device {
            id,
            name,
            agent,
            added: now,
            last_seen: now,
            last_addr: addr,
            key: hex::encode(key.as_bytes()),
        }
    }
    fn key(&self) -> Option<DeviceKey> {
        let b: [u8; 32] = hex::decode(&self.key).ok()?.try_into().ok()?;
        Some(DeviceKey::from_bytes(b))
    }
}

/// What the owner's window shows for a device (no key).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub agent: String,
    pub added: u64,
    pub last_seen: u64,
    pub last_addr: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Devices {
    #[serde(default)]
    list: Vec<Device>,
}

impl Devices {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("plain data")
    }
    pub fn len(&self) -> usize {
        self.list.len()
    }
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Adds a device; when full, the one seen longest ago makes room (the owner
    /// just allowed this one, so it wins over a phone unused for months).
    pub fn add(&mut self, d: Device) -> Option<Device> {
        self.list.retain(|x| x.id != d.id);
        let dropped = if self.list.len() >= MAX_DEVICES {
            let (i, _) = self
                .list
                .iter()
                .enumerate()
                .min_by_key(|(_, x)| x.last_seen)?;
            Some(self.list.remove(i))
        } else {
            None
        };
        self.list.push(d);
        dropped
    }

    pub fn forget(&mut self, id: &str) -> bool {
        let n = self.list.len();
        self.list.retain(|x| x.id != id);
        self.list.len() != n
    }

    pub fn rename(&mut self, id: &str, name: &str) -> bool {
        match self.list.iter_mut().find(|x| x.id == id) {
            Some(d) => {
                d.name = crate::pending::clean_name(name);
                true
            }
            None => false,
        }
    }

    pub fn name_of(&self, id: &str) -> Option<String> {
        self.list
            .iter()
            .find(|x| x.id == id)
            .map(|d| d.name.clone())
    }

    /// A session's proof: the device exists and the HMAC over this connection's
    /// challenge is right. Marks it seen.
    pub fn admit(&mut self, id: &str, chal: &str, mac: &[u8], addr: &str, now: u64) -> bool {
        let Some(d) = self.list.iter_mut().find(|x| x.id == id) else {
            return false;
        };
        let ok = d.key().is_some_and(|k| hello_ok(&k, id, chal, mac));
        if ok {
            d.last_seen = now;
            d.last_addr = addr.to_string();
        }
        ok
    }

    /// [`crate::pair::fp_mac`] for an admitted device.
    pub fn fp_mac(&self, id: &str, chal: &str, fp: &str) -> Option<[u8; 32]> {
        let k = self.list.iter().find(|x| x.id == id)?.key()?;
        Some(crate::pair::fp_mac(&k, chal, fp))
    }

    pub fn views(&self) -> Vec<DeviceView> {
        let mut v: Vec<DeviceView> = self
            .list
            .iter()
            .map(|d| DeviceView {
                id: d.id.clone(),
                name: d.name.clone(),
                agent: d.agent.clone(),
                added: d.added,
                last_seen: d.last_seen,
                last_addr: d.last_addr.clone(),
            })
            .collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.last_seen));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pair::hello_mac;

    fn dev(id: &str, seen: u64) -> Device {
        let mut d = Device::new(
            id.into(),
            id.into(),
            "Chrome".into(),
            "192.168.0.2".into(),
            &DeviceKey::from_bytes([id.len() as u8; 32]),
            1,
        );
        d.last_seen = seen;
        d
    }

    #[test]
    fn a_device_proves_itself_with_its_key_and_this_challenge() {
        let mut ds = Devices::default();
        let k = DeviceKey::from_bytes([7; 32]);
        ds.add(Device::new(
            "d1".into(),
            "Pixel".into(),
            "".into(),
            "".into(),
            &k,
            1,
        ));
        let mac = hello_mac(&k, "d1", "ab");
        assert!(ds.admit("d1", "ab", &mac, "192.168.0.9", 50));
        assert_eq!(ds.views()[0].last_seen, 50);
        assert_eq!(ds.views()[0].last_addr, "192.168.0.9");
        assert!(!ds.admit("d1", "ac", &mac, "x", 60), "another challenge");
        assert!(!ds.admit("d2", "ab", &mac, "x", 60), "unknown device");
        assert_eq!(ds.views()[0].last_seen, 50, "a failed proof marks nothing");
    }

    #[test]
    fn a_forgotten_device_can_no_longer_watch() {
        let mut ds = Devices::default();
        let k = DeviceKey::from_bytes([7; 32]);
        ds.add(Device::new(
            "d1".into(),
            "Pixel".into(),
            "".into(),
            "".into(),
            &k,
            1,
        ));
        assert!(ds.forget("d1"));
        assert!(!ds.admit("d1", "ab", &hello_mac(&k, "d1", "ab"), "x", 2));
        assert!(!ds.forget("d1"));
    }

    #[test]
    fn when_full_the_device_seen_longest_ago_makes_room() {
        let mut ds = Devices::default();
        for i in 0..MAX_DEVICES {
            ds.add(dev(&format!("d{i}"), 100 + i as u64));
        }
        let gone = ds.add(dev("new", 1)).unwrap();
        assert_eq!(gone.id, "d0");
        assert_eq!(ds.len(), MAX_DEVICES);
    }

    #[test]
    fn the_file_round_trips_and_views_never_carry_the_key() {
        let mut ds = Devices::default();
        ds.add(dev("d1", 5));
        let back = Devices::from_json(&ds.to_json()).unwrap();
        assert_eq!(back.views(), ds.views());
        let shown = serde_json::to_string(&ds.views()).unwrap();
        assert!(!shown.contains("key"), "{shown}");
        assert!(
            !format!("{:?}", ds.list[0]).contains("0101"),
            "Debug hides the key"
        );
    }

    #[test]
    fn rename_cleans_the_name() {
        let mut ds = Devices::default();
        ds.add(dev("d1", 5));
        assert!(ds.rename("d1", " Living\u{202E}room TV "));
        assert_eq!(ds.name_of("d1").unwrap(), "Livingroom TV");
    }
}
