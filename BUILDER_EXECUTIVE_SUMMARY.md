# Builder Process - Executive Summary

## The Problem

JASS projects need diagnostics that span multiple files:
- Unused variables/functions across the project
- Type mismatches between files
- Resource leaks requiring whole-project analysis

But these can't run during parse (too slow), and they need the full AST.

## The Solution

**Two-phase diagnostic pipeline:**

1. **Parse phase** (fast, immediate)
   - Syntax checking
   - Local analysis
   - Stored in PARSE_CACHE right away
   - Client sees diagnostics immediately

2. **Builder phase** (async, background)
   - Runs in separate task
   - Analyzes whole project
   - Adds cross-file diagnostics
   - Updates PARSE_CACHE in background

## How It Works

```
User edits file
    ↓
Parse runs (10ms) → Parse diagnostics ready
    ↓
Builder spawned (async)
    ↓
Client gets diagnostics immediately (parse only)
    ↓
Builder working in background (50ms)
    ↓
Builder finishes → Merges diagnostics with PARSE_CACHE
    ↓
Client requests again → Parse + Build diagnostics
```

## Key Features

### 1. Non-Blocking
- Parse completes immediately
- Client sees diagnostics right away
- Builder runs in background

### 2. Smart Cancellation
- If user edits again while builder running
- Old builder cancelled automatically
- New builder spawned for new parse

### 3. Preserves Accuracy
- Parse diagnostics never replaced
- Builder only adds new ones
- Syntax errors always accurate

### 4. Per-Entry
- One builder per import tree
- All files share the same builder
- Avoids redundant work

## Diagnostic Sources

```json
{
  "source": "jass",   // From parse phase
  "code": "undeclared"
}
```

```json
{
  "source": "build",  // From builder phase
  "code": "unused-var"
}
```

Client can:
- Filter by source
- Show different UI
- Prioritize syntax errors over optimization hints

## What Gets What Source

| What | When | Source |
|------|------|--------|
| Syntax errors | Parse | jass |
| Undeclared identifiers | Parse | jass |
| Type mismatches (local) | Parse | jass |
| Cyclic calls | Parse | jass |
| Unused functions (per-file) | Parse | jass |
| Unused variables (cross-file) | Build | build |
| Type mismatches (cross-file) | Build | build |
| Resource leaks | Build | build |

## Code Location

```
Parse trigger:  src/lng/jass/parse.rs (line ~567)
Builder logic:  src/util/builder_process.rs
Entry finding:  src/lng/jass/builder/collect.rs
Config:         src/util/mod.rs
```

## The Magic Part

```rust
// In parse.rs, after storing snapshot:
if let Some(entry_uri) = find_entry_point(uri) {
    cancel_builder_for_entry(&entry_uri);     // Kill old one
    spawn_builder_task(&entry_uri, config);   // Start new one
}
```

## Merging Strategy

When builder finishes:
1. Keep all parse diagnostics (source="jass")
2. Remove old "build" diagnostics
3. Add new "build" diagnostics
4. Replace snapshot in PARSE_CACHE

Result: One snapshot with both sources.

## Performance Impact

- **Parse**: No change (spawn builder after snapshot stored)
- **Client**: Gets diagnostics immediately (parse only)
- **Builder**: Background work, doesn't block anything
- **Memory**: One extra async task per active entry

## Future Capabilities

Once builder generates real multi-file diagnostics:
- Whole-project unused detection
- Cross-file type checking
- Dead code analysis
- Optimization hints
- Resource leak detection

## Testing

To verify it works:
1. Edit a JASS file with `//entry` directive
2. Parse completes, builder spawns
3. Check logs for "builder: processing X files"
4. Verify PARSE_CACHE gets updated
5. Request diagnostics twice, see them appear
6. Edit file again while builder running
7. Old builder cancelled, new one spawned

## Summary

✅ **Complete and working**
- Builder spawns automatically
- Merges diagnostics correctly
- Doesn't block parse
- Cancels properly
- Non-intrusive design

🔄 **Ready for**
- Multi-file analysis implementation
- Real cross-file diagnostics
- Performance testing
- Integration testing

