# `//import` — File Import Directive

The `//import` directive allows you to include another JASS file into the
current compilation unit.  It must appear **at the very beginning** of the file,
before any language statements (`globals`, `function`, `type`, etc.).

## Syntax

```jass
//import path/to/file.j
```

* The `//import` token must start at **column 0** (no leading spaces).
* Everything after `//import ` until end-of-line is the **file path**.
* Both `/` and `\` are accepted as path separators.
* Paths can be **relative** (resolved from the current file's directory) or
  **absolute** (`/usr/share/jass/common.j` or `C:/maps/lib.j`).

## Example

```jass
//import common/natives.j
//import utils/math.j

globals
    integer count = 0
endglobals
```

## Behaviour

| Feature | Description |
|---------|-------------|
| **Order** | Imports are processed top-to-bottom; declarations from imported files are available below the directive. |
| **Cycles** | Circular imports are detected and reported as errors. |
| **Ctrl+Click** | Clicking the path opens the imported file in the editor. |
| **Auto-update** | When an imported file is renamed or moved, the path is automatically rewritten. |

---

# `//import!` — Frozen Import

`//import!` works exactly like `//import`, but marks the target file as
**frozen** (read-only).

```jass
//import! blizzard/common.j
```

## Differences from `//import`

| | `//import` | `//import!` |
|-|------------|-------------|
| Declarations pulled | ✅ Yes | ✅ Yes |
| File editable | ✅ Yes | ❌ No — treated as read-only |
| Use case | Your own project files | SDK / engine files you should not modify |

A frozen file is never modified by auto-rename, refactoring, or other
write operations performed by the language server.

