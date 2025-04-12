#!/bin/bash
set -e

# Проверка на наличие jq
if ! command -v jq &> /dev/null; then
    echo "❌ Требуется 'jq'. Установи: brew install jq"
    exit 1
fi

# Get name from Cargo.toml
PROJECT_NAME=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].name')
DIST_DIR="target/dist"
mkdir -p "$DIST_DIR"

echo "📦 Building universal macOS binary..."
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

lipo -create \
  -output "$DIST_DIR/$PROJECT_NAME-macos" \
  "target/aarch64-apple-darwin/release/$PROJECT_NAME" \
  "target/x86_64-apple-darwin/release/$PROJECT_NAME"

file "$DIST_DIR/$PROJECT_NAME-macos"

echo "🐧 Building for Linux (x86_64-unknown-linux-musl)..."
rustup target add x86_64-unknown-linux-musl &>/dev/null || true
cargo build --release --target x86_64-unknown-linux-musl
cp "target/x86_64-unknown-linux-musl/release/$PROJECT_NAME" "$DIST_DIR/$PROJECT_NAME-linux"

echo "🪟 Building for Windows (x86_64-pc-windows-gnu)..."
rustup target add x86_64-pc-windows-gnu &>/dev/null || true
cargo build --release --target x86_64-pc-windows-gnu
cp "target/x86_64-pc-windows-gnu/release/$PROJECT_NAME.exe" "$DIST_DIR/$PROJECT_NAME-windows.exe"

echo ""
echo "✅ Все сборки завершены. Бинарники:"
ls -lh "$DIST_DIR"
