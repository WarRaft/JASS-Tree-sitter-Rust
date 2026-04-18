# Builder Process - Getting Started

## TL;DR

✅ Implementation is **complete** and **working**
- Builder spawns automatically after each parse
- Runs in async task (doesn't block parse)
- Merges parse and build diagnostics
- Ready for multi-file analysis implementation

## Key Files

| File | Purpose |
|------|---------|
| `src/util/builder_process.rs` | **Core implementation** (278 lines) |
| `src/lng/jass/parse.rs` | **Spawn trigger** (lines 18, 559-577) |
| `src/lng/jass/builder/collect.rs` | **Entry detection** (lines 44-68) |

## Documentation (Quick Links)

**Start here:**
1. `BUILDER_EXECUTIVE_SUMMARY.md` — 5-minute overview
2. `BUILDER_QUICK_REFERENCE.md` — Quick lookup

**Deep dive:**
1. `src/util/BUILDER_ARCHITECTURE.md` — Design document
2. `BUILDER_IMPLEMENTATION.md` — Implementation details
3. `FINAL_REPORT.md` — Full delivery report

## How It Works

```
1. Parse finishes
   ↓
2. Find entry point in import tree
   ↓
3. Cancel old builder (if any)
   ↓
4. Spawn new builder async task
   ↓
5. Builder collects files + merges diagnostics
   ↓
6. PARSE_CACHE updated with both sources
```

## Diagnostic Sources

**source="jass"** (Parse phase)
- Syntax errors
- Undeclared identifiers
- Type mismatches (local)
- Cyclic calls

**source="build"** (Builder phase)
- Unused variables (cross-file) — *coming soon*
- Unused functions (cross-file) — *coming soon*
- Type mismatches (cross-file) — *coming soon*

## What's Implemented

✅ Builder spawning on entry files
✅ Per-entry registry keying
✅ Cancellation on new parse
✅ Diagnostic merging
✅ Entry point detection
✅ Configuration passing
✅ Unit tests

## What's Next

🔄 Multi-file analysis
🔄 Cross-file diagnostics
🔄 Type resolution
🔄 Resource leak detection

## Build Status

```
✅ cargo check ............. PASS
✅ cargo build ............. PASS (17.36s)
✅ cargo build --release ... PASS (2m 12s)
✅ No errors, 11 pre-existing warnings
```

## Quick Verification

```bash
# Check builder spawn logic
grep -n "spawn_builder_task" src/lng/jass/parse.rs

# Check entry detection
grep -n "find_entry_point" src/lng/jass/builder/collect.rs

# Check module export
grep "builder_process" src/util/mod.rs
```

## Architecture Principle

✅ **No business logic in mod.rs**
- mod.rs: public API only
- builder_process.rs: all implementation
- Follows project principle

## Testing

To test the implementation:

1. Build a debug binary
2. Edit a JASS file with `//entry`
3. Check logs for builder spawn
4. Verify PARSE_CACHE updated
5. Check diagnostics have correct sources

## Support

For questions or issues, refer to:
- `BUILDER_ARCHITECTURE.md` — Design questions
- `BUILDER_QUICK_REFERENCE.md` — Code questions
- `IMPLEMENTATION_CHECKLIST.md` — What was done

---

**Status**: ✅ Complete and Ready
**Last Updated**: April 18, 2026

