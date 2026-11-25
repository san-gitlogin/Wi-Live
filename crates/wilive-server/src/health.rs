//! The app watching itself: memory and CPU, for the owner's window and for the soak
//! tests that hold it to a ceiling. Cheap: two OS calls, once a second at most.

use std::sync::Mutex;
use std::time::Instant;

/// CPU time used by this process so far (user + system), in seconds.
fn cpu_seconds() -> Option<f64> {
    #[cfg(unix)]
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 {
            return None;
        }
        let t = |v: libc::timeval| v.tv_sec as f64 + v.tv_usec as f64 / 1e6;
        Some(t(u.ru_utime) + t(u.ru_stime))
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
        let z = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut c, mut e, mut k, mut u) = (z, z, z, z);
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) == 0 {
            return None;
        }
        let ft =
            |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64 / 1e7;
        Some(ft(k) + ft(u))
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

pub struct Health {
    started: Instant,
    last: Mutex<Option<(Instant, f64, f64)>>,
}

impl Default for Health {
    fn default() -> Self {
        Health {
            started: Instant::now(),
            last: Mutex::new(None),
        }
    }
}

impl Health {
    /// `{rss_mb, cpu_pct, uptime_s}`. CPU is the share of one core since the last
    /// call (at least 0.5 s apart; nearer calls repeat the last figure).
    pub fn now(&self) -> serde_json::Value {
        let rss = memory_stats::memory_stats().map(|m| m.physical_mem as f64 / 1_048_576.0);
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let cpu = match (cpu_seconds(), *last) {
            (Some(c), Some((at, c0, pct))) => {
                let dt = at.elapsed().as_secs_f64();
                if dt >= 0.5 {
                    let p = ((c - c0) / dt * 100.0).max(0.0);
                    *last = Some((Instant::now(), c, p));
                    Some(p)
                } else {
                    Some(pct)
                }
            }
            (Some(c), None) => {
                *last = Some((Instant::now(), c, 0.0));
                None
            }
            _ => None,
        };
        let r1 = |v: f64| (v * 10.0).round() / 10.0;
        serde_json::json!({
            "rss_mb": rss.map(r1), "cpu_pct": cpu.map(r1),
            "uptime_s": self.started.elapsed().as_secs(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_and_cpu_are_measured() {
        let h = Health::default();
        let a = h.now();
        assert!(a["rss_mb"].as_f64().unwrap() > 1.0, "{a}");
        let end = Instant::now() + std::time::Duration::from_millis(600);
        let mut x = 0u64;
        while Instant::now() < end {
            x = x.wrapping_add(1);
        }
        let b = h.now();
        assert!(
            b["cpu_pct"].as_f64().unwrap() > 30.0,
            "a busy loop shows as CPU: {b} ({x})"
        );
    }
}
