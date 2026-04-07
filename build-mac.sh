#!/bin/bash
set -e

if ! command -v jq &> /dev/null; then
    echo "❌ Требуется 'jq'. Установи: brew install jq"
    exit 1
fi

PROJECT_NAME=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].name')
DIST_DIR="bin"
mkdir -p "$DIST_DIR"

export AR="/usr/local/opt/llvm/bin/llvm-ar"

echo "📦 Building macOS Intel (x86_64-apple-darwin)..."
cargo build --release --target x86_64-apple-darwin
cp "target/x86_64-apple-darwin/release/$PROJECT_NAME" "$DIST_DIR/$PROJECT_NAME-macos"

file "$DIST_DIR/$PROJECT_NAME-macos"

echo ""
echo "✅ Build complete:"
ls -lh "$DIST_DIR"
