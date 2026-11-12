<div align="center">

<img src="docs/icon.png" width="96" alt="">

# Voice

**Haunt says out loud which agent needs you while you're away.**

[![Haunt plugin](https://img.shields.io/badge/Haunt-plugin-D97757)](https://github.com/highskore/haunt-hackathon)
![Plugin API 0.5](https://img.shields.io/badge/plugin%20API-0.5-111)
![WASM component](https://img.shields.io/badge/WASM-component-654ff0?logo=webassembly&logoColor=white)
![MIT licence](https://img.shields.io/badge/licence-MIT-6b7280)

</div>

When an agent starts waiting on you while you're away from the screen (locked, asleep or idle), Haunt says so in the system voice: "api needs you: git push origin feat/auth". Run **Who's waiting?** from the ⌘K palette to hear everyone who waits, longest first: "api, for 1 minute: git push. web, for under a minute". Speech stops when you come back.

Only the first four words of a command are read, with punctuation dropped, so a long or hostile command can't fill the room. Voice speaks only; it never listens and never answers a prompt.

## Settings

```toml
[plugins."dev.haunt.voice"]
announce = true        # say it when an agent starts waiting
only_when_away = true  # only while you're away from the screen
read_commands = true   # read the waiting command's first words
unverified = false     # also follow events without the tab's nonce
```

## Capabilities

| capability | why |
|---|---|
| `events` (session start, prompt, subagent, tool, permission request, question, stop) | each agent's state |
| `event_details` | the waiting command, read aloud; haunt asks in red, since commands can hold secrets |
| `speech` | speaks in the system voice |
| `away` | to know when you've stepped away |
| `timer` | when two agents start waiting within 2 s, says who's waiting a little later instead of dropping one |

`[plugin_fx] speech = false` in Haunt's config mutes plugin speech; `sound = false` mutes speech and sound.

## Build and test

```sh
cargo build --release --target wasm32-wasip2   # or ./build.sh
cargo test                                     # unit tests
haunt plugin install .
```

`e2e/voice.rs` runs the built plugin against a real Haunt core: copy it into a Haunt checkout's `cli/tests/` and run `HAUNT_PLUGINS_DIR=<this repo's parent> cargo test -p haunt-cli --features plugins --test voice`.

Needs Haunt with plugin API 0.5 (`api = "0.5"`) and its `speech` import and `on-away-changed` export.

## Licence

MIT, see `LICENSE`.
