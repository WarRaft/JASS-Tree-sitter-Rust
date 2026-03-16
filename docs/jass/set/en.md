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
//set ref-tip 1

globals
    integer count = 0
endglobals
```

## Available Settings

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `ref-tip` | `0 \| 1` | `0` | Show / hide reference ID inlay hints next to each identifier. Useful for debugging symbol resolution and rename refactoring. |
| `type-tip` | `0 \| 1` | `0` | Show / hide type-annotation inlay hints for variables and parameters (e.g. `: integer`, `: constant real array`). |
| `build-jass` | `<path>` | `./` | Output path for the JASS build. Merges the entire import tree into a single `.j` file: types → natives → globals → functions (topologically sorted) → `main`. If the path is a directory, `war3map.j` is appended. |
| `build-as` | `<path>` | `./` | Output path for the AngelScript build. Same merge logic, but emits `.as` syntax. Reserved-word conflicts are resolved by appending a numeric suffix. |

## Behaviour

* Settings are scoped to a single file — they do not propagate through
  `//import`.
* Unrecognized keys are silently accepted (for forward compatibility).
* A missing value produces a warning diagnostic.


