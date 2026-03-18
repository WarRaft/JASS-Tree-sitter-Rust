[![](https://dcbadge.limes.pink/api/server/https://discord.gg/CNeQmXAgVq)](https://discord.gg/CNeQmXAgVq)

**English** | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-cn.md) | [繁體中文](README.zh-tw.md)

# JASS Tree-sitter Rust

Yes, the name literally describes the stack — a Tree-sitter grammar written in Rust.  
We put JASS front and center to draw attention (and nostalgia), and of course, we fully support it.

## [VSCode](https://code.visualstudio.com)

The plugin collects various tools for working with classic WarCraft III content, offering syntax support, editor features,
and a few modern conveniences along the way.

👉 [VSCode Marketplace](https://marketplace.visualstudio.com/items?itemName=WarRaft.jass-tree-sitter-rust)

---

## Supported Languages

### [JASS](https://github.com/WarRaft/tree-sitter-jass) — `.j`, `.pld`, `.ai`

The primary language of WarCraft III scripting. Full support based on a dedicated
[tree-sitter-jass](https://github.com/WarRaft/tree-sitter-jass) grammar.

### [AngelScript](https://github.com/WarRaft/tree-sitter-as) — `.as`

AngelScript support for UJAPI-based WarCraft III modding. Grammar —
[tree-sitter-as](https://github.com/WarRaft/tree-sitter-as).

### [BNI](https://github.com/WarRaft/tree-sitter-bni) — `.bni`

**BNI** (Blizzard Notation Ini) — a structured configuration format used in Warcraft III modding.  
Grammar — [tree-sitter-bni](https://github.com/WarRaft/tree-sitter-bni).

### BLP — `.blp`

Built-in image viewer for the **BLP** texture format used by WarCraft III.

### DOO — `.doo`

Built-in viewer for **DOO** placement files (`war3map.doo`, `war3mapUnits.doo`).
Displays unit/doodad placements, positions, rawcodes, and cliff decorations in a structured table.

### W3I — `.w3i`

Built-in viewer for **W3I** map information files (`war3map.w3i`).
Displays map metadata: name, author, players, forces, camera bounds, fog/weather settings, random groups, and more.

---

## LSP Features

The extension ships a standalone Rust-based LSP server (Linux, macOS, Windows) that provides:

| Feature | JASS | AngelScript | BNI |
|---------|:----:|:-----------:|:---:|
| **Semantic highlighting** | ✅ | ✅ | ✅ |
| **Folding ranges** | ✅ | ✅ | ✅ |
| **Document symbols** | ✅ | ✅ | ✅ |
| **Diagnostics** | ✅ | ✅ | — |
| **Go to definition** | ✅ | ✅ | — |
| **Find all references** | ✅ | ✅ | — |
| **Document highlight** | ✅ | ✅ | — |
| **Rename** | ✅ | ✅ | — |
| **Hover** | ✅ | ✅ | — |
| **Completion** | ✅ | ✅ | — |
| **Inlay hints** | ✅ | ✅ | — |
| **Document links** | ✅ | ✅ | — |

---

## Import System

JASS files can be linked together using special comment-based directives:

```jass
//import path/to/file.j
//import! blizzard/common.j
```

- `//import` — links another file into a shared scope. All top-level declarations (functions, globals, types, natives) become available.
- `//import!` — **frozen** import. Same as `//import`, but the target file is treated as read-only and will not be modified by refactoring or auto-rename.

Directives must appear at the very beginning of the file, before any language statements.

### Import features

- **Path completion** — autocomplete for file paths after `//import`.
- **Ctrl+Click** — opens the imported file in the editor.
- **Invalid path diagnostics** — non-existent paths are highlighted as errors.
- **Auto-update on rename/move** — when an imported file is renamed or moved, paths in all referencing files are automatically rewritten.
- **Cross-platform paths** — `/` and `\` are interchangeable; supports relative, absolute, and Windows-style (`C://`) paths.
- **Cycle detection** — circular imports are detected and reported.

---

## `//set` — Per-File Configuration

```jass
//set ref-tip 1
//set build-jass ./out/war3map.j
//set build-as ./out/war3map.as
//set unused 0
```

| Key | Values | Description |
|-----|--------|-------------|
| `ref-tip` | `1` / `0` | Show / hide reference-ID inlay hints next to each identifier — useful for debugging symbol resolution. |
| `unused` | `1` / `0` | Enable / disable unused-function diagnostics for the entire file. Default `1` (enabled). |
| `build-jass` | `<path>` | Output path for the JASS build. Merges the entire import tree into a single `.j` file. |
| `build-as` | `<path>` | Output path for the AngelScript build. Same merge logic, but emits `.as` syntax. |

---

## `//*` — Doc Comments

Lines starting with `//*` directly above a declaration are treated as **doc comments** (Markdown). They appear in hover tooltips and completion details.

```jass
//* Spawns a unit at the given position.
//* Returns the created unit handle.
function SpawnUnit takes integer id, real x, real y returns unit
    // ...
endfunction
```

Multiple consecutive `//*` lines are joined. The prefix `//* ` (with a trailing space) is stripped; `//*text` is also accepted.

---

## `//@ignore` — Per-Declaration Diagnostic Suppression

A `//@ignore` comment placed directly above a function, variable, type, or native declaration suppresses the listed diagnostic tags for that specific declaration.

```jass
//@ignore unused
function HelperFunc takes nothing returns nothing
    // No "Unused function" diagnostic will be reported for HelperFunc.
endfunction
```

### Syntax

```jass
//@ignore tag1 tag2 ...
```

Tags are space-separated. Currently supported tags:

| Tag | Suppresses |
|-----|-----------|
| `unused` | "Unused function" hint |

`//@ignore` can be combined with `//*` doc comments in any order:

```jass
//* Internal helper — not called directly.
//@ignore unused
function InternalHelper takes nothing returns nothing
endfunction
```

---

## Cross-File Intelligence

All files linked by `//import` form a **connected component** — a shared global scope:

- **Scope resolver** — persistent O(1) name lookup across all imported files, preserved between server restarts.
- **Two-phase resolution** — Phase 1 resolves symbols locally; Phase 2 links unresolved references against imported symbols.
- **Export diffing** — only re-parses dependent files when the set of exported declarations actually changes.
- **Push diagnostics** — errors are reported for affected files immediately, even if they are not open in the editor.

---

## Call Graph

The server builds a function call graph across the connected component:

- **Unused function detection** — functions not reachable from `main` / `config` entry points are flagged.
- **Cycle detection** — cyclic call chains are reported via diagnostics.
- **Topological sort** — used by the build system to ensure callees appear before callers (required by JASS).

A D3.js-powered **Call Graph** panel is available via the editor title bar button.

---

## Import Graph Visualization

A D3.js-powered **Import Graph** panel shows the dependency tree of the current file. Available via the editor title bar button. All visualization assets are bundled — no internet connection required.

---

## Build System

The `//set build-jass <path>` and `//set build-as <path>` directives trigger a build that:

1. Collects all files in the import tree.
2. Performs topological sort on functions.
3. Merges everything into a single output file: **types → globals → functions → `main`**.
4. Skips `native` declarations and type definitions (they are engine-provided).
5. Bare top-level call expressions are folded into `main`.

---

## Persistent Caching

All heavy data structures are serialized to disk via **bincode** and restored on server restart:

- **Import graph** — file dependency graph (petgraph-based).
- **Scope resolver** — global symbol index.
- **Symbol cache** — per-file function/variable/type declarations.
- **Reference cache** — per-file reference maps.

This means near-instant startup even for large projects.

---

## Architecture

- **Tree-sitter** — incremental parsing for all supported grammars.
- **`ParseSnapshot`** — atomic immutable snapshot of all LSP data per file, stored in `Arc<ParseSnapshot>` for lock-free concurrent reads.
- **`CancellationToken`** — per-file cancellation: new edits abort stale parse tasks immediately.
- **DashMap** — concurrent file store for all snapshots.
- **petgraph** — import graph and call graph analysis.

---

## Keyboard Shortcuts

All commands are available via the editor title bar buttons, but you can also assign custom keyboard shortcuts.

Open **Keyboard Shortcuts** (`Ctrl+K Ctrl+S` / `⌘K ⌘S`), search for the command name, and bind any key combination.

| Command ID | Description |
|------------|-------------|
| `importGraph.show` | Show Import Graph |
| `callGraph.show` | Show Call Graph |
| `typeGraph.show` | Show Type Graph |
| `rescan.execute` | Rescan All Files |
| `build.execute` | Build (JASS / AngelScript) |

Alternatively, add bindings directly to `keybindings.json` (`Ctrl+Shift+P` → *Preferences: Open Keyboard Shortcuts (JSON)*):

```json
[
  { "key": "ctrl+shift+i", "command": "importGraph.show",  "when": "resourceLangId == jass || resourceLangId == angelscript" },
  { "key": "ctrl+shift+g", "command": "callGraph.show",    "when": "resourceLangId == jass || resourceLangId == angelscript" },
  { "key": "ctrl+shift+t", "command": "typeGraph.show",    "when": "resourceLangId == jass || resourceLangId == angelscript" },
  { "key": "ctrl+shift+r", "command": "rescan.execute",    "when": "resourceLangId == jass || resourceLangId == angelscript" },
  { "key": "ctrl+shift+b", "command": "build.execute",     "when": "resourceLangId == jass || resourceLangId == angelscript" }
]
```

---

## License

[MIT](LICENSE)
