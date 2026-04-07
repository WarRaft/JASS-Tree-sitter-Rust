# Binary Protocol — Data Transport & WebSocket Messaging

> **Status:** Design document (not yet implemented)
>
> Rust: `src/util/columnar.rs` (planned), `src/lsp/wire.rs` (planned) —
> JS: `extension/vendor/columnar.js` (planned), `extension/vendor/wire.js` (planned)
>
> This document covers **two** binary protocols:
> 1. **WOBJ** — columnar binary format for catalog data (doodads, units, etc.)
>    over HTTP endpoints.
> 2. **Wire** — binary WebSocket framing that replaces JSON-RPC for all
>    extension ↔ server communication.
>
> **Goal:** zero JSON anywhere in the protocol. All communication is binary.
> TypedArray access on the JS side, no `JSON.parse()`, no `JSON.stringify()`.

---

# Part I — WOBJ Columnar Format

## Motivation: browser-ready architecture

The map editor currently runs as a VS Code webview backed by a Rust server.
The **long-term goal is a standalone browser version** — the same
editor UI running in a browser tab, with the Rust backend compiled to WASM
or served remotely.

All communication must be binary — no JSON serialisation, no JSON parsing.

### Current architecture (JSON over WebSocket)

```
VS Code extension
  └─ ServerClient (JSON-RPC over WebSocket)
       └─ document/open, color/presentation, …
            → Rust server (axum WS handler)
              → serde_json::from_str() → MethodCall enum
                → process → serde_json::to_string()
                  → JSON text frame back to extension
```

Problems:
- All messages are JSON text frames — `JSON.parse()` / `JSON.stringify()` overhead.
- Binary data (BLP textures, terrain, MPQ contents) must be base64-encoded.
- Webview relay still goes through extension host for non-HTTP data.
- Method dispatch uses string matching on JSON `"method"` field.

### Target architecture (fully binary)

```
VS Code extension / Browser
  ├─ WebSocket (binary frames)
  │    └─ 8-byte header + typed payload
  │         → Rust server: read header → match method_id → decode payload
  │           → process → encode response → binary frame
  └─ HTTP (binary responses)
       └─ fetch('/w3e/catalog/doodads')
            → WOBJ binary → zero-copy TypedArray views
```

Benefits:
- **Zero JSON.** No `JSON.parse()`, no `JSON.stringify()`, no `serde_json`.
- **Binary transport.** No base64 encoding — raw bytes everywhere.
- **Fast dispatch.** `match method_id` on `u16` instead of string comparison.
- **Same code path** for VS Code extension and standalone browser.
- **Parallelisable.** HTTP `fetch()` for bulk data, WebSocket for interactive messaging.

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

---

# Part II — WebSocket Binary Messaging

Replaces JSON-RPC text frames on the WebSocket channel.
All WebSocket frames are **binary** (`opcode 0x02`), never text.

## Frame Header (8 bytes)

Every WebSocket message starts with an 8-byte header:

```
┌──────────────────────────────────────┐
│ byte 0:    type   (u8)               │
│ byte 1:    flags  (u8)               │
│ bytes 2–3: method (u16 LE)           │
│ bytes 4–7: id     (u32 LE)           │
├──────────────────────────────────────┤
│ bytes 8+:  payload (method-specific) │
└──────────────────────────────────────┘
```

| Field    | Type  | Description |
|----------|-------|-------------|
| `type`   | `u8`  | Message type (see below) |
| `flags`  | `u8`  | Reserved, must be `0` |
| `method` | `u16` | Method ID (LE). Identifies the operation |
| `id`     | `u32` | Request ID (LE). `0` for notifications. Responses echo the request's ID |

### Message types

| Value | Name           | Description |
|:-----:|----------------|-------------|
| 0     | `Notification` | Fire-and-forget. No response expected. `id = 0` |
| 1     | `Request`      | Client or server expects a `Response` or `Error` with matching `id` |
| 2     | `Response`     | Success response. `method` echoes the request's method. Payload is result |
| 3     | `Error`        | Error response. Payload is error info |

### Error payload

When `type = 3` (Error), the payload is:

```
code:       i32 LE    (application error code, 0 = generic)
message:    str       (human-readable error text)
```

## Payload Encoding Primitives

All multi-byte integers are **little-endian**.

| Notation     | Wire format | Size |
|-------------|-------------|------|
| `u8`        | raw byte | 1 |
| `u16`       | 2 bytes LE | 2 |
| `u32`       | 4 bytes LE | 4 |
| `i32`       | 4 bytes LE (signed) | 4 |
| `f64`       | 8 bytes LE (IEEE 754) | 8 |
| `bool`      | `u8`: `0` = false, `1` = true | 1 |
| `str`       | `u32 len` + `[u8; len]` UTF-8 bytes | 4 + len |
| `opt<T>`    | `u8 present` (0/1) + `T` if present | 1 [+ sizeof(T)] |
| `array<T>`  | `u32 count` + `T` repeated | 4 + count × sizeof(T) |

**Strings** are NOT null-terminated (unlike the WOBJ string table).
Length prefix provides exact bounds.

### Common structures

**Position:**
```
line:      u32
character: u32
```
(8 bytes)

**Range:**
```
startLine: u32
startChar: u32
endLine:   u32
endChar:   u32
```
(16 bytes)

**Location:**
```
uri:   str
range: Range
```

## Method ID Registry

### 0x00xx — Protocol control

| ID | Name | Type | Direction | Payload |
|----|------|------|-----------|---------|
| `0x0001` | cancel | Notification | C→S | `id: u32` (the request ID to cancel) |

### 0x01xx — Document sync

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0100` | document/open | Notification | C→S |
| `0x0101` | document/change | Notification | C→S |
| `0x0102` | document/close | Notification | C→S |

### 0x02xx — Server push

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0200` | parseResult | Notification | S→C |
| `0x0201` | debugLog | Notification | S→C |
| `0x0202` | watchers/register | Request | S→C |

### 0x03xx — File watchers

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0300` | files/changed | Notification | C→S |

### 0x04xx — Language features

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0400` | completion | Request | C→S |
| `0x0401` | hover | Request | C→S |
| `0x0402` | definition | Request | C→S |
| `0x0403` | references | Request | C→S |
| `0x0404` | documentHighlight | Request | C→S |
| `0x0405` | prepareRename | Request | C→S |
| `0x0406` | rename | Request | C→S |
| `0x0407` | signatureHelp | Request | C→S |
| `0x0408` | codeAction | Request | C→S |
| `0x0409` | codeLens | Request | C→S |
| `0x040A` | formatting | Request | C→S |
| `0x040B` | color/presentation | Request | C→S |
| `0x040C` | prepareCallHierarchy | Request | C→S |
| `0x040D` | incomingCalls | Request | C→S |
| `0x040E` | outgoingCalls | Request | C→S |
| `0x040F` | prepareTypeHierarchy | Request | C→S |
| `0x0410` | supertypes | Request | C→S |
| `0x0411` | subtypes | Request | C→S |
| `0x0412` | willRenameFiles | Request | C→S |

### 0x05xx — Commands

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0500` | rescan/execute | Request | C→S |
| `0x0501` | build/execute | Request | C→S |
| `0x0502` | build/hooks | Request | C→S |
| `0x0503` | ujapi/download | Request | C→S |

### 0x06xx — Graph panels

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0600` | importGraph/subgraph | Request | C→S |
| `0x0601` | callGraph/subgraph | Request | C→S |
| `0x0602` | typeGraph/subgraph | Request | C→S |

### 0x07xx — Binary format rendering

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0700` | blp/render | Request | C→S |
| `0x0701` | mdx/render | Request | C→S |
| `0x0702` | doo/render | Request | C→S |
| `0x0703` | w3i/render | Request | C→S |
| `0x0704` | w3e/render | Request | C→S |
| `0x0705` | w3obj/render | Request | C→S |

### 0x08xx — MPQ

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0800` | mpq/info | Request | C→S |
| `0x0801` | mpq/list | Request | C→S |
| `0x0802` | mpq/read | Request | C→S |

### 0x09xx — SLK

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0900` | slk/render | Request | C→S |
| `0x0901` | slk/edit | Request | C→S |

### 0x0Axx — Debug

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0A00` | debugLogEnable | Notification | C→S |
| `0x0A01` | debugInit | Request | C→S |

### 0x0Bxx — Game path

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0B00` | w3e/gamePath/set | Request | C→S |
| `0x0B01` | w3e/gamePath/status | Request | C→S |

### 0x0Cxx — Terrain catalogs

| ID | Name | Type | Direction |
|----|------|------|-----------|
| `0x0C00` | w3e/terrainSlk | Request | C→S |
| `0x0C01` | w3e/doodadsSlk | Request | C→S |
| `0x0C02` | w3e/unitsSlk | Request | C→S |
| `0x0C03` | w3e/destructablesSlk | Request | C→S |
| `0x0C04` | w3e/lookupFile | Request | C→S |

## Message Payloads — Document Sync

### 0x0100 document/open

```
languageId: u8        (0=bni, 1=jass, 2=angelscript, 3=wts, 4=slk)
version:    u32
uri:        str
text:       str
```

### 0x0101 document/change

```
version:     u32
uri:         str
changeCount: u32
── per change ──
  startLine: u32
  startChar: u32
  endLine:   u32
  endChar:   u32
  text:      str
```

### 0x0102 document/close

```
uri: str
```

### Language ID enum

| Value | Language |
|:-----:|----------|
| 0 | `bni` |
| 1 | `jass` |
| 2 | `angelscript` |
| 3 | `wts` |
| 4 | `slk` |

## Message Payloads — Server Push

### 0x0200 parseResult

The most frequent and heaviest server→client notification. Sent after every
parse cycle. Contains all derived data for a single document.

```
uri: str

── Semantic tokens ──
tokenDataLen: u32                     (count of u32 values, always multiple of 5)
tokenData:    [u32; tokenDataLen]     (delta-encoded: Δline, Δstart, length, type, modifiers)

── Diagnostics ──
diagCount: u32
── per diagnostic ──
  range:          Range               (16 bytes)
  severity:       u8                  (1=error, 2=warning, 3=info, 4=hint)
  message:        str
  source:         opt<str>
  code:           opt<str>
  codeHref:       opt<str>
  tagCount:       u8
  tags:           [u8; tagCount]      (1=unnecessary, 2=deprecated)
  relatedCount:   u16
  ── per related ──
    location:     Location            (str uri + Range)
    message:      str

── Inlay hints ──
hintCount: u32
── per hint ──
  line:           u32
  character:      u32
  label:          str
  kind:           u8                  (0=none, 1=type, 2=parameter)
  paddingLeft:    bool
  paddingRight:   bool

── Folding ranges ──
foldCount: u32
── per fold ──
  startLine:      u32
  endLine:        u32
  kind:           u8                  (0=none, 1=comment, 2=imports, 3=region)

── Document symbols (flattened tree) ──
symbolCount: u32
── per symbol ──
  name:           str
  detail:         str
  kind:           u8                  (VS Code SymbolKind, 1–26)
  range:          Range
  selectionRange: Range
  tags:           u8                  (bitflags: bit0=deprecated)
  childCount:     u16                 (number of direct children following)

── Document links ──
linkCount: u32
── per link ──
  range:          Range
  target:         opt<str>
  tooltip:        opt<str>

── Colors ──
colorCount: u32
── per color ──
  range:          Range
  red:            f64
  green:          f64
  blue:           f64
  alpha:          f64
```

**Document symbols** are serialised in pre-order tree traversal with `childCount`
per node. The reader reconstructs the tree by counting children:

```js
function readSymbols(view, offset) {
    const count = view.getUint32(offset, true); offset += 4;
    const roots = [];
    const stack = [];
    for (let i = 0; i < count; i++) {
        const sym = readOneSymbol(view, offset); // reads fields, advances offset
        offset = sym.nextOffset;
        // Find the parent: walk up the stack while children are full
        while (stack.length > 0 && stack[stack.length - 1].remaining === 0) stack.pop();
        if (stack.length > 0) {
            stack[stack.length - 1].node.children.push(sym.node);
            stack[stack.length - 1].remaining--;
        } else {
            roots.push(sym.node);
        }
        if (sym.childCount > 0) {
            stack.push({ node: sym.node, remaining: sym.childCount });
            sym.node.children = [];
        }
    }
    return roots;
}
```

### 0x0201 debugLog

```
timestamp: f64       (milliseconds since epoch)
method:    str
status:    u8        (0=created, 1=completed, 2=cancelled, 3=error)
id:        opt<u32>  (request ID, if applicable)
uri:       opt<str>
duration:  opt<f64>  (milliseconds)
```

### 0x0202 watchers/register (server → client request)

```
registrationCount: u32
── per registration ──
  id:      str                     (registration ID)
  watcherCount: u32
  ── per watcher ──
    globPattern: str
    kind:        u8                (bitmask: 1=create, 2=change, 4=delete)
```

The client responds with an empty `Response` (type=2, no payload).

### 0x0300 files/changed

```
changeCount: u32
── per change ──
  uri:  str
  type: u8     (1=created, 2=changed, 3=deleted)
```

## Message Payloads — Language Features

Most language feature requests share a common **TextDocumentPosition** pattern:

```
── TextDocumentPosition ──
uri:       str
line:      u32
character: u32
```

### Common request payloads

| Method | Request payload |
|--------|----------------|
| 0x0400 completion | TextDocumentPosition |
| 0x0401 hover | TextDocumentPosition |
| 0x0402 definition | TextDocumentPosition |
| 0x0403 references | TextDocumentPosition + `includeDeclaration: bool` |
| 0x0404 documentHighlight | TextDocumentPosition |
| 0x0405 prepareRename | TextDocumentPosition |
| 0x0407 signatureHelp | TextDocumentPosition |
| 0x040C prepareCallHierarchy | TextDocumentPosition |
| 0x040F prepareTypeHierarchy | TextDocumentPosition |

### 0x0406 rename

```
uri:       str
line:      u32
character: u32
newName:   str
```

### 0x0408 codeAction

```
uri:       str
range:     Range
diagCount: u32
── per diagnostic ── (same format as in parseResult)
```

### 0x0409 codeLens / 0x040A formatting

```
uri: str
```

(Formatting also includes `tabSize: u32`, `insertSpaces: bool`.)

### 0x040B color/presentation

```
uri:   str
range: Range
red:   f64
green: f64
blue:  f64
alpha: f64
```

### 0x040D incomingCalls / 0x040E outgoingCalls

```
── CallHierarchyItem ──
name:           str
kind:           u8
uri:            str
range:          Range
selectionRange: Range
```

### 0x0410 supertypes / 0x0411 subtypes

Same as CallHierarchyItem (TypeHierarchyItem has same shape).

### 0x0412 willRenameFiles

```
fileCount: u32
── per file ──
  oldUri: str
  newUri: str
```

### Common response payloads

#### Hover response (0x0401)

```
hasResult: bool
── if true ──
  kind:     u8        (0=plaintext, 1=markdown)
  contents: str
  hasRange: bool
  ── if true ──
    range:  Range
```

#### Definition / References response (0x0402, 0x0403)

```
locationCount: u32
── per location ──
  uri:   str
  range: Range
```

#### DocumentHighlight response (0x0404)

```
highlightCount: u32
── per highlight ──
  range: Range
  kind:  u8         (1=text, 2=read, 3=write)
```

#### Completion response (0x0400)

```
itemCount: u32
── per item ──
  label:       str
  kind:        u8         (VS Code CompletionItemKind)
  detail:      opt<str>
  sortText:    opt<str>
  filterText:  opt<str>
  insertText:  opt<str>
  hasTextEdit: bool
  ── if hasTextEdit ──
    range:     Range
    newText:   str
  docKind:     u8         (0=none, 1=plaintext, 2=markdown)
  ── if docKind > 0 ──
    documentation: str
  deprecated:  bool
  preselect:   bool
```

#### Rename response (0x0406)

```
changeCount: u32                  (number of documents with edits)
── per document ──
  uri:       str
  editCount: u32
  ── per edit ──
    range:   Range
    newText: str
```

#### CodeAction response (0x0408)

```
actionCount: u32
── per action ──
  title:       str
  kind:        opt<str>
  isPreferred: bool
  ── WorkspaceEdit (same format as rename response) ──
  changeCount: u32
  ── per document ──
    uri:       str
    editCount: u32
    ── per edit ──
      range:   Range
      newText: str
```

#### CodeLens response (0x0409)

```
lensCount: u32
── per lens ──
  range:   Range
  title:   str
  command: opt<str>
```

#### SignatureHelp response (0x0407)

```
hasResult: bool
── if true ──
  activeSignature: u32
  activeParameter: u32
  sigCount:        u32
  ── per signature ──
    label:         str
    documentation: opt<str>
    paramCount:    u32
    ── per parameter ──
      label: str
```

#### CallHierarchy / TypeHierarchy response (0x040C–0x0411)

```
itemCount: u32
── per item ──
  name:           str
  kind:           u8
  uri:            str
  range:          Range
  selectionRange: Range
  detail:         opt<str>
```

For `incomingCalls` / `outgoingCalls`, each item also includes:

```
  fromRangeCount: u32
  fromRanges:     [Range; fromRangeCount]
```

#### Color presentation response (0x040B)

```
presentationCount: u32
── per presentation ──
  label:       str
  hasTextEdit: bool
  ── if hasTextEdit ──
    range:     Range
    newText:   str
```

#### Formatting response (0x040A)

```
editCount: u32
── per edit ──
  range:   Range
  newText: str
```

## Message Payloads — Commands

### 0x0500 rescan/execute

Request: `uri: str`

Response:
```
ok:      bool
message: str
errorCount: u32
── per error ──
  error: str
```

### 0x0501 build/execute

Request: `uri: str`

Response:
```
ok:      bool
message: str
```

### 0x0502 build/hooks

Request: `uri: str`

Response:
```
hasBefore: bool
── if true ──
  beforeCmd: str
hasAfter:  bool
── if true ──
  afterCmd:  str
cwd: str
```

### 0x0503 ujapi/download

Request:
```
uri:  str
path: str
```

Response:
```
ok:      bool
message: str
```

## Message Payloads — Rendering & Data

### 0x0700–0x0705 render requests

Request payload varies:

| Method | Fields |
|--------|--------|
| blp/render | `uri: str` |
| mdx/render | `uri: str` |
| doo/render | `uri: str`, `isUnit: bool`, `archivePath: opt<str>` |
| w3i/render | `uri: str`, `archivePath: opt<str>` |
| w3e/render | `uri: str`, `archivePath: opt<str>` |
| w3obj/render | `uri: str`, `levelData: bool`, `archivePath: opt<str>` |

Response: raw binary blob. Format depends on the method (render-specific).
These responses MAY use WOBJ format where columnar data fits.

### 0x0800–0x0802 MPQ

**mpq/info** (0x0800):
Request: `archivePath: str`
Response: format-specific binary.

**mpq/list** (0x0801):
Request: `archivePath: str`
Response:
```
entryCount: u32
── per entry ──
  path: str
  type: u8      (1=file, 2=directory)
```

**mpq/read** (0x0802):
Request: `archivePath: str`, `filePath: str`
Response: raw bytes (the file content — no base64, no wrapping).

### 0x0900–0x0901 SLK

**slk/render** (0x0900):
Request: `uri: str`
Response: format-specific binary (cell data).

**slk/edit** (0x0901):
Request:
```
uri:   str
start: u32
len:   u32
value: str
```
Response: `ok: bool`

### 0x0A00–0x0A01 Debug

**debugLogEnable** (0x0A00): `enabled: bool`
**debugInit** (0x0A01): empty request. Response: format-specific binary.

### 0x0B00–0x0B01 Game path

**w3e/gamePath/set** (0x0B00): `gamePath: str`
Response: `ok: bool`, `message: str`

**w3e/gamePath/status** (0x0B01): empty request.
Response: `installed: bool`, `gamePath: opt<str>`

### 0x0C00–0x0C04 Terrain catalogs

**w3e/terrainSlk, doodadsSlk, unitsSlk, destructablesSlk** (0x0C00–0x0C03):
Request: `archivePath: opt<str>`
Response: WOBJ binary (domain-specific columnar data).

**w3e/lookupFile** (0x0C04):
Request: `path: str`, `archivePath: opt<str>`
Response: raw bytes (the resolved file content).

## Graph panel payloads (0x0600–0x0602)

All three graph methods share the same pattern:

Request: `uri: str`

Response:
```
nodeCount: u32
── per node ──
  id:    str
  label: str
  kind:  u8

edgeCount: u32
── per edge ──
  from: u32       (node index)
  to:   u32       (node index)
```

## JS Wire Reader/Writer (planned: `extension/vendor/wire.js`)

```js
const HEADER_SIZE = 8;
const TYPE_NOTIFY = 0, TYPE_REQUEST = 1, TYPE_RESPONSE = 2, TYPE_ERROR = 3;

class WireReader {
    constructor(buffer) {
        this._buf = buffer;
        this._view = new DataView(buffer);
        this._off = 0;
        this._dec = new TextDecoder();
    }

    /** Read the 8-byte frame header. */
    readHeader() {
        const type   = this._view.getUint8(0);
        const flags  = this._view.getUint8(1);
        const method = this._view.getUint16(2, true);
        const id     = this._view.getUint32(4, true);
        this._off = HEADER_SIZE;
        return { type, flags, method, id };
    }

    u8()   { const v = this._view.getUint8(this._off);           this._off += 1; return v; }
    u16()  { const v = this._view.getUint16(this._off, true);    this._off += 2; return v; }
    u32()  { const v = this._view.getUint32(this._off, true);    this._off += 4; return v; }
    i32()  { const v = this._view.getInt32(this._off, true);     this._off += 4; return v; }
    f64()  { const v = this._view.getFloat64(this._off, true);   this._off += 8; return v; }
    bool() { return this.u8() !== 0; }

    str() {
        const len = this.u32();
        const bytes = new Uint8Array(this._buf, this._off, len);
        this._off += len;
        return this._dec.decode(bytes);
    }

    opt(readFn) {
        return this.bool() ? readFn.call(this) : null;
    }

    range() {
        return {
            start: { line: this.u32(), character: this.u32() },
            end:   { line: this.u32(), character: this.u32() },
        };
    }

    /** Read raw u32 array (e.g. semantic tokens). */
    u32array() {
        const count = this.u32();
        const arr = new Uint32Array(this._buf, this._off, count);
        this._off += count * 4;
        return arr;
    }

    /** Remaining bytes as raw Uint8Array. */
    rest() {
        return new Uint8Array(this._buf, this._off);
    }
}

class WireWriter {
    constructor(method, type = TYPE_REQUEST, id = 0) {
        this._parts = [];
        this._size = 0;
        // Write header
        const hdr = new ArrayBuffer(HEADER_SIZE);
        const hv = new DataView(hdr);
        hv.setUint8(0, type);
        hv.setUint8(1, 0); // flags
        hv.setUint16(2, method, true);
        hv.setUint32(4, id, true);
        this._parts.push(new Uint8Array(hdr));
        this._size += HEADER_SIZE;
    }

    u8(v)   { const b = new Uint8Array(1); b[0] = v;                                        this._push(b); }
    u16(v)  { const b = new ArrayBuffer(2); new DataView(b).setUint16(0, v, true);          this._push(new Uint8Array(b)); }
    u32(v)  { const b = new ArrayBuffer(4); new DataView(b).setUint32(0, v, true);          this._push(new Uint8Array(b)); }
    i32(v)  { const b = new ArrayBuffer(4); new DataView(b).setInt32(0, v, true);           this._push(new Uint8Array(b)); }
    f64(v)  { const b = new ArrayBuffer(8); new DataView(b).setFloat64(0, v, true);         this._push(new Uint8Array(b)); }
    bool(v) { this.u8(v ? 1 : 0); }

    str(s) {
        const enc = new TextEncoder();
        const bytes = enc.encode(s);
        this.u32(bytes.length);
        this._push(bytes);
    }

    opt(v, writeFn) {
        if (v != null) { this.bool(true); writeFn.call(this, v); }
        else           { this.bool(false); }
    }

    range(r) {
        this.u32(r.start.line); this.u32(r.start.character);
        this.u32(r.end.line);   this.u32(r.end.character);
    }

    /** Finalize to a single ArrayBuffer. */
    finish() {
        const buf = new ArrayBuffer(this._size);
        const out = new Uint8Array(buf);
        let off = 0;
        for (const part of this._parts) {
            out.set(part, off);
            off += part.length;
        }
        return buf;
    }

    _push(bytes) { this._parts.push(bytes); this._size += bytes.length; }
}
```

## Rust Wire Reader/Writer (planned: `src/lsp/wire.rs`)

```rust
use bytes::{Buf, BufMut, BytesMut};

#[repr(u8)]
pub enum MsgType { Notification = 0, Request = 1, Response = 2, Error = 3 }

pub struct FrameHeader {
    pub msg_type: MsgType,
    pub flags: u8,
    pub method: u16,
    pub id: u32,
}

impl FrameHeader {
    pub fn decode(buf: &[u8]) -> Self {
        Self {
            msg_type: match buf[0] { 1 => MsgType::Request, 2 => MsgType::Response,
                                      3 => MsgType::Error, _ => MsgType::Notification },
            flags: buf[1],
            method: u16::from_le_bytes([buf[2], buf[3]]),
            id: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
        }
    }

    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_u8(self.msg_type as u8);
        buf.put_u8(self.flags);
        buf.put_u16_le(self.method);
        buf.put_u32_le(self.id);
    }
}

/// Reading helpers — consume from a byte slice.
pub struct WireReader<'a> { buf: &'a [u8], pos: usize }

impl<'a> WireReader<'a> {
    pub fn new(payload: &'a [u8]) -> Self { Self { buf: payload, pos: 0 } }

    pub fn u8(&mut self) -> u8       { let v = self.buf[self.pos]; self.pos += 1; v }
    pub fn u16(&mut self) -> u16     { let v = u16::from_le_bytes(self.buf[self.pos..self.pos+2].try_into().unwrap()); self.pos += 2; v }
    pub fn u32(&mut self) -> u32     { let v = u32::from_le_bytes(self.buf[self.pos..self.pos+4].try_into().unwrap()); self.pos += 4; v }
    pub fn i32(&mut self) -> i32     { let v = i32::from_le_bytes(self.buf[self.pos..self.pos+4].try_into().unwrap()); self.pos += 4; v }
    pub fn f64(&mut self) -> f64     { let v = f64::from_le_bytes(self.buf[self.pos..self.pos+8].try_into().unwrap()); self.pos += 8; v }
    pub fn bool(&mut self) -> bool   { self.u8() != 0 }

    pub fn str(&mut self) -> &'a str {
        let len = self.u32() as usize;
        let s = std::str::from_utf8(&self.buf[self.pos..self.pos+len]).unwrap_or("");
        self.pos += len;
        s
    }

    pub fn opt<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> Option<T> {
        if self.bool() { Some(f(self)) } else { None }
    }

    pub fn rest(&self) -> &'a [u8] { &self.buf[self.pos..] }
}

/// Writing helpers — append to a BytesMut.
pub struct WireWriter { buf: BytesMut }

impl WireWriter {
    pub fn new() -> Self { Self { buf: BytesMut::with_capacity(256) } }
    pub fn with_header(header: &FrameHeader) -> Self {
        let mut w = Self::new();
        header.encode(&mut w.buf);
        w
    }

    pub fn u8(&mut self, v: u8)      { self.buf.put_u8(v); }
    pub fn u16(&mut self, v: u16)    { self.buf.put_u16_le(v); }
    pub fn u32(&mut self, v: u32)    { self.buf.put_u32_le(v); }
    pub fn i32(&mut self, v: i32)    { self.buf.put_i32_le(v); }
    pub fn f64(&mut self, v: f64)    { self.buf.put_f64_le(v); }
    pub fn bool(&mut self, v: bool)  { self.buf.put_u8(if v { 1 } else { 0 }); }

    pub fn str(&mut self, s: &str) {
        self.buf.put_u32_le(s.len() as u32);
        self.buf.put_slice(s.as_bytes());
    }

    pub fn opt<T>(&mut self, v: Option<&T>, f: impl FnOnce(&mut Self, &T)) {
        match v {
            Some(v) => { self.bool(true); f(self, v); }
            None    => { self.bool(false); }
        }
    }

    pub fn finish(self) -> Vec<u8> { self.buf.to_vec() }
}
```

## Size Comparison: JSON-RPC vs Binary Wire

Typical `document/change` message for a single character edit:

| Format | Size | Breakdown |
|--------|-----:|-----------|
| JSON-RPC | ~280 B | `{"jsonrpc":"2.0","method":"document/change","params":{"textDocument":{"uri":"file:///path/to/file.j","version":42},"contentChanges":[{"range":{"start":{"line":10,"character":5},"end":{"line":10,"character":5}},"text":"x"}]}}` |
| Binary | ~47 B | 8 (header) + 4 (version) + 4+27 (uri) + 4 (count) + 16 (range) + 4+1 (text) = ~68 B. Shorter URIs → even less |

Typical `parseResult` notification for a 200-line file:

| Format | Size |
|--------|-----:|
| JSON | ~15–50 KB (JSON keys repeated per token, per diagnostic, string quoting) |
| Binary | ~4–15 KB (raw u32 arrays, no keys, no quoting) |

---

---

## Migration Plan

### Phase 1 — Wire protocol (WebSocket binary framing)

Replace JSON-RPC text frames with binary frames on the WebSocket channel.

**Rust side:**
- Implement `WireReader` / `WireWriter` in `src/lsp/wire.rs`
- Replace `serde_json::from_str::<LspMessage>()` in `main.rs` with binary header dispatch
- Replace `serde_json::to_string()` in `send.rs` with `WireWriter`
- Remove `LspMessage`, `LspCall`, `RequestMessage`, `ResponseMessage` structs from `protocol.rs`
- Replace `MethodCall` enum's `#[serde(rename)]` with numeric method IDs
- Remove `serde_json` from the WebSocket path entirely

**JS side:**
- Implement `WireReader` / `WireWriter` in `extension/vendor/wire.js`
- Replace `ServerClient._sendText()` with binary frame writing
- Replace `JSON.parse()` in `_handleMessage()` with `WireReader`
- Switch WebSocket to binary mode (send `ArrayBuffer` instead of strings)
- Update `extension.js` to encode/decode binary payloads for document sync,
  parseResult, and all notification/request handlers

**Files changed:**
- `src/lsp/wire.rs` (new)
- `src/lsp/protocol.rs` (remove JSON serde, add method ID enum)
- `src/main.rs` (binary dispatch loop)
- `src/lsp/send.rs` (binary output)
- `src/http/ws.rs` (binary WebSocket frames)
- `extension/vendor/wire.js` (new)
- `extension/serverClient.js` (binary transport)
- `extension/extension.js` (binary encode/decode)

### Phase 2 — WOBJ + HTTP endpoints

Implement WOBJ columnar format for bulk data over HTTP.

- Implement `ColumnarWriter` in `src/util/columnar.rs`
- Implement `WobjReader` in `extension/vendor/columnar.js`
- Add HTTP endpoints: `/w3e/catalog/doodads`, `/w3e/catalog/units`, etc.
- Map editor webview uses `fetch()` → WOBJ binary directly
- Rendering requests (blp, mdx, doo, w3i, w3e) return binary over WebSocket
- MPQ read returns raw bytes (no base64)

### Phase 3 — Remove all JSON

- Remove `serde_json` from `Cargo.toml` dependencies (keep only in `[build-dependencies]`)
- Remove `#[derive(Serialize, Deserialize)]` from data structs where only
  binary encoding is used
- Remove all `JSON.parse()` / `JSON.stringify()` from extension JS
- HTTP endpoints return `application/octet-stream` exclusively

### Phase 4 — Browser version

Same Rust code compiles to WASM. Two modes:

**Mode A — WASM in-browser (local files):**
```
Browser
  └─ WASM module (compiled from same Rust crate)
       ├─ WireWriter → binary messages (via wasm_bindgen)
       └─ ColumnarWriter → WOBJ buffers
            → WobjReader / WireReader (same JS, no changes)
```

**Mode B — Remote server (cloud-hosted):**
```
Browser
  ├─ WebSocket (binary frames) → same wire protocol
  └─ fetch('/w3e/catalog/doodads') → same WOBJ binary
```

In both modes, the JS client code is **identical** — all communication
is `ArrayBuffer`, regardless of where it came from.

## References

- [Apache Arrow Columnar Format](https://arrow.apache.org/docs/format/Columnar.html) — inspiration for columnar layout
- [FlatBuffers](https://google.github.io/flatbuffers/) — why vtable overhead matters
- [`http/terrain.rs`](../../src/http/terrain.rs) — existing binary terrain endpoint (same philosophy)
- [`terrain.md`](./terrain.md) — existing terrain format documentation
- [`src/lsp/wire.rs`](../../src/lsp/wire.rs) — binary wire protocol (planned)
- [`src/util/columnar.rs`](../../src/util/columnar.rs) — WOBJ writer (planned)
- [`extension/vendor/wire.js`](../../extension/vendor/wire.js) — JS wire reader/writer (planned)
- [`extension/vendor/columnar.js`](../../extension/vendor/columnar.js) — JS WOBJ reader (planned)
- [`extension/serverClient.js`](../../extension/serverClient.js) — WebSocket transport (to be updated)
- [`extension/extension.js`](../../extension/extension.js) — extension entry point (to be updated)

