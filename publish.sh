#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "🔨 Running build.sh..."
"$SCRIPT_DIR/build.sh"

echo "📦 Publishing patch..."
vsce publish patch

