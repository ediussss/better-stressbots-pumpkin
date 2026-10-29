#!/usr/bin/env bash
# Install the BetterStressBots Pumpkin bundle into a Pumpkin server directory.
#
# Usage:
#   ./install.sh /path/to/pumpkin-server
#
# The script:
#   1. backs up the existing server binary,
#   2. installs the patched Linux x86_64 binary when possible,
#   3. copies the plugin WASM into plugins/,
#   4. removes stale plugin cache entries.
#
# You must stop the server first and start it again afterwards.

set -euo pipefail

BUNDLE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_DIR="${1:-}"

if [[ -z "$SERVER_DIR" ]]; then
    echo "Usage: $0 /path/to/pumpkin-server" >&2
    exit 1
fi

if [[ ! -d "$SERVER_DIR" ]]; then
    echo "Server directory not found: $SERVER_DIR" >&2
    exit 1
fi

if [[ ! -d "$SERVER_DIR/plugins" ]]; then
    echo "No plugins/ directory in: $SERVER_DIR" >&2
    exit 1
fi

STAMP="$(date +%Y%m%d-%H%M%S)"

# Back up and replace the server binary.
if [[ -f "$SERVER_DIR/pumpkin" ]]; then
    cp -f "$SERVER_DIR/pumpkin" "$SERVER_DIR/pumpkin.bak.$STAMP"
    echo "Backed up server binary to pumpkin.bak.$STAMP"
fi

if [[ "$(uname -s)" == "Linux" && "$(uname -m)" == "x86_64" ]]; then
    cp -f "$BUNDLE_DIR/pumpkin-server-patched-linux-x86_64" "$SERVER_DIR/pumpkin"
    chmod +x "$SERVER_DIR/pumpkin"
    echo "Installed patched Pumpkin binary."
else
    echo "WARNING: this bundle's patched binary is Linux x86_64 only."
    echo "Apply $BUNDLE_DIR/host-patches/pumpkin-playerinfo-host-v0_1.patch"
    echo "to your Pumpkin source and rebuild for this platform."
fi

# Install the plugin.
cp -f "$BUNDLE_DIR/better-stressbots-pumpkin.wasm" "$SERVER_DIR/plugins/better-stressbots-pumpkin.wasm"
echo "Installed better-stressbots-pumpkin.wasm"

# Remove stale compiled plugin cache entries.
find "$SERVER_DIR/plugins" -maxdepth 3 -type f \
    \( -name '*better-stressbots*.cwasm' -o -name '*better_stressbots*.cwasm' \) \
    -delete 2>/dev/null || true

echo
echo "Done. Restart the Pumpkin server, then reconnect with a Java 1.26.51 client."
echo "Verify the log contains:"
echo "  better-stressbots loaded (NPC approximation mode, build tablist-hostfix-2026-10-28)"
