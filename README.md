# Better StressBots for Pumpkin

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Pumpkin](https://img.shields.io/badge/Pumpkin-0.2.1%2B26.3--26.51-blue)](https://github.com/Pumpkin-MC/Pumpkin)
[![Minecraft](https://img.shields.io/badge/Minecraft-1.26.51-green)](https://www.minecraft.net/)

A Pumpkin port of **BetterStresstestbots 2.0.0** (`BetterStresstestbots-2.0.0-26.3.jar`).

> **NPC approximation, not a true fake-player port.** Pumpkin's WASM API has no
> way to construct a `ServerPlayer` or open a socket. This port spawns
> `minecraft:mannequin` entities and moves them every tick. It stresses entity
> ticking and movement packets, not login/connection paths.

---

## Table of contents

- [Requirements](#requirements)
- [Installation](#installation)
- [Tab list setup](#tab-list-setup)
- [Commands](#commands)
- [Behavior](#behavior)
- [Verification](#verification)
- [Troubleshooting](#troubleshooting)
- [Uninstall](#uninstall)
- [Limitations](#limitations)
- [Build from source](#build-from-source)
- [Source layout](#source-layout)
- [Bundled scripts](#bundled-scripts)
- [Contributing](#contributing)
- [License](#license)

---

## Requirements

- Pumpkin `0.2.1+26.3-26.51` (Minecraft Java `1.26.51`, protocol 777)
- WASM plugin support (official release includes it)
- `.wasm` placed in the server's `plugins/` directory
- Java client for mannequin rendering and tab list (Bedrock cannot render them)

`allow-flight=true` is **not** required. Bots are NPCs with gravity disabled.

---

## Installation

### Quick install (Linux x86_64)

```bash
./install.sh /path/to/pumpkin-server
```

The installer backs up `pumpkin`, installs the patched binary, copies the WASM
into `plugins/`, and clears stale plugin cache entries. Then restart the server.

### Manual install

1. Copy `better-stressbots-pumpkin.wasm` into `plugins/`.
   Remove any older stress-bot `.wasm` copies first.
2. Restart the server (do not rely on hot-reload).
3. Make yourself an operator, or grant `better-stressbots:use` (default: op 2).
4. Run `/stress status` to confirm the plugin loaded.

> Pumpkin requires plugin permissions to be namespaced. The Paper node
> `stresstestbots.use` becomes **`better-stressbots:use`** here.

---

## Tab list setup

Bots are mannequin entities. They do **not** appear in the tab list on a stock
Pumpkin server because the WASM host drops `CPlayerInfoUpdate`.

To enable tab-list entries:

1. Replace the server binary with the patched one (see [Installation](#installation)).
   The included binary is **statically linked** and runs on any Linux.
2. Restart the server.
3. Run `/start 5`, close any menu, and hold Tab.

For a different Pumpkin version or to rebuild manually:

```bash
git apply host-patches/pumpkin-playerinfo-host-v0_1.patch
cargo build --release -p pumpkin
```

See `INSTALL-EXTERNAL.md` for full deployment notes.

---

## Commands

All commands require `better-stressbots:use` (op level 2).

| Command | Description |
| --- | --- |
| `/stress` / `/stress status` | Show bot count, target, speed, radius, mode |
| `/start [count] [none\|walk\|fly]` | Set count and mode, then spawn |
| `/stress start [count] [mode]` | Alias for `/start` |
| `/stress stop` | Remove every bot |
| `/stress count <n>` | Set target count (0–5000) |
| `/stress speed <n>` | Movement speed in blocks/tick (default `0.1`) |
| `/stress radius <n>` | Wander radius (default `500`) |
| `/stress mode <none\|walk\|fly>` | Movement mode (default `none`) |
| `/stress chat <msg>` | Broadcast a chat line as every bot |
| `/stress tp` / `teleport` | Arrange bots in a ring around you |
| `/goto [x y z [world]]` | Send bots to a location |

### Examples

```text
/start 100
/start 100 walk
/stress count 250
/stress speed 0.2
/stress radius 200
/stress mode fly
/goto 0 80 0
/goto 100 70 -200 world_nether
/stress chat hello
/stress tp
/stress stop
```

### Tab completion

- `/stress ` lists all subcommands.
- `/stress count|speed|radius ` suggest common values.
- `/stress mode ` and `/stress start`'s second argument suggest `none`, `walk`, `fly`.
- `/stress goto ` suggests `<x> <y> <z>`, then loaded world names.

> Pumpkin 0.2.1 only answers suggestions after a space.

---

## Behavior

- Bots spawn in the sender's world, in a spiral ring starting 10 blocks away.
- Console `/start` uses the first online player's position; falls back to world
  spawn if none online.
- Names: `StressBot_0`, `StressBot_1`, ...
- Bots spawn two ticks apart (100 bots ≈ 10 seconds; 5000 ≈ 8 min 20 s).
- Each bot announces `StressBot_N joined the game` / `left the game` in chat.
- `none`: stays until `/goto`.
- `walk`: wanders within radius, follows terrain (min Y 60).
- `fly`: wanders within radius, target Y clamped 64–250.
- `/goto` makes bots walk or fly; cross-world bots teleport to the target world.
- `/stress tp` clears `/goto` and arranges bots in a ring around you.
- On unload/shutdown, bots are removed and worlds are saved.
- Stale `StressBot_*` mannequins are swept on startup, join, every 200 ticks,
  on `/start`, and on `/stress stop`. Live bots are never removed by sweeps.

---

## Verification

Tested against official Pumpkin `0.2.1+26.3-26.51`:

- Plugin loads/unloads without errors
- All commands and suggestions
- Staggered spawning and 5000 cap
- 100-bot spawn/status/stop cycle
- Java client renders mannequins with name tags and join messages
- Stale-bot cleanup after hard kill and restart
- Tab-list entries with patched host

Final tested artifact SHA-256:

```text
e7e685758ae74a36007913239e3e0c0ee2e552c71fc8b78d65c681cd94004051
```

Build stamp: `tablist-hostfix-2026-10-28-r2`

---

## Troubleshooting

- **Bots spawn but you cannot see them**: run `/stress tp`. Check the log for
  `Spawning bots in world '...' at (x, y, z)`.
- **`/start` reports bots but status stays 0**: check the log for load errors;
  ensure only one `.wasm` copy exists.
- **No join messages**: confirm the plugin loaded and you use a Java client.
- **Bots sink while walking**: reduce `/stress radius` so they stay in loaded
  terrain.
- **Stale entities after a crash**: swept automatically; last resort
  `/kill @e[type=minecraft:mannequin]`.
- **After replacing the `.wasm`, restart the server.** Do not hot-reload.

---

## Limitations (read before using)

- Bots are **mannequin entities**, not real `ServerPlayer`s. They do not load
  chunks and disappear from the tab list on a stock server (hence the patched
  binary).
- Tab-list entries only work with the **patched** Pumpkin binary
  (`host-patches/` + `build-patched-server.sh`).
- The native real-player approach (Option B) was attempted but is not shipped;
  see `PROJECT-DEEPDIVE.md`.
- The patched binary is Linux x86_64 / glibc only.
- Hot-reload is not supported; restart the server after replacing files.

## Uninstall

To remove all modifications:

```bash
./uninstall.sh /path/to/pumpkin-server
```

The script:

1. Restores the original server binary from the latest `pumpkin.bak.*` backup.
2. Removes `plugins/better-stressbots-pumpkin.wasm`.
3. Deletes stale `*better-stressbots*.cwasm` / `*better_stressbots*.cwasm`
   cache files.

Restart the server after running it.

---

## Limitations

- Not real players: no connections, auth, player count, tickets, chunk loading.
- Not in `/list` player count, but tab-list entries work with a patched host.
- No chunk loading; bots may sit underground in unloaded areas.
- No network or login stress.
- No custom skins, ping, stats, or advancements.
- Chat is a system broadcast.
- Mannequin support required (Java client only).
- Permission node differs from Paper: `better-stressbots:use`.
- `/goto` cross-world teleports bots (Paper only changes coordinates).

---

## Build from source

```bash
rustup target add wasm32-wasip2
cargo build --release
```

Output:

```text
target/wasm32-wasip2/release/better_stressbots_pumpkin.wasm
```

Pinned to `pumpkin-plugin-api = "0.2.1"` (synchronous WIT API).

---

## Source layout

- `src/lib.rs` — metadata, permission, command registration, ticker, cleanup
- `src/bots.rs` — bot state, spawning, movement, tab-list sync, cleanup
- `src/commands.rs` — command parsing and suggestions

---

## Bundled scripts

- `install.sh` — install patched binary + WASM into a server directory
- `build-patched-server.sh` — build the patched server on the target machine
- `uninstall.sh` — restore the original binary and remove the plugin

---

## Contributing

Issues and pull requests are welcome. Please keep changes focused and test
against Pumpkin `0.2.1+26.3-26.51`.

## License

MIT — see [LICENSE](LICENSE).
