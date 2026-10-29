#!/usr/bin/env bash
# Build a patched Pumpkin server binary on the machine where it will run.
#
# Usage:
#   ./build-patched-server.sh /path/to/pumpkin-source
#
# This avoids glibc mismatches because the binary is compiled against the
# libraries present on the target machine.

set -euo pipefail

SRC_DIR="${1:-}"
if [[ -z "$SRC_DIR" || ! -d "$SRC_DIR" ]]; then
    echo "Usage: $0 /path/to/pumpkin-source" >&2
    exit 1
fi

cd "$SRC_DIR"

if [[ ! -f crates/pumpkin-wasm-host-v0_1/src/generated_packets.rs ]]; then
    echo "Pumpkin source not found in: $SRC_DIR" >&2
    exit 1
fi

# Apply the patch for the v0.1 host (this plugin's API version).
if ! grep -q "ClientboundPacket::CPlayerInfoUpdate" crates/pumpkin-wasm-host-v0_1/src/generated_packets.rs; then
    patch -p1 < "$(dirname "$0")/host-patches/pumpkin-playerinfo-host-v0_1.patch"
fi

# Build.
cargo build --release -p pumpkin

echo
echo "Built: $SRC_DIR/target/release/pumpkin"
echo "Copy it over your server binary, then restart."
