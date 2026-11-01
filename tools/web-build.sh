#!/usr/bin/env bash
# ブラウザ版（wasm）を作る。出力は web/pkg/。web/ を静的に配信して index.html を開く
#   例: python3 -m http.server -d web 8000 → http://localhost:8000/
#
# 事前に: rustup target add wasm32-unknown-unknown と、
# Cargo.lock の wasm-bindgen と同じ版の wasm-bindgen-cli（cargo install wasm-bindgen-cli --version <版>）
set -euo pipefail
cd "$(dirname "$0")/.."
(cd game && cargo build -p reprise --release --target wasm32-unknown-unknown "$@")
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
  game/target/wasm32-unknown-unknown/release/reprise.wasm
echo "web/pkg に出力しました"
