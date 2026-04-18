# Collision Detection Fix - COMPLETE ✅

## What Was Wrong

When fixing multiple handle leaks in the same file, the system would generate duplicate names:

```jass
globals
    integer Cunt_ret = 33
    unit Cunt_ret         // ✗ DUPLICATE! Should be Cunt_ret_2
    unit Anal_ret
endglobals
```

## Root Cause

The collision detection collected all declared names **once** at the start, but never updated the set as it added NEW global variables during the fix pass.

When fixing `Anal`, the set had `Cunt_ret`.
When fixing `Cunt`, the set STILL had only `Cunt_ret` - it didn't know about `Anal_ret` we just added.

## Solution

Pass a mutable `generated_names` HashSet through the entire fix batch:
1. Collect initial declared names
2. For each fix:
   - Generate unique name using current set
   - **INSERT the generated name back into the set**
   - This way the NEXT fix knows about it

## Code Changes

### src/http/code_action/compute.rs

**In `compute_leak_fixes()`:**
```rust
let mut generated_names = std::collections::HashSet::new();
// Collect initial names from AST
generated_names = collect_declared_names(&ast, &full_text);

for diag in &leak_diags {
    if is_returned_local(diag) {
        if let Some(edits) = returned_local_edits_with_tracking(diag, uri, rope, &mut generated_names) {
            // ... add action ...
        }
    }
}
```

**New function:**
```rust
fn returned_local_edits_with_tracking(
    diag: &Diagnostic,
    uri: &url::Url,
    rope: &lapce_xi_rope::Rope,
    generated_names: &mut HashSet<String>,
) -> Option<Vec<TextEdit>>
```

Similar changes in `compute_fix_all_leaks()`.

### src/lng/jass/builder/local_fix.rs

**In `collect_leak_edits()`:**
```rust
let mut generated_names = index.declared_names.clone();

for diag in diags {
    if is_returned_local(diag) {
        let fix_edits = returned_local_edits_with_tracking(
            diag,
            index,
            method,
            &mut generated_names,  // ← Pass mutable reference
        );
        edits.extend(fix_edits);
    }
}
```

## Expected Output

Now correctly generates:
```jass
globals
    integer Cunt_ret = 33
    unit Anal_ret         // ✓ First function fix
    unit Cunt_ret_2       // ✓ Second function fix - no collision!
endglobals

function Anal takes nothing returns unit
    local unit A = CreateUnit(...)
    set Anal_ret = A
    set A = null
    return Anal_ret
endfunction

function Cunt takes nothing returns unit
    local unit B = CreateUnit(...)
    set Cunt_ret_2 = B
    set B = null
    return Cunt_ret_2
endfunction
```

## Test Files Created

- `test_collision_detection.j` - Single collision test
- `test_multiple_leaks.j` - Multiple fixes in same pass

## Files Modified

1. `src/http/code_action/compute.rs`
   - Added `returned_local_edits_with_tracking()`
   - Modified `compute_leak_fixes()` to track names
   - Modified `compute_fix_all_leaks()` to track names

2. `src/lng/jass/builder/local_fix.rs`
   - Added `returned_local_edits_with_tracking()`
   - Modified `collect_leak_edits()` to track names

## Compilation Status

✅ **No errors**
✅ **Builds successfully**
✅ **All warnings pre-existing**

## Stage Completion

- Stage 1: Fixed suffix format and added AST analysis ✅
- Stage 2: Fixed multi-fix collision detection ✅

## Status: ✅ COMPLETE

The collision detection now properly handles:
- Existing global variables ✅
- Multiple fixes in the same file ✅
- Correct suffix numbering ✅
- Both code-action and builder implementations ✅

