//! The access log: who paired, watched, was refused or ended (R-26).
//!
//! Bounded ([`CAP`] events, oldest dropped), kept on this computer only, and never
//! holds a key, a proof or a code.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const CAP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    PairAsked,
    PairAllowed,
    PairDenied,
    Watching,
    Ended,
    Refused,
    Forgotten,
    Paused,
    Resumed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Unix seconds.
    pub at: u64,
    pub kind: Kind,
    /// The device's name, or "" for an unknown one.
    pub who: String,
    pub addr: String,
    /// Short, plain words: why refused, which display, how long it lasted.
    pub detail: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct AccessLog {
    events: VecDeque<Event>,
}

impl AccessLog {
    pub fn push(&mut self, e: Event) {
        if self.events.len() >= CAP {
            self.events.pop_front();
        }
        let mut e = e;
        e.who = e.who.chars().take(60).collect();
        e.detail = e.detail.chars().take(120).collect();
        self.events.push_back(e);
    }

    /// Newest first, at most `n`.
    pub fn recent(&self, n: usize) -> Vec<Event> {
        self.events.iter().rev().take(n).cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("plain data")
    }
    /// A damaged file starts an empty log rather than stopping the app.
    pub fn from_json(s: &str) -> Self {
        let mut l: AccessLog = serde_json::from_str(s).unwrap_or_default();
        while l.events.len() > CAP {
            l.events.pop_front();
        }
        l
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(at: u64) -> Event {
        Event { at, kind: Kind::Watching, who: "Pixel".into(), addr: "192.168.0.5".into(), detail: String::new() }
    }

    #[test]
    fn it_keeps_the_newest_and_never_grows_past_its_cap() {
        let mut l = AccessLog::default();
        for i in 0..(CAP as u64 + 20) {
            l.push(ev(i));
        }
        assert_eq!(l.len(), CAP);
        assert_eq!(l.recent(1)[0].at, CAP as u64 + 19);
        assert_eq!(l.recent(CAP).last().unwrap().at, 20);
    }

    #[test]
    fn a_damaged_file_starts_empty() {
        assert!(AccessLog::from_json("{not json").is_empty());
        let mut l = AccessLog::default();
        l.push(ev(1));
        assert_eq!(AccessLog::from_json(&l.to_json()).recent(5), l.recent(5));
    }

    #[test]
    fn long_fields_are_cut() {
        let mut l = AccessLog::default();
        let mut e = ev(1);
        e.detail = "x".repeat(1000);
        l.push(e);
        assert_eq!(l.recent(1)[0].detail.len(), 120);
    }
}
