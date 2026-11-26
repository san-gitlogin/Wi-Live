//! Wi-Live's rules, free of any OS: pairing crypto, pairing requests, the allowed
//! devices and the access log. Everything is passed its time, so every rule has a
//! test that runs in milliseconds.
pub mod devices;
pub mod log;
pub mod pair;
pub mod pending;
pub mod update;
