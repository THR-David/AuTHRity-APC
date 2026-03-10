#!/bin/bash
set -euo pipefail

# Cross-compile AuTHRity binaries for Windows from Linux.
# Output binaries end up under each service target/x86_64-pc-windows-gnu/release/.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET="x86_64-pc-windows-gnu"

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "ERROR: Required command '$1' not found."
        exit 1
    fi
}

need_cmd rustup
need_cmd cargo
need_cmd npm
need_cmd x86_64-w64-mingw32-gcc

cd "$REPO_ROOT"

echo "==> Ensuring Rust Windows target is installed"
rustup target add "$TARGET"

echo "==> Building Rust services for $TARGET"
(
    cd opcua_server
    cargo build --release --target "$TARGET"
)
(
    cd apc_engine
    cargo build --release --target "$TARGET"
)
(
    cd hmi
    cargo build --release --target "$TARGET"
)
(
    cd virtual_plant
    cargo build --release --target "$TARGET"
)
(
    cd controller_host
    cargo build --release --target "$TARGET"
)

echo "==> Building HMI frontend assets"
(
    cd hmi/frontend
    if [ ! -d node_modules ]; then
        npm install
    fi
    npm run build
)

echo "==> Cross-compile build complete"
