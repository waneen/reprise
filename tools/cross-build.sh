#!/usr/bin/env bash
# 他アーキテクチャ向けのクロスビルドを手元で行う（CI では行わない）。
#
# 使い方: bash tools/cross-build.sh <target>...
#   例: bash tools/cross-build.sh aarch64-unknown-linux-gnu
#
# 事前に `rustup target add <target>` が必要。ターゲットごとの追加の準備:
# - aarch64-unknown-linux-gnu（Linux x86_64 上）: gcc-aarch64-linux-gnu と、Bevy が使う
#   ALSA・udev・Wayland・xkbcommon・fontconfig の arm64 版（Debian/Ubuntu なら dpkg --add-architecture arm64 のうえで
#   libasound2-dev:arm64 libudev-dev:arm64 libwayland-dev:arm64 libxkbcommon-dev:arm64 libfontconfig-dev:arm64）
# - aarch64-pc-windows-msvc（Windows 上）: Visual Studio の ARM64 ビルドツール
# - x86_64-apple-darwin / aarch64-apple-darwin（macOS 上）: 追加の準備なし
set -euo pipefail

if [ $# -eq 0 ]; then
  echo "使い方: $0 <target>..." >&2
  exit 2
fi

cd "$(dirname "$0")/.."
for target in "$@"; do
  if [ "$target" = "aarch64-unknown-linux-gnu" ] && [ "$(uname -m)" != "aarch64" ]; then
    export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
    export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
    export PKG_CONFIG_ALLOW_CROSS=1
    export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
  fi
  for ws in chart-formats skin-formats game; do
    echo "== $ws ($target)"
    (cd "$ws" && cargo build --workspace --release --target "$target")
  done
done
