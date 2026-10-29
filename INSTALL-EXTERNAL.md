# Deploying the tab-list fix to an external Pumpkin server

The `.wasm` plugin alone cannot add bots to the tab list on a stock Pumpkin
server. Pumpkin's WASM host silently drops `CPlayerInfoUpdate` packets because
the generated host serializer has no arm for that packet.

This release therefore contains:

- `better-stressbots-pumpkin.wasm` — the plugin
- `pumpkin-server-patched-linux-x86_64` — a Pumpkin `0.2.1+26.3-26.51`
  Linux x86_64 server binary with the host serializer fixed
- `host-patches/pumpkin-playerinfo-host-v0_1.patch` — the source patch used for
  plugins built against the v0.1 WIT API (this plugin)
- `host-patches/pumpkin-playerinfo-host-v0_2.patch` — the same fix for the v0.2
  host, for future plugins

## Option A — replace the server binary

Only do this if your external server is Linux x86_64 and runs Pumpkin
`0.2.1+26.3-26.51`.

1. Stop the Pumpkin server.
2. Back up the existing server binary.
3. Upload `pumpkin-server-patched-linux-x86_64` as the new server binary
   (normally named `pumpkin`) and make it executable.
4. Put `better-stressbots-pumpkin.wasm` in the server's `plugins/` directory.
5. Start the server.
6. Reconnect with a Java client, run `/start 5`, close any menu, and hold Tab.

If your external server is a different OS, architecture, or Pumpkin build,
use Option B instead.

## Option B — patch and rebuild Pumpkin from source

From your Pumpkin source checkout:

```bash
git apply host-patches/pumpkin-playerinfo-host-v0_1.patch
cargo build --release -p pumpkin
```

Then replace your server binary with `target/release/pumpkin`, put the `.wasm`
in `plugins/`, and restart.

## Notes

- The patched binary was built locally for testing only; it is not connected to
  your external server.
- Vanilla Pumpkin does not count WASM-plugin table entries in `/list` or the
  server player count. The entries are tab-list UI entries backed by mannequin
  entities, not real player connections.
- Bedrock clients do not render Java tab-list entries.
