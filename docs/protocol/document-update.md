# Document Update Protocol & Semantic Tokens

> **Status:** Implemented and working.
>
> This document describes the current architecture of the document update
> pipeline: how edits reach the server, how semantic tokens are computed and
> delivered incrementally, and how stale responses are rejected.
>
> See also: [`binary-protocol.md`](./binary-protocol.md) (planned binary
> transport), [`terrain.md`](./terrain.md) (terrain data format).

---

## Transport — HTTP only

The extension communicates with the Rust backend exclusively via HTTP.
No WebSocket, no LSP, no JSON-RPC.

### Startup handshake

1. The extension spawns the server binary as a child process.
2. The server picks a random TCP port, generates a one-time auth token,
   and prints `{"port": <n>, "token": "<s>"}` to stdout.
3. The extension reads this line and initialises `HttpTransport`
   (`http.Agent` with keep-alive on `127.0.0.1:<port>`).
4. Every HTTP request includes `?token=…` for authentication.
5. `stdin` is kept open for lifecycle only — when VS Code exits, stdin
   closes and the server shuts down.

### Request types

| Content-Type | Used for |
|---|---|
| `application/octet-stream` | `POST /document/update` — binary TLV body + binary TLV response |
| `application/json` | All other routes — JSON request / JSON or binary response |

There is no multiplexing — each HTTP request is independent and
sequential per URI (see [Serial update queue](#serial-update-queue)).

---

## Document version stamping

Stale semantic tokens / inlay hints could be applied to a newer document
version when the server's response arrived after the client had already
received another edit (race condition).

### Problem

The client tracked a per-URI "generation counter" to discard stale
responses, but the counter was tied to the **request**, not the
**document state**. If a response's `.then()` microtask was already
queued before the next abort took effect, or the HTTP abort didn't
cancel the connection fast enough, tokens computed for document version N
could overwrite the locally-adjusted cache for version N+1.

### Solution — version echo

1. The client maintains a **monotonic version counter** per URI
   (`_docVersion`), incremented on every `didOpen` and every
   `didChange`.
2. The version is sent as the `version` query parameter in
   `POST /document/update`.
3. The server echoes it back as the **first 4 bytes** (`u32 LE`) of the
   response body, before any TLV sections.
4. The client reads the echoed version and **discards the response** if
   it no longer matches `_docVersion.get(uri)`.

### Wire format

**Request:**
```
POST /document/update?token=…&uri=…&languageId=jass&version=42
```

**Response body:**
```
[u32 LE version][TLV sections…]
```

---

## Serial update queue

`req.destroy()` (HTTP abort) was the root cause of all document
desynchronisation bugs: the client could never know whether the server
had received the aborted request's edits.

### Root cause

`AbortController.abort()` calls `req.destroy()` on the HTTP request.
Three outcomes are possible, and the client cannot distinguish them:

1. Body never reached the server → server at version N
2. Body reached, edit applied → server at version N+1
3. Body partially reached → server rejected (truncated) → version N

### Solution — never abort, serial queue

The client maintains a **per-URI serial queue**. Only ONE
`/document/update` request is in flight per URI at any time. Edits that
arrive while a request is running are **accumulated** as TLV sections
and sent as a **single batch** when the running request completes.

```
Edit A → send immediately (no request in flight)
Edit B → in-flight → accumulate
Edit C → in-flight → accumulate
Response for A arrives → send [B, C] as one batch
```

**Guarantees:**

- All edits reach the server **in order** (no abort → no lost edits)
- Only **one parse** per batch (edits B+C share a single parse)
- **Version echo** still discards stale responses
- No `AbortController`, no `req.destroy()`, no uncertainty

### Implementation

| Component | Role |
|---|---|
| `_sending: Map<string, boolean>` | Per-URI "request in flight" flag |
| `_pending: Map<string, {version, languageId, sections}>` | Accumulated TLV sections |
| `_enqueueUpdate()` | If sending → accumulate; otherwise → send |
| `_flushQueue()` | Send request; on `.finally()` drain pending |

---

## Server-side file reading on open (`SECTION_OPEN_URI`)

For `file://` scheme documents, the server reads the file from disk
itself — the client no longer sends the full document text over the wire.
This eliminates sending 40 MB mono-files over HTTP on first open.

A new TLV section type `0x12` (OPEN_URI) with **zero-length payload**.
The URI is already in the query parameters. When the server receives
this section, it reads the file from disk via `tokio::fs::read_to_string`
and calls `init()`.

`SECTION_FULL_TEXT` (0x10) is preserved for `mpq://` scheme files where
the server cannot access the file directly.

### Wire format

```
POST /document/update?…&languageId=jass&version=1
Body: [0x12][u32 LE = 0]   (5 bytes total — section type + zero length)
```

---

## Incremental tree-sitter parsing — reverted

Tree-sitter's incremental parsing (`parser.parse(text, Some(old_tree))`)
was implemented and then **reverted**. Tree-sitter reuses subtrees based
purely on byte-range overlap — it does **not** re-evaluate context. For
example, inserting `//` at the start of a line does not cause tree-sitter
to re-parse tokens on that line, producing stale / incorrect AST nodes.

Full reparse (`parser.parse(text, None)`) is the only way to guarantee
correctness.

---

## Incremental semantic token responses (token-aware delta)

For a 40k-line file, the full semantic token array is ~4 MB of `u32`
values. Previously this was re-sent in every `/document/update`
response, even when a single-character edit changed only a handful of
tokens near the cursor. Now the server computes a **token-aware diff**
and sends a compact stream of COPY/SKIP/INSERT commands
(`SECTION_SEMANTIC_EDIT`, type `0x03`) — typically a few hundred bytes
instead of megabytes.

### Problem

Every response to `/document/update` included the complete
`SECTION_SEMANTIC` (0x01) with all tokens. For a 40k-line JASS file:

- ~200k tokens × 5 u32 values × 4 bytes = **~4 MB per keystroke**
- At 5 keystrokes/second = 20 MB/s of token data, most of it identical

### Solution — result ID + token-aware delta

**Result ID tracking:**

1. Server assigns a monotonic `resultId` (u32) to every semantic
   response (full or delta).
2. The `resultId` is the **first 4 bytes** of the `SECTION_SEMANTIC` and
   `SECTION_SEMANTIC_EDIT` data payloads.
3. Client stores the `resultId` and sends it back as `lastResultId`
   query parameter in the next `/document/update` request.
4. If `lastResultId` matches the server's stored ID → compute delta.
   If not (first open, discarded response, etc.) → send full.

**Token-aware delta format:**

The diff is computed by the unified `diff` module (`src/http/diff.rs`).
Both old and new delta-encoded arrays are converted to absolute `Item`s
(`Pos` + `Payload::Semantic`), diffed, then the result is encoded back
to a COPY/SKIP/INSERT stream.

The comparison uses a **hybrid strategy**:

- **Prefix**: matched by **absolute** position + payload (so a
  line-shift edit stops the prefix exactly at the edit point).
- **Suffix**: matched by **delta-key** — position delta to predecessor
  plus payload equality.  This guarantees `COPY` is always safe
  without boundary fixup: the delta-key match ensures that the old
  array's stored delta produces the correct absolute position relative
  to any output predecessor.

The sentinel value `0xFFFFFFFF` marks command tuples — it can never
appear as a valid `deltaLine` in real data.

Each 5-u32 tuple in the diff stream is one of:

| Tuple | Meaning |
|-------|---------|
| `[deltaLine, deltaChar, len, type, mods]` | Insert this token into result |
| `[0xFFFFFFFF, 0, count, 0, 0]` | **COPY** — copy `count` tokens from old array |
| `[0xFFFFFFFF, 1, count, 0, 0]` | **SKIP** — skip `count` tokens in old (delete) |

**Skip if unchanged:**

If all tokens are identical, the diff stream is empty → no semantic
section is included in the response at all — zero bytes.

### Wire format

**Request (new query param):**
```
POST /document/update?…&version=42&lastResultId=7
```

**Response — full (type 0x01):**
```
[u8 type=0x01][u32 LE byte_len][u32 resultId][u32… tokens]
```

**Response — token-aware delta (type 0x03):**
```
[u8 type=0x03][u32 LE byte_len][u32 resultId][5×u32 tuples…]
```

**Response — unchanged:**

No semantic section in response. The client keeps its cached data.

### Client-side dual cache

The client maintains two semantic caches per URI:

| Cache | Purpose |
|-------|---------|
| `semanticBase` | Last server-sent data (unadjusted) — base for applying deltas |
| `semanticCache` | What VS Code reads — may be locally shifted by `_adjustSemanticTokens` |

On server response (full or delta): both caches are set to the same
new data. On local edit: only `semanticCache` is adjusted (for instant
visual feedback). Deltas are always applied to `semanticBase`, not the
locally-adjusted `semanticCache`.

### Stale response handling

When a response's echoed version no longer matches `_docVersion`, the
response is **stale** — its tokens correspond to an older document state
and must not be displayed.

However, the delta-tracking state (`semanticBase`, `semanticResultId`)
is **always** updated from every response, stale or fresh. This keeps
the client's delta base in sync with the server's `SEMANTIC_LAST`, so
the **next** request can send `lastResultId` and receive a compact delta
instead of a full 4+ MB token array.

| Response | `semanticBase` | `semanticResultId` | `semanticCache` | `semanticChanged` |
|---|---|---|---|---|
| Fresh (version match) | ✅ updated | ✅ updated | ✅ updated | ✅ fired |
| Stale (version mismatch) | ✅ updated | ✅ updated | — kept as-is | — not fired |

During fast typing, most responses are stale (the user has already typed
further). Without this strategy, `semanticResultId` would be cleared on
every stale response, forcing the server to send full tokens every time
— megabytes per keystroke on a 40k-line file.

---

## Module layout

### Diff infrastructure (`src/http/diff.rs`)

Unified diff module for all positioned document items. Provides:

- `Pos` — absolute `(line, character)` position.
- `Payload` — enum: `Semantic(len, type, mods)` | `Hint { kind, label }`.
- `Item` — `Pos` + `Payload`, the unit of diffing.
- `Item::from_semantic_u32` / `Item::to_semantic_u32` — convert between
  delta-encoded `u32` arrays and absolute `Item`s.
- `Item::from_hints` / `Item::to_hints` — convert `InlayHint` arrays.
- `Diff::compute(old, new)` — prefix by absolute position, suffix by
  delta-key (position delta to predecessor + payload equality).
- `Diff::encode_semantic` — emit COPY/SKIP/INSERT wire stream.
- `semantic_diff(old_u32, new_u32)` — convenience one-liner.

### Semantic tokens (`src/http/semantic/`)

Token types (`Kind`) and modifiers (`Mod`), the token collector
(`Hub`), and the delta-encoding logic (`Hub::data()`). `Hub::data()`
returns `Vec<u32>` (matches the wire format — `Uint32Array` on JS side).

### Inlay hints (`src/http/inlay_hint.rs`)

The inlay hint module lives in `src/http/inlay_hint.rs`. Hints are
pushed as part of the `/document/update` response alongside semantic
tokens.

Hint types are configured via the `//set hint` directive:

```
//set hint ref type
```

Available tags: `ref` (reference-ID debug hints), `type` (type-annotation
hints).  Without the directive, only ujapi version hints are generated.

The `hints` query parameter controls whether the server includes hints
in the response at all:

```
POST /document/update?…&hints=1     ← include hints (file settings decide which)
POST /document/update?…&hints=      ← skip hints (0 bytes)
```

Binary format per hint:

```
[u32 line][u32 char][u8 kind][u16 label_len][…label UTF-8…]
```

`padding_left` / `padding_right` are hardcoded on the JS side.

---

## HTTP API

All communication is HTTP. Routes are served by axum on `127.0.0.1`.

### HTTP routes — rendering

| Route |
|-------|
| `POST /render/blp` |
| `POST /render/mdx` |
| `POST /render/doo` |
| `POST /render/w3i` |
| `POST /render/w3e` |
| `POST /render/w3obj` |
| `POST /render/slk` |

### HTTP routes — data

| Route |
|-------|
| `POST /slk/edit` |
| `POST /w3e/catalog/terrain` |
| `POST /w3e/catalog/doodads` |
| `POST /w3e/catalog/units` |
| `POST /w3e/catalog/destructables` |
| `POST /w3e/lookupFile` |
| `POST /mpq/info` |
| `POST /mpq/list` |
| `POST /mpq/read` |

### HTTP routes — graphs, build, utility

| Route |
|-------|
| `POST /graph/import` |
| `POST /graph/call` |
| `POST /graph/type` |
| `POST /build/execute` |
| `POST /build/hooks` |
| `POST /rescan` |
| `POST /ujapi/download` |

### HTTP routes — language features

| Route |
|-------|
| `POST /lsp/completion` |
| `POST /lsp/hover` |
| `POST /lsp/highlight` |
| `POST /lsp/definition` |
| `POST /lsp/references` |
| `POST /lsp/formatting` |
| `POST /lsp/prepareRename` |
| `POST /lsp/rename` |
| `POST /lsp/willRenameFiles` |
| `POST /lsp/colorPresentation` |
| `POST /lsp/codeAction` |
| `POST /lsp/signatureHelp` |
| `POST /lsp/codeLens` |
| `POST /lsp/callHierarchy/prepare` |
| `POST /lsp/callHierarchy/incoming` |
| `POST /lsp/callHierarchy/outgoing` |
| `POST /lsp/typeHierarchy/prepare` |
| `POST /lsp/typeHierarchy/supertypes` |
| `POST /lsp/typeHierarchy/subtypes` |

---

## References

- [`binary-protocol.md`](./binary-protocol.md) — binary transport format (WOBJ)
- [`terrain.md`](./terrain.md) — terrain data format
- [`src/http/diff.rs`](../../src/http/diff.rs) — unified diff infrastructure (Pos, Payload, Item, Diff)
- [`src/http/semantic/`](../../src/http/semantic/) — semantic token types (`token.rs`) and delta-encoder (`hub.rs`)
- [`src/http/api.rs`](../../src/http/api.rs) — HTTP route handlers
- [`src/http/server.rs`](../../src/http/server.rs) — HTTP server
- [`extension/extension.js`](../../extension/extension.js) — document update pipeline, serial queue, semantic cache
- [`extension/httpTransport.js`](../../extension/httpTransport.js) — HTTP transport layer (keep-alive agent)
- [`extension/serverClient.js`](../../extension/serverClient.js) — server process lifecycle

