# Fix: Collision Detection in Leak Fix Global Variables

## Problem

When fixing handle leaks by creating global temporary variables, the system didn't properly detect collisions with existing declarations. 

**Example:**
```jass
function Anal takes nothing returns unit
    local unit A = CreateUnit('null', 0, 0., 0., 0.)
    return A
endfunction

integer Anal_ret = 33  // This global already exists!
```

The old code would try to generate `Anal_ret` but fail to detect the collision because:

1. **Text-based search** in `src/http/code_action/compute.rs` used `rope.contains()` which only does substring matching
2. **Wrong suffix format** was used: `base1`, `base2` instead of `base_2`, `base_3`
3. **No AST analysis** - the code didn't actually parse all declarations

## Solution

### 1. Fixed suffix format (both files)

**Before:**
```rust
let candidate = format!("{}{}", base, suffix);  // "Anal_ret1"
```

**After:**
```rust
let candidate = format!("{}_{}", base, suffix);  // "Anal_ret_2"
```

### 2. Added AST-based collision detection

**New functions in `src/http/code_action/compute.rs`:**
- `collect_declared_names()` - Traverses AST to find all variable declarations
- `collect_local_names()` - Recursively collects local variables in function bodies

**Modified function signature:**
```rust
fn returned_local_edits(
    diag: &Diagnostic,
    uri: &url::Url,
    rope: &lapce_xi_rope::Rope,
) -> Option<Vec<TextEdit>>
```

Now it has access to AST via TREE_MAP to properly detect collisions.

### 3. Updated local_fix.rs

Fixed the same suffix format issue in `src/lng/jass/builder/local_fix.rs`:

**Before:**
```rust
let candidate = format!("{}{}", base, suffix);  // "Anal_ret1"
```

**After:**
```rust
let candidate = format!("{}_{}", base, suffix);  // "Anal_ret_2"
```

## Files Changed

1. **src/http/code_action/compute.rs**
   - Added `collect_declared_names()` function
   - Added `collect_local_names()` helper
   - Modified `unique_global_name()` to use HashSet instead of text search
   - Modified `returned_local_edits()` to collect declarations and pass uri

2. **src/lng/jass/builder/local_fix.rs**
   - Fixed suffix format in `unique_global_name()`
   - Fixed suffix format in `unique_local_name()`

## Example Output

**Input:**
```jass
function Anal takes nothing returns unit
    local unit A = CreateUnit('null', 0, 0., 0., 0.)
    return A
endfunction

integer Anal_ret = 33
```

**After fix (before):**
- Would incorrectly use `Anal_ret` (collision!)

**After fix (now):**
- Correctly generates `Anal_ret_2`:
```jass
globals
    unit Anal_ret_2
endglobals

function Anal takes nothing returns unit
    local unit A = CreateUnit('null', 0, 0., 0., 0.)
    set Anal_ret_2 = A
    set A = null
    return Anal_ret_2
endfunction

integer Anal_ret = 33
```

## Testing

See `test_collision_detection.j` for test case.

To verify:
1. Open the test file in the editor
2. Check for "Handle leak" diagnostics
3. Apply the leak fix
4. Verify it creates `Anal_ret_2` not `Anal_ret`

## Commits

- Fixed collision detection in leak fix global variables
- Improved from text-based to AST-based name resolution
- Consistent suffix formatting across both implementations

