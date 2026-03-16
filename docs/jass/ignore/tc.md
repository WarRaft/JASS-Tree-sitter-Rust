# `//ignore` — 診斷抑制指令

`//ignore` 指令用於抑制**整個檔案**的特定診斷。
它必須出現在檔案的**最開頭**，與 `//import` 和 `//set` 指令一起，在任何語言陳述式之前。

要抑制單個宣告的診斷，請在函式或變數上方使用 `//@ignore`。

## 語法

```jass
//ignore <標籤…>
```

* `//ignore` 必須從**第 0 欄**開始（沒有前導空格）。
* 可以在同一行中列出一個或多個標籤，用空格分隔。

## 範例

```jass
//import common/natives.j
//ignore unused leak

function Helper takes nothing returns nothing
endfunction
```

## 可用標籤

| 標籤 | 說明 |
|------|------|
| `unused` | 抑制**未使用函式**診斷。 |
| `leak` | 抑制 **handle 洩漏**診斷。 |
| `cycle` | 抑制**循環呼叫鏈**診斷。 |

## 單宣告抑制

在宣告正上方的註解中使用 `//@ignore`：

```jass
//@ignore unused
function Helper takes nothing returns nothing
endfunction

function Foo takes nothing returns nothing
    //@ignore leak
    local unit u = CreateUnit()
endfunction
```

## 行為

* 標籤僅作用於單個檔案 — 不會透過 `//import` 傳播。
* 無法辨識的標籤會被靜默接受（為了向前相容）。
* 缺少標籤會產生警告診斷。


