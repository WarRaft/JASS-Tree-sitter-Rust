# war3map.w3e — Terrain Format

> Based on [HiveWE wiki](https://github.com/stijnherfst/HiveWE/wiki/war3map.w3e-Terrain)
>
> ImHex pattern: [`w3e.hexpat`](../../src/lng/w3e/w3e.hexpat) — Rust parser: [`parse.rs`](../../src/lng/w3e/parse.rs) — renderer: [`terrain.js`](../../extension/mapEditor/terrain.js)

All multi-byte integers are **little-endian**.

## Overview

The map is divided into square **tiles**. Each tile has 4 corners called **tilepoints**.
A 256×256 map has 257×257 tilepoints. The first tilepoint in the file is the lower-left corner; data goes row by row (bottom → top).

```
point(0, H-1) ──── … ──── point(W-1, H-1)    ← top row
     │                          │
     …        (W-1)×(H-1)      …                cells
     │          cells           │
point(0, 0)  ──── … ──── point(W-1, 0)         ← bottom row
```

Data index: `idx = sy * W + sx` where `sx: 0..W-1`, `sy: 0..H-1`, `sy=0` is the bottom row.

## File Header

| Offset | Type | Name | Description |
|--------|------|------|-------------|
| 0x00 | `char[4]` | magic | `"W3E!"` |
| 0x04 | `s32` | version | Format version (`11`) |
| 0x08 | `char` | tileset | Base tileset type (see table below) |
| 0x09 | `s32` | customTileset | `0` = standard, `1` = custom tileset |
| 0x0D | `s32` | groundTileCount | Number of ground tilesets used |
| 0x11 | `char[4][N]` | groundTiles | Ground tileset rawcodes (`TerrainArt\Terrain.slk`) |
| … | `s32` | cliffTileCount | Number of cliff tilesets used |
| … | `char[4][N]` | cliffTiles | Cliff tileset rawcodes (`TerrainArt\CliffTypes.slk`) |
| … | `s32` | mapWidth | Map width + 1 (number of tilepoints) |
| … | `s32` | mapHeight | Map height + 1 (number of tilepoints) |
| … | `float` | offsetX | X offset from origin |
| … | `float` | offsetY | Y offset from origin |
| … | `Point[W×H]` | points | Tilepoint array (`mapHeight × mapWidth` entries) |

### Tileset codes

| Code | Tileset | Code | Tileset |
|:----:|---------|:----:|---------|
| `A` | Ashenvale | `N` | Northrend |
| `B` | Barrens | `O` | Outland |
| `C` | Felwood | `Q` | Village Fall |
| `D` | Dungeon | `V` | Village |
| `F` | Lordaeron Fall | `W` | Lordaeron Winter |
| `G` | Underground | `X` | Dalaran |
| `I` | Icecrown Glacier | `Y` | Cityscape |
| `J` | Dalaran Ruins | `Z` | Sunken Ruins |
| `K` | Black Citadel | `L` | Lordaeron Summer |

### Tileset MPQ archives

Warcraft III ships a separate MPQ archive for each tileset, named `{tileset_code}.mpq` (e.g. `L.mpq` for Lordaeron Summer). These archives contain tileset-specific assets:

- **Cliff textures** — `ReplaceableTextures\Cliff\Cliff0.blp`, `Cliff1.blp`
- **Terrain textures** — ground tile BLPs
- **Water textures** — animated water frames
- **Doodad models/textures** — environment-specific props
- **Unit textures** — tileset-specific unit skins

When a map is opened and its `war3map.w3e` is parsed, the tileset letter is stored globally on the server side (`set_tileset`). From that point on, **all** file lookups (`lookup_file`) automatically include `{tileset}.mpq` in the cascade — not just cliff textures, but SLK data, models, unit textures, doodads, and everything else. The tileset MPQ is searched **first** among the MPQ chain, before `War3Patch.mpq` and other standard archives.

### Tileset IDs

- **Ground** — 4-char rawcodes, e.g. `"Ldrt"` = Lordaeron Summer Dirt. Lookup in `TerrainArt\Terrain.slk`. Max 16 usable (4-bit index).
- **Cliff** — 4-char rawcodes, e.g. `"CLdi"` = Lordaeron Cliff Dirt. Lookup in `TerrainArt\CliffTypes.slk`. Max 15 usable (value `15` is reserved). The cliff tile list is actually ignored by the World Editor — it simply adds cliff tiles for each ground tile that has a cliff version.

### Offsets

```
offsetX = −(mapWidth  − 1) × 128 / 2
offsetY = −(mapHeight − 1) × 128 / 2
```

`128` is the world-unit size of one tile. The offset translates the origin to the centre of the map.

## Tilepoint (7 bytes)

```
byte 0–1:  u16              groundHeight
byte 2–3:  u16              waterHeight (14 bits) + edgeFlag (bit 14) + padding (bit 15)
byte 4:    u8               groundTexture (bits 0–3) + flags (bits 4–7)
byte 5:    u8               groundVariation (bits 0–4) + cliffVariation (bits 5–7)
byte 6:    u8               layerHeight (bits 0–3) + cliffTexture (bits 4–7)
```

### Byte 0–1 — `groundHeight` (`u16`)

Raw ground height. `8192` (`0x2000`) = zero height.

### Byte 2–3 — `waterHeight` + `edgeFlag` (`u16`)

| Bits | Mask | Field | Description |
|------|------|-------|-------------|
| 0–13 | `& 0x3FFF` | waterHeight | Water surface height |
| 14 | `& 0x4000` | edgeFlag | Camera boundary flag 1 (shadow on map edge) |
| 15 | | | padding |

### Byte 4 — `textureFlags` (`u8`)

| Bits | Mask | Field | Description |
|------|------|-------|-------------|
| 0–3 | `& 0x0F` | groundTexture | Index into the ground tileset list |
| 4 | `& 0x10` | ramp | Ramp flag — allows units to walk between layers |
| 5 | `& 0x20` | blight | Blight flag — Undead ground overlay |
| 6 | `& 0x40` | water | Water flag — enable water rendering |
| 7 | `& 0x80` | boundary | Boundary flag 2 — camera bounds area |

### Byte 5 — `variation` (`u8`)

| Bits | Mask | Field | Description |
|------|------|-------|-------------|
| 0–4 | `& 0x1F` | groundVariation | Texture variation (bones, holes, etc.) |
| 5–7 | `(& 0xE0) >> 5` | cliffVariation | Cliff model variation (0–7) |

### Byte 6 — `layer` (`u8`)

| Bits | Mask | Field | Description |
|------|------|-------|-------------|
| 0–3 | `& 0x0F` | layerHeight | Layer height (changed by cliffs) |
| 4–7 | `(& 0xF0) >> 4` | cliffTexture | Cliff texture index (value `15` reserved) |

## Height Calculation

### Final height

```js
const TILE    = 128   // world units per tile edge — also the layer height step
const H_ZERO  = 8192  // groundHeight baseline (raw zero level)
const H_SCALE = 4     // raw groundHeight → world units divisor

base        = (layerHeight − 2) * TILE            // layer contribution
deformation = (groundHeight − H_ZERO) / H_SCALE   // ground deformation
finalZ      = base + deformation
```

| Constant | Value | Meaning |
|----------|-------|---------|
| `TILE` | 128 | Layer step in world units. Raising layer by 1 in the editor = +128 |
| `H_ZERO` | 8192 | Raw `groundHeight` value that corresponds to zero elevation |
| `H_SCALE` | 4 | 4 raw `groundHeight` units = 1 world unit |
| Layer zero | 2 | `layerHeight` value that corresponds to base elevation |

> **Note on the HiveWE wiki formula.**
> The wiki presents the height as `(groundHeight − 8192 + (layer − 2) × 512) / 4`.
> The `512` there is **not** the actual layer step — it is an artefact of pulling the
> `/ 4` divisor outside the entire expression. The real layer step in world units
> is `512 / 4 = 128`, i.e. one tile edge. Our formula applies the division inline,
> making the constants match what you actually see in the World Editor.

### Water level

```
waterLevel = (waterHeight − H_ZERO) / H_SCALE − waterZero
```

Where `waterZero` is a per-tileset value from `TerrainArt\Water.slk` multiplied by `TILE` (e.g. `−0.7 × 128 = −89.6`).

## World Dimensions

```
points:  W × H               (e.g. 65 × 65)
cells:   (W−1) × (H−1)       (e.g. 64 × 64)
world:   cells × 128          (e.g. 64 × 128 = 8192)
```

## Ground Texture Transitions

A tile has 4 corner tilepoints, each with a `groundTexture` index. When a tile has multiple different textures at its corners, **transitions** are rendered by layering sub-tile shapes.

### Texture atlas layout

Each ground texture is a 4×4 grid of 16 sub-tiles. **Extended** textures are 8×4 (64×64 cells), with 16 additional full-tile variations in the right half.

```
Square (4×4):                    Extended (8×4):

┌────┬────┬────┬────┐            ┌────┬────┬────┬────┬────┬────┬────┬────┐
│  1 │  2 │  3 │  4 │            │  1 │  2 │  3 │  4 │ 17 │ 18 │ 19 │ 20 │
├────┼────┼────┼────┤            ├────┼────┼────┼────┼────┼────┼────┼────┤
│  5 │  6 │  7 │  8 │            │  5 │  6 │  7 │  8 │ 21 │ 22 │ 23 │ 24 │
├────┼────┼────┼────┤            ├────┼────┼────┼────┼────┼────┼────┼────┤
│  9 │ 10 │ 11 │ 12 │            │  9 │ 10 │ 11 │ 12 │ 25 │ 26 │ 27 │ 28 │
├────┼────┼────┼────┤            ├────┼────┼────┼────┼────┼────┼────┼────┤
│ 13 │ 14 │ 15 │ 16 │            │ 13 │ 14 │ 15 │ 16 │ 29 │ 30 │ 31 │ 32 │
└────┴────┴────┴────┘            └────┴────┴────┴────┴────┴────┴────┴────┘
```

Sub-tiles 1 and 16 are full fills (`ABCD`). Sub-tiles 17–32 (extended only) are additional full-tile variations for reducing repetition.

### Sub-tile shapes

Each sub-tile covers a specific combination of corners. The corners are labelled:

```
A(0,0) ── B(1,0)       A = TL (top-left)      B = TR (top-right)
  │          │          D = BL (bottom-left)   C = BR (bottom-right)
D(0,1) ── C(1,1)
```

Transition edges are cut at **25%** from each corner:

```
Top:    AB(.25, 0)   BA(.75, 0)
Right:  BC(1, .25)   CB(1, .75)
Bottom: CD(.75, 1)   DC(.25, 1)
Left:   DA(0, .75)   AD(0, .25)
```

The 16 sub-tiles and the corners they cover:

```
 1: ABCD (full)     2: C only        3: D only        4: CD
 5: B only          6: BC            7: BD             8: BCD
 9: A only         10: AC           11: AD            12: ACD
13: AB             14: ABC          15: ABD           16: ABCD (full)
```

### Transition algorithm

For each tile (cell), read `groundTexture` from its 4 corner tilepoints:

```js
const bl = groundTexture[cy * W + cx]          // bottom-left  = D
const br = groundTexture[cy * W + cx + 1]      // bottom-right = C
const tl = groundTexture[(cy + 1) * W + cx]    // top-left     = A
const tr = groundTexture[(cy + 1) * W + cx + 1] // top-right   = B
```

1. Collect unique texture indices. **Sort ascending.**
2. The **lowest** (first) texture always draws a **full fill** — it is the base layer.
3. For each subsequent texture, compute a 4-bit corner mask:

```
bit 3 (8) = TL matches
bit 2 (4) = TR matches
bit 1 (2) = BL matches
bit 0 (1) = BR matches
```

4. The mask value maps directly to a sub-tile position in the texture atlas:

| mask | binary | corners | sub-tile |
|:----:|:------:|---------|:--------:|
| 1 | `0001` | C (BR) | 2 |
| 2 | `0010` | D (BL) | 3 |
| 3 | `0011` | CD | 4 |
| 4 | `0100` | B (TR) | 5 |
| 5 | `0101` | BC | 6 |
| 6 | `0110` | BD | 7 |
| 7 | `0111` | BCD | 8 |
| 8 | `1000` | A (TL) | 9 |
| 9 | `1001` | AC | 10 |
| 10 | `1010` | AD | 11 |
| 11 | `1011` | ACD | 12 |
| 12 | `1100` | AB | 13 |
| 13 | `1101` | ABC | 14 |
| 14 | `1110` | ABD | 15 |
| 15 | `1111` | ABCD | 1 (full) |

Lookup table: `subtile = [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 1][mask]`

5. Layers are drawn bottom-to-top (lowest texture index first), so higher-index textures overlay lower ones.

### Full-tile variation selection

When a tile is a full fill (`mask = 15`), the `groundVariation` field of the bottom-left corner selects which full-tile sub-tile to use:

- **Square texture**: cycles between sub-tiles `[1, 16]`.
- **Extended texture**: cycles through `[17, 18, …, 32, 1, 16]`.

```js
const pool = isExtended ? [17, 18, …, 32, 1, 16] : [1, 16]
subtile = pool[groundVariation % pool.length]
```

> **Note on the HiveWE wiki.**
> The wiki describes the same bit-mask algorithm using C++ `std::bitset` with
> `index[0]=BR, index[1]=BL, index[2]=TR, index[3]=TL` — identical bit assignments.
> However the wiki indexes variations 0–15 (0-based), while our atlas is 1–16
> (1-based), hence the `+1` offset in the lookup table. The result is the same
> texture region. Our implementation is verified pixel-perfect against the game.

### Blight

Each tilepoint has a boolean `blight` flag. When set, the blight texture replaces the ground texture. Blight draws on top of all ground textures but below cliff textures.

### Cliff ground tiles

When a tilepoint is a corner of at least one cliff cell (where corner `layerHeight` values differ), the cliff type's `groundTile` from `TerrainArt\CliffTypes.slk` replaces its normal ground texture for rendering. Only the four corner points of the cliff cell itself are affected — adjacent points that belong exclusively to flat cells keep their original texture.

```
 ── flat cell ──┬── cliff cell ──
 │              │               │
 TL ─────────── TR/TL ──────── TR     ← TR/TL is shared between cells
 │              │               │
 │   flat       │    cliff      │
 │              │               │
 BL ─────────── BR/BL ──────── BR
 │              │               │
 ── flat cell ──┴── cliff cell ──

TR/TL and BR/BL are corners of the cliff cell,
so their ground texture is replaced with groundTile.
TL and BL of the flat cell are NOT cliff corners,
so they keep their original texture.
The flat cell renders a transition between the two.
```

See [Ground tile replacement](#ground-tile-replacement-groundtile) for details.

**Draw priority:** cliff `groundTile` → blight → groundTexture.

## Packed flags (webview transport)

The Rust server packs per-point boolean flags into a single byte for efficient transfer:

| Bit | Flag |
|-----|------|
| 0 | water |
| 1 | boundary |
| 2 | blight |
| 3 | ramp |

## Cliffs

Cliffs are `.mdx` models selected based on `layerHeight` differences between the 4 corners of a tile.

### `TerrainArt\CliffTypes.slk`

Each cliff tile rawcode listed in the w3e header maps to a row in `TerrainArt\CliffTypes.slk`. The SLK columns define everything needed to render a cliff type:

| Column | Example | Description |
|--------|---------|-------------|
| `cliffID` | `CLdi` | 4-char rawcode (matches the w3e cliff tile list) |
| `cliffModelDir` | `Cliffs` | Subdirectory under `Doodads\Terrain\` for cliff wall models |
| `rampModelDir` | `CliffTrans` | Subdirectory for ramp / slope transition models |
| `cliffClass` | `c1`, `c2` | Cliff class identifier |
| `texDir` | `ReplaceableTextures\Cliff` | Directory for cliff wall textures |
| `texFile` | `Cliff0` | Cliff wall texture filename (without extension) |
| `groundTile` | `Ldrt` | Ground tile rawcode that replaces the terrain near cliffs |
| `upperTile` | `_` | Upper tile override (usually `_` = none) |

Each tileset typically has **two** cliff types that form a pair — one with `cliffClass = "c1"` (using `Cliff1` texture) and one with `cliffClass = "c2"` (using `Cliff0` texture). Some tilesets have a `CityCliffs` variant alongside the regular `Cliffs`.

### `cliffTexture` index

The tilepoint field `cliffTexture` (byte 6, bits 4–7) is a 4-bit index into the cliff tile list from the w3e header (analogous to how `groundTexture` indexes the ground tile list). The value `15` is reserved (no cliff). The index determines which `CliffTypes.slk` row to use, and thus which model directory, textures, and ground tile override apply to that cliff cell.

### Cliff detection

A tile (cell) is a cliff when its 4 corner `layerHeight` values are not all equal:

```js
const base = Math.min(lBL, lBR, lTL, lTR)
const peak = Math.max(lBL, lBR, lTL, lTR)
if (base !== peak) { /* this cell is a cliff */ }
```

The `cliffTexture` index is read from the **bottom-left** corner of the cell.

### Filename derivation

The model filename is built from the `cliffModelDir` of the matched `CliffTypes.slk` row and the `layerHeight` differences:

```
base = min(bottomLeft, bottomRight, topLeft, topRight)

pattern  = char('A' + topLeft     − base)
         + char('A' + topRight    − base)
         + char('A' + bottomRight − base)
         + char('A' + bottomLeft  − base)

filename = cliffModelDir               // e.g. "Cliffs" or "CityCliffs"
         + pattern
         + min(cliffVariation, maxVariation)  // clamped to max from Cliffs.slk / CityCliffs.slk
         + ".mdx"
```

The `maxVariation` per pattern is stored in `data/warcraft/Cliffs.slk` (for regular `Cliffs` directory) and `data/warcraft/CityCliffs.slk` (for `CityCliffs` directory). HiveWE clamps the variation: `std::clamp(cliff_variation, 0, cliff_variations[pattern])` (terrain.ixx line 1080).

Full path: `Doodads\Terrain\{cliffModelDir}\{filename}`

Example: layer heights `[TL=13, TR=12, BR=12, BL=12]`, `cliffModelDir = "Cliffs"` → differences `[1, 0, 0, 0]` → `Doodads\Terrain\Cliffs\CliffsBAAAx.mdx`.

Height differences greater than 2 (letter `C`) are skipped — models with `D` or higher do not exist. Cells where all differences are 0 (`AAAA`) are also skipped.

### CityCliffs

Same naming pattern but with `cliffModelDir = "CityCliffs"` and `rampModelDir = "CityCliffTrans"`. The cliff type is determined by the `cliffTexture` index → `CliffTypes.slk` row, so both regular and city cliff types coexist in the same SLK.

### Ground tile replacement (`groundTile`)

Each `CliffTypes.slk` entry has a `groundTile` field — a rawcode (e.g. `"Ldrt"`) referencing `TerrainArt\Terrain.slk`. When a tilepoint is a corner of a cliff cell (i.e. a quad whose 4 corners have different `layerHeight` values), the engine replaces that point's displayed ground texture with the cliff type's `groundTile` texture. Only the four corners of the cliff cell are overridden — points that belong exclusively to adjacent flat cells keep their original texture.

#### `upperTile` — peak corner override

Some cliff types also define an `upperTile` field (e.g. Outland `COrd` has `upperTile = "Osmb"`). When `upperTile` is not `"_"` (empty/none), cliff corners whose `layerHeight` equals the **peak** (maximum) of the cell's four corners use `upperTile` instead of `groundTile`. This allows the top of a cliff to display a different ground texture than the base — for example, Outland cliffs show abyss (`Oaby`) at the bottom and scorched magma (`Osmb`) at the top.

**Priority:** `upperTile` (peak corners) → `groundTile` (other cliff corners) → blight → normal `groundTexture`.

Because each tilepoint can be a corner of up to 4 cells, a single tilepoint may participate in both cliff and flat cells. The override is per-point: if the point is a corner of **any** cliff cell, its texture is replaced.

```
        flat cell          cliff cell
   ┌───────────────┬───────────────┐
   │               │               │
   │ grass  grass  │ DIRT    DIRT  │    ← tilepoint textures
   │               │               │
   │ grass  grass  │ DIRT    DIRT  │    DIRT = groundTile override
   │               │               │
   └───────────────┴───────────────┘

The flat cell's right-side corners are shared with the cliff cell,
so they become DIRT. The left-side corners stay grass.
→ The flat cell renders a grass-to-dirt transition.
```

**Draw priority:** cliff `groundTile` → blight → normal `groundTexture`.

**Example:** On a Lordaeron Summer map with cliff type `CLdi` (`groundTile = "Ldrt"`), the tilepoints that are corners of cliff cells render as Lordaeron Dirt, even if the mapper painted them as grass. Adjacent flat cells that share those corner points will show a smooth transition from their original texture to dirt.

### Cliff wall textures

Cliff wall models use **replaceable textures**. The `texDir` and `texFile` fields from `CliffTypes.slk` determine the wall texture path:

```
{texDir}\{texFile}.blp        (e.g. ReplaceableTextures\Cliff\Cliff0.blp)
```

The tileset-specific appearance comes from **which MPQ archive is searched**, not from filename prefixes. Warcraft III ships a separate MPQ per tileset (e.g. `L.mpq` for Lordaeron Summer, `A.mpq` for Ashenvale, etc.). Each tileset MPQ contains its own `ReplaceableTextures\Cliff\Cliff0.blp` and `Cliff1.blp` with unique textures for that environment.

#### Lookup cascade

The file lookup searches in this order (used for **all** game file lookups, not just cliffs):

1. Map archive (custom map may override textures)
2. **`{tileset}.mpq`** — automatically included when map's w3e tileset is known
3. Game folder (loose files on disk)
4. `War3Patch.mpq`
5. `War3xLocal.mpq`
6. `War3x.mpq`
7. `War3.mpq` — base game archive (generic fallback textures)

The tileset is stored globally when `war3map.w3e` is parsed, so **every** `lookup_file` call benefits — SLK loading, texture fetching, model resolution, etc.

#### Replaceable texture IDs

Cliff models have a single material with **Replaceable ID 11**. The actual texture is resolved from the cliff type's `texDir` and `texFile` fields in `CliffTypes.slk`:

```
{texDir}\{texFile}.blp        (e.g. ReplaceableTextures\Cliff\Cliff0.blp)
```

The tileset-specific appearance comes from searching the tileset MPQ (e.g. `L.mpq`) which contains its own versions of `Cliff0.blp` and `Cliff1.blp`.

### Cliff deformation

Cliff models are **not** stretched or scaled. They are placed at the base layer height and each vertex is Z-shifted by the terrain height at its world position. HiveWE implements this in the vertex shader:

```glsl
// cliff.vert (HiveWE)
// 1. Un-rotate WC3 cliff model (rotated 90° in MDX) and convert to tile coords
vec3 rotated = vec3(vPosition.y, -vPosition.x, vPosition.z) / 128.0 + vOffset.xyz;

// 2. Sample terrain height at this vertex's integer tile position
ivec2 height_pos = ivec2(rotated.xy);
float height = cliff_levels[height_pos.y * map_size.x + height_pos.x];

// 3. Compute terrain normal from 4 neighbor height samples
float hL = cliff_levels[height_pos.y * map_size.x + max(height_pos.x - 1, 0)];
float hR = cliff_levels[height_pos.y * map_size.x + min(height_pos.x + 1, map_size.x)];
float hD = cliff_levels[max(height_pos.y - 1, 0) * map_size.x + height_pos.x];
float hU = cliff_levels[min(height_pos.y + 1, map_size.y) * map_size.x + height_pos.x];
vec3 terrain_normal = normalize(vec3(hL - hR, hD - hU, 2.0));

// 4. Final Z = model Z + terrain height
gl_Position = VP * vec4(rotated.xy, rotated.z + height, 1);

// 5. Blend model normal with terrain normal for continuous lighting
vec3 rotated_normal = vec3(vNormal.y, -vNormal.x, vNormal.z);
Normal = normalize(vec3(rotated_normal.xy + terrain_normal.xy,
                        rotated_normal.z * terrain_normal.z));
```

Where:
- `vOffset.xyz` = `(cellX, cellY, min_layer_height - 2)` — instance position in tile coords
- `cliff_levels[]` = `ground_heights[]` = `height` (ground deformation only, in tile units; **not** `final_ground_height`). Despite the variable name in the shader, the cliff shader binds `ground_height_buffer` (terrain.ixx line 578), which stores `corners[i][j].height` = `(rawGroundHeight - 8192) / 512` without any layer contribution. The layer contribution is already in `vOffset.z` and in the cliff model geometry itself.
- `vPosition` = original MDX vertex position (128 units = 1 tile)

This means each cliff vertex independently samples the terrain height at its own XY position. The cliff model conforms to the terrain surface: if one side is on higher ground, that side of the cliff shifts up accordingly. Different height differences between corners are handled by **different models** (`BAAA`, `ABBA`, etc.), not by scaling.

## Ramps

Ramp transitions span 2 tiles. One side (left/right or top/bottom) has the ramp flag set, the other doesn't. The transition model filename uses different character encodings for ramp vs non-ramp corners:

- **Non-ramp corners**: `'A' + (layerHeight - base)` → `A`, `B`, `C`
- **Ramp corners**: `'L' + (layerHeight - base) * (-4)` → `L` (diff 0), `H` (diff 1)

```
Non-ramp:   A=65 (diff 0)   B=66 (diff 1)   C=67 (diff 2)
Ramp:       L=76 (diff 0)   H=72 (diff 1)
```

The editor and game do **not** load models containing `X` or `C`.

### Ramp transition model placement

HiveWE places ramp transition models (`CliffTrans`) before regular cliff models, in this order (terrain.ixx `update_cliff_meshes` lines 980-1083):

1. **Vertical ramp transitions** — spans 2 cells vertically (6 corners: BL, BR, TL, TR, TTL, TTR). One side (left or right) has ramp, the other doesn't. Model path: `Doodads\Terrain\CliffTrans\CliffTrans{pattern}0.mdx`. **Only placed if the model file exists** (HiveWE: `hierarchy.file_exists`). Patterns where all 4 outer corners are at base height (all A/L, no height change) have no corresponding model file and are skipped. Affected corners get the `romp` flag. Cell is skipped for further processing.

2. **Horizontal ramp transitions** — spans 2 cells horizontally (6 corners: BL, BR, TL, TR, BRR, TRR). Top or bottom side has ramp, the other doesn't. Same model path format and `file_exists` guard. Affected corners get the `romp` flag.

3. **Regular cliff models** — skips cells with `romp` flag or ramp entrance. Uses `cliffModelDir` from `CliffTypes.slk`.

The `romp` flag affects `ground_exists`: cells where the bottom-left corner has `romp` OR `cliff` flag (and is NOT a ramp entrance) have their terrain mesh hidden.

### Slope rendering (terrain mesh)

HiveWE boosts the **low corners** of ramp entrance cells by `+0.5` tile units (`64` world units), creating a slope through the ramp entrance cell itself (terrain.ixx `update_ground_heights` lines 882-919).

**Algorithm:**

For each tilepoint `(i, j)`:

1. `ramp_height = 0`
2. Check all **4 cells** this point belongs to (offsets `x: -1..0`, `y: -1..0`).
3. For each cell, get 4 corner `layerHeight` values. Compute `base = min(BL, BR, TL, TR)`.
4. If this point's `layerHeight ≠ base` → skip (only **low** corners get boosted).
5. If the cell is a **ramp entrance** (`is_corner_ramp_entrance`: all 4 corners have ramp flag AND `!(BL == TR && TL == BR)`) → `ramp_height = 0.5`, break.
6. `final_height = final_ground_height() + ramp_height` = `(height + layer_height - 2) + ramp_height` (tile units).

High corners are **not touched**. Only low corners at the base of a ramp entrance get the `+0.5` boost.

```
Standard ramp (layer 2 → 3, 2 rows of ramp points):

 row3: layer=3, no ramp  → final = 1.0    (128 wu)
 row2: layer=3, ramp     → final = 1.0    (128 wu, high corner — no boost)
 row1: layer=2, ramp     → final = 0.5    ( 64 wu, low corner — +0.5 boost)
 row0: layer=2, no ramp  → final = 0.0    (  0 wu)

 Cells:
  (0-1): 0 → 64   — entry slope
  (1-2): 64 → 128  — ramp entrance slope
  (2-3): 128 → 128 — flat high ground
```

The terrain vertex shader (`terrain.vert`) uses `final_ground_heights` for the Z position and `ground_heights` (deformation only) for normals — matching the cliff shader's normal calculation.

## Water

Water uses 45 looping textures from the tileset MPQ. Animation speed is in `TerrainArt\Water.slk`.

Colour and transparency interpolate based on depth (distance from water plane to ground).
Depth ranges from `UI\MiscData.txt`:

```
MinDepth  = 10
DeepLevel = 64
MaxDepth  = 72
```

Colours are defined in `TerrainArt\Water.slk` for shallow (MinDepth → DeepLevel) and deep (DeepLevel → MaxDepth) ranges.

The diagonal triangulation goes from top-left to bottom-right to match the World Editor.

## References

- [HiveWE C++ implementation](https://github.com/stijnherfst/HiveWE/blob/master/HiveWE/Terrain.cpp)
- [mdx-m3-viewer JavaScript implementation](https://github.com/flowtsohg/mdx-m3-viewer/tree/master/src/viewer/handlers/w3x)

