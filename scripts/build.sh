#!/usr/bin/env bash
# Build Linux + Windows binaries (debug by default).
#
# Usage:
#   ./scripts/build.sh
#   ./scripts/build.sh --release
#   RELEASE=1 ./scripts/build.sh
#
# Windows cross target (override with WIN_TARGET):
#   x86_64-pc-windows-gnu   (needs mingw-w64 linker)
#   x86_64-pc-windows-msvc  (needs Windows SDK / lld-link setup)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE=debug
CARGO_FLAGS=(--bins)
if [[ "${1:-}" == "--release" || "${1:-}" == "-r" || "${RELEASE:-0}" == "1" ]]; then
  PROFILE=release
  CARGO_FLAGS+=(--release)
fi

WIN_TARGET="${WIN_TARGET:-x86_64-pc-windows-gnu}"
LINUX_TARGET="${LINUX_TARGET:-}"

echo "==> fuckFM build (${PROFILE})"
echo "    root: $ROOT"

need_target() {
  local t="$1"
  if ! rustup target list --installed | grep -qx "$t"; then
    echo "    installing rustup target: $t"
    rustup target add "$t"
  fi
}

stage_assets() {
  local out_dir="$1"
  mkdir -p "$out_dir/assets/icons"
  if [[ -d "$ROOT/assets/icons" ]]; then
    cp -a "$ROOT/assets/icons/." "$out_dir/assets/icons/"
  fi
  if [[ -d "$ROOT/assets" ]]; then
    # fonts next to bins when present
    for f in classic.ttf ui.ttf; do
      if [[ -f "$ROOT/assets/$f" ]]; then
        cp -f "$ROOT/assets/$f" "$out_dir/assets/"
      fi
    done
  fi
}

echo "==> Linux"
if [[ -n "$LINUX_TARGET" ]]; then
  need_target "$LINUX_TARGET"
  cargo build "${CARGO_FLAGS[@]}" --target "$LINUX_TARGET"
  LINUX_OUT="target/${LINUX_TARGET}/${PROFILE}"
else
  cargo build "${CARGO_FLAGS[@]}"
  LINUX_OUT="target/${PROFILE}"
fi
stage_assets "$LINUX_OUT"
echo "    linux bins → $LINUX_OUT"
ls -la "$LINUX_OUT"/fuckfm "$LINUX_OUT"/fuckwm "$LINUX_OUT"/b00merang2fficons 2>/dev/null || true

echo "==> Windows ($WIN_TARGET)"
need_target "$WIN_TARGET"
if ! cargo build "${CARGO_FLAGS[@]}" --target "$WIN_TARGET"; then
  echo "error: Windows build failed for $WIN_TARGET" >&2
  echo "  tip: install a linker, e.g.  pacman -S mingw-w64-gcc" >&2
  echo "  or:  WIN_TARGET=x86_64-pc-windows-msvc ./scripts/build.sh" >&2
  exit 1
fi
WIN_OUT="target/${WIN_TARGET}/${PROFILE}"
stage_assets "$WIN_OUT"
echo "    windows bins → $WIN_OUT"
ls -la "$WIN_OUT"/fuckfm.exe "$WIN_OUT"/fuckwm.exe "$WIN_OUT"/b00merang2fficons.exe 2>/dev/null || true

echo "==> done (${PROFILE})"
echo "    Linux:   $LINUX_OUT"
echo "    Windows: $WIN_OUT"
