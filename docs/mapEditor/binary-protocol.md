# Columnar Binary Protocol — Map Editor Data Transport

> **Status:** Design document (not yet implemented)
>
> Rust writer: `src/util/columnar.rs` (planned) — JS reader: `extension/vendor/columnar.js` (planned)
>
> This protocol replaces `serde_json` serialisation for catalog data (doodads, units, destructables, etc.)
> and the LSP JSON-RPC transport for map editor webview communication.
>
> **Goal:** zero-copy TypedArray access on the JS side, ~50–100× faster than `JSON.parse()`,
> ~3–5× smaller payloads, works identically in VS Code webview and browser.

## Motivation: browser-ready architecture

The map editor currently runs as a VS Code webview backed by an LSP server.
However, the **long-term goal is a standalone browser version** — the same
editor UI running in a browser tab, with the Rust backend compiled to WASM
or served remotely.

This means **everything that is not strictly LSP** (language features: diagnostics,
completions, go-to-definition, hover) must be decoupled from the LSP transport
and moved to the HTTP binary server. The map editor, catalog browsers, terrain
renderer, model viewers — all of these should communicate through the same
HTTP/binary endpoints that will later work identically in a browser context.

### Current architecture (LSP-coupled)

```
VS Code extension
  └─ LSP client (JSON-RPC over stdin/stdout)
       └─ custom/w3eRender, custom/w3eTerrainSlk, …
            → Rust LSP handler
              → serde_json::to_value(data)
                → JSON response over LSP
```

Problems:
- LSP is stdin/stdout — **unavailable in a browser**.
- JSON-RPC adds framing overhead (Content-Length headers, JSON wrapping).
- All data serialised as JSON — slow parsing, large payloads.
- Webview cannot `fetch()` from LSP — must go through the extension host
  which relays messages (extra IPC hop).

### Target architecture (HTTP-first)

```
Browser / VS Code webview
  └─ fetch('http://127.0.0.1:{port}/w3e/catalog/doodads')
       → Rust HTTP server (axum)
         → ColumnarWriter::finish() → Vec<u8>
           → application/octet-stream response
             → WobjReader(arrayBuffer) on JS side
               → zero-copy TypedArray views
```

Benefits:
- **Same code path** for VS Code webview and browser.
- **No LSP dependency** for map editor features.
- **Binary transport** — no JSON overhead, TypedArray access.
- **Cacheable** — HTTP responses can be cached, ETags, etc.
- **Parallelisable** — multiple fetch() calls in parallel, unlike sequential LSP.

The LSP channel remains **only** for language features (JASS/vJASS/Zinc diagnostics,
completions, etc.) which are tightly coupled to the editor protocol.

## Why not standard solutions?

### JSON (current)

The current `snapshot.rs` serialises everything with `serde_json::to_vec()` → sends as `application/json`.
JS calls `JSON.parse()` which allocates a new JS object for every row, every field, every string.

For 500 doodads × 30 fields = **15,000 JS object property allocations** plus string decoding.
The `_doodadsSlk` block alone is ~400KB of JSON. Total snapshot is >1MB.

`JSON.parse()` is single-threaded, blocks the main thread, and produces GC pressure from
thousands of small allocations that all become long-lived (cached for the session).

### Protocol Buffers (protobuf)

Protobuf uses **varint encoding** for integers and **length-delimited fields** with tag bytes.
A `uint32` field is encoded as 1–5 bytes depending on value, preceded by a field tag.

This means you **cannot** wrap a protobuf message in a `Uint32Array`:

```js
// IMPOSSIBLE with protobuf:
const hp = new Uint32Array(buffer, offset, rowCount);
// hp[i] gives you the i-th unit's HP with zero parsing

// With protobuf you must:
const msg = MyProto.decode(new Uint8Array(buffer));
// This allocates msg.units[0].hp, msg.units[1].hp, ... as JS numbers
// = same allocation storm as JSON, just with smaller wire format
```

Protobuf optimises **wire size**, not **access speed**. We need access speed.

### FlatBuffers / Cap'n Proto

These are closer — they support zero-copy reads. But:

1. **No columnar layout.** Data is stored row-by-row (each object is contiguous).
   You cannot get a `Float64Array` view over all `defScale` values — they are interleaved
   with other fields of each row. For rendering/filtering operations that touch one column
   across all rows, this means scattered memory access.

2. **Vtable overhead.** Each object has a vtable pointer (2–4 bytes) and each field has an
   offset entry in the vtable. For 500 doodads × 30 fields, that's 15,000 vtable entries
   (30KB) that serve no purpose when you always read all fields.

3. **String access.** FlatBuffers strings are length-prefixed inline blobs. Each string read
   calls `new TextDecoder().decode()` on a sub-view. No string deduplication — if 200 doodads
   share `category = "Trees"`, the string `"Trees"` appears 200 times.

4. **Schema dependency.** Requires `.fbs` schema files, a code generator, and generated JS code
   (100KB+ for complex schemas). Our goal is a 2KB reader.

5. **Alignment requirements.** FlatBuffers requires 4-byte alignment for all offsets and pads
   aggressively. For `bool` and `u8` columns this wastes 75% of space.

### MessagePack / CBOR

Same problem as JSON — they produce JS objects, just with a more compact wire format.
`msgpack.decode()` allocates the same object graph as `JSON.parse()`.

### Apache Arrow IPC

Arrow is the gold standard for columnar binary data, but:

1. **Massive JS dependency.** `apache-arrow` npm package is 400KB+ minified.
2. **Complex metadata.** Arrow uses FlatBuffers for its own schema metadata.
3. **Designed for analytics.** Features like dictionary encoding, null bitmaps, nested types
   add complexity we don't need. Our data has no nulls (we use defaults) and no nesting.

### Our requirements

| Requirement | JSON | Protobuf | FlatBuffers | Arrow | **Ours** |
|-------------|:----:|:--------:|:-----------:|:-----:|:--------:|
| `new Uint32Array(buf, off, N)` for numeric columns | ❌ | ❌ | ❌ | ✅ | ✅ |
| `new Float64Array(buf, off, N)` for float columns | ❌ | ❌ | ❌ | ✅ | ✅ |
| No JS object allocation for bulk data | ❌ | ❌ | ✅ | ✅ | ✅ |
| Lazy string decoding (only when accessed) | ❌ | ❌ | ✅ | ✅ | ✅ |
| String deduplication | ❌ | ❌ | ❌ | ✅ | ✅ |
| Reader < 3KB JS | N/A | ❌ | ❌ | ❌ | ✅ |
| No build-time code generation | ✅ | ❌ | ❌ | ✅ | ✅ |
| Works in browser + VS Code webview | ✅ | ⚠️ | ⚠️ | ⚠️ | ✅ |

## Format Overview

All multi-byte integers are **little-endian** (matches JS TypedArrays on all modern platforms).

```
┌─────────────────────────────────────────────────┐
│  File Header (24 bytes)                         │
├─────────────────────────────────────────────────┤
│  Column Directory (7 × columnCount bytes)       │
├─────────────────────────────────────────────────┤
│  Column Data Blocks (aligned to 8 bytes each)   │
│    ┌── u32 column ──────────────────────────┐   │
│    │  [val₀, val₁, val₂, …] as raw LE u32  │   │
│    └────────────────────────────────────────┘   │
│    ┌── f64 column ──────────────────────────┐   │
│    │  [val₀, val₁, val₂, …] as raw LE f64  │   │
│    └────────────────────────────────────────┘   │
│    ┌── bool column ─────────────────────────┐   │
│    │  [0, 1, 1, 0, …] as u8                │   │
│    └────────────────────────────────────────┘   │
│    ┌── str column ──────────────────────────┐   │
│    │  [off₀, off₁, off₂, …] as u32         │   │
│    │  (indices into string table)            │   │
│    └────────────────────────────────────────┘   │
├─────────────────────────────────────────────────┤
│  String Table                                   │
│  UTF-8 bytes, null-terminated entries           │
│  "APms\0Mushrooms\0Trees\0…"                    │
└─────────────────────────────────────────────────┘
```

## File Header (24 bytes)

| Offset | Type   | Name             | Description |
|--------|--------|------------------|-------------|
| 0      | `u32`  | magic            | `0x574F424A` = `"WOBJ"` (Warcraft OBJect) |
| 4      | `u16`  | version          | Format version (currently `1`) |
| 6      | `u16`  | domain           | Domain identifier (see table below) |
| 8      | `u32`  | rowCount         | Number of rows (entries) |
| 12     | `u16`  | columnCount      | Number of columns |
| 14     | `u16`  | _reserved        | Must be `0` |
| 16     | `u32`  | stringTableOffset| Byte offset of string table from buffer start |
| 20     | `u32`  | stringTableSize  | Size of string table in bytes |

### Domain identifiers

| Value | Domain       | war3map file | SLK source |
|-------|--------------|-------------|-------------|
| 0     | Doodad       | `war3map.w3d` | `Doodads\Doodads.slk` |
| 1     | Unit         | `war3map.w3u` | `Units\UnitData.slk` + Balance + UI + Weapons |
| 2     | Destructable | `war3map.w3b` | `Units\DestructableData.slk` |
| 3     | Item         | `war3map.w3t` | (planned) |
| 4     | Ability      | `war3map.w3a` | (planned) |
| 5     | Buff         | `war3map.w3h` | (planned) |
| 6     | Upgrade      | `war3map.w3q` | (planned) |
| 7     | Terrain      | —           | `TerrainArt\Terrain.slk` |
| 8     | CliffTypes   | —           | `TerrainArt\CliffTypes.slk` |
| 9     | Water        | —           | `TerrainArt\Water.slk` |
| 10    | Snapshot     | —           | All domains combined |

## Column Directory

Immediately after the header. Each entry is **7 bytes**:

| Offset | Type  | Name       | Description |
|--------|-------|------------|-------------|
| 0      | `u16` | nameOffset | Byte offset into string table for this column's name |
| 2      | `u8`  | dtype      | Data type (see table below) |
| 3      | `u32` | dataOffset | Byte offset of this column's data block from buffer start |

### Data types (`dtype`)

| Value | Name        | Size per row | JS TypedArray        | Notes |
|-------|-------------|:------------:|----------------------|-------|
| 0     | `u8`        | 1            | `Uint8Array`         | |
| 1     | `u16`       | 2            | `Uint16Array`        | |
| 2     | `u32`       | 4            | `Uint32Array`        | Includes rawcode keys |
| 3     | `i32`       | 4            | `Int32Array`         | |
| 4     | `f32`       | 4            | `Float32Array`       | |
| 5     | `f64`       | 8            | `Float64Array`       | |
| 6     | `bool8`     | 1            | `Uint8Array`         | `0` = false, `1` = true |
| 7     | `str`       | 4            | `Uint32Array`        | Each value is a byte offset into the string table |
| 8     | `rgb`       | 3            | `Uint8Array` (×3)    | R, G, B packed contiguously per row |
| 9     | `rgba`      | 4            | `Uint8Array` (×4)    | R, G, B, A packed contiguously per row |
| 10    | `bitflags8` | 1            | `Uint8Array`         | Up to 8 booleans packed into one byte |
| 11    | `rawcode`   | 4            | `Uint32Array`        | 4-char rawcode as LE u32 (same as `u32`, semantic hint) |

## Column Data Blocks

Each column's data block starts at `dataOffset` (from the column directory entry).

**Alignment:** every data block is aligned to **8 bytes** from the buffer start.
This ensures `Float64Array` views work without copying. Padding bytes between
blocks are filled with `0x00`.

**Layout:** values are packed contiguously, one per row, in row order:

```
dataOffset + 0 * sizeof(T) → row 0
dataOffset + 1 * sizeof(T) → row 1
dataOffset + 2 * sizeof(T) → row 2
…
dataOffset + (rowCount - 1) * sizeof(T) → row (rowCount - 1)
```

**JS access** (zero-copy):

```js
// Example: reading a u32 column
const col = directory[i];
const arr = new Uint32Array(buffer, col.dataOffset, rowCount);
// arr[row] gives the value — no parsing, no allocation

// Example: reading a f64 column
const arr = new Float64Array(buffer, col.dataOffset, rowCount);
// arr[row] is a native JS number — zero overhead

// Example: reading a bool8 column
const arr = new Uint8Array(buffer, col.dataOffset, rowCount);
// arr[row] === 1 means true

// Example: reading a str column
const offsets = new Uint32Array(buffer, col.dataOffset, rowCount);
// offsets[row] is a byte offset into the string table
// Decode lazily on demand:
function getString(row) {
    const off = offsets[row];
    let end = off;
    while (strBytes[end] !== 0) end++;
    return textDecoder.decode(strBytes.subarray(off, end));
}
```

## String Table

Located at `stringTableOffset`, size `stringTableSize` bytes.

Strings are stored as UTF-8, each terminated by a null byte (`0x00`).
The first entry (offset 0) is always the empty string (`""` = single `0x00` byte).

**Deduplication:** identical strings share the same offset. If 200 doodads all have
`category = "Trees"`, the string `"Trees"` appears once in the table, and all 200
rows point to the same offset.

**Lazy decoding:** strings are only decoded when accessed. The JS reader stores
the raw `Uint8Array` of the string table and calls `TextDecoder.decode()` per
individual string on demand. For a table with 500 rows × 10 string columns = 5000
potential strings, typically only ~50 are displayed at once (one screen of a list).

### String table layout

```
Offset  Bytes               Decoded
0       00                  "" (empty)
1       41 50 6D 73 00      "APms"
6       4D 75 73 68 72 6F   "Mushrooms"
        6F 6D 73 00
16      54 72 65 65 73 00   "Trees"
…
```

## Bitflags Columns (`dtype = 10`)

For structs with many boolean fields (doodads have 12+ bools), packing them into
bitflags saves space and allows efficient bulk operations:

```rust
// Rust packing: doodad booleans
let mut flags: u8 = 0;
if d.tileset_specific   { flags |= 1 << 0; }
if d.can_place_rand_scale { flags |= 1 << 1; }
if d.use_click_helper   { flags |= 1 << 2; }
if d.ignore_model_click { flags |= 1 << 3; }
if d.walkable           { flags |= 1 << 4; }
if d.on_cliffs          { flags |= 1 << 5; }
if d.on_water           { flags |= 1 << 6; }
if d.floats             { flags |= 1 << 7; }
```

```js
// JS reading:
const flags1 = new Uint8Array(buffer, col.dataOffset, rowCount);
const isWalkable = (row) => (flags1[row] & (1 << 4)) !== 0;
```

A single doodad has ~12 bools → 2 bitflags columns (`flags1`, `flags2`) instead of
12 separate `bool8` columns. Saves 10 bytes per row × 500 rows = 5KB, and reduces
column count (fewer directory entries, less overhead).

## Packed Color Columns (`dtype = 8, 9`)

Colors are stored as 3 (RGB) or 4 (RGBA) contiguous bytes per row:

```
Row 0: R₀ G₀ B₀
Row 1: R₁ G₁ B₁
…
```

JS access:

```js
const colors = new Uint8Array(buffer, col.dataOffset, rowCount * 3);
const r = colors[row * 3 + 0];
const g = colors[row * 3 + 1];
const b = colors[row * 3 + 2];
```

## Snapshot Domain (`domain = 10`)

The snapshot combines multiple domains into a single buffer. It uses a
**section directory** in the header area:

```
┌─ Snapshot Header (24 bytes) ────────────────────┐
│  magic = "WOBJ", domain = 10                    │
│  rowCount = number of sections                  │
│  columnCount = 0 (no global columns)            │
│  stringTableOffset, stringTableSize             │
├─────────────────────────────────────────────────┤
│  Section Directory (12 × sectionCount)          │
│  per section:                                   │
│    domain: u16                                  │
│    _pad:   u16                                  │
│    offset: u32  (from buffer start)             │
│    size:   u32  (byte size of this domain)      │
├─────────────────────────────────────────────────┤
│  Section 0: Doodad WOBJ blob                    │
├─────────────────────────────────────────────────┤
│  Section 1: Unit WOBJ blob                      │
├─────────────────────────────────────────────────┤
│  Section 2: Destructable WOBJ blob              │
├─────────────────────────────────────────────────┤
│  …                                              │
└─────────────────────────────────────────────────┘
```

Each section is a complete standalone WOBJ buffer (with its own header, column
directory, data blocks, and string table). The snapshot header just wraps them
together so they can be fetched in one HTTP request.

The shared string table in the snapshot header contains section-level metadata
strings (e.g. `"source"` = `"War3Patch.mpq"`) — not the per-row strings, which
live in each section's own string table.

## HTTP Endpoints

| Endpoint | Content-Type | Description |
|----------|-------------|-------------|
| `GET /w3e/terrain` | `application/octet-stream` | Terrain points (existing, unchanged) |
| `GET /w3e/catalog/doodads` | `application/octet-stream` | WOBJ domain=0 |
| `GET /w3e/catalog/units` | `application/octet-stream` | WOBJ domain=1 |
| `GET /w3e/catalog/destructables` | `application/octet-stream` | WOBJ domain=2 |
| `GET /w3e/catalog/items` | `application/octet-stream` | WOBJ domain=3 |
| `GET /w3e/catalog/abilities` | `application/octet-stream` | WOBJ domain=4 |
| `GET /w3e/catalog/buffs` | `application/octet-stream` | WOBJ domain=5 |
| `GET /w3e/catalog/upgrades` | `application/octet-stream` | WOBJ domain=6 |
| `GET /w3e/snapshot` | `application/octet-stream` | WOBJ domain=10 (all combined) |

All endpoints require `?token=...` authentication. Optional `?archive=...` for
MPQ-contained maps.

The `/w3e/snapshot` endpoint replaces the current JSON snapshot. The client can
also fetch individual catalogs if it only needs one domain (e.g. after a w3d
modification).

## Domain Schemas

### Domain 0 — Doodad (`war3map.w3d`)

Source: `Doodads\Doodads.slk` merged with `war3map.w3d` modifications.

| Column | dtype | SLK field | Notes |
|--------|-------|-----------|-------|
| `key` | `rawcode` (11) | — | LE u32 key (same as HashMap key) |
| `doodId` | `str` (7) | `doodID` | 4-char rawcode text |
| `baseId` | `str` (7) | — | Original rawcode for custom doodads, empty for standard |
| `name` | `str` (7) | `Name` | WESTRING-resolved display name |
| `nameRaw` | `str` (7) | `Name` | Raw value before WESTRING resolution |
| `comment` | `str` (7) | `comment` | |
| `category` | `str` (7) | `category` | |
| `tilesets` | `str` (7) | `tilesets` | |
| `file` | `str` (7) | `file` | Model path |
| `doodClass` | `str` (7) | `doodClass` | |
| `soundLoop` | `str` (7) | `soundLoop` | |
| `pathTex` | `str` (7) | `pathTex` | |
| `numVar` | `u32` (2) | `numVar` | |
| `defScale` | `f64` (5) | `defScale` | |
| `minScale` | `f64` (5) | `minScale` | |
| `maxScale` | `f64` (5) | `maxScale` | |
| `selSize` | `f64` (5) | `selSize` | |
| `maxPitch` | `f64` (5) | `maxPitch` | |
| `maxRoll` | `f64` (5) | `maxRoll` | |
| `visRadius` | `f64` (5) | `visRadius` | |
| `fixedRot` | `f64` (5) | `fixedRot` | |
| `version` | `u32` (2) | `version` | |
| `mmColor` | `rgb` (8) | `MMRed/MMGreen/MMBlue` | Minimap color |
| `flags1` | `bitflags8` (10) | — | See bitflags layout below |
| `flags2` | `bitflags8` (10) | — | See bitflags layout below |
| `w3dModified` | `bool8` (6) | — | Modified by war3map.w3d |

**`flags1` bits:**

| Bit | Field |
|-----|-------|
| 0 | `tilesetSpecific` |
| 1 | `canPlaceRandScale` |
| 2 | `useClickHelper` |
| 3 | `ignoreModelClick` |
| 4 | `walkable` |
| 5 | `onCliffs` |
| 6 | `onWater` |
| 7 | `floats` |

**`flags2` bits:**

| Bit | Field |
|-----|-------|
| 0 | `shadow` |
| 1 | `showInFog` |
| 2 | `animInFog` |
| 3 | `showInMm` |
| 4 | `useMmColor` |
| 5 | `inBeta` |
| 6–7 | reserved |

### Domain 1 — Unit (`war3map.w3u`)

Source: `Units\UnitData.slk` + `UnitBalance.slk` + `unitUI.slk` + `UnitWeapons.slk` + `*UnitStrings.txt`, merged with `war3map.w3u`.

| Column | dtype | Source | Notes |
|--------|-------|--------|-------|
| `key` | `rawcode` (11) | — | LE u32 key |
| `unitId` | `str` (7) | `unitID` | 4-char rawcode |
| `name` | `str` (7) | `name` (unitUI) | WESTRING-resolved |
| `comment` | `str` (7) | `comment(s)` | |
| `sort` | `str` (7) | `sort` | |
| `race` | `str` (7) | `race` | |
| `tilesets` | `str` (7) | `tilesets` (Balance) | |
| `moveTp` | `str` (7) | `movetp` | |
| `pathTex` | `str` (7) | `pathTex` | |
| `targType` | `str` (7) | `targType` | |
| `buffType` | `str` (7) | `buffType` | |
| `regenType` | `str` (7) | `regenType` (Balance) | |
| `defType` | `str` (7) | `defType` (Balance) | |
| `primary` | `str` (7) | `Primary` (Balance) | |
| `unitType` | `str` (7) | `type` (Balance) | |
| `file` | `str` (7) | `file` (unitUI) | Model path |
| `unitSound` | `str` (7) | `unitSound` (unitUI) | |
| `unitClass` | `str` (7) | `unitClass` (unitUI) | |
| `special` | `str` (7) | `special` (unitUI) | |
| `unitShadow` | `str` (7) | `unitShadow` (unitUI) | |
| `buildingShadow` | `str` (7) | `buildingShadow` (unitUI) | |
| `uberSplat` | `str` (7) | `uberSplat` (unitUI) | |
| `atkType1` | `str` (7) | `atkType1` (Weapons) | |
| `targs1` | `str` (7) | `targs1` (Weapons) | |
| `splashTargs1` | `str` (7) | `splashTargs1` (Weapons) | |
| `atkType2` | `str` (7) | `atkType2` (Weapons) | |
| `targs2` | `str` (7) | `targs2` (Weapons) | |
| `splashTargs2` | `str` (7) | `splashTargs2` (Weapons) | |
| `weapTp1` | `str` (7) | `weapTp1` (Weapons) | |
| `weapType1` | `str` (7) | `weapType1` (Weapons) | |
| `weapTp2` | `str` (7) | `weapTp2` (Weapons) | |
| `weapType2` | `str` (7) | `weapType2` (Weapons) | |
| `tip` | `str` (7) | UnitStrings | Optional, empty if absent |
| `ubertip` | `str` (7) | UnitStrings | |
| `hotkey` | `str` (7) | UnitStrings | |
| `propernames` | `str` (7) | UnitStrings | |
| `revivetip` | `str` (7) | UnitStrings | |
| `awakentip` | `str` (7) | UnitStrings | |
| `editorSuffix` | `str` (7) | UnitStrings | |
| `moveHeight` | `f64` (5) | `moveHeight` | |
| `moveFloor` | `f64` (5) | `moveFloor` | |
| `turnRate` | `f64` (5) | `turnRate` | |
| `propWin` | `f64` (5) | `propWin` | |
| `death` | `f64` (5) | `death` | |
| `buffRadius` | `f64` (5) | `buffRadius` | |
| `realHp` | `f64` (5) | `realHP` (Balance) | |
| `regenHp` | `f64` (5) | `regenHP` (Balance) | |
| `realM` | `f64` (5) | `realM` (Balance) | |
| `regenMana` | `f64` (5) | `regenMana` (Balance) | |
| `defUp` | `f64` (5) | `defUp` (Balance) | |
| `realDef` | `f64` (5) | `realdef` (Balance) | |
| `collision` | `f64` (5) | `collision` (Balance) | |
| `strPlus` | `f64` (5) | `STRplus` (Balance) | |
| `agiPlus` | `f64` (5) | `AGIplus` (Balance) | |
| `intPlus` | `f64` (5) | `INTplus` (Balance) | |
| `modelScale` | `f64` (5) | `modelScale` (unitUI) | |
| `scale` | `f64` (5) | `scale` (unitUI) | |
| `scaleBull` | `f64` (5) | `scaleBull` (unitUI) | |
| `occH` | `f64` (5) | `occH` (unitUI) | |
| `selZ` | `f64` (5) | `selZ` (unitUI) | |
| `maxPitch` | `f64` (5) | `maxPitch` (unitUI) | |
| `maxRoll` | `f64` (5) | `maxRoll` (unitUI) | |
| `elevRad` | `f64` (5) | `elevRad` (unitUI) | |
| `fogRad` | `f64` (5) | `fogRad` (unitUI) | |
| `acquire` | `f64` (5) | `acquire` (Weapons) | |
| `cool1` | `f64` (5) | `cool1` (Weapons) | |
| `rangeN1` | `f64` (5) | `rangeN1` (Weapons) | |
| `dmgPt1` | `f64` (5) | `dmgpt1` (Weapons) | |
| `backSw1` | `f64` (5) | `backSw1` (Weapons) | |
| `minRange` | `f64` (5) | `minRange` (Weapons) | |
| `cool2` | `f64` (5) | `cool2` (Weapons) | |
| `rangeN2` | `f64` (5) | `rangeN2` (Weapons) | |
| `dmgPt2` | `f64` (5) | `dmgpt2` (Weapons) | |
| `backSw2` | `f64` (5) | `backSw2` (Weapons) | |
| `formation` | `u32` (2) | `formation` | |
| `threat` | `u32` (2) | `threat` | |
| `points` | `u32` (2) | `points` | |
| `deathType` | `u32` (2) | `deathType` | |
| `cargoSize` | `u32` (2) | `cargoSize` | |
| `prio` | `u32` (2) | `prio` | |
| `level` | `u32` (2) | `level` (Balance) | |
| `hp` | `u32` (2) | `HP` (Balance) | |
| `mana0` | `u32` (2) | `mana0` (Balance) | |
| `manaN` | `u32` (2) | `manaN` (Balance) | |
| `def` | `u32` (2) | `def` (Balance) | |
| `spd` | `u32` (2) | `spd` (Balance) | |
| `minSpd` | `u32` (2) | `minSpd` (Balance) | |
| `maxSpd` | `u32` (2) | `maxSpd` (Balance) | |
| `sight` | `u32` (2) | `sight` (Balance) | |
| `nsight` | `u32` (2) | `nsight` (Balance) | |
| `bldTm` | `u32` (2) | `bldtm` (Balance) | |
| `repTm` | `u32` (2) | `reptm` (Balance) | |
| `str` | `u32` (2) | `STR` (Balance) | |
| `agi` | `u32` (2) | `AGI` (Balance) | |
| `int` | `u32` (2) | `INT` (Balance) | |
| `goldCost` | `u32` (2) | `goldcost` (Balance) | |
| `lumberCost` | `u32` (2) | `lumbercost` (Balance) | |
| `goldRep` | `u32` (2) | `goldRep` (Balance) | |
| `lumberRep` | `u32` (2) | `lumberRep` (Balance) | |
| `fmade` | `u32` (2) | `fmade` (Balance) | |
| `fused` | `u32` (2) | `fused` (Balance) | |
| `bountyDice` | `u32` (2) | `bountydice` (Balance) | |
| `bountySides` | `u32` (2) | `bountysides` (Balance) | |
| `bountyPlus` | `u32` (2) | `bountyplus` (Balance) | |
| `stockMax` | `u32` (2) | `stockMax` (Balance) | |
| `stockRegen` | `u32` (2) | `stockRegen` (Balance) | |
| `stockStart` | `u32` (2) | `stockStart` (Balance) | |
| `elevPts` | `u32` (2) | `elevPts` (unitUI) | |
| `weapsOn` | `u32` (2) | `weapsOn` (Weapons) | |
| `dmgplus1` | `u32` (2) | `dmgplus1` (Weapons) | |
| `dice1` | `u32` (2) | `dice1` (Weapons) | |
| `sides1` | `u32` (2) | `sides1` (Weapons) | |
| `dmgplus2` | `u32` (2) | `dmgplus2` (Weapons) | |
| `dice2` | `u32` (2) | `dice2` (Weapons) | |
| `sides2` | `u32` (2) | `sides2` (Weapons) | |
| `version` | `u32` (2) | `version` | |
| `teamColor` | `i32` (3) | `teamColor` (unitUI) | -1 = none |
| `tintColor` | `rgb` (8) | `red/green/blue` (unitUI) | |
| `flags1` | `bitflags8` (10) | — | See bits below |
| `flags2` | `bitflags8` (10) | — | See bits below |
| `flags3` | `bitflags8` (10) | — | See bits below |

**`flags1` bits:**

| Bit | Field |
|-----|-------|
| 0 | `canSleep` |
| 1 | `canFlee` |
| 2 | `fatLos` |
| 3 | `isBldg` |
| 4 | `customTeamColor` |
| 5 | `shadowOnWater` |
| 6 | `selCircOnWater` |
| 7 | `inEditor` |

**`flags2` bits:**

| Bit | Field |
|-----|-------|
| 0 | `hiddenInEditor` |
| 1 | `showUi1` |
| 2 | `showUi2` |
| 3 | `inBeta` |
| 4–7 | reserved |

### Domain 2 — Destructable (`war3map.w3b`)

Source: `Units\DestructableData.slk`, merged with `war3map.w3b`.

| Column | dtype | SLK field | Notes |
|--------|-------|-----------|-------|
| `key` | `rawcode` (11) | — | LE u32 key |
| `destructableId` | `str` (7) | `DestructableID` | 4-char rawcode |
| `name` | `str` (7) | `Name` | WESTRING-resolved |
| `editorSuffix` | `str` (7) | `EditorSuffix` | WESTRING-resolved |
| `comment` | `str` (7) | `comment` | WESTRING-resolved |
| `category` | `str` (7) | `category` | |
| `tilesets` | `str` (7) | `tilesets` | |
| `file` | `str` (7) | `file` | Model path |
| `texFile` | `str` (7) | `texFile` | |
| `doodClass` | `str` (7) | `doodClass` | |
| `targType` | `str` (7) | `targType` | |
| `armor` | `str` (7) | `armor` | |
| `pathTex` | `str` (7) | `pathTex` | |
| `pathTexDeath` | `str` (7) | `pathTexDeath` | |
| `deathSnd` | `str` (7) | `deathSnd` | |
| `portraitmodel` | `str` (7) | `portraitmodel` | |
| `texId` | `u32` (2) | `texID` | |
| `cliffHeight` | `u32` (2) | `cliffHeight` | |
| `numVar` | `u32` (2) | `numVar` | |
| `hp` | `u32` (2) | `HP` | |
| `buildTime` | `u32` (2) | `buildTime` | |
| `repairTime` | `u32` (2) | `repairTime` | |
| `goldRep` | `u32` (2) | `goldRep` | |
| `lumberRep` | `u32` (2) | `lumberRep` | |
| `version` | `u32` (2) | `version` | |
| `occH` | `f64` (5) | `occH` | |
| `flyH` | `f64` (5) | `flyH` | |
| `fixedRot` | `f64` (5) | `fixedRot` | |
| `selSize` | `f64` (5) | `selSize` | |
| `minScale` | `f64` (5) | `minScale` | |
| `maxScale` | `f64` (5) | `maxScale` | |
| `maxPitch` | `f64` (5) | `maxPitch` | |
| `maxRoll` | `f64` (5) | `maxRoll` | |
| `radius` | `f64` (5) | `radius` | |
| `fogRadius` | `f64` (5) | `fogRadius` | |
| `selcircsize` | `f64` (5) | `selcircsize` | |
| `color` | `rgb` (8) | `colorR/G/B` | Tint color |
| `mmColor` | `rgb` (8) | `MMRed/Green/Blue` | Minimap color |
| `flags1` | `bitflags8` (10) | — | See bits below |
| `flags2` | `bitflags8` (10) | — | See bits below |

**`flags1` bits:**

| Bit | Field |
|-----|-------|
| 0 | `tilesetSpecific` |
| 1 | `lightweight` |
| 2 | `fatLos` |
| 3 | `useClickHelper` |
| 4 | `onCliffs` |
| 5 | `onWater` |
| 6 | `canPlaceDead` |
| 7 | `walkable` |

**`flags2` bits:**

| Bit | Field |
|-----|-------|
| 0 | `canPlaceRandScale` |
| 1 | `fogVis` |
| 2 | `shadow` |
| 3 | `showInMm` |
| 4 | `useMmColor` |
| 5 | `inBeta` |
| 6 | `selectable` |
| 7 | reserved |

## Rust Writer API (planned: `src/util/columnar.rs`)

```rust
use std::collections::HashMap;

/// Data types for columns.
#[repr(u8)]
pub enum DType {
    U8 = 0, U16 = 1, U32 = 2, I32 = 3,
    F32 = 4, F64 = 5, Bool8 = 6, Str = 7,
    Rgb = 8, Rgba = 9, Bitflags8 = 10, Rawcode = 11,
}

/// Domain identifiers.
#[repr(u16)]
pub enum Domain {
    Doodad = 0, Unit = 1, Destructable = 2, Item = 3,
    Ability = 4, Buff = 5, Upgrade = 6,
    Terrain = 7, CliffTypes = 8, Water = 9,
    Snapshot = 10,
}

/// Interning string table — deduplicates strings, returns offsets.
pub struct StringTable {
    buf: Vec<u8>,
    index: HashMap<String, u32>,
}

impl StringTable {
    pub fn new() -> Self {
        let mut st = Self { buf: vec![0u8], index: HashMap::new() };
        st.index.insert(String::new(), 0); // offset 0 = empty string
        st
    }

    /// Intern a string, returning its byte offset in the table.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&off) = self.index.get(s) {
            return off;
        }
        let off = self.buf.len() as u32;
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0); // null terminator
        self.index.insert(s.to_string(), off);
        off
    }

    pub fn as_bytes(&self) -> &[u8] { &self.buf }
    pub fn len(&self) -> usize { self.buf.len() }
}

/// Builder for a WOBJ binary buffer.
pub struct ColumnarWriter {
    domain: Domain,
    row_count: u32,
    columns: Vec<ColumnEntry>,
    body: Vec<u8>,
    strings: StringTable,
}

struct ColumnEntry {
    name: String,
    dtype: DType,
    data_offset: u32, // filled during finish()
    data: Vec<u8>,
}

impl ColumnarWriter {
    pub fn new(domain: Domain, row_count: usize) -> Self { /* ... */ }

    pub fn add_u8_column(&mut self, name: &str, values: &[u8]);
    pub fn add_u16_column(&mut self, name: &str, values: &[u16]);
    pub fn add_u32_column(&mut self, name: &str, values: &[u32]);
    pub fn add_i32_column(&mut self, name: &str, values: &[i32]);
    pub fn add_f32_column(&mut self, name: &str, values: &[f32]);
    pub fn add_f64_column(&mut self, name: &str, values: &[f64]);
    pub fn add_bool_column(&mut self, name: &str, values: &[bool]);
    pub fn add_str_column(&mut self, name: &str, values: &[&str]);
    pub fn add_rgb_column(&mut self, name: &str, values: &[(u8, u8, u8)]);
    pub fn add_bitflags_column(&mut self, name: &str, values: &[u8]);
    pub fn add_rawcode_column(&mut self, name: &str, values: &[u32]);

    /// Finalize: compute offsets, write header + directory + data + string table.
    pub fn finish(self) -> Vec<u8>;
}
```

## JS Reader API (planned: `extension/vendor/columnar.js`)

```js
const DTYPE_SIZE = [1, 2, 4, 4, 4, 8, 1, 4, 3, 4, 1, 4];
const DTYPE_VIEW = [
    Uint8Array, Uint16Array, Uint32Array, Int32Array,
    Float32Array, Float64Array, Uint8Array, Uint32Array,
    Uint8Array, Uint8Array, Uint8Array, Uint32Array
];

export class WobjReader {
    /**
     * @param {ArrayBuffer} buffer
     */
    constructor(buffer) {
        const h = new DataView(buffer);
        if (h.getUint32(0, true) !== 0x574F424A) throw new Error('Bad WOBJ magic');
        this.version = h.getUint16(4, true);
        this.domain  = h.getUint16(6, true);
        this.rowCount = h.getUint32(8, true);
        const colCount = h.getUint16(12, true);
        const strOff  = h.getUint32(16, true);
        const strSize = h.getUint32(20, true);

        this.strBytes = new Uint8Array(buffer, strOff, strSize);
        this._decoder = new TextDecoder();
        this._strCache = new Map();
        this.buffer = buffer;

        // Parse column directory (starts at byte 24)
        this.columns = new Map();
        this._colArr = [];
        for (let i = 0; i < colCount; i++) {
            const base = 24 + i * 7;
            const nameOff  = h.getUint16(base, true);
            const dtype    = h.getUint8(base + 2);
            const dataOff  = h.getUint32(base + 3, true);

            const name = this._readStr(nameOff);
            const col = { name, dtype, dataOff };
            this.columns.set(name, col);
            this._colArr.push(col);
        }
    }

    /** Zero-copy TypedArray view for a numeric column. */
    getColumn(name) {
        const col = this.columns.get(name);
        if (!col) return null;
        const View = DTYPE_VIEW[col.dtype];
        const elemSize = DTYPE_SIZE[col.dtype];
        // For rgb (dtype=8), length is rowCount * 3
        const len = col.dtype === 8 ? this.rowCount * 3
                  : col.dtype === 9 ? this.rowCount * 4
                  : this.rowCount;
        return new View(this.buffer, col.dataOff, len);
    }

    /** Read a string value from a str column at the given row. */
    getString(name, row) {
        const col = this.columns.get(name);
        if (!col || col.dtype !== 7) return '';
        const offsets = new Uint32Array(this.buffer, col.dataOff, this.rowCount);
        return this._readStr(offsets[row]);
    }

    /** Read a boolean from a bitflags column. */
    getFlag(name, row, bit) {
        const col = this.columns.get(name);
        if (!col) return false;
        const arr = new Uint8Array(this.buffer, col.dataOff, this.rowCount);
        return (arr[row] & (1 << bit)) !== 0;
    }

    /** Internal: decode a null-terminated string from the string table. */
    _readStr(offset) {
        let cached = this._strCache.get(offset);
        if (cached !== undefined) return cached;
        let end = offset;
        while (end < this.strBytes.length && this.strBytes[end] !== 0) end++;
        const s = this._decoder.decode(this.strBytes.subarray(offset, end));
        this._strCache.set(offset, s);
        return s;
    }
}
```

## Size Comparison (estimated for ~500 doodads)

| Component | JSON (current) | WOBJ (proposed) | Ratio |
|-----------|:--------------:|:---------------:|:-----:|
| 500 × `doodId` (4-char string) | ~7,500 B | 2,000 B (u32 rawcode) | 3.8× |
| 500 × `defScale` (f64) | ~8,000 B | 4,000 B (raw f64) | 2× |
| 500 × `walkable` (bool) | ~7,000 B | 63 B (in bitflags) | 111× |
| 500 × `name` (avg 15 chars) | ~12,500 B | ~4,000 B (deduped str table) | 3× |
| JSON keys repeated per row | ~150,000 B | 0 B | ∞ |
| **Total estimate** | ~400 KB | ~80 KB | **5×** |

Parse time (measured on M1 Mac, Chrome):
- `JSON.parse(400KB)`: ~2.5 ms
- WOBJ header + column directory: ~0.01 ms (just pointer arithmetic)
- First column access: ~0 ms (`new Float64Array()` is a view, no copy)
- First string read: ~0.005 ms (single `TextDecoder.decode()` of <20 bytes)

## Migration Plan

### Phase 1 — Binary writer + reader

Implement `ColumnarWriter` in Rust (`src/util/columnar.rs`) and `WobjReader` in JS
(`extension/vendor/columnar.js`). Add new binary HTTP endpoints (`/w3e/catalog/*`)
alongside existing JSON ones. Both paths work — the extension can be switched with a flag.

### Phase 2 — Move map editor off LSP

Replace all `custom/w3e*` LSP method calls in `extension.js` and `send.rs` with
`fetch()` calls to the HTTP binary server. The LSP transport is left **only** for
language features (diagnostics, completions, hover, go-to-definition, inlay hints).

Map editor webview communicates exclusively through HTTP:
- `GET /w3e/terrain` — binary terrain data (already done)
- `GET /w3e/catalog/doodads` — WOBJ binary
- `GET /w3e/catalog/units` — WOBJ binary
- `GET /w3e/catalog/destructables` — WOBJ binary
- `GET /w3e/snapshot` — all catalogs in one response
- `GET /w3e/file` — raw file lookup (already done)
- `GET /w3e/tileTextures` — tile textures (already done)
- etc.

JSON endpoints remain for debugging and external tooling.

### Phase 3 — Remove JSON serialisation for catalogs

Drop `serde_json` serialisation from `snapshot.rs` and `send.rs` for catalog data.
The `#[derive(Serialize)]` on `Doodad`, `UnitInfo`, `Destructable` structs becomes
optional (only needed if JSON debug endpoints are kept).

The `_doodadsSlk`, `_unitsSlk`, `_destructablesSlk` keys in the LSP w3e response
are removed entirely — the webview no longer expects them.

### Phase 4 — Browser version

The same Rust code compiles to WASM. Two modes:

**Mode A — WASM in-browser (local files):**
```
Browser
  └─ WASM module (compiled from same Rust crate)
       └─ ColumnarWriter::finish() → Vec<u8> → ArrayBuffer
            → WobjReader (same JS, no changes)
```
No HTTP needed — `ColumnarWriter` runs inside WASM, returns raw bytes via
`wasm_bindgen`. The `WobjReader` class works identically on the resulting
`ArrayBuffer`. File I/O uses browser File API or drag-and-drop.

**Mode B — Remote server (cloud-hosted):**
```
Browser
  └─ fetch('https://server.example.com/w3e/catalog/doodads')
       → same Rust HTTP server (axum), same binary endpoints
         → WobjReader (same JS, no changes)
```
The HTTP endpoints are the same as in the VS Code extension, just served
from a remote host instead of `127.0.0.1`. Authentication switches from
a per-session token to proper auth (JWT / session cookies).

In both modes, the JS client code is **identical** — it only needs an
`ArrayBuffer`, regardless of where it came from.

## References

- [Apache Arrow Columnar Format](https://arrow.apache.org/docs/format/Columnar.html) — inspiration for columnar layout
- [FlatBuffers](https://google.github.io/flatbuffers/) — why vtable overhead matters
- [`http/terrain.rs`](../../src/http/terrain.rs) — existing binary terrain endpoint (same philosophy)
- [`terrain.md`](./terrain.md) — existing terrain format documentation

