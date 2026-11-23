use crate::fuzzy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Restart,
    Shutdown,
    Sleep,
    Hibernate,
}

impl PowerAction {
    pub const ALL: [PowerAction; 4] = [Self::Restart, Self::Shutdown, Self::Sleep, Self::Hibernate];

    /// Row title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Restart => "Restart",
            Self::Shutdown => "Shut down",
            Self::Sleep => "Sleep",
            Self::Hibernate => "Hibernate",
        }
    }

    /// Lowercase verb for error copy: "Couldn't <verb> (error <code>)."
    pub fn verb(self) -> &'static str {
        match self {
            Self::Restart => "restart",
            Self::Shutdown => "shut down",
            Self::Sleep => "sleep",
            Self::Hibernate => "hibernate",
        }
    }

    fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Restart => &["reboot"],
            Self::Shutdown => &["shutdown", "power off", "turn off"],
            Self::Sleep => &["suspend", "standby"],
            Self::Hibernate => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerHit {
    pub action: PowerAction,
    /// Highlight positions in the title; empty when an alias matched.
    pub indices: Vec<u32>,
}

/// Best match per action, best first. An empty query lists every action.
pub fn matches(query: &str) -> Vec<PowerHit> {
    let q = query.trim();
    if q.is_empty() {
        return PowerAction::ALL
            .iter()
            .map(|&action| PowerHit { action, indices: Vec::new() })
            .collect();
    }

    let mut names = Vec::new();
    let mut owners = Vec::new();
    for action in PowerAction::ALL {
        names.push(action.title());
        owners.push((action, true));
        for &alias in action.aliases() {
            names.push(alias);
            owners.push((action, false));
        }
    }

    let mut out: Vec<PowerHit> = Vec::new();
    for hit in fuzzy::rank(q, &names, &vec![0.0; names.len()], names.len()) {
        let (action, is_title) = owners[hit.index];
        if out.iter().any(|h| h.action == action) {
            continue;
        }
        out.push(PowerHit {
            action,
            indices: if is_title { hit.indices } else { Vec::new() },
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_lists_all_in_order() {
        let actions: Vec<PowerAction> = matches("").iter().map(|h| h.action).collect();
        assert_eq!(actions, PowerAction::ALL);
        assert!(matches("  ").iter().all(|h| h.indices.is_empty()));
    }

    #[test]
    fn alias_matches_without_highlight() {
        let hits = matches("reboot");
        assert_eq!(hits[0].action, PowerAction::Restart);
        assert!(hits[0].indices.is_empty());
    }

    #[test]
    fn title_match_highlights() {
        assert_eq!(
            matches("sl")[0],
            PowerHit {
                action: PowerAction::Sleep,
                indices: vec![0, 1]
            }
        );
    }

    #[test]
    fn shutdown_one_word() {
        assert_eq!(matches("shutdown")[0].action, PowerAction::Shutdown);
    }

    #[test]
    fn no_match_is_empty() {
        assert!(matches("xyz").is_empty());
    }
}
