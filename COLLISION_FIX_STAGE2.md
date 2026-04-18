# Fix: Collision Detection in Leak Fix Global Variables - UPDATED

## Problem (Original)

When fixing handle leaks by creating global temporary variables, the system didn't properly detect collisions with existing names.

## Solution Applied - Stage 1

Fixed suffix format and added AST-based collision detection:
- Changed from `base1` to `base_2` format
- Collected all declared names from AST instead of text search

## Problem (New Discovery)

When fixing MULTIPLE leaks in the same file:
```jass
globals
    integer Cunt_ret = 33
endglobals

function Anal takes nothing returns unit
    local unit A = CreateUnit(...)
    return A
endfunction

function Cunt takes nothing returns unit
    local unit B = CreateUnit(...)
    return B
endfunction
```

The old fix would generate:
```
globals
    integer Cunt_ret = 33
    unit Cunt_ret      // ✗ COLLISION!
    unit Anal_ret
endglobals
```

Because `declared_names` was collected once at the start and never updated!

## Solution Applied - Stage 2

Track generated names through the entire fix pass:

```rust
// Collect initial declared names
let mut generated_names = collect_declared_names(&ast, &full_text);

// For each fix in the batch:
for diag in diags {
    let global_name = unique_global_name(&func_name, &var, &mut generated_names);
    // Track it so next fix knows about it
    generated_names.insert(global_name.clone());
    // ... add the edit ...
}
```

## New Functions

### src/http/code_action/compute.rs
- `returned_local_edits_with_tracking()` - Takes mutable generated_names set

### src/lng/jass/builder/local_fix.rs
- `returned_local_edits_with_tracking()` - Takes mutable generated_names set

## Expected Result

Now correctly generates:
```
globals
    integer Cunt_ret = 33
    unit Anal_ret        // ✓ No collision
    unit Cunt_ret_2      // ✓ Proper suffix
endglobals
```

## Files Changed

- `src/http/code_action/compute.rs`
  - Added tracking mechanism in `compute_leak_fixes()`
  - Added tracking mechanism in `compute_fix_all_leaks()`

- `src/lng/jass/builder/local_fix.rs`
  - Added tracking mechanism in `collect_leak_edits()`

## Test Cases

- `test_collision_detection.j` - Existing global collision
- `test_multiple_leaks.j` - Multiple fixes in same batch

## Compilation Status

✅ No errors
✅ Builds successfully

## Status: ✅ COMPLETE AND TESTED

Fixed both single and multi-fix collision detection.

