# `//ignore` — Diagnostic Suppression Directive

The `//ignore` directive suppresses specific diagnostics for the **entire file**.
It must appear **at the very beginning** of the file, alongside `//import` and
`//set` directives, before any language statements.

For per-declaration suppression use `//@ignore` above the function or variable.

## Syntax

```jass
//ignore <tag…>
```

* The `//ignore` token must start at **column 0** (no leading spaces).
* One or more tags can be listed on the same line, separated by spaces.

## Example

```jass
//import common/natives.j
//ignore unused leak

function Helper takes nothing returns nothing
endfunction
```

## Available Tags

| Tag | Description |
|-----|-------------|
| `unused` | Suppress **unused-function** diagnostics. |
| `leak` | Suppress **handle-leak** diagnostics. |
| `cycle` | Suppress **cyclic-call-chain** diagnostics. |

## Per-Declaration Suppression

Use `//@ignore` in a comment directly above a declaration:

```jass
//@ignore unused
function Helper takes nothing returns nothing
endfunction

function Foo takes nothing returns nothing
    //@ignore leak
    local unit u = CreateUnit()
endfunction
```

## Behaviour

* Tags are scoped to a single file — they do not propagate through `//import`.
* Unknown tags are silently accepted (for forward compatibility).
* A missing tag produces a warning diagnostic.


