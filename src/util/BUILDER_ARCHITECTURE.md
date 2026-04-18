# Builder Process Architecture

## Overview

The builder **augments** diagnostics from the parse phase by adding multi-file analysis.
It preserves all syntax and local diagnostics while adding cross-file insights.

## Two-Phase Diagnostic Pipeline

### Phase 1: Parse
- **Location**: `src/lng/jass/parse.rs`
- **Output**: Diagnostics stored in `PARSE_CACHE` with source="jass"
- **Triggers**: Every time a file is edited
- **Diagnostics generated** (single-file scope):
  - Syntax errors (from tree-sitter)
  - Undeclared references
  - Type mismatches (local)
  - Unused function hints (per-file)
  - Cyclic call warnings (per-file)
  - Inline hints
  - Import errors

### Phase 2: Builder (Separate Task)
- **Location**: `src/util/builder_process.rs`
- **Trigger**: Automatically spawned after each parse
- **Key property**: Runs in a separate tokio task (non-blocking)
- **Cancellation**: If a new parse starts, the old builder is cancelled
- **Diagnostics added** (multi-file scope):
  - Unused variables across the project
  - Unused functions across the project
  - Cross-file type mismatches
  - Resource leaks (when applicable)

## How It Works

1. **Parse finishes** → Stores diagnostics with source="jass"
2. **Find entry point** → Locate the entry file in the import tree
3. **Cancel old builder** → If builder running for same entry, cancel it
4. **Spawn new builder** → Async task starts running
5. **Builder collects** → Reads all files in the import tree
6. **Builder processes** →
   - Takes parse diagnostics from PARSE_CACHE
   - Keeps them (source="jass")
   - Removes old "build" diagnostics (from previous run)
   - Generates new multi-file diagnostics
7. **Update PARSE_CACHE** →
   - Merges parse + build diagnostics
   - Both kept in same list

## Key Design Decisions

### Why preserve parse diagnostics?
- **Accuracy**: Syntax errors are immediate and precise
- **Consistency**: Don't lose local analysis
- **Speed**: Parse runs immediately, builder is background work

### Why separate task?
- **Non-blocking**: Parse loop stays responsive
- **Cancellable**: Can be killed if another parse starts
- **Scalable**: Multiple builders can run for different import trees

### Per-entry keying
- **Not per-file**: All files in an import tree share one builder
- **Entry point**: Builder is keyed by the entry file URI
- **Efficiency**: Avoids redundant work for related files

## Diagnostic Source Headers

All diagnostics have source fields:

```json
{
  "source": "jass",      // From parse phase
  "code": "undeclared",
  ...
}
```

```json
{
  "source": "build",     // From builder phase
  "code": "unused-var",
  ...
}
```

This allows the client to:
- Filter by source
- Display different UI for parse vs build
- Disable specific sources

## Merging Strategy

When builder finishes:
1. Copy all parse diagnostics (source="jass")
2. Remove any old "build" diagnostics
3. Add new "build" diagnostics
4. Replace snapshot in PARSE_CACHE

Result: Single diagnostic list with mixed sources.

## Future Extensions

The builder can eventually detect:
- Unused variables across the project
- Dead code paths
- Optimization opportunities
- Cross-file consistency issues

## Diagnostic Flow Diagram

```
┌─────────────────────────────────────────────────────────┐
│ User edits JASS file                                    │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ Parse Phase (src/lng/jass/parse.rs)                    │
├─────────────────────────────────────────────────────────┤
│ • Build AST                                             │
│ • Resolve imports                                       │
│ • Cursor walk (single-file analysis)                   │
│ • Generate diagnostics (source="jass")                 │
│ • Store in PARSE_CACHE                                 │
└────────────────────┬────────────────────────────────────┘
                     │
         ┌───────────┴───────────┐
         │                       │
         ▼                       ▼
   Find entry point       Create diag
   in component          with source="jass"
         │
         ├─→ Entry found
         │   │
         │   ▼
         │ ┌────────────────────────────────────────────┐
         │ │ Builder Process (async task)              │
         │ ├────────────────────────────────────────────┤
         │ │ 1. Cancel old builder (if running)        │
         │ │ 2. Collect file order                     │
         │ │ 3. Read all files                         │
         │ │ 4. Get parse diags from PARSE_CACHE       │
         │ │ 5. Remove old "build" diags               │
         │ │ 6. Generate new "build" diags             │
         │ │ 7. Merge both into snapshot               │
         │ │ 8. Update PARSE_CACHE                     │
         │ └─────────────┬──────────────────────────────┘
         │               │
         │               ▼
         │       ┌───────────────────┐
         │       │ PARSE_CACHE       │
         │       │ Snapshot with:    │
         │       │ • parse diags     │
         │       │ • build diags     │
         │       └───────────────────┘
         │
         └─→ No entry
             (library file)
             → Only parse diags
```

## Timing

```
t=0ms   File edited
t=1ms   Parse starts (blocking thread)
t=10ms  Parse completes
t=11ms  Builder spawn (async task)
t=15ms  Client requests diagnostics -> parse diags returned immediately
t=50ms  Builder collects files + runs analysis
t=100ms Builder updates PARSE_CACHE
t=105ms Client requests diagnostics again -> merged diags returned
```

## State Transitions

```
                    ┌──────────────────┐
                    │ Entry not found  │
                    └──────────────────┘
                            ▲
                            │
        ┌───────────────────┴──────────────────┐
        │                                      │
        ▼                                      ▼
┌──────────────────┐              ┌──────────────────┐
│ Parser only      │              │ Builder created  │
│ (no async work)  │              │ (async task)     │
└──────────────────┘              └────────┬─────────┘
                                           │
                                   ┌───────┴──────┐
                                   │              │
                        ┌──────────▼────┐  ┌──────▼────────────┐
                        │ Parse rerun   │  │ Builder running   │
                        │ (cancel old)  │  │ (work in progress)│
                        └────────┬──────┘  └───────────────────┘
                                 │
                         ┌───────▼────────┐
                         │ New builder    │
                         │ spawned        │
                         └────────────────┘
```
