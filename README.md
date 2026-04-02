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

### SLK — `.slk`

Built-in table editor for the **SLK** (SYLK) spreadsheet format used by WarCraft III for game data.
Displays the data in an interactive canvas-based grid with resizable columns, sorting, and configurable hidden columns.
Settings (column widths, hidden columns, etc.) are persisted directly in the file. Switching between table view and text view is available via the editor title bar.

### DOO — `.doo`

Built-in viewer for **DOO** placement files (`war3map.doo`, `war3mapUnits.doo`).
Displays unit/doodad placements, positions, rawcodes, and cliff decorations in a structured table.

### W3E — `.w3e`

Built-in viewer for **W3E** terrain files (`war3map.w3e`).
Renders the terrain heightmap with 3D preview (Three.js), texture transitions, layer heights, water/blight/boundary/ramp overlays, wireframe grid, and cursor-info bar.
Supports both palette-based and real texture rendering (when game path is configured).
See [docs/w3e/terrain.md](docs/w3e/terrain.md) for format details.

### W3I — `.w3i`

Built-in viewer for **W3I** map information files (`war3map.w3i`).
Displays map metadata: name, author, players, forces, camera bounds, fog/weather settings, random groups, and more.

### MPQ / W3X / W3M / W3N — `.mpq`, `.w3x`, `.w3m`, `.w3n`

Built-in archive viewer/browser for **MPQ** archives, including WarCraft III map (`.w3x`, `.w3m`) and campaign (`.w3n`) files.
Provides a virtual file system to browse and open files inside the archive directly from the VS Code explorer.

### WTS — `.wts`

Syntax support for **WTS** (WarCraft III Trigger Strings) files used for localization.

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
| **Code actions** | ✅ | — | — |
| **Formatting** | ✅ | — | — |
| **Color picker** | ✅ | — | — |

---

## Import System

JASS files can be linked together using special comment-based directives:

```jass
//import path/to/file.j
//import! blizzard/common.j
//import-ujapi! ujapi/common.j
```

- `//import` — links another file into a shared scope. All top-level declarations (functions, globals, types, natives) become available.
- `//import!` — **frozen** import. Same as `//import`, but the target file is treated as read-only and will not be modified by refactoring or auto-rename.
- `//import-ujapi!` — **UjAPI frozen import**. Downloads and imports a UjAPI file. If the file does not exist locally, a code action is offered to download it.

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
//set type-tip 1
//set build-jass ./out/war3map.j
//set build-as ./out/war3map.as
//set backup ./backup
```

| Key | Values | Description |
|-----|--------|-------------|
| `ref-tip` | `1` / `0` | Show / hide reference-ID inlay hints next to each identifier — useful for debugging symbol resolution. Default `0`. |
| `type-tip` | `1` / `0` | Show / hide type-annotation inlay hints for variables and parameters (e.g. `: integer`, `: constant real array`). Default `0`. |
| `build-jass` | `<path>` | Output path for the JASS build. Merges the entire import tree into a single `.j` file. If the path is a directory, `war3map.j` is appended. When the path points to a `.w3x` / `.w3m` archive, the script is injected directly into the map. |
| `build-as` | `<path>` | Output path for the AngelScript build. Same merge logic, but emits `.as` syntax. Reserved-word conflicts are resolved by appending a numeric suffix. When the path points to a `.w3x` / `.w3m` archive, the script is injected directly into the map. |
| `backup` | `<path>` | Backup directory for the map archive before build injection. A date-prefixed copy (`YYYY_MM_DD_FileName.w3x`) is saved before modifying the archive. |

---

## `//entry` — Build Entry Point

```jass
//entry
//import common/natives.j
//set build-jass ./out/war3map.j
```

The `//entry` directive marks a file as the **root of the build** and tree-shaking scope.

- The build starts from an `//entry` file. The `//set build-jass` / `//set build-as` directives are only honoured in entry files.
- Only files transitively reachable via `//import` from an entry point are included in the build output.
- Only `main` / `config` functions **declared in entry-point files** are considered live roots for unused-function analysis.
- If no `//entry` directives exist in the project, the old behaviour is preserved: all `main` / `config` functions are treated as entry points everywhere.

---

## `//ignore` — File-Level Diagnostic Suppression

```jass
//ignore unused leak
```

The `//ignore` directive suppresses entire categories of diagnostics for the whole file. It must appear at the beginning of the file, alongside `//import` and `//set`.

Multiple tags can be listed on one line, separated by spaces. Supported tags are the same as for `//@ignore` (see below).

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
| `leak` | Handle-leak diagnostic — local handle-type variables not nullified before function exit |
| `cycle` | Cyclic call chain diagnostic |

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

## Handle Leak Detection

JASS does not run destructors on local variables. If a local handle-type variable holds a non-null reference when the function exits, the underlying object is never released — a **handle leak**.

The server performs data-flow analysis on every function body to detect handle variables not nullified (`set v = null`) before every exit point (`return` and implicit `endfunction`).

- **Per-variable quick fix** — a code action inserts `set v = null` before the return.
- **Fix all leaks** — a single code action fixes all handle leaks in the file.
- Suppressed with `//@ignore leak` (per-function) or `//ignore leak` (whole file).

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
| `ujapi.download` | Download UjAPI common.j |
| `jass.restartServer` | Restart JASS LSP Server |
| `mpq.browse` | Browse MPQ Archive |
| `mpq.openFile` | Open File from MPQ Archive |
| `slk.openTable` | Open SLK as Table |
| `slk.openText` | Open SLK as Text |

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
