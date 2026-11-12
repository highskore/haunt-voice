//! End to end against a real haunt core. Copy into haunt's `cli/tests/` and run
//! `cargo test -p haunt-cli --features plugins --test voice` (see README).
#![cfg(feature = "plugins")]

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use haunt_core::plugin_store::{PluginSet, Store, attach};
use haunt_core::plugins::{HostRequest, PluginHost};
use haunt_core::{Core, SurfaceEnv};

const ID: &str = "dev.haunt.voice";

fn repo() -> PathBuf {
    std::env::var_os("HAUNT_PLUGINS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../haunt-plugins"))
        .join("haunt-voice")
}

fn send(sock: &mut UnixStream, env: &SurfaceEnv, nonce: bool, cwd: &str, rest: &str) {
    let nonce = if nonce { format!(r#","nonce":"{}""#, env.nonce) } else { String::new() };
    let line = format!(r#"{{"v":1,"agent":"claude","surface_id":"{}"{nonce},"session_id":"s1","cwd":"{cwd}",{rest}}}"#, env.surface_id);
    sock.write_all(format!("{line}\n").as_bytes()).unwrap();
}

/// What Voice said, with "stop" for a cut-off.
fn said(set: &Mutex<PluginSet>) -> Vec<String> {
    set.lock().unwrap().take_requests().into_iter().filter_map(|r| match r {
        HostRequest::Say { plugin_id, text } if plugin_id == ID => Some(text),
        HostRequest::SayStop { plugin_id } if plugin_id == ID => Some("stop".into()),
        _ => None,
    }).collect()
}

fn until(what: &str, f: impl Fn() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn speaks_up_only_while_you_are_away() {
    let dir = repo();
    if !dir.join("haunt-plugin.toml").is_file() {
        eprintln!("skipped: haunt-voice not found; set HAUNT_PLUGINS_DIR");
        return;
    }
    let home = PathBuf::from(format!("/tmp/haunt-fp-voice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--release", "--target", "wasm32-wasip2"])
        .current_dir(&dir)
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .unwrap();
    assert!(status.success());
    let o = Command::new(env!("CARGO_BIN_EXE_haunt")).args(["plugin", "install", dir.to_str().unwrap(), "--yes"]).env("HAUNT_HOME", &home).output().unwrap();
    assert!(o.status.success(), "{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));

    let core = Core::new(&home);
    core.start().unwrap();
    let (api, web) = (core.register_surface("/tmp").unwrap(), core.register_surface("/tmp").unwrap());
    let set = Store::open(&home).unwrap().load_all(&PluginHost::new().unwrap(), Some(&core.handle())).unwrap();
    assert!(set.errors().is_empty(), "{:?}", set.errors());
    let set = Arc::new(Mutex::new(set));
    attach(&core, &set);
    let mut sock = UnixStream::connect(&api.socket_path).unwrap();

    send(&mut sock, &api, true, "/tmp/api", r#""event":"permission_seen","tool_name":"Bash","tool_input":{"command":"git push"}"#);
    let titles = || set.lock().unwrap().palette_items(None, "waiting").into_iter().map(|i| i.title).collect::<Vec<_>>();
    assert_eq!(titles(), ["Who's waiting?"]);
    // Over a second, so the two waits get different times on a coarse plugin clock.
    std::thread::sleep(Duration::from_millis(1100));
    assert_eq!(said(&set), Vec::<String>::new(), "you're at the screen");

    let t = Instant::now();
    assert_eq!(set.lock().unwrap().presence(true, 0, t), Some((true, 0)));
    send(&mut sock, &web, false, "/tmp/web", r#""event":"permission_seen","tool_name":"Bash","tool_input":{"command":"say pwned"}"#);
    send(&mut sock, &web, true, "/tmp/web", r#""event":"permission_seen","tool_name":"Bash","tool_input":{"command":"rm -rf $(echo dist) && x"}"#);
    let heard = Mutex::new(Vec::new());
    until("the announcement", || {
        heard.lock().unwrap().extend(said(&set));
        !heard.lock().unwrap().is_empty()
    });
    assert_eq!(heard.into_inner().unwrap(), ["web needs you: rm -rf echo dist"], "the forged event says nothing, the command only its first words");

    std::thread::sleep(Duration::from_millis(2100));
    set.lock().unwrap().invoke(ID, "waiting", true).unwrap();
    let answer = said(&set);
    assert!(matches!(answer.as_slice(), [a] if a.starts_with("api") && a.contains("git push") && a.contains("web")), "{answer:?}");

    set.lock().unwrap().presence(false, 0, t + Duration::from_secs(60));
    assert_eq!(said(&set), ["stop"], "back at the screen: quiet");

    std::fs::remove_dir_all(&home).unwrap();
}
