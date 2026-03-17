# `//ignore` — 诊断抑制指令

`//ignore` 指令用于抑制**整个文件**的特定诊断。
它必须出现在文件的**最开头**，与 `//import` 和 `//set` 指令一起，在任何语言语句之前。

要抑制单个声明的诊断，请在函数或变量上方使用 `//@ignore`。

## 语法

```jass
//ignore <标签…>
```

* `//ignore` 必须从**第 0 列**开始（没有前导空格）。
* 可以在同一行中列出一个或多个标签，用空格分隔。

## 示例

```jass
//import common/natives.j
//ignore unused leak

function Helper takes nothing returns nothing
endfunction
```

## 可用标签

| 标签 | 文件 (`//ignore`) | 函数 (`//@ignore`) | 变量 (`//@ignore`) |
|-----|:-:|:-:|:-:|
| `unused` | ✔ | ✔ | — |
| `leak` | ✔ | ✔ | ✔ |
| `cycle` | ✔ | ✔ | — |

* **`unused`** — 抑制**未使用函数**诊断。
* **`leak`** — 抑制 **handle 泄漏**诊断。
* **`cycle`** — 抑制**循环调用链**诊断。

## 单声明抑制

在声明正上方的注释中使用 `//@ignore`。
标签可以组合在一行中：`//@ignore unused cycle`。

### 函数级别

在函数上方放置 `//@ignore` 仅抑制该函数的诊断：

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

### 变量级别

在 `local` 声明上方放置 `//@ignore leak` 仅抑制该变量的泄漏诊断：

```jass
function Foo takes nothing returns nothing
    //@ignore leak
    local unit u = CreateUnit()
    local unit v = CreateUnit()  // ← 仍会被诊断
endfunction
```

## 行为

* 标签仅作用于单个文件 — 不会通过 `//import` 传播。
* 无法识别的标签会被静默接受（为了向前兼容）。
* 缺少标签会产生警告诊断。


