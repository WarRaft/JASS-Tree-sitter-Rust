#!/usr/bin/env bash
# Test builder process integration
#
# This script verifies that the builder process is spawned correctly
# and processes diagnostics when a JASS file is edited.

# For now, this is a placeholder showing the expected flow.
# Later, we can add actual integration tests.

echo "Builder Integration Test Plan"
echo "=============================="
echo ""
echo "1. Create test entry file with //entry directive"
echo "2. Parse it -> triggers builder spawn"
echo "3. Builder should:"
echo "   - Find entry point"
echo "   - Collect file order"
echo "   - Read all files"
echo "   - Merge diagnostics"
echo "   - Replace PARSE_CACHE snapshot"
echo ""
echo "4. Verify diagnostics have correct sources:"
echo "   - source='jass' for parse diagnostics"
echo "   - source='build' for builder diagnostics"
echo ""
echo "5. Cancel old builder when new parse starts"

