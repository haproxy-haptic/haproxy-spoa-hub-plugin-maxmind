use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Bounded warning deduplication helper.
///
/// Keyed on `(lookup_name, path_display)`, returns `true` from `should_warn` at
/// most once per key per `window`. Bounds memory by opportunistically GC'ing
/// expired entries when the map exceeds `MAX_ENTRIES`.
pub struct WarnOnce {
    window: Duration,
    seen: Mutex<HashMap<(String, String), Instant>>,
}

const MAX_ENTRIES: usize = 1024;

impl WarnOnce {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `true` if the caller should emit the warning for this
    /// `(lookup, path)` pair, `false` if it has already been warned within the
    /// current window.
    pub fn should_warn(&self, lookup: &str, path: &str) -> bool {
        let now = Instant::now();
        let mut map = self.seen.lock().expect("WarnOnce mutex poisoned");

        if map.len() > MAX_ENTRIES {
            let window = self.window;
            map.retain(|_, t| now.duration_since(*t) < window);
        }

        let key = (lookup.to_owned(), path.to_owned());
        match map.get(&key) {
            Some(&t) if now.duration_since(t) < self.window => false,
            _ => {
                map.insert(key, now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn first_call_returns_true() {
        let w = WarnOnce::new(Duration::from_secs(60));
        assert!(w.should_warn("country_code", "country/iso_code"));
    }

    #[test]
    fn second_call_within_window_returns_false() {
        let w = WarnOnce::new(Duration::from_secs(60));
        assert!(w.should_warn("country_code", "country/iso_code"));
        assert!(!w.should_warn("country_code", "country/iso_code"));
    }

    #[test]
    fn second_call_after_window_returns_true() {
        let w = WarnOnce::new(Duration::from_millis(50));
        assert!(w.should_warn("country_code", "country/iso_code"));
        sleep(Duration::from_millis(80));
        assert!(w.should_warn("country_code", "country/iso_code"));
    }

    #[test]
    fn distinct_keys_warn_independently() {
        let w = WarnOnce::new(Duration::from_secs(60));
        assert!(w.should_warn("a", "p"));
        assert!(w.should_warn("b", "p"));
        assert!(w.should_warn("a", "q"));
        assert!(!w.should_warn("a", "p"));
    }

    #[test]
    fn gc_runs_above_threshold() {
        let w = WarnOnce::new(Duration::from_millis(10));
        for i in 0..(MAX_ENTRIES + 5) {
            w.should_warn(&format!("lookup{i}"), "path");
        }
        sleep(Duration::from_millis(30));
        // Trigger another insertion which should run the retain GC.
        w.should_warn("trigger", "trigger");
        let map = w.seen.lock().unwrap();
        // After GC, only the recent (post-sleep) insertion should remain.
        assert!(
            map.len() < MAX_ENTRIES,
            "expected GC to drop expired entries, got {}",
            map.len()
        );
    }
}
