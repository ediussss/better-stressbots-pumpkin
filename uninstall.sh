#!/usr/bin/env bash
# Remove BetterStressBots and restore the original Pumpkin server binary.
#
# Usage:
#   ./uninstall.sh /path/to/pumpkin-server

set -euo pipefail

SERVER_DIR="${1:-}"
if [[ -z "$SERVER_DIR" || ! -d "$SERVER_DIR" ]]; then
    echo "Usage: $0 /path/to/pumpkin-server" >&2
    exit 1
fi

# Restore the most recent backup of the server binary.
latest_backup="$(ls -1 "$SERVER_DIR"/pumpkin.bak.* 2>/dev/null | sort | tail -n 1 || true)"
if [[ -n "$latest_backup" ]]; then
    cp -f "$latest_backup" "$SERVER_DIR/pumpkin"
    chmod +x "$SERVER_DIR/pumpkin"
    echo "Restored server binary from $latest_backup"
else
    echo "No pumpkin.bak.* found; leaving the current server binary in place."
fi

# Remove the plugin and its cache.
rm -f "$SERVER_DIR/plugins/better-stressbots-pumpkin.wasm"
find "$SERVER_DIR/plugins" -maxdepth 3 -type f \
    \( -name '*better-stressbots*.cwasm' -o -name '*better_stressbots*.cwasm' \) \
    -delete 2>/dev/null || true
echo "Removed plugin and cache entries."

echo
echo "Restart the Pumpkin server to complete uninstallation."
