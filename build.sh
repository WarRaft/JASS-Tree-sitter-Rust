#!/usr/bin/env bash
set -euo pipefail

# ---------------- helpers ----------------
need() { command -v "$1" >/dev/null 2>&1 || { echo "❌ Missing: $1"; exit 1; }; }
msg()  { echo -e "$*"; }

need cargo
need jq
need rustup
need lipo
need file

PROJECT_NAME="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].name')"
DIST_DIR="bin"
mkdir -p "$DIST_DIR"

# -------- find llvm-strip (prefer rustup, then brew, then xcode) --------
find_llvm_strip() {
  # shellcheck disable=SC2034
  local host triple sysroot p
  host="$(rustc -vV | sed -n 's/^host: //p')"
  sysroot="$(rustc --print sysroot)"
  for p in \
    "$sysroot/lib/rustlib/$host/bin/llvm-strip" \
    "$sysroot/lib/rustlib/bin/llvm-strip" \
    "/opt/homebrew/opt/llvm/bin/llvm-strip" \
    "/usr/local/opt/llvm/bin/llvm-strip" \
    "$(xcrun --find llvm-strip 2>/dev/null || true)"; do
    if [[ -n "${p:-}" && -x "$p" ]]; then
      echo "$p"
      return 0
    fi
  done
  return 1
}

LLVM_STRIP="$(find_llvm_strip || true)"
if [[ -z "$LLVM_STRIP" ]]; then
  msg "🔧 Installing rustup component llvm-tools-preview..."
  rustup component add llvm-tools_preview || rustup component add llvm-tools-preview
  LLVM_STRIP="$(find_llvm_strip || true)"
fi

if [[ -n "$LLVM_STRIP" ]]; then
  msg "🔎 llvm-strip: $LLVM_STRIP"
else
  msg "⚠️  llvm-strip не найден. Буду использовать системный 'strip' где возможно."
fi

strip_safe() {
  # $1 = file path, $2 = kind: macos|linux|windows
  local f="$1" kind="${2:-}"
  if [[ ! -f "$f" ]]; then return 0; fi
  case "$kind" in
    macos)
      # macOS системный strip отлично чистит universal бинарь
      if command -v strip >/dev/null 2>&1; then /usr/bin/strip -x "$f" || true; fi
      ;;
    linux|windows)
      if [[ -n "$LLVM_STRIP" ]]; then
        "$LLVM_STRIP" -s "$f" || true
      else
        # как запасной вариант: системный strip может сломать чужую ABI — лучше просто предупредить
        msg "⚠️  Пропускаю strip для $f (нет llvm-strip)"
      fi
      ;;
    *)
      # неизвестно — попробуем по расширению
      if [[ -n "$LLVM_STRIP" ]]; then "$LLVM_STRIP" -s "$f" || true; fi
      ;;
  esac
}

maybe_upx() {
  # упаковываем только если UPX=1 и есть upx
  if [[ "${UPX:-0}" = "1" ]] && command -v upx >/dev/null 2>&1; then
    upx --best --lzma "$1" || true
  fi
}

# ----------- optional: size-oriented RUSTFLAGS -----------
# Можно отключить, если используешь профиль в Cargo.toml.
export RUSTFLAGS="${RUSTFLAGS:-} -C strip=symbols"

# ---------------- macOS (universal) ----------------
msg "📦 Building universal macOS binary…"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null 2>&1 || true
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

MAC_UNI="$DIST_DIR/$PROJECT_NAME-macos"
lipo -create \
  -output "$MAC_UNI" \
  "target/aarch64-apple-darwin/release/$PROJECT_NAME" \
  "target/x86_64-apple-darwin/release/$PROJECT_NAME"

strip_safe "$MAC_UNI" macos
file "$MAC_UNI"

# ---------------- Linux (x86_64-unknown-linux-musl) ----------------
msg "🐧 Building for Linux (x86_64-unknown-linux-musl)…"
rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1 || true
cargo build --release --target x86_64-unknown-linux-musl
LIN_BIN="$DIST_DIR/$PROJECT_NAME-linux"
cp "target/x86_64-unknown-linux-musl/release/$PROJECT_NAME" "$LIN_BIN"
strip_safe "$LIN_BIN" linux
file "$LIN_BIN"

# ---------------- Windows (x86_64-pc-windows-gnu) ----------------
msg "🪟 Building for Windows (x86_64-pc-windows-gnu)…"
rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1 || true
cargo build --release --target x86_64-pc-windows-gnu
WIN_EXE="$DIST_DIR/$PROJECT_NAME-windows.exe"
cp "target/x86_64-pc-windows-gnu/release/$PROJECT_NAME.exe" "$WIN_EXE"
strip_safe "$WIN_EXE" windows
maybe_upx "$WIN_EXE"
file "$WIN_EXE"

# ---------------- summary ----------------
msg "\n✅ Build complete:"
ls -lh "$DIST_DIR"
