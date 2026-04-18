# `//set` — File Configuration Directive

The `//set` directive allows you to configure per-file settings for the
language server.  It must appear **at the very beginning** of the file,
alongside `//import` directives, before any language statements.

## Syntax

```jass
//set <key> <value>
```

* The `//set` token must start at **column 0** (no leading spaces).
* `<key>` is the setting name (no spaces allowed in the key).
* `<value>` is everything after the key until end-of-line (trimmed).

## Example

```jass
//import common/natives.j
//set hint ref type
//set lens fn

globals
    integer count = 0
endglobals
```

## Available Settings

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `hint` | `ref` `type` | | Inlay hint types to display. `ref` — reference ID hints next to each identifier (useful for debugging symbol resolution and rename refactoring), `type` — type-annotation hints for variables and parameters (e.g. `: integer`, `: constant real array`). Without the directive no hints are shown (except ujapi). |
| `lens` | `fn` `var` `arg` | | Code lenses to display above declarations. `fn` — reference count above functions and natives, `var` — reference count above variables (globals + locals), `arg` — reference count above function parameters. Without the directive no code lenses are shown. |
| `build-jass` | `<path>` | `./` | Output path for the JASS build. Merges the entire import tree into a single `.j` file: types → natives → globals → functions (topologically sorted) → `main`. If the path is a directory, `war3map.j` is appended. When the path points to a `.w3x` or `.w3m` archive, the script is injected directly into the map. |
| `build-as` | `<path>` | `./` | Output path for the AngelScript build. Same merge logic, but emits `.as` syntax. Reserved-word conflicts are resolved by appending a numeric suffix. When the path points to a `.w3x` or `.w3m` archive, the script is injected directly into the map. |
| `backup` | `<path>` | `./` | Backup path for the map archive. Before injecting the script into a `.w3x` / `.w3m` file, a copy of the original archive is saved to this path with a date prefix: `YYYY_MM_DD_FileName.w3x`. If the path is a directory, the date-prefixed archive filename is appended. |
| `build-opts` | `uglify` `nolocal` | | Build option tags. `uglify` enables identifier minification in build output. `nolocal` switches leak fixing for returned locals to the no-local strategy (use a temp global instead of introducing a temp local). Multiple tags may be combined in one directive. |
| `build-before` | `<command>` | | Shell command to run **before** the build. Executed via `sh -c` (Unix) or `cmd /C` (Windows). The working directory is the folder of the `//entry` file. Supports `{{variable}}` template placeholders (see below). |
| `build-after` | `<command>` | | Shell command to run **after** the build (only on success). Same execution rules as `build-before`. |

## Template Variables

Commands in `build-before` and `build-after` support `{{variable}}`
placeholders that are expanded to full normalized paths before execution.
This lets you pass build paths to external scripts reliably.

| Variable | Description |
|----------|-------------|
| `{{entry}}` | Full normalized path to the `//entry` file. |
| `{{entry-dir}}` | Full normalized path to the directory containing the `//entry` file. |
| `{{target-jass}}` | Full normalized path to the JASS build output file (from `//set build-jass`). Empty if not configured. |
| `{{target-as}}` | Full normalized path to the AngelScript build output file (from `//set build-as`). Empty if not configured. |

### Template Example

```jass
//entry
//set build-jass ./out/war3map.j
//set build-before echo "Building from {{entry}}..."
//set build-after my-post-build.sh {{target-jass}}
```

## Behaviour

* Settings are scoped to a single file — they do not propagate through
  `//import`.
* Unrecognized keys are silently accepted (for forward compatibility).
* A missing value produces a warning diagnostic.


