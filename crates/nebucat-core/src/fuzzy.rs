use std::cmp::Ordering;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub index: usize,
    pub score: f64,
    pub indices: Vec<u32>,
}

pub fn rank(query: &str, names: &[&str], boosts: &[f64], limit: usize) -> Vec<Hit> {
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut indices = Vec::new();
    let mut hits = Vec::new();

    for (index, name) in names.iter().enumerate() {
        indices.clear();
        let Some(score) = pattern.indices(Utf32Str::new(name, &mut buf), &mut matcher, &mut indices)
        else {
            continue;
        };
        // Multi-word patterns append indices per atom, unsorted and possibly duplicated.
        indices.sort_unstable();
        indices.dedup();
        hits.push(Hit {
            index,
            score: score as f64 + boosts.get(index).copied().unwrap_or(0.0),
            indices: indices.clone(),
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| cmp_names(names[a.index], names[b.index]))
            .then_with(|| a.index.cmp(&b.index))
    });
    hits.truncate(limit);
    hits
}

fn cmp_names(a: &str, b: &str) -> Ordering {
    a.chars()
        .count()
        .cmp(&b.chars().count())
        .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::boost;

    #[test]
    fn vsc_matches_visual_studio_code_initials() {
        let hits = rank("vsc", &["Visual Studio Code", "Voice Recorder"], &[0.0, 0.0], 8);
        assert_eq!(hits[0].index, 0);
        assert_eq!(hits[0].indices, vec![0, 7, 14]);
    }

    #[test]
    fn history_boost_lifts_weaker_match() {
        let names = ["Notepad", "OneNote"];
        let plain = rank("note", &names, &[0.0, 0.0], 8);
        assert_eq!(plain.len(), 2);
        assert_eq!(plain[0].index, 0);
        assert!(plain[0].score > plain[1].score);

        let boosted = rank("note", &names, &[0.0, boost(20)], 8);
        assert_eq!(boosted[0].index, 1);
    }

    #[test]
    fn ties_prefer_shorter_then_alphabetical() {
        let hits = rank("", &["bbb", "Aaa", "cc"], &[0.0, 0.0, 0.0], 8);
        let order: Vec<usize> = hits.iter().map(|h| h.index).collect();
        assert_eq!(order, vec![2, 1, 0]);
    }

    #[test]
    fn limit_truncates_and_non_matches_drop() {
        let hits = rank("a", &["a1", "a2", "a3", "zzz"], &[0.0; 4], 2);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.index < 3));
    }
}
