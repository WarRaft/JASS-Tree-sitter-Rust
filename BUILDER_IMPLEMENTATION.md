# Builder Process Implementation Summary

## What Was Implemented

### 1. New Module: `src/util/builder_process.rs`
- **BuilderProcessState**: Manages per-entry builder lifecycle
  - Cancellation token for killing old tasks
  - Running flag to track state
- **BUILDER_PROCESSES**: Global registry keyed by entry URI
- **BuilderConfig**: Configuration struct (mode, options)
- **BuilderDiagnostics**: Results from builder (URI + diagnostics list)

### 2. Integration in Parse Pipeline
**File**: `src/lng/jass/parse.rs`

After parse completes and snapshot is stored:
1. Find entry point in the connected component
2. Cancel any old builder for that entry
3. Spawn new builder task with configuration
4. Builder runs asynchronously (non-blocking)

### 3. Builder Startup Sequence
```rust
if let Some(entry_uri) = crate::lng::jass::builder::collect::find_entry_point(uri) {
    builder_process::cancel_builder_for_entry(&entry_uri);
    
    let config = builder_process::BuilderConfig {
        mode: crate::lng::jass::builder::PipelineMode::Diagnostics,
        opts: crate::lng::jass::builder::collect::build_opt_tags(
            &new_snapshot.file_symbols.file_settings,
        ),
    };
    builder_process::spawn_builder_task(&entry_uri, config);
}
```

### 4. New Function in collect.rs
- **find_entry_point(uri)**: Finds entry point in connected component
  - Returns first `//entry` file found
  - Returns None for library files

### 5. Diagnostic Merging Strategy
When builder finishes:
1. **Preserve parse diagnostics**: Keep source="jass" diagnostics
2. **Remove stale build diags**: Clear old source="build" diagnostics
3. **Add new build diags**: Append new multi-file diagnostics
4. **Replace snapshot**: Update PARSE_CACHE with merged result

### 6. Documentation
- **BUILDER_ARCHITECTURE.md**: Comprehensive design document
  - Two-phase pipeline explanation
  - Timing diagrams
  - State transitions
  - Merging strategy
- **test_builder_integration.sh**: Test plan placeholder

## Key Architecture Decisions

### Per-Entry, Not Per-File
- All files in an import tree share one builder
- Keyed by entry URI, not individual file URIs
- Prevents redundant work

### Async Task with Cancellation
- Spawned with `tokio::spawn()`
- Has own CancellationToken
- Doesn't block parse loop
- Can be killed if new parse starts

### Diagnostic Sources
```
source="jass"   <- From parse phase (syntax, local analysis)
source="build"  <- From builder phase (multi-file analysis)
```

### Non-Blocking Design
- Parse phase completes immediately
- Diagnostics available to client right away
- Builder augments them in background
- Client sees updates when builder finishes

## What Happens on File Edit

```
User types → parse() spawned
    ↓
parse finishes (10ms) → diagnostics in PARSE_CACHE
    ↓
builder_task spawned (async)
    ↓
client can fetch diagnostics immediately (parse only)
    ↓
builder running in background (50ms)
    ↓
builder updates PARSE_CACHE (merge diags)
    ↓
client fetches again (parse + build diags)
```

## If New Parse Starts While Builder Running

```
builder1 running
    ↓
new parse starts
    ↓
cancel_builder_for_entry() called
    ↓
builder1 cancelled token fires
    ↓
builder1 exits
    ↓
builder2 spawned (for new parse)
```

## No Business Logic in mod.rs
The principle is maintained:
- All implementation in `builder_process.rs`
- `mod.rs` is just public API
- Sub-modules have specific responsibilities

## Future Work

1. **Actual multi-file analysis**: Currently builder just copies diags
2. **Cross-file unused detection**: Requires AST analysis
3. **Type resolution**: Build type info across files
4. **Resource leak detection**: Whole-project analysis
5. **Performance optimization**: Incremental builds

## Testing Guidance

To test the implementation:
1. Edit a JASS file with `//entry` directive
2. Check that builder spawns (look for log messages)
3. Verify PARSE_CACHE gets updated
4. Edit again while builder running → old one should cancel
5. Verify diagnostics have correct sources ("jass" vs "build")

