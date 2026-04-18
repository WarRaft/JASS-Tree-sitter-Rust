# Quick Reference: Builder Process

## Flow Diagram

```
Parse → Find entry → Cancel old builder → Spawn new builder → Async work
  ↓                                                                 ↓
source="jass"                                                   source="build"
diags in PARSE_CACHE                                    merge & update PARSE_CACHE
(immediate)                                              (background, async)
```

## Key Files

| File | Purpose |
|------|---------|
| `src/util/builder_process.rs` | Builder lifecycle management |
| `src/lng/jass/parse.rs` | Spawn builder after parse |
| `src/lng/jass/builder/collect.rs` | Find entry point + collect files |
| `src/util/BUILDER_ARCHITECTURE.md` | Design document |

## Key Types

```rust
pub struct BuilderProcessState {
    pub cancel_token: CancellationToken,  // Kill old builder
    pub is_running: AtomicBool,           // Track state
}

pub struct BuilderConfig {
    pub mode: PipelineMode,               // Diagnostics or Build
    pub opts: HashSet<String>,            // uglify, nolocal, etc.
}

pub struct BuilderDiagnostics {
    pub uri: Url,                         // Which file
    pub diagnostics: Vec<Diagnostic>,     // Results
}
```

## Key Functions

```rust
// Get or create builder state for an entry
pub fn get_or_create_builder_state(entry_uri: &Url) -> Arc<BuilderProcessState>

// Cancel old builder when new parse starts
pub fn cancel_builder_for_entry(entry_uri: &Url)

// Spawn builder async task
pub fn spawn_builder_task(entry_uri: &Url, config: BuilderConfig)

// Find entry point in connected component
pub fn find_entry_point(uri: &Url) -> Option<Url>
```

## Diagnostic Merging Logic

```rust
// Get old diagnostics
let mut merged = old_snap.diagnostics.clone();

// Remove stale "build" diagnostics
merged.retain(|d| d.source.as_deref() != Some("build"));

// Add new "build" diagnostics
merged.extend(result.diagnostics);

// Replace snapshot
PARSE_CACHE.insert(uri, new_snapshot_with_merged_diags);
```

## What Gets Which Source

| Diagnostic | Source |
|------------|--------|
| Syntax errors | "jass" |
| Undeclared identifiers | "jass" |
| Type mismatches (local) | "jass" |
| Unused functions (per-file) | "jass" |
| Cyclic calls (per-file) | "jass" |
| Unused variables (cross-file) | "build" |
| Unused functions (cross-file) | "build" |
| Type mismatches (cross-file) | "build" |

## Per-Entry Keying

**Why per-entry, not per-file?**
- One builder per import tree
- All files share it
- Entry URI is key

```
import tree:
  common.j (entry)
  ├── utils.j
  ├── helpers.j
  └── main.j

All 4 files → 1 builder keyed by common.j URI
```

## Cancellation Behavior

```
Builder1 running for entry "X"
    ↓
New parse for any file in X's tree
    ↓
cancel_builder_for_entry("X") called
    ↓
Builder1's token fires
    ↓
Builder1 exits gracefully
    ↓
Builder2 spawned for new parse
```

## Testing Checklist

- [ ] Builder spawns on entry file parse
- [ ] Builder doesn't spawn on library file
- [ ] Old builder cancels on new parse
- [ ] Diagnostics have source="jass" from parse
- [ ] Diagnostics have source="build" from builder
- [ ] Merged list in PARSE_CACHE has both
- [ ] Builder doesn't block parse loop
- [ ] Entry point correctly found in component

