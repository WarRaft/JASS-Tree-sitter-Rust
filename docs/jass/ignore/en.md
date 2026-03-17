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

| Tag | File (`//ignore`) | Function (`//@ignore`) | Variable (`//@ignore`) |
|-----|:-:|:-:|:-:|
| `unused` | ✔ | ✔ | — |
| `leak` | ✔ | ✔ | ✔ |
| `cycle` | ✔ | ✔ | — |

* **`unused`** — suppress **unused-function** diagnostics.
* **`leak`** — suppress **handle-leak** diagnostics.
* **`cycle`** — suppress **cyclic-call-chain** diagnostics.

## Per-Declaration Suppression

Use `//@ignore` in a comment directly above a declaration.
Tags can be combined on one line: `//@ignore unused cycle`.

### Function level

Placing `//@ignore` above a function suppresses diagnostics for that function only:

```jass
//@ignore unused
function Helper takes nothing returns nothing
endfunction

//@ignore cycle
function Recursive takes nothing returns nothing
    call Recursive()
endfunction

//@ignore leak
function Setup takes nothing returns nothing
    local unit u = CreateUnit()
endfunction
```

### Variable level

Placing `//@ignore leak` above a `local` declaration suppresses the leak diagnostic for that single variable:

```jass
function Foo takes nothing returns nothing
    //@ignore leak
    local unit u = CreateUnit()
    local unit v = CreateUnit()  // ← still diagnosed
endfunction
```

## Behaviour

* Tags are scoped to a single file — they do not propagate through `//import`.
* Unknown tags are silently accepted (for forward compatibility).
* A missing tag produces a warning diagnostic.


