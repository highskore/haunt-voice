//! Voice: Haunt says out loud when an agent needs you, by default only while you're away from the
//! screen, and answers "who's waiting?" from the ⌘K palette. Speech out only.
// wit-bindgen's flattened `notify` import has 14 params.
#![allow(clippy::too_many_arguments)]

wit_bindgen::generate!({ path: "wit/haunt-plugin.wit", world: "plugin" });

// Shared with the other fleet-following plugins; not every helper is used here.
#[allow(dead_code)]
mod fleet;
mod voice;

use std::collections::BTreeMap;
use std::sync::Mutex;

use fleet::{Ev, Fleet, State};
use haunt::plugin::log::log;
use haunt::plugin::types::LogLevel;
use haunt::plugin::{palette, speech, timer};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Say "api needs you: git push" when an agent starts waiting.
    pub announce: bool,
    /// Only while you're away from the screen.
    pub only_when_away: bool,
    /// Read the waiting command's first words.
    pub read_commands: bool,
    pub unverified: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { announce: true, only_when_away: true, read_commands: true, unverified: false }
    }
}

impl Config {
    fn voice(&self) -> voice::Settings {
        voice::Settings { announce: self.announce, read_commands: self.read_commands }
    }
}

#[derive(Default)]
struct Live {
    config: Config,
    fleet: Fleet,
    asks: BTreeMap<String, String>,
    away: bool,
    /// An announcement was refused (one utterance every 2 s); the timer says who's waiting instead.
    behind: bool,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Live) -> R) -> R {
    f(LIVE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Live::default))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn say(text: &str) -> bool {
    match speech::say(text) {
        Ok(()) => true,
        Err(e) => {
            log(LogLevel::Warn, &format!("voice: not spoken: {e}"));
            false
        }
    }
}

fn announce(l: &mut Live, text: &str) {
    if !say(text) && !l.behind {
        l.behind = true;
        let _ = timer::every(15);
    }
}

fn catch_up(l: &mut Live) {
    let wanted = l.behind && l.config.announce && (l.away || !l.config.only_when_away) && l.fleet.count(State::NeedsYou) > 0;
    if wanted && !say(&voice::whos_waiting(&l.fleet, |s| l.asks.get(s).cloned(), now_ms(), &l.config.voice())) {
        return;
    }
    l.behind = false;
    timer::stop();
}

/// What to say when `name` starts waiting, if anything.
pub fn on_wait(name: &str, ask: Option<&str>, c: &Config, away: bool) -> Option<String> {
    if c.only_when_away && !away {
        return None;
    }
    voice::announce(name, ask, &c.voice())
}

struct Plugin;

impl Guest for Plugin {
    fn init(json: String) {
        with(|l| l.config = serde_json::from_str(&json).unwrap_or_default());
        palette::command("waiting", "Who's waiting?", None);
    }

    fn on_settings_changed(json: String) {
        with(|l| l.config = serde_json::from_str(&json).unwrap_or_default());
        palette::command("waiting", "Who's waiting?", None);
    }

    fn on_event(event: AgentEvent) {
        let Some(surface) = event.surface.as_deref() else { return };
        with(|l| {
            if !event.verified && !l.config.unverified {
                return;
            }
            let ev = Ev { surface, kind: &event.kind, cwd: event.cwd.as_deref(), summary: event.summary.as_deref(), subagent: event.agent_id.is_some() };
            let Some(change) = l.fleet.apply(&ev, now_ms()) else { return };
            if change.to != State::NeedsYou {
                l.asks.remove(surface);
                return;
            }
            match event.command.as_deref().or(event.tool.as_deref()) {
                Some(c) => l.asks.insert(surface.to_owned(), c.to_owned()),
                None => l.asks.remove(surface),
            };
            let name = l.fleet.agents.get(surface).map(|a| a.name.clone()).unwrap_or_default();
            if let Some(text) = on_wait(&name, l.asks.get(surface).map(String::as_str), &l.config, l.away) {
                announce(l, &text);
            }
        });
    }

    fn palette_items(_mode: String, _query: String) -> Vec<PaletteItem> {
        Vec::new()
    }

    fn on_action(action_id: String, _payload: String) {
        if action_id != "waiting" {
            return;
        }
        let text = with(|l| voice::whos_waiting(&l.fleet, |s| l.asks.get(s).cloned(), now_ms(), &l.config.voice()));
        say(&text);
    }

    fn row_menu(_surface: String) -> Vec<MenuItem> {
        Vec::new()
    }

    fn on_row_moved(_surface: String, _group: String, _before: Option<String>) {}

    fn on_theme_changed() {}

    fn on_tick(_now_ms: u64) {
        with(catch_up);
    }

    fn on_exec(_job: u64, _outcome: Result<haunt::plugin::exec::Output, String>) {}

    fn on_away_changed(away: bool, _away_ms: u64) {
        with(|l| {
            l.away = away;
            if !away {
                l.behind = false;
                timer::stop();
            }
        });
        if !away {
            speech::stop();
        }
    }

    fn on_hover_hold(_held: bool, _surface: String) {}

    fn on_fs_tree(_job: u64, _outcome: Result<haunt::plugin::fs_list::TreeResult, String>) {}
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaks_up_only_while_you_are_away_unless_asked() {
        let c = Config::default();
        assert_eq!(on_wait("api", Some("git push"), &c, false), None);
        assert_eq!(on_wait("api", Some("git push"), &c, true).as_deref(), Some("api needs you: git push"));
        assert_eq!(on_wait("api", None, &Config { only_when_away: false, ..c.clone() }, false).as_deref(), Some("api needs you"));
        assert_eq!(on_wait("api", None, &Config { announce: false, ..c }, true), None);
    }

    #[test]
    fn manifest_asks_for_what_it_uses_and_defaults_match() {
        let m = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/haunt-plugin.toml")).unwrap();
        let t: toml::Table = toml::from_str(&m).unwrap();
        let caps = t["capabilities"].as_table().unwrap();
        assert_eq!(caps.keys().collect::<Vec<_>>(), ["away", "event_details", "events", "speech", "timer"]);
        let defaults: serde_json::Map<String, serde_json::Value> =
            t["settings"].as_table().unwrap().iter().map(|(k, v)| (k.clone(), serde_json::to_value(&v["default"]).unwrap())).collect();
        assert_eq!(serde_json::from_value::<Config>(serde_json::Value::Object(defaults)).unwrap(), Config::default());
    }
}
