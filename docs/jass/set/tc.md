# `//set` — 檔案配置指令

`//set` 指令用於為語言伺服器配置單檔案級別的設定。
它必須出現在檔案的**最開頭**，與 `//import` 指令一起，在任何語言陳述式之前。

## 語法

```jass
//set <鍵> <值>
```

* `//set` 必須從**第 0 欄**開始（沒有前導空格）。
* `<鍵>` 是設定名稱（鍵中不允許有空格）。
* `<值>` 是鍵之後到行末的所有內容（會去除首尾空格）。

## 範例

```jass
//import common/natives.j
//set hint ref type
//set build-jass ./out/war3map.j

globals
    integer count = 0
endglobals
```

## 可用設定

| 鍵 | 類型 | 預設 | 說明 |
|----|------|------|------|
| `hint` | `ref` `type` | | 要顯示的內嵌提示類型。`ref` — 參考 ID（用於除錯符號解析），`type` — 型別註解（例如 `: integer`、`: constant real array`）。無指令時不顯示提示（ujapi 除外）。 |
| `build-jass` | `<路徑>` | `./` | JASS 建構的輸出路徑。將整個匯入樹合併為單個 `.j` 檔案：型別 → native → globals → 函式（拓撲排序）→ `main`。如果路徑是目錄，則附加 `war3map.j`。當路徑指向 `.w3x` 或 `.w3m` 存檔時，腳本會直接注入到地圖中。 |
| `build-as` | `<路徑>` | `./` | AngelScript 建構的輸出路徑。相同的合併邏輯，但輸出 `.as` 語法。保留字衝突透過附加數字後綴解決。當路徑指向 `.w3x` 或 `.w3m` 存檔時，腳本會直接注入到地圖中。 |
| `backup` | `<路徑>` | `./` | 地圖存檔備份路徑。在向 `.w3x` / `.w3m` 檔案注入腳本之前，原始存檔的副本會儲存到此路徑，檔名帶有日期前綴：`YYYY_MM_DD_原始檔名.w3x`。如果路徑是目錄，則帶日期前綴的檔名會放入該目錄。 |
| `build-uglify` | `0 \| 1` | `0` | 在建構輸出中壓縮標識符。啟用後，函式和變數名稱將被縮短以減小檔案大小。 |
| `build-before` | `<命令>` | | 建構**之前**執行的終端命令。透過 `sh -c`（Unix）或 `cmd /C`（Windows）執行。工作目錄為 `//entry` 檔案所在目錄。支援 `{{變數}}` 模板佔位符（見下文）。 |
| `build-after` | `<命令>` | | 建構**之後**執行的終端命令（僅在成功時）。執行規則與 `build-before` 相同。 |

## 模板變數

`build-before` 和 `build-after` 中的命令支援 `{{變數}}`
佔位符，在執行前會被展開為完整的正規化路徑。
這使您可以可靠地將建構路徑傳遞給外部腳本。

| 變數 | 說明 |
|------|------|
| `{{entry}}` | `//entry` 檔案的完整正規化路徑。 |
| `{{entry-dir}}` | 包含 `//entry` 檔案的目錄的完整正規化路徑。 |
| `{{target-jass}}` | JASS 建構輸出檔案的完整正規化路徑（來自 `//set build-jass`）。未配置時為空。 |
| `{{target-as}}` | AngelScript 建構輸出檔案的完整正規化路徑（來自 `//set build-as`）。未配置時為空。 |

### 模板範例

```jass
//entry
//set build-jass ./out/war3map.j
//set build-before echo "正在從 {{entry}} 建構..."
//set build-after my-post-build.sh {{target-jass}}
```

## 行為

* 設定僅作用於單個檔案 — 不會透過 `//import` 傳播。
* 無法辨識的鍵會被靜默接受（為了向前相容）。
* 缺少值會產生警告診斷。


