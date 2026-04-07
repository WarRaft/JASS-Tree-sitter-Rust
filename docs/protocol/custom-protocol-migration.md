# Protocol Migration — TODO

> **Status:** Server-side migration complete. Client-side migration TODO.
>
> Current architecture is documented in [`document-update.md`](./document-update.md).
> Binary protocol design is in [`binary-protocol.md`](./binary-protocol.md).

---

## Problem

The extension uses a custom `ServerClient` (WebSocket transport) for **everything** —
language features and completely unrelated binary-format tasks: BLP textures,
MDX models, SLK catalogs, MPQ archives, map editor, build system, graph panels,
debug panel.

### What the server is forced to do

#### 1. Everything through JSON

Binary data (BLP textures, MDX models, MPQ file contents, terrain data)
must be base64-encoded, wrapped in a JSON object, and decoded on the client.
For a 1 MB terrain snapshot → ~1.33 MB JSON payload + `JSON.parse()`.

#### 2. Webview relay through extension host

Webview (map editor) cannot talk directly to the server — only through
the extension host:

```
Webview
  → postMessage() → extension host
    → client.sendRequest() → WebSocket → Rust server
      → process → JSON result → WebSocket → extension host
        → webviewPanel.webview.postMessage() → Webview
```

Four hops instead of one `fetch()`. Each hop = JSON serialize/deserialize.

#### 3. Cancel overhead for heavy requests

Opening the map editor sends several heavy requests sequentially.
If user closes the tab before completion — client sends cancel for each,
but the server already spent resources parsing. Meanwhile `didChange`,
`semanticTokens`, `diagnostic` from the text editor compete for the
same WebSocket connection.

### What already works over HTTP

A parallel axum HTTP server is running. All server-side HTTP routes are
ready (see [`document-update.md`](./document-update.md) § HTTP API). But the client
(extension JS) still sends all requests via WebSocket
`client.sendRequest()`.

---

## Audit: every client.sendRequest / client.sendNotification call

### extension/extension.js

| Call | Used by |
|------|---------|
| `client.sendRequest('rescan/execute', {uri})` | `rescan.execute` command |
| `client.sendRequest('build/hooks', {uri})` | `build.execute` command |
| `client.sendRequest('build/execute', {uri})` | `build.execute` command |
| `client.sendRequest('ujapi/download', {uri, path})` | `ujapi.download` command |

### extension/mapEditor/index.js

| Call | Used by |
|------|---------|
| `client.sendRequest('mpq/info', {archivePath})` | Archive opening |
| `client.sendRequest('w3e/render', {uri, archivePath?})` | Terrain loading |
| `client.sendRequest('w3i/render', {uri, archivePath?})` | Map info loading |
| `client.sendRequest('doo/render', {uri, isUnit, archivePath?})` | Doodad/unit placement |
| `client.sendRequest('mdx/render', {uri})` | Model rendering |
| `client.sendRequest('w3e/lookupFile', {path, archivePath?})` | File lookup (fallback) |
| `client.sendRequest('w3e/gamePath/set', {gamePath})` | Game path (fallback) |
| `client.sendRequest('w3e/gamePath/status', {})` | Game path (fallback) |

### extension/mapEditor/resolveBlpEditor.js

| Call | Used by |
|------|---------|
| `client.sendRequest('blp/render', {uri})` | BLP preview editor |

### extension/resolveSlkEditor.js

| Call | Used by |
|------|---------|
| `client.sendRequest('slk/render', {uri})` | SLK table editor |
| `client.sendRequest('slk/edit', {uri, start, len, value})` | SLK cell edit |

### extension/importGraphPanel.js

| Call | Used by |
|------|---------|
| `client.sendRequest('importGraph/subgraph', {uri})` | Import graph panel |

### extension/callGraphPanel.js

| Call | Used by |
|------|---------|
| `client.sendRequest('callGraph/subgraph', {uri})` | Call graph panel |

### extension/typeGraphPanel.js

| Call | Used by |
|------|---------|
| `client.sendRequest('typeGraph/subgraph', {uri})` | Type graph panel |

### extension/debugSidebarProvider.js

| Call | Used by |
|------|---------|
| `client.sendNotification('custom/debugLogEnable', {enabled})` | Debug log toggle |
| `client.sendRequest('custom/debugInit', {})` | Debug init data |

### extension/mpqFileSystemProvider.js

| Call | Used by |
|------|---------|
| `client.sendRequest('mpq/list', {archivePath})` | Directory listing |
| `client.sendRequest('mpq/read', {archivePath, filePath})` | File reading |

---

## TODO: Phase 2 — Client-side migration

Each file below needs to be migrated from WebSocket `client.sendRequest()`
to `fetch()`.

### extension/serverClient.js

- [ ] Remove `sendRequest()` method entirely — no more WebSocket request/response
- [ ] Keep `sendNotification()` for document sync only
- [ ] Remove request ID tracking, pending requests map, cancel logic
- [ ] WebSocket message format: send `{method, params}` without `jsonrpc`/`id`

### extension/extension.js

- [ ] `rescan.execute` command: → `httpPost('/rescan', {uri})`
- [ ] `build.execute` command: → `httpPost('/build/hooks')` / `httpPost('/build/execute')`
- [ ] `ujapi.download` command: → `httpPost('/ujapi/download')`
- [ ] All language feature handlers (completion, hover, definition, etc.): → `httpPost('/lsp/...')`
- [ ] Remove WebSocket handlers for `textDocument/*` responses

### extension/mapEditor/index.js

- [ ] `mpq/info` → `httpPost('/mpq/info', {archivePath})`
- [ ] `w3e/render` → `httpPost('/render/w3e', {uri, archivePath})`
- [ ] `w3i/render` → `httpPost('/render/w3i', {uri, archivePath})`
- [ ] `doo/render` → `httpPost('/render/doo', {uri, isUnit, archivePath})`
- [ ] `mdx/render` → `httpPost('/render/mdx', {uri})`
- [ ] Remove all WebSocket fallback code for `w3e/lookupFile`, `w3e/gamePath/*`
- [ ] Map editor webview: call `fetch()` directly (no relay through extension host)

### extension/mapEditor/resolveBlpEditor.js

- [ ] `blp/render` → `httpPost('/render/blp', {uri})`

### extension/resolveSlkEditor.js

- [ ] `slk/render` → `httpPost('/render/slk', {uri})`
- [ ] `slk/edit` → `httpPost('/slk/edit', {uri, start, len, value})`

### extension/mpqFileSystemProvider.js

- [ ] `mpq/list` → `httpPost('/mpq/list', {archivePath})`
- [ ] `mpq/read` → `httpPost('/mpq/read', {archivePath, filePath})`
- [ ] For `mpq/read`: response is JSON `{content, size}` — later switch to raw `application/octet-stream`

### extension/importGraphPanel.js

- [ ] `importGraph/subgraph` → `httpPost('/graph/import', {uri})`

### extension/callGraphPanel.js

- [ ] `callGraph/subgraph` → `httpPost('/graph/call', {uri})`

### extension/typeGraphPanel.js

- [ ] `typeGraph/subgraph` → `httpPost('/graph/type', {uri})`

### extension/debugSidebarProvider.js

- [ ] `custom/debugLogEnable` → `httpPost('/debug/log/enable', {enabled})`
- [ ] `custom/debugInit` → `httpPost('/debug/init', {})`
- [ ] Note: debug endpoints not yet created on server — add `POST /debug/log/enable` and `GET /debug/init` routes

### Helper: create extension/httpClient.js

- [ ] Create shared `httpGet(path, params)` and `httpPost(path, body)` helper
- [ ] Auto-inject `token` query parameter from binary server info
- [ ] Error handling with proper status codes

---

## TODO: Phase 3 — Cleanup

After client migration:

- [ ] Remove `ServerClient.sendRequest()` / `ServerClient.sendNotification()` (except document sync)
- [ ] Remove `lsp/cancel.rs` module (no more request IDs or cancellation)
- [ ] Remove dead `send()` modules from `lsp/*/send.rs` (only `compute()` needed)
- [ ] Clean up unused warnings in `lsp/` modules
- [ ] Consider removing `lsp/` folder name — rename to `lang` or merge into `http/`
- [ ] Switch `mpq/read` to `application/octet-stream` (no more base64)
- [ ] Direct webview→server fetch (no extension host relay)

---

## TODO: Server-side parse cancellation

With the serial queue, the first request's parse runs to completion
before the second batch is sent. For fast typing this means ~2× latency
compared to the ideal. A future optimisation: add a lightweight
`POST /document/cancel?uri=…` endpoint so the client can cancel only the
**parse** (not the edit application) while the request is still in
flight. The `.finally()` callback would then fire sooner, draining the
queue faster.

---

## References

- [`document-update.md`](./document-update.md) — current architecture (completed phases)
- [`binary-protocol.md`](./binary-protocol.md) — binary transport format (WOBJ) and WebSocket binary framing
- [`terrain.md`](./terrain.md) — terrain data format
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
