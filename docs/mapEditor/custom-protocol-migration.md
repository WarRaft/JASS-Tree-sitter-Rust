# Migrating custom methods from vscode-languageclient to HTTP

> **Status:** In progress — Phase 0 (LSP naming cleanup + initialization removal) complete
>
> Dependencies: [`binary-protocol.md`](./binary-protocol.md) (Columnar Binary Protocol)

## Completed: Phase 0 — LSP naming cleanup

The extension no longer uses `vscode-languageclient`. Communication happens over
a custom WebSocket transport (`ServerClient`). This phase removed all remaining
LSP naming conventions from the wire protocol and extension code.

### Method renames (wire protocol)

| Old (LSP convention)                  | New (custom)           | Direction        |
|---------------------------------------|------------------------|------------------|
| `textDocument/didOpen`                | `document/open`        | client → server  |
| `textDocument/didChange`              | `document/change`      | client → server  |
| `textDocument/didClose`               | `document/close`       | client → server  |
| `textDocument/colorPresentation`      | `color/presentation`   | client → server  |
| `workspace/didChangeWatchedFiles`     | `files/changed`        | client → server  |
| `client/registerCapability`           | `watchers/register`    | server → client  |

### Removed dead code

| Item                                  | File              | Reason |
|---------------------------------------|-------------------|--------|
| `window/logMessage` handler           | `extension.js`    | Server never sends this notification |

### Comment cleanup

All references to "LSP", "vscode-languageclient", and LSP-style method names
removed from `extension.js` comments. Corresponding Rust comments in `main.rs`,
`file_store.rs`, `color/lsp.rs`, `color/send.rs` also updated.

### Files changed

- `extension/extension.js` — method renames, dead handler removal, comment cleanup
- `src/lsp/protocol.rs` — `#[serde(rename)]` attributes updated
- `src/main.rs` — `method_name()` function, registration send block, comments
- `src/util/file_store.rs` — comment update
- `src/lsp/color/lsp.rs` — comment update
- `src/lsp/color/send.rs` — comment update

## Completed: Phase 0b — LSP initialization removal

The LSP initialization handshake (`initialize` → `initialized` → `shutdown` →
`exit`) was dead code since the extension switched to a custom WebSocket
transport. The client (`ServerClient`) never sent these messages — it connects
via WebSocket and immediately starts sending `document/open` etc.

### Removed from server

| Item | File | Reason |
|------|------|--------|
| `initialize.rs` module | `src/lsp/initialize.rs` | Dead code — `InitializeParams`, `InitializeResult`, `ServerCapabilities` structs |
| `initialized.rs` module | `src/lsp/initialized.rs` | Dead code — `InitializedParams` struct |
| `set_trace.rs` module | `src/lsp/set_trace.rs` | Dead code — `SetTraceParams` struct |
| `MethodCall::Initialize` | `src/lsp/protocol.rs` | Never received from client |
| `MethodCall::Initialized` | `src/lsp/protocol.rs` | Never received from client |
| `MethodCall::Shutdown` | `src/lsp/protocol.rs` | Never received from client |
| `MethodCall::Exit` | `src/lsp/protocol.rs` | Never received from client |
| `MethodCall::SetTrace` | `src/lsp/protocol.rs` | Never received from client |
| Initialize handler | `src/main.rs` | Built `InitializeResult` with `ServerCapabilities`, sent response — dead code |
| Initialized handler | `src/main.rs` | Cache loading, file watcher registration, `binaryServerReady` notification — all dead code |
| Shutdown/Exit handler | `src/main.rs` | Sent empty response and broke loop — dead code (shutdown via stdin close) |
| SetTrace handler | `src/main.rs` | No-op handler — dead code |
| `store_init_request()` | `src/util/debug_log.rs` | Stored raw initialize request for debug panel — no longer sent |
| `store_init_response()` | `src/util/debug_log.rs` | Stored initialize response for debug panel — no longer generated |
| `INIT_REQUEST`, `INIT_RESPONSE` statics | `src/util/debug_log.rs` | `OnceLock` statics for debug panel — removed |

### What was in the `Initialized` handler (all dead code)

The `Initialized` handler contained significant startup logic that was never
executed since the WebSocket migration:

1. **`custom/binaryServerReady` notification** — no extension handler exists
   (extension reads port/token from stdout at startup)
2. **Cache database init** — `cache_db::was_purged()`, scope resolver loading
3. **File cache loading** — `file_cache::load_all()`, snapshot reconstruction
4. **GC orphaned graph nodes** — `IMPORT_GRAPH.gc_orphans()`, `SCOPE_RESOLVER.gc()`
5. **Stale file re-parsing** — progress notifications, `open_by_uri()` for each stale file
6. **File watcher registration** — `watchers/register` request for `*.j`, `*.ai`, `*.as`

Items 2–6 may need to be re-implemented as startup logic (before the message
loop) or triggered lazily. Currently the server works without them.

### Files changed

- `src/lsp/initialize.rs` — **deleted**
- `src/lsp/initialized.rs` — **deleted**
- `src/lsp/set_trace.rs` — **deleted**
- `src/lsp/mod.rs` — removed module declarations
- `src/lsp/protocol.rs` — removed 5 enum variants + imports
- `src/main.rs` — removed handlers, imports, `method_name()` arms
- `src/util/debug_log.rs` — removed init storage, simplified `get_init_data()`

## Problem

The extension uses a custom `ServerClient` (WebSocket transport) for **everything** —
language features and completely
unrelated binary-format tasks: BLP textures, MDX models, SLK catalogs,
MPQ archives, map editor, build system, graph panels, debug panel.

> **Note:** The transport has already been migrated from `vscode-languageclient`
> (stdin/stdout) to a custom WebSocket client. The issues below describe the
> remaining architectural problems that the HTTP migration will solve.

### What the current architecture does

`LanguageClient` is a JSON-RPC client over `stdin/stdout`. It implements the
[Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
and enforces a rigid contract. Once the client starts:

1. **Automatic initialization sequence.** Client sends `initialize`, then
   `initialized`, then starts sending `textDocument/didOpen` for every open
   file, `workspace/didChangeWatchedFiles` for disk changes.

2. **Automatic request cascade on every keystroke.** User types a single
   character → client sends `didChange`, then automatically fires:
   - `textDocument/semanticTokens/full` (or `/range`)
   - `textDocument/diagnostic`
   - `textDocument/inlayHint`
   - `textDocument/codeLens`
   - `textDocument/documentLink`
   - `textDocument/foldingRange`
   - `textDocument/documentColor`
   - `textDocument/documentSymbol`

   Each of these is JSON-serialized, wrapped in a JSON-RPC frame with
   `Content-Length` header, sent to `stdin`, deserialized on the server,
   processed, the result JSON-serialized back, and written to `stdout`.
   With 10–20 open files this is hundreds of requests per second.

3. **Automatic cancel requests.** Client aggressively sends `$/cancelRequest`
   for in-flight requests that become stale (user keeps typing, previous
   `semanticTokens/full` is no longer relevant). Server must:
   - Track every request ID
   - Check cancellation status before and during processing
   - Respond with error code `RequestCancelled (-32800)`

   This is already implemented — see `CancelCheck` trait, `CancelId::was_cancelled()`,
   `send_cancelled()`, and the triple `ct.is_cancelled() || call.id.was_cancelled()`
   checks throughout `main.rs`.

4. **Single-channel serialization.** `stdin/stdout` is one pipe. All responses
   serialize through a single `Arc<Mutex<Stdout>>`. While tokio handles requests
   concurrently, the actual write to stdout is a sequential bottleneck.

### What the server is forced to do because of this

Because LSP is used for non-LSP tasks, the server has these architectural
workarounds:

#### 1. Giant `MethodCall` enum (protocol.rs)

All custom methods are crammed into one `enum MethodCall` alongside standard
LSP methods. Currently **55 variants**, each with its own param struct,
deserialized via `#[serde(tag = "method", content = "params")]`.

**Standard LSP variants (should stay):** 27
```
Cancel,
DidClose, DidOpen, DidChange, DidChangeWatchedFiles,
SemanticFull, SemanticRange, Diagnostic, DocumentSymbol, Folding,
Completion, Hover, DocumentHighlight, Definition, References,
InlayHint, DocumentLink, Formatting, PrepareRename, Rename,
WillRenameFiles, DocumentColor, ColorPresentation, CodeAction,
SignatureHelp, CodeLens, PrepareCallHierarchy, IncomingCalls,
OutgoingCalls, PrepareTypeHierarchy, Supertypes, Subtypes
```

**Custom variants (should move to HTTP):** 23
```
BlpRender, MdxRender, DooRender, W3iRender, W3eRender, W3ObjRender,
W3eGamePathSet, W3eGamePathStatus, W3eTerrainSlk, W3eDoodadsSlk,
W3eUnitsSlk, W3eDestructablesSlk, W3eLookupFile,
MpqInfo, MpqList, MpqRead,
SlkRender, SlkEdit,
ImportGraphSubgraph, CallGraphSubgraph, TypeGraphSubgraph,
BuildExecute, BuildHooks, RescanExecute, UjapiDownload,
DebugLogEnable, DebugInit
```

#### 2. Single dispatch loop (main.rs)

One 1860-line `main.rs` with a giant `match` on all `MethodCall` variants.
BLP texture rendering runs in the same loop as `textDocument/hover`.
No prioritization — all requests are equal.

```
stdin → deserialize JSON-RPC → match MethodCall {
    Initialize(_)        → …,
    DidChange(_)         → …,
    SemanticFull(_)      → …,
    BlpRender(_)         → …,      // ← not LSP
    MdxRender(_)         → …,      // ← not LSP
    W3eRender(_)         → …,      // ← not LSP
    MpqList(_)           → …,      // ← not LSP
    W3eTerrainSlk(_)     → …,      // ← not LSP
    … 40+ more variants …
}
```

#### 3. Everything through JSON

Binary data (BLP textures, MDX models, MPQ file contents, terrain data)
must be base64-encoded, wrapped in a JSON object, and decoded on the client.
For a 1 MB terrain snapshot → ~1.33 MB JSON payload + `JSON.parse()`.

Example from `mpqFileSystemProvider.js`:
```js
result = await client.sendRequest('mpq/read', {archivePath, filePath})
return Buffer.from(result.content, 'base64')  // base64 → binary
```

Example from `main.rs` (`W3eLookupFile`):
```rust
use base64::Engine;
serde_json::json!({
    "content": base64::engine::general_purpose::STANDARD.encode(&buf),
    "source": source,
    "resolvedPath": resolved_path,
})
```

#### 4. Webview relay through extension host

Webview (map editor) cannot talk directly to the LSP server — only through
the extension host. Request path:

```
Webview
  → postMessage() → extension host (index.js)
    → client.sendRequest('w3e/terrainSlk', …) → stdin → Rust server
      → process → JSON result → stdout → extension host
        → webviewPanel.webview.postMessage() → Webview
```

Four hops instead of one `fetch()`. Each hop = JSON serialize/deserialize.

#### 5. Cancel overhead for heavy requests

Opening the map editor sends several heavy requests sequentially:
`w3e/render`, `w3i/render`, `doo/render` × 2, `mpq/info`.
If user closes the tab before completion — client sends `$/cancelRequest`
for each, but the server already spent resources parsing.

Meanwhile `didChange`, `semanticTokens`, `diagnostic` from the text editor
compete for the same `stdin/stdout` pipe.

### What already works over HTTP

A parallel axum HTTP server is already running. Some endpoints exist:

**Existing axum routes (server.rs):**
```rust
.route("/w3e/terrain",          get(terrain_handler))
.route("/w3e/file",             get(file_lookup_handler))
.route("/w3e/snapshot",         get(snapshot_handler))
.route("/w3e/tileTextures",     get(tile_textures_handler))
.route("/w3e/gamePath/status",  get(game_path_status_handler))
.route("/w3e/gamePath/set",     post(game_path_set_handler))
.route("/w3e/pathTex",          get(path_tex_handler))
.route("/mdx/texture",          get(mdx_texture_handler))
.route("/blp/render",           get(blp_render_handler))
```

The server prints `{port, token}` JSON to stdout on startup.
The extension reads it before connecting via WebSocket.

**But**: most code still falls back to LSP. Example from `mapEditor/index.js`:
```js
// HTTP attempt first…
const resp = await fetch(`http://127.0.0.1:${bs.port}/w3e/file?${params}`)
// …fallback to LSP
const result = await client.sendRequest('w3e/lookupFile', { path: tryPath, … })
```

```js
// gamePath: tries HTTP first, falls back to LSP
const status = await setGamePathViaHttp(msg.value) ||
    await client.sendRequest('w3e/gamePath/set', {gamePath: msg.value})
```

The goal is to remove all LSP fallbacks.

## Audit: every client.sendRequest / client.sendNotification call

### extension.js (main extension)

| Call | Line | Used by |
|------|------|---------|
| `client.sendRequest('rescan/execute', {uri})` | 577 | `rescan.execute` command |
| `client.sendRequest('build/hooks', {uri})` | 612 | `build.execute` command |
| `client.sendRequest('build/execute', {uri})` | 624 | `build.execute` command |
| `client.sendRequest('ujapi/download', {uri, path})` | 684 | `ujapi.download` command |

### mapEditor/index.js

| Call | Line | Used by |
|------|------|---------|
| `client.sendRequest('mpq/info', {archivePath})` | 51 | Archive opening |
| `client.sendRequest('w3e/render', {uri, archivePath?})` | 59, 132, 166, 229 | Terrain loading |
| `client.sendRequest('w3i/render', {uri, archivePath?})` | 69, 115, 178, 246 | Map info loading |
| `client.sendRequest('doo/render', {uri, isUnit, archivePath?})` | 80, 91, 148, 269, 283 | Doodad/unit placement |
| `client.sendRequest('mdx/render', {uri})` | 195, 613, 768 | Model rendering |
| `client.sendRequest('w3e/lookupFile', {path, archivePath?})` | 562, 732 | File lookup (fallback) |
| `client.sendRequest('w3e/gamePath/set', {gamePath})` | 807, 843 | Game path (fallback) |
| `client.sendRequest('w3e/gamePath/status', {})` | 363, 823 | Game path (fallback) |

### mapEditor/resolveBlpEditor.js

| Call | Line | Used by |
|------|------|---------|
| `client.sendRequest('blp/render', {uri})` | 33 | BLP preview editor |

### resolveSlkEditor.js

| Call | Line | Used by |
|------|------|---------|
| `client.sendRequest('slk/render', {uri})` | 149 | SLK table editor |
| `client.sendRequest('slk/edit', {uri, start, len, value})` | 219 | SLK cell edit |

### importGraphPanel.js

| Call | Line | Used by |
|------|------|---------|
| `client.sendRequest('importGraph/subgraph', {uri})` | 35 | Import graph panel |

### callGraphPanel.js

| Call | Line | Used by |
|------|------|---------|
| `client.sendRequest('callGraph/subgraph', {uri})` | ~41 | Call graph panel |

### typeGraphPanel.js

| Call | Line | Used by |
|------|------|---------|
| `client.sendRequest('typeGraph/subgraph', {uri})` | 40 | Type graph panel |

### debugSidebarProvider.js

| Call | Line | Used by |
|------|------|---------|
| `client.sendNotification('custom/debugLogEnable', {enabled})` | 101, 122 | Debug log toggle |
| `client.sendRequest('custom/debugInit', {})` | 105 | Debug init data |

### mpqFileSystemProvider.js

| Call | Line | Used by |
|------|------|---------|
| `client.sendRequest('mpq/list', {archivePath})` | 116 | Directory listing |
| `client.sendRequest('mpq/read', {archivePath, filePath})` | 265 | File reading |

## Target architecture

### What stays on WebSocket

Core language features and document sync (custom method names):

| Category | Methods |
|----------|---------|
| Document sync | `document/open`, `document/change`, `document/close`, `files/changed`, `watchers/register` |
| Language features | `textDocument/completion`, `textDocument/hover`, `textDocument/definition`, `textDocument/references`, `textDocument/formatting`, `textDocument/rename`, `textDocument/prepareRename`, `textDocument/codeAction`, `textDocument/codeLens`, `textDocument/signatureHelp`, `textDocument/documentHighlight`, `textDocument/prepareCallHierarchy`, `textDocument/prepareTypeHierarchy`, `color/presentation`, `workspace/willRenameFiles`, `callHierarchy/*`, `typeHierarchy/*` |
| Custom notifications | `custom/parseResult`, `custom/debugLog` |
| Internal | `$/cancelRequest` |

### What moves to HTTP

| Current LSP method | HTTP endpoint | Format | Client file |
|--------------------|---------------|--------|-------------|
| `blp/render` | `GET /blp/render` | `application/json` (mipmaps w/ data URLs) → later PNG binary | `resolveBlpEditor.js` |
| `mdx/render` | `GET /mdx/render` | `application/json` (geosets, textures, materials) | `mapEditor/index.js` |
| `doo/render` | `GET /doo/render` | `application/json` | `mapEditor/index.js` |
| `w3i/render` | `GET /w3i/render` | `application/json` | `mapEditor/index.js` |
| `w3e/render` | `GET /w3e/terrain` | already exists | `mapEditor/index.js` |
| `w3obj/render` | `GET /w3obj/render` | `application/json` | (custom editor) |
| `w3e/terrainSlk` | `GET /w3e/catalog/terrain` | `application/json` → WOBJ binary | `mapEditor/index.js` |
| `w3e/doodadsSlk` | `GET /w3e/catalog/doodads` | `application/json` → WOBJ binary | `mapEditor/index.js` |
| `w3e/unitsSlk` | `GET /w3e/catalog/units` | `application/json` → WOBJ binary | `mapEditor/index.js` |
| `w3e/destructablesSlk` | `GET /w3e/catalog/destructables` | `application/json` → WOBJ binary | `mapEditor/index.js` |
| `w3e/lookupFile` | `GET /w3e/file` | already exists | `mapEditor/index.js` |
| `w3e/gamePath/set` | `POST /w3e/gamePath/set` | already exists | `mapEditor/index.js` |
| `w3e/gamePath/status` | `GET /w3e/gamePath/status` | already exists | `mapEditor/index.js` |
| `mpq/info` | `GET /mpq/info` | `application/json` | `mapEditor/index.js`, `mpqFileSystemProvider.js` |
| `mpq/list` | `GET /mpq/list` | `application/json` | `mpqFileSystemProvider.js` |
| `mpq/read` | `GET /mpq/read` | `application/octet-stream` (no more base64) | `mpqFileSystemProvider.js` |
| `slk/render` | `GET /slk/render` | `application/json` | `resolveSlkEditor.js` |
| `slk/edit` | `POST /slk/edit` | `application/json` | `resolveSlkEditor.js` |
| `importGraph/subgraph` | `GET /graph/import` | `application/json` | `importGraphPanel.js` |
| `callGraph/subgraph` | `GET /graph/call` | `application/json` | `callGraphPanel.js` |
| `typeGraph/subgraph` | `GET /graph/type` | `application/json` | `typeGraphPanel.js` |
| `build/execute` | `POST /build/execute` | `application/json` | `extension.js` |
| `build/hooks` | `GET /build/hooks` | `application/json` | `extension.js` |
| `rescan/execute` | `POST /rescan` | `application/json` | `extension.js` |
| `ujapi/download` | `POST /ujapi/download` | `application/json` | `extension.js` |
| `custom/debugLogEnable` | `POST /debug/log/enable` | `application/json` | `debugSidebarProvider.js` |
| `custom/debugInit` | `GET /debug/init` | `application/json` | `debugSidebarProvider.js` |

### Benefits

1. **Clean separation.** LSP channel stays lightweight — only language features.
   Heavy operations (rendering, parsing, catalogs) go over HTTP and don't block
   the diagnostics/highlighting pipeline.

2. **True parallelism.** HTTP allows concurrent `fetch()` requests with separate
   response streams. `stdin/stdout` is a single pipe, single write mutex.

3. **Binary transport.** HTTP endpoints return `application/octet-stream` with
   no base64 overhead. MPQ file reads go from `base64 JSON → Buffer.from(…, 'base64')`
   to a direct `arrayBuffer()`. Saves ~33% bandwidth + eliminates `JSON.parse()`
   on large payloads.

4. **Direct webview access.** Map editor webview calls `fetch()` directly to
   `http://127.0.0.1:{port}/…` — no relay through extension host, no
   `postMessage()` hops. Currently 4 hops → 1 hop.

5. **Portability.** Same HTTP endpoints work in a standalone browser
   (WASM + local file API or remote server). LSP ties us to VS Code only.

6. **Simpler server.** `MethodCall` enum shrinks to ~27 language-feature variants.
   Custom requests become separate axum handlers with proper routing,
   middleware, typed parameters, and error handling.

7. **No cancel overhead.** HTTP requests are cancelled via `AbortController`
   on the client side. Server doesn't need to track IDs or respond with `-32800`.

## Migration plan

### Phase 1 — Map editor endpoints

Move all map editor requests to HTTP. Map editor stops using
`client.sendRequest()` entirely — only `fetch()`.

**Server side (Rust):**
- Add axum routes: `GET /w3e/render`, `GET /w3i/render`, `GET /doo/render`,
  `GET /mdx/render`, `GET /mpq/info`
- Move handler logic from `main.rs` match arms to `src/http/*.rs` modules
- Remove `W3eRender`, `W3iRender`, `DooRender`, `MdxRender`, `MpqInfo`
  from `MethodCall` enum in `protocol.rs`

**Client side (JS):**
- In `mapEditor/index.js`: replace all `client.sendRequest('w3e/render', …)` →
  `fetch(`http://127.0.0.1:${bs.port}/w3e/render?…`)`
- Same for `w3i/render`, `doo/render`, `mdx/render`, `mpq/info`
- Remove LSP fallback code for `w3e/lookupFile`, `w3e/gamePath/*`

**Remove from MethodCall:**
```
W3eRender, W3iRender, DooRender, MdxRender, W3ObjRender,
W3eTerrainSlk, W3eDoodadsSlk, W3eUnitsSlk, W3eDestructablesSlk,
W3eLookupFile, W3eGamePathSet, W3eGamePathStatus, MpqInfo
```

### Phase 2 — BLP, SLK, MPQ file system

Move remaining binary/file format endpoints to HTTP.

**Server side:**
- `GET /blp/render` — already exists for webview, extend for editor use
- `GET /mpq/list`, `GET /mpq/read` — add axum routes
- `GET /slk/render`, `POST /slk/edit` — add axum routes

**Client side:**
- `resolveBlpEditor.js`: `client.sendRequest('blp/render')` → `fetch()`
- `mpqFileSystemProvider.js`: `client.sendRequest('mpq/list')` → `fetch()`,
  `client.sendRequest('mpq/read')` → `fetch()` returning raw `arrayBuffer()`
  (eliminating base64 entirely)
- `resolveSlkEditor.js`: `client.sendRequest('slk/render')` → `fetch()`,
  `client.sendRequest('slk/edit')` → `fetch()` with POST

**Remove from MethodCall:**
```
BlpRender, MpqList, MpqRead, SlkRender, SlkEdit
```

### Phase 3 — Utility endpoints

Move build, rescan, graph panels, debug panel, ujapi to HTTP.

**Server side:**
- `POST /build/execute`, `GET /build/hooks`
- `POST /rescan`
- `POST /ujapi/download`
- `GET /graph/import`, `GET /graph/call`, `GET /graph/type`
- `POST /debug/log/enable`, `GET /debug/init`

**Client side:**
- `extension.js`: replace `client.sendRequest('build/execute')` → `fetch()`
- `extension.js`: replace `client.sendRequest('rescan/execute')` → `fetch()`
- `extension.js`: replace `client.sendRequest('ujapi/download')` → `fetch()`
- `importGraphPanel.js`, `callGraphPanel.js`, `typeGraphPanel.js`: → `fetch()`
- `debugSidebarProvider.js`:
  `client.sendNotification('custom/debugLogEnable')` → `fetch()` POST,
  `client.sendRequest('custom/debugInit')` → `fetch()` GET

**Remove from MethodCall:**
```
BuildExecute, BuildHooks, RescanExecute, UjapiDownload,
ImportGraphSubgraph, CallGraphSubgraph, TypeGraphSubgraph,
DebugLogEnable, DebugInit
```

### Phase 4 — Cleanup

After all custom methods moved to HTTP:

1. **`protocol.rs`**: `MethodCall` contains only language-feature variants (~27).
   All custom param structs (`DooRenderParams`, `MpqArchiveParams`,
   `SlkEditParams`, etc.) move to their respective `src/http/*.rs` modules.

2. **`main.rs`**: the dispatch match shrinks from ~1860 lines to ~800.
   Only language features + document sync remain. Readable and maintainable.

3. **`extension.js`**: `ServerClient` is only used for language features
   and document sync. All custom `sendRequest()` / `sendNotification()`
   calls for rendering, build, graphs etc. are replaced with `fetch()`.

4. **HTTP server (`server.rs`)**: all routes in one place. Auth via token
   query param (already implemented). CORS already configured.

5. **`MpqFileSystemProvider`**: no longer needs `getClient` / `clientReady` —
   it needs `getBinaryServer()` instead. `readFile()` returns raw
   `Uint8Array` from `fetch().arrayBuffer()` — no base64.

## Helper: HTTP client utility

To avoid repeating `fetch()` boilerplate, create a shared helper:

```js
// extension/httpClient.js
let _bs = null

function setBinaryServer(info) { _bs = info }

async function httpGet(path, params = {}) {
    if (!_bs) throw new Error('Binary server not ready')
    const url = new URL(`http://127.0.0.1:${_bs.port}${path}`)
    url.searchParams.set('token', _bs.token)
    for (const [k, v] of Object.entries(params)) {
        if (v != null) url.searchParams.set(k, v)
    }
    const resp = await fetch(url)
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${await resp.text()}`)
    return resp
}

async function httpPost(path, body, params = {}) {
    if (!_bs) throw new Error('Binary server not ready')
    const url = new URL(`http://127.0.0.1:${_bs.port}${path}`)
    url.searchParams.set('token', _bs.token)
    for (const [k, v] of Object.entries(params)) {
        if (v != null) url.searchParams.set(k, v)
    }
    const resp = await fetch(url, {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify(body),
    })
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${await resp.text()}`)
    return resp
}

module.exports = { setBinaryServer, httpGet, httpPost }
```

## References

- [`binary-protocol.md`](./binary-protocol.md) — binary transport format (WOBJ)
- [`terrain.md`](./terrain.md) — terrain data format
- [`src/lsp/protocol.rs`](../../src/lsp/protocol.rs) — `MethodCall` enum (wire protocol)
- [`src/main.rs`](../../src/main.rs) — dispatch loop
- [`src/http/server.rs`](../../src/http/server.rs) — HTTP server
- [`extension/extension.js`](../../extension/extension.js) — WebSocket client setup
- [`extension/serverClient.js`](../../extension/serverClient.js) — custom WebSocket transport
- [`extension/mapEditor/index.js`](../../extension/mapEditor/index.js) — map editor (partially on HTTP)
- [`extension/mpqFileSystemProvider.js`](../../extension/mpqFileSystemProvider.js) — MPQ filesystem (WebSocket)
- [`extension/mapEditor/resolveBlpEditor.js`](../../extension/mapEditor/resolveBlpEditor.js) — BLP preview (WebSocket)
- [`extension/resolveSlkEditor.js`](../../extension/resolveSlkEditor.js) — SLK editor (WebSocket)
- [`extension/importGraphPanel.js`](../../extension/importGraphPanel.js) — import graph (WebSocket)
- [`extension/callGraphPanel.js`](../../extension/callGraphPanel.js) — call graph (WebSocket)
- [`extension/typeGraphPanel.js`](../../extension/typeGraphPanel.js) — type graph (WebSocket)
- [`extension/debugSidebarProvider.js`](../../extension/debugSidebarProvider.js) — debug panel (WebSocket)
