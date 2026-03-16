[![](https://dcbadge.limes.pink/api/server/https://discord.gg/CNeQmXAgVq)](https://discord.gg/CNeQmXAgVq)

[English](README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-cn.md) | **繁體中文**

# JASS Tree-sitter Rust

沒錯，名稱就是對技術堆疊的直白描述——用 Rust 編寫的 Tree-sitter 語法。  
把 JASS 放在最前面是為了吸引目光（和懷舊），當然，我們也完全支援它。

## [VSCode](https://code.visualstudio.com)

本外掛匯集了各種用於處理經典 WarCraft III 內容的工具，提供語法支援、編輯器功能以及一些現代化的便利特性。

👉 [VSCode Marketplace](https://marketplace.visualstudio.com/items?itemName=WarRaft.jass-tree-sitter-rust)

---

## 支援的語言

### [JASS](https://github.com/WarRaft/tree-sitter-jass) — `.j`、`.pld`

WarCraft III 的主要腳本語言。基於專用的
[tree-sitter-jass](https://github.com/WarRaft/tree-sitter-jass) 語法提供完整支援。

### [AngelScript](https://github.com/WarRaft/tree-sitter-as) — `.as`

為基於 UJAPI 的 WarCraft III 模組開發提供 AngelScript 支援。語法 —
[tree-sitter-as](https://github.com/WarRaft/tree-sitter-as)。

### [BNI](https://github.com/WarRaft/tree-sitter-bni) — `.bni`

**BNI**（Blizzard Notation Ini）— Warcraft III 模組開發中使用的結構化設定格式。  
語法 — [tree-sitter-bni](https://github.com/WarRaft/tree-sitter-bni)。

### BLP — `.blp`

內建的 **BLP** 紋理格式圖片檢視器，BLP 是 WarCraft III 使用的紋理格式。

---

## LSP 功能

擴充功能附帶獨立的 Rust 編寫的 LSP 伺服器（支援 Linux、macOS、Windows）：

| 功能 | JASS | AngelScript | BNI |
|------|:----:|:-----------:|:---:|
| **語意醒目提示** | ✅ | ✅ | ✅ |
| **程式碼摺疊** | ✅ | ✅ | ✅ |
| **文件符號** | ✅ | ✅ | ✅ |
| **診斷** | ✅ | ✅ | — |
| **跳轉到定義** | ✅ | ✅ | — |
| **尋找所有參考** | ✅ | ✅ | — |
| **醒目提示出現位置** | ✅ | ✅ | — |
| **重新命名** | ✅ | ✅ | — |
| **暫留提示** | ✅ | ✅ | — |
| **自動完成** | ✅ | ✅ | — |
| **內嵌提示** | ✅ | ✅ | — |
| **文件連結** | ✅ | ✅ | — |

---

## 匯入系統

JASS 檔案可以透過特殊的註解指令相互連結：

```jass
//import path/to/file.j
//import! blizzard/common.j
```

- `//import` — 將另一個檔案連結到共享作用域。所有頂層宣告（函式、全域變數、型別、原生函式）都將可用。
- `//import!` — **凍結**匯入。與 `//import` 相同，但目標檔案被視為唯讀，不會被重構或自動重新命名修改。

指令必須出現在檔案最開頭，在任何語言陳述式之前。

### 匯入功能

- **路徑補全** — `//import` 後的檔案路徑自動補全。
- **Ctrl+Click** — 在編輯器中開啟匯入的檔案。
- **無效路徑診斷** — 不存在的路徑會被醒目提示為錯誤。
- **重新命名/移動時自動更新** — 當匯入的檔案被重新命名或移動時，所有參考檔案中的路徑會自動重寫。
- **跨平台路徑** — `/` 和 `\` 可互換；支援相對路徑、絕對路徑和 Windows 風格路徑（`C://`）。
- **循環偵測** — 偵測並回報循環匯入。

---

## `//set` — 檔案級設定

```jass
//set ref-tip 1
//set build-jass ./out/war3map.j
//set build-as ./out/war3map.as
//set unused 0
```

| 鍵 | 值 | 說明 |
|----|-----|------|
| `ref-tip` | `1` / `0` | 顯示/隱藏每個識別碼旁的參考 ID 內嵌提示——對偵錯符號解析很有用。 |
| `unused` | `1` / `0` | 啟用/停用整個檔案的未使用函式診斷。預設 `1`（啟用）。 |
| `build-jass` | `<路徑>` | JASS 建置的輸出路徑。將整個匯入樹合併為單一 `.j` 檔案。 |
| `build-as` | `<路徑>` | AngelScript 建置的輸出路徑。相同的合併邏輯，但輸出 `.as` 語法。 |

---

## `//*` — 文件註解

緊接在宣告之前以 `//*` 開頭的行被視為**文件註解**（Markdown）。它們會顯示在懸停提示和補全詳情中。

```jass
//* 在指定位置生成一個單位。
//* 返回建立的單位控制代碼。
function SpawnUnit takes integer id, real x, real y returns unit
    // ...
endfunction
```

多個連續的 `//*` 行會被合併。前綴 `//* `（帶尾部空格）會被去除；`//*文字` 也可以。

---

## `//@ignore` — 按宣告抑制診斷

將 `//@ignore` 註解放在函式、變數、型別或 native 宣告之前，可以為該特定宣告抑制列出的診斷標籤。

```jass
//@ignore unused
function HelperFunc takes nothing returns nothing
    // 不會為 HelperFunc 回報「Unused function」診斷。
endfunction
```

### 語法

```jass
//@ignore tag1 tag2 ...
```

標籤以空格分隔。目前支援的標籤：

| 標籤 | 抑制 |
|------|------|
| `unused` | 「Unused function」提示 |

`//@ignore` 可以與 `//*` 文件註解以任意順序組合：

```jass
//* 內部輔助函式——不直接呼叫。
//@ignore unused
function InternalHelper takes nothing returns nothing
endfunction
```

---

## 跨檔案智慧

所有透過 `//import` 連結的檔案形成一個**連通元件**——共享的全域作用域：

- **作用域解析器** — 跨所有匯入檔案的持久 O(1) 名稱查詢，在伺服器重啟之間保持。
- **兩階段解析** — 第一階段在本機解析符號；第二階段將未解析的參考與匯入的符號連結。
- **匯出差異偵測** — 僅在匯出宣告集實際發生變化時才重新解析相依檔案。
- **推送診斷** — 立即為受影響的檔案回報錯誤，即使它們未在編輯器中開啟。

---

## 呼叫圖

伺服器在整個連通元件中建置函式呼叫圖：

- **未使用函式偵測** — 從 `main` / `config` 進入點不可達的函式會被標記。
- **循環偵測** — 透過診斷回報循環呼叫鏈。
- **拓撲排序** — 建置系統使用它來確保被呼叫函式出現在呼叫函式之前（JASS 要求）。

基於 D3.js 的**呼叫圖**面板可透過編輯器標題列按鈕存取。

---

## 匯入圖視覺化

基於 D3.js 的**匯入圖**面板顯示目前檔案的相依樹。可透過編輯器標題列按鈕存取。所有視覺化資源已內建——無需網際網路連線。

---

## 建置系統

`//set build-jass <路徑>` 和 `//set build-as <路徑>` 指令觸發建置：

1. 收集匯入樹中的所有檔案。
2. 對函式執行拓撲排序。
3. 將所有內容合併到單一輸出檔案：**型別 → 全域變數 → 函式 → `main`**。
4. 跳過 `native` 宣告和型別定義（它們由引擎提供）。
5. 頂層的裸呼叫運算式被摺疊到 `main` 中。

---

## 持久快取

所有重量級資料結構透過 **bincode** 序列化到磁碟，並在伺服器重啟時還原：

- **匯入圖** — 檔案相依圖（基於 petgraph）。
- **作用域解析器** — 全域符號索引。
- **符號快取** — 按檔案儲存的函式/變數/型別宣告。
- **參考快取** — 按檔案儲存的參考對映。

這意味著即使對於大型專案也能幾乎即時啟動。

---

## 架構

- **Tree-sitter** — 所有支援語法的增量解析。
- **`ParseSnapshot`** — 每個檔案所有 LSP 資料的原子不可變快照，儲存在 `Arc<ParseSnapshot>` 中以實現無鎖並行讀取。
- **`CancellationToken`** — 按檔案取消：新編輯立即中止過時的解析工作。
- **DashMap** — 所有快照的並行檔案儲存。
- **petgraph** — 匯入圖和呼叫圖分析。

---

## 鍵盤快速鍵

所有命令都可透過編輯器標題列按鈕使用，但您也可以指派自訂鍵盤快速鍵。

開啟**鍵盤快速鍵**（`Ctrl+K Ctrl+S` / `⌘K ⌘S`），搜尋命令名稱，然後繫結任意組合鍵。

| 命令 ID | 說明 |
|---------|------|
| `importGraph.show` | 顯示匯入圖 |
| `callGraph.show` | 顯示呼叫圖 |
| `build.execute` | 建置（JASS / AngelScript） |

或者直接在 `keybindings.json` 中新增繫結（`Ctrl+Shift+P` → *Preferences: Open Keyboard Shortcuts (JSON)*）：

```json
[
  { "key": "ctrl+shift+i", "command": "importGraph.show", "when": "resourceLangId == jass || resourceLangId == angelscript" },
  { "key": "ctrl+shift+g", "command": "callGraph.show",   "when": "resourceLangId == jass || resourceLangId == angelscript" },
  { "key": "ctrl+shift+b", "command": "build.execute",    "when": "resourceLangId == jass || resourceLangId == angelscript" }
]
```

---

## 授權條款

[MIT](LICENSE)

