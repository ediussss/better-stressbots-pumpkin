# BetterStressBots-Pumpkin — deep dive (for Gemini)

## What it is

`BetterStressBots-Pumpkin` is a Rust WebAssembly plugin for the Pumpkin Minecraft server. It is a port of the Paper plugin **BetterStresstestbots 2.0.0** (`BetterStresstestbots-2.0.0-26.3.jar`), built against Pumpkin `0.2.1+26.3-26.51` (Minecraft Java `1.26.51`, protocol 777).

It is **not** a true fake-player plugin. Pumpkin’s WASM API cannot construct a `ServerPlayer` or open a network socket, so the port spawns `minecraft:mannequin` entities (player-shaped NPCs) and moves them every tick. The result stresses entity ticking and movement packets, not login/connection paths.

## Repository layout

```
better-stressbots-pumpkin/
├── Cargo.toml
├── better-stressbots-pumpkin.wasm        # release artifact
├── pumpkin-server-patched-linux-x86_64   # patched host binary (Linux x86_64)
├── host-patches/
│   ├── pumpkin-playerinfo-host-v0_1.patch
│   └── pumpkin-playerinfo-host-v0_2.patch
├── INSTALL-EXTERNAL.md
├── README.md
└── src/
    ├── lib.rs        # metadata, permission, commands, ticker, cleanup
    ├── bots.rs       # state, spawning, movement, tab-list sync, cleanup
    └── commands.rs   # command parsing and suggestions
```

## Plugin API version

The plugin uses `pumpkin-plugin-api = "0.2.1"`, whose WIT package is `pumpkin:plugin@0.1.0`. Therefore Pumpkin loads it through the **v0_1 WASM host** (`pumpkin-wasm-host-v0_1`). This detail is critical: fixing the v0_2 host does not help this plugin.

## Architecture

- Language/runtime: Rust → `wasm32-wasip2` component.
- Plugin model: synchronous WIT API (`EventHandler`, `SchedulerExt`).
- State: one global `static MANAGER: Mutex<Manager>` holding all bots and config.
- Tick loop: `schedule_repeating_task(1, 1, ...)` calls `bots::tick`.
- Cleanup: delayed task at 100 ticks, repeating every 200 ticks, plus `PlayerJoinEvent`.
- Shutdown: `on_unload` removes bots and saves worlds.

### Key types

- `Mode { None, Walk, Fly }`
- `Bot { entity, uuid, name, spawn, target, goto_target, mode }`
- `Manager { bots, speed, radius, target_count, mode, spawning, tick, rng, spawn_origin }`

### Bot lifecycle

1. `/start [count] [mode]` sets target count and starts staggered spawning.
2. Every 2 ticks one bot spawns (spiral ring around player or world spawn).
3. Each bot gets a UUID and `StressBot_N` name, is spawned as `EntityType::Mannequin`, configured (gravity off, invulnerable, name visible), and added to tab list.
4. `tick` moves all bots toward wander targets or `/goto` destinations.
5. `/stress stop` removes all bots and cleans stale entities.
6. `on_unload` removes remaining bots and saves worlds.

### Movement

- `none`: stays unless `/goto`.
- `walk`: moves to random points within radius, follows terrain (`surface_y`, min Y 60), with step-up and body-collision checks.
- `fly`: moves in 3D toward random points, Y clamped 64–250.
- `/goto`: walk or fly toward destination; cross-world bots are teleported into the target world.
- `/stress tp`: arranges bots in a ring around the player and re-rolls targets.

### Stale entity cleanup

Mannequins are saveable, so crashes can leave `StressBot_*` entities. The plugin:
- removes loaded `StressBot_*` mannequins on startup (delayed 100 ticks),
- on every player join (`JoinCleanup`),
- every 200 ticks,
- on `/start` and `/stress stop`.
- Managed bots are tracked by entity ID and excluded from sweeps, so live bots are never removed.

### Tab list injection

The plugin attempts to add bots to the Java client tab list using:

- `ClientboundPacket::CPlayerInfoUpdate` with flags `0x01 | 0x04 | 0x08 | 0x10 | 0x40 | 0x80`.
- Actions: `AddPlayer(name)`, `UpdateGameMode(1)`, `UpdateListed(true)`, `UpdateLatency(0)`, `UpdateListOrder(0)`, `UpdateHat(true)`.
- Removal via `CRemovePlayerInfo`.

Packets are sent through `Player::as_java().send_packet(&packet)` to every online Java client. On join, `sync_tab_list_all` re-sends all bots so late joiners see them.

## Root cause of the tab-list bug

Pumpkin’s host serializer (`pumpkin-wasm-host-v0_1/src/generated_packets.rs`) had **no match arm for `CPlayerInfoUpdate`**. `serialize_java_packet` returned `None`, so packets were silently dropped. The plugin logs showed success, but nothing reached the client.

Fix: patch the host to serialize `CPlayerInfoUpdate`, rebuild the Pumpkin server binary, and use that binary. The v0_2 host was patched too for future plugins.

## Bugs found and fixed during development

1. Host serializer missing `CPlayerInfoUpdate` (silent drop).
2. Wrong host version patched first (v0_2 instead of v0_1).
3. Bot game mode was `0` (Survival) initially; changed to `1` (Creative) to avoid survival side effects.
4. Stale mannequins persisted across crashes; added multi-sweep cleanup.
5. Cross-world `/goto` originally impossible; implemented teleport to target world.
6. Permission node must be namespaced: `better-stressbots:use`.
7. Command suggestions only appear after a space on Pumpkin 0.2.1 (host behavior).
8. Build stamp added to prove which WASM is loaded.

## Build and deploy

Build plugin:

```bash
rustup target add wasm32-wasip2
cargo build --release
# output: target/wasm32-wasip2/release/better_stressbots_pumpkin.wasm
```

Build patched server (if needed):

```bash
cd pumpkin-source
git apply host-patches/pumpkin-playerinfo-host-v0_1.patch
cargo build --release -p pumpkin
```

Deploy:
- Copy `.wasm` to `plugins/`.
- Replace server binary with patched one (Linux x86_64) or rebuild.
- Restart server (do not rely on hot-reload).
- Reconnect client, run `/start 5`, close menus, hold Tab.

## Usage summary

Commands: `/stress`, `/start`, `/stress stop`, `/stress count`, `/stress speed`, `/stress radius`, `/stress mode`, `/stress chat`, `/stress tp`, `/goto`, `/stress goto`. Defaults: speed 0.1, radius 500, mode none, cap 5000. Bots spawn staggered every 2 ticks.

## Limitations

- Not real players: no connections, auth, player count, tickets, chunk loading.
- Not in `/list` player count, but synthetic tab-list entries are now possible with patched host.
- No chunk loading; bots may sit underground in unloaded areas.
- No network/login stress.
- No custom skins/ping/stats.
- Chat is system broadcast.
- Mannequin support required (Java client).
- Permission node differs from Paper.
- `/goto` cross-world behavior differs.

## Verification markers

- Plugin log: `better-stressbots loaded (NPC approximation mode, build tablist-hostfix-2026-10-28)`
- Tab list: bot entries appear only with patched host binary.
- Build stamp string present in WASM confirms which file is loaded.

## Current artifact

- WASM: `better-stressbots-pumpkin.wasm`
- SHA-256: `9b435bf477c16aa4da839d6de4b71a32b00e8295e7f11efc6986f764f94ef93a`
- Size: 643,341 bytes
- Patched server binary: `pumpkin-server-patched-linux-x86_64` (Pumpkin `0.2.1+26.3-26.51`, Linux x86_64)
