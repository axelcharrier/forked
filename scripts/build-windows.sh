#!/bin/bash
# Cross-compiles Forked for Windows from macOS and packs an archive to copy to
# a PC: unzip it there and run install.ps1 (no Rust needed on the PC).
# Requires: brew install mingw-w64 && rustup target add x86_64-pc-windows-gnu
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET=x86_64-pc-windows-gnu
OUT="$ROOT/target/dist/Forked-windows"

if ! command -v cargo >/dev/null; then
    export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
fi
if ! command -v x86_64-w64-mingw32-gcc >/dev/null; then
    echo "MinGW-w64 manque : brew install mingw-w64" >&2
    exit 1
fi
rustup target add "$TARGET" >/dev/null

echo "==> Compilation pour Windows ($TARGET)"
cargo build --release --target "$TARGET" --manifest-path "$ROOT/Cargo.toml"

echo "==> Archive"
rm -rf "$OUT" "$OUT.zip"
mkdir -p "$OUT"
cp "$ROOT/target/$TARGET/release/forked.exe" "$OUT/Forked.exe"
cp "$ROOT/assets/icon/forked.ico" "$OUT/forked.ico"
cp "$ROOT/scripts/install.ps1" "$OUT/install.ps1"
(cd "$(dirname "$OUT")" && zip -qr "Forked-windows.zip" "Forked-windows")

echo "✔ $OUT.zip"
echo "  Sur le PC : décompressez, puis lancez"
echo "  powershell -ExecutionPolicy Bypass -File install.ps1"
