use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub fn boost(count: u32) -> f64 {
    12.0 * (1.0 + count as f64).ln()
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct History {
    pub apps: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Entry {
    pub count: u32,
    pub last: u64,
}

impl History {
    pub fn record(&mut self, id: &str, now: u64) {
        let entry = self.apps.entry(id.to_string()).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.last = now;
    }

    pub fn recent(&self, n: usize) -> Vec<&str> {
        let mut entries: Vec<(&String, &Entry)> = self.apps.iter().collect();
        entries.sort_by(|a, b| b.1.last.cmp(&a.1.last).then_with(|| a.0.cmp(b.0)));
        entries.into_iter().take(n).map(|(id, _)| id.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_grows_with_count() {
        assert_eq!(boost(0), 0.0);
        assert!(boost(20) > boost(1));
    }

    #[test]
    fn record_counts_and_updates_last() {
        let mut h = History::default();
        h.record("a", 10);
        h.record("a", 30);
        assert_eq!(h.apps["a"], Entry { count: 2, last: 30 });
    }

    #[test]
    fn recent_orders_by_last_descending() {
        let mut h = History::default();
        h.record("old", 1);
        h.record("new", 3);
        h.record("mid", 2);
        assert_eq!(h.recent(10), vec!["new", "mid", "old"]);
        assert_eq!(h.recent(2), vec!["new", "mid"]);
    }

    #[test]
    fn serde_round_trip() {
        let mut h = History::default();
        h.record("x", 5);
        let json = serde_json::to_string(&h).unwrap();
        let back: History = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
    }
}
