'use strict';

(async function () {
    const DATA = window.__W3E_DATA__
    const U = window._W3E_UTILS || null
    if (!DATA) return

    const vscode = (typeof acquireVsCodeApi === 'function') ? acquireVsCodeApi() : null

    W3E.init({
        vscode: vscode,
        groundTileCodes: DATA.groundTileCodes,
        cliffTileCodes: DATA.cliffTileCodes,
        isArchive: DATA.isArchive,
        initialDoodadsSlk: DATA.initialDoodadsSlk,
        initialDestructablesSlk: DATA.initialDestructablesSlk,
        initialUnitsSlk: DATA.initialUnitsSlk,
        initialCliffTypesSlk: DATA.initialCliffTypesSlk,
        doodadDooItems: DATA.doodadDooItems || [],
        unitDooItems: DATA.unitDooItems || [],
        w3rRegions: DATA.w3rRegions || null
    })

    // ── Notify the extension host that the WebView is ready ─────────────
    // The host delays emitGamePathChanged until this signal so that the
    // gamePathChanged message is never delivered before the JS listener
    // is registered (race condition when server data is cached).
    if (vscode) {
        vscode.postMessage({command: 'webviewReady'})
    }

    // ── Three.js setup ──────────────────────────────────────────
    try {

        const hasTerrain = DATA.hasTerrain
        const canvas = document.getElementById('terrain')
        const renderer = new THREE.WebGLRenderer({canvas, antialias: true})
        renderer.setPixelRatio(window.devicePixelRatio)

        const scene = new THREE.Scene()
        scene.background = new THREE.Color(0x1e1e1e)

        const camera = new THREE.PerspectiveCamera(50, 1, 1, 100000)
        camera.position.set(0, -5000, 3500)
        camera.lookAt(0, 0, 0)

        scene.add(new THREE.AmbientLight(0xffffff, 0.7))
        const dirLight = new THREE.DirectionalLight(0xffffff, 1.0)
        dirLight.position.set(1, 2, 1.5).normalize()
        scene.add(dirLight)
        const dirLight2 = new THREE.DirectionalLight(0xffffff, 0.3)
        dirLight2.position.set(-1, -1, 0.5).normalize()
        scene.add(dirLight2)

        let maxDim = 10000
        let mesh = null
        let _onAnimateWater = null  // set inside hasTerrain block

        if (hasTerrain) {
            const D = DATA.renderData

             // ── Binary data: fetch from HTTP server or decode base64 ─
             // When the binary HTTP server is available, we fetch raw bytes
             // directly — zero JSON/base64 overhead. Falls back to base64.
             const binaryUrl = DATA.binaryTerrainUrl

             if (binaryUrl) {
                 // ── Direct binary fetch (optimal path) ───────────
                 try {
                     const resp = await fetch(binaryUrl)
                     if (!resp.ok) {
                         throw new Error('HTTP ' + resp.status)
                     }
                     const buf = await resp.arrayBuffer()
                     const view = new DataView(buf)
                     let off = 0
                     D.w = view.getUint32(off, true); off += 4
                     D.h = view.getUint32(off, true); off += 4
                     D.offsetX = view.getFloat32(off, true); off += 4
                     D.offsetY = view.getFloat32(off, true); off += 4
                     D.totalTiles = view.getUint32(off, true); off += 4

                     const N = D.w * D.h
                     D.groundHeight = new Uint16Array(buf, off, N); off += N * 2
                     D.waterHeight = new Uint16Array(buf, off, N); off += N * 2
                     D.groundTexture = new Uint8Array(buf, off, N); off += N
                     D.groundVariation = new Uint8Array(buf, off, N); off += N
                     D.cliffVariation = new Uint8Array(buf, off, N); off += N
                     D.cliffTexture = new Uint8Array(buf, off, N); off += N
                     D.layerHeight = new Uint8Array(buf, off, N); off += N
                     D.flags = new Uint8Array(buf, off, N)
                 } catch (e) {
                     console.warn('Binary fetch failed, falling back to base64:', e)
                     decodeFallback(D)
                 }
             } else {
                 // ── Base64 fallback ──────────────────────────────
                 decodeFallback(D)
             }

            function decodeFallback(D) {
                function b64ToUint8(b64) {
                    const bin = atob(b64)
                    const u8 = new Uint8Array(bin.length)
                    for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i)
                    return u8
                }
                function b64ToUint16(b64) {
                    const bin = atob(b64)
                    const buf = new ArrayBuffer(bin.length)
                    const u8 = new Uint8Array(buf)
                    for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i)
                    return new Uint16Array(buf)
                }
                D.groundHeight = b64ToUint16(D.groundHeight)
                D.waterHeight = b64ToUint16(D.waterHeight)
                D.groundTexture = b64ToUint8(D.groundTexture)
                D.groundVariation = b64ToUint8(D.groundVariation)
                D.cliffVariation = b64ToUint8(D.cliffVariation)
                D.cliffTexture = b64ToUint8(D.cliffTexture)
                D.layerHeight = b64ToUint8(D.layerHeight)
                D.flags = b64ToUint8(D.flags)
            }
            // flags bitfield: bit0=water, bit1=boundary, bit2=blight, bit3=ramp
            // ── W3E data model ──────────────────────────────────
            // The map is stored as a grid of W×H tilepoints (vertices).
            // Between them there are (W-1)×(H-1) square cells (tiles).
            //
            //   point(0,H-1) ──── … ──── point(W-1,H-1)    ← top row
            //        │                        │
            //        …       (W-1)×(H-1)      …              cells
            //        │         cells           │
            //   point(0, 0) ──── … ──── point(W-1, 0)       ← bottom row
            //
            // Each tilepoint stores: groundHeight, layerHeight,
            // groundTexture, groundVariation, waterFlag, boundaryFlag, etc.
            //
            // Data index: idx = sy * W + sx   (sx: 0..W-1, sy: 0..H-1)
            // sy=0 is the bottom row, sy=H-1 is the top row.
            const W = D.w, H = D.h
            const TILE = 128      // world units per tile edge (128 = WC3 standard)
            const H_ZERO = 8192   // groundHeight baseline (sea level in raw units)
            const H_SCALE = 4     // groundHeight divisor → world Z units

            // Golden-angle HSL palette: generates visually distinct colours
            // for each tile texture index so they are easy to tell apart.
            function indexToColor(index) {
                const golden = 137.508 // golden angle in degrees
                const hue = (index * golden) % 360
                const sat = 0.55 + 0.15 * ((index % 3) / 2)
                const lum = 0.45 + 0.10 * ((index % 5) / 4)
                const c = (1 - Math.abs(2 * lum - 1)) * sat
                const x = c * (1 - Math.abs(((hue / 60) % 2) - 1))
                const m = lum - c / 2
                let r, g, b
                if (hue < 60) {
                    r = c
                    g = x
                    b = 0
                } else if (hue < 120) {
                    r = x
                    g = c
                    b = 0
                } else if (hue < 180) {
                    r = 0
                    g = c
                    b = x
                } else if (hue < 240) {
                    r = 0
                    g = x
                    b = c
                } else if (hue < 300) {
                    r = x
                    g = 0
                    b = c
                } else {
                    r = c
                    g = 0
                    b = x
                }
                return [r + m, g + m, b + m]
            }

            const palette = []
            for (let i = 0; i < D.totalTiles; i++) palette.push(indexToColor(i))

            // ── World dimensions ─────────────────────────────────
            // World size = number of cells × cell size, NOT number of points.
            // Points:  W × H         (e.g. 65 × 65)
            // Cells:   (W-1) × (H-1) (e.g. 64 × 64)
            // World:   cells × TILE   (e.g. 64 × 128 = 8192)
            const worldW = (W - 1) * TILE
            const worldH = (H - 1) * TILE
            maxDim = Math.max(worldW, worldH)

            camera.far = maxDim * 20
            camera.position.set(0, -maxDim * 0.7, maxDim * 0.5)
            camera.lookAt(0, 0, 0)
            camera.updateProjectionMatrix()

            let showWater = true, showBoundary = false, showBlight = false, showRamp = false
            let showDeformation = true
            let showSlopes = true
            let showCliffs = true

            // ── Geometry ─────────────────────────────────────────
            // PlaneGeometry(worldW, worldH, W-1, H-1) creates exactly
            // W×H vertices and (W-1)×(H-1) quads — a 1:1 match with
            // tilepoints and cells.
            //
            // Vertex layout (stride = W):
            //   vi = gj * W + gi     (gi: 0..W-1, gj: 0..H-1)
            //
            // THREE.js gj=0 → y = +worldH/2 (top of screen)
            // W3E      sy=0 → bottom of map
            // Mapping: idx = (H - 1 - gj) * W + gi
            const geo = new THREE.PlaneGeometry(worldW, worldH, W - 1, H - 1)

            // ── Ramp / slope height adjustment ─────────────────
            // Matching HiveWE update_ground_heights (terrain.ixx lines 882-919).
            // For each tilepoint, check all 4 cells it belongs to.
            // If the point is a LOW corner (layer_height == cell base)
            // of a ramp entrance cell → boost by +0.5 tile units.
            // High corners stay at their original height.
            // This creates a slope through the ramp entrance cell.
            // Returns Float32Array (boost produces fractional values).
            function computeRampLayerHeight() {
                const N = W * H
                const adjusted = new Float32Array(N)
                for (let i = 0; i < N; i++) adjusted[i] = D.layerHeight[i]

                for (let sy = 0; sy < H; sy++) {
                    for (let sx = 0; sx < W; sx++) {
                        const idx = sy * W + sx
                        const myLayer = D.layerHeight[idx]
                        let boost = 0

                        // Check all 4 cells this point can be a corner of
                        // (cell BL at (sx+xoff, sy+yoff))
                        for (let yoff = -1; yoff <= 0 && boost === 0; yoff++) {
                            for (let xoff = -1; xoff <= 0 && boost === 0; xoff++) {
                                const cx = sx + xoff
                                const cy = sy + yoff
                                if (cx < 0 || cx >= W - 1 || cy < 0 || cy >= H - 1) continue

                                const iBL = cy * W + cx
                                const iBR = cy * W + cx + 1
                                const iTL = (cy + 1) * W + cx
                                const iTR = (cy + 1) * W + cx + 1

                                const lBL = D.layerHeight[iBL]
                                const lBR = D.layerHeight[iBR]
                                const lTL = D.layerHeight[iTL]
                                const lTR = D.layerHeight[iTR]

                                const base = Math.min(lBL, lBR, lTL, lTR)

                                // Only low corners (at base height) get the boost
                                if (myLayer !== base) continue

                                // Is this cell a ramp entrance?
                                // All 4 corners have ramp flag AND heights
                                // are NOT diagonally symmetric.
                                if ((D.flags[iBL] & 8) && (D.flags[iBR] & 8) &&
                                    (D.flags[iTL] & 8) && (D.flags[iTR] & 8) &&
                                    !(lBL === lTR && lTL === lBR)) {
                                    boost = 0.5
                                }
                            }
                        }

                        adjusted[idx] += boost
                    }
                }
                return adjusted
            }

            // ── Height formula ──────────────────────────────────
            // Base height:   (layerHeight - 2) * TILE
            // Deformation:   (groundHeight - 8192) / 4
            // Final Z:       base + deformation (if enabled)
            // Slopes:        ramp-adjusted layerHeight (if enabled)
            function applyHeights() {
                const pos = geo.attributes.position
                const norm = geo.attributes.normal
                const layer = showSlopes ? computeRampLayerHeight() : D.layerHeight
                for (let gj = 0; gj < H; gj++) {
                    for (let gi = 0; gi < W; gi++) {
                        const vi = gj * W + gi
                        const idx = (H - 1 - gj) * W + gi
                        let h = (layer[idx] - 2) * TILE
                        if (showDeformation) {
                            h += (D.groundHeight[idx] - H_ZERO) / H_SCALE
                        }
                        pos.setZ(vi, h)
                    }
                }
                pos.needsUpdate = true

                // Compute normals from deformation-only height (matching HiveWE
                // terrain.vert lines 49-53 & cliff.vert lines 29-33).
                // Both terrain and cliff normals must use the same height source
                // (ground_heights / deformation only — no layer_height) so that
                // lighting is continuous at the terrain-cliff boundary.
                // Using computeVertexNormals() would include layer_height steps,
                // producing normals inconsistent with the cliff shader and causing
                // a visible shadow seam where cliff models meet the terrain mesh.
                for (let gj = 0; gj < H; gj++) {
                    for (let gi = 0; gi < W; gi++) {
                        const vi = gj * W + gi
                        const i = gi
                        const j = H - 1 - gj

                        const iL = Math.max(i - 1, 0)
                        const iR = Math.min(i + 1, W - 1)
                        const jD = Math.max(j - 1, 0)
                        const jU = Math.min(j + 1, H - 1)

                        let hL = 0, hR = 0, hD = 0, hU = 0
                        if (showDeformation) {
                            hL = (D.groundHeight[j * W + iL] - H_ZERO) / H_SCALE
                            hR = (D.groundHeight[j * W + iR] - H_ZERO) / H_SCALE
                            hD = (D.groundHeight[jD * W + i] - H_ZERO) / H_SCALE
                            hU = (D.groundHeight[jU * W + i] - H_ZERO) / H_SCALE
                        }

                        const nx = hL - hR
                        const ny = hD - hU
                        const nz = 2.0 * TILE
                        const len = Math.sqrt(nx * nx + ny * ny + nz * nz)

                        norm.setXYZ(vi, nx / len, ny / len, nz / len)
                    }
                }
                norm.needsUpdate = true

                // Notify region overlays to re-sync their Z values
                document.dispatchEvent(new Event('terrain-heights-changed'))
            }

            applyHeights()

            // ── Cliff ground-tile override ────────────────────────
            // Per-tilepoint override matching HiveWE real_tile_texture
            // (terrain.ixx lines 670-707). For each tilepoint, check
            // the 2×2 neighborhood of cells/corners for cliff or romp.
            // When a nearby corner has romp or the cell has a cliff,
            // the tilepoint's ground texture is replaced by the cliff
            // type's groundTile rawcode. Ramp entrance cells (all 4
            // corners have ramp AND none have romp) are an exception —
            // they skip the override entirely (goto out_of_loop).
            let _cliffGroundOverride = null // Int8Array, -1 = no override

            function computeCliffGroundOverride(romp, cliffCells) {
                const arr = new Int8Array(W * H).fill(-1)
                const groundCodes = DATA.groundTileCodes || []
                const ctMap = DATA.cliffTypeMap || {}
                const rompMap = romp || new Map()
                const cellMap = cliffCells || new Map()

                if (groundCodes.length === 0 || Object.keys(ctMap).length === 0) return arr

                // Build groundTile rawcode → ground tile index
                const groundCodeIndex = {}
                for (let i = 0; i < groundCodes.length; i++) {
                    const c = typeof groundCodes[i] === 'string' ? groundCodes[i] : groundCodes[i].text || ''
                    if (c) groundCodeIndex[c] = i
                }

                // Resolve cliff rawcode → ground tile index via ctMap
                function _rawcodeToGround(rawcode) {
                    if (!rawcode) return -1
                    const ct = ctMap[rawcode]
                    if (!ct || !ct.groundTile) return -1
                    const gtIdx = groundCodeIndex[ct.groundTile]
                    return gtIdx !== undefined ? gtIdx : -1
                }

                // Resolve cliff rawcode → upper tile index via ctMap
                function _rawcodeToUpper(rawcode) {
                    if (!rawcode) return -1
                    const ct = ctMap[rawcode]
                    if (!ct || !ct.upperTile || ct.upperTile === '_') return -1
                    const utIdx = groundCodeIndex[ct.upperTile]
                    return utIdx !== undefined ? utIdx : -1
                }

                // Precompute cliff flag per cell BL corner (HiveWE compute_cliff_flags):
                // corners[x][y].cliff = layer heights of (x,y),(x+1,y),(x,y+1),(x+1,y+1) differ
                const cellW = W - 1, cellH = H - 1
                const cliffFlag = new Uint8Array(W * H) // indexed by corner position
                for (let cy = 0; cy < cellH; cy++) {
                    for (let cx = 0; cx < cellW; cx++) {
                        const iBL = cy * W + cx
                        const lBL = D.layerHeight[iBL]
                        if (lBL !== D.layerHeight[cy * W + cx + 1] ||
                            lBL !== D.layerHeight[(cy + 1) * W + cx] ||
                            lBL !== D.layerHeight[(cy + 1) * W + cx + 1]) {
                            cliffFlag[iBL] = 1
                        }
                    }
                }

                // Per-tilepoint: check 2×2 neighborhood of cells/corners
                // matching HiveWE real_tile_texture (terrain.ixx lines 670-707)
                for (let sy = 0; sy < H; sy++) {
                    for (let sx = 0; sx < W; sx++) {
                        const idx = sy * W + sx
                        // i ∈ {-1, 0}, j ∈ {-1, 0} — matching HiveWE: for (int i=-1; i<1; i++)
                        check_loop:
                        for (let di = -1; di <= 0; di++) {
                            for (let dj = -1; dj <= 0; dj++) {
                                const nx = sx + di
                                const ny = sy + dj
                                if (nx < 0 || nx >= W || ny < 0 || ny >= H) continue
                                const nIdx = ny * W + nx

                                // Is the cell with BL at (nx, ny) a cliff?
                                const isCliff = (nx < cellW && ny < cellH) && cliffFlag[nIdx]

                                // Ramp entrance exception (HiveWE lines 674-684):
                                // cliff cell where all 4 corners have ramp AND
                                // none have romp → goto out_of_loop (skip override)
                                if (isCliff) {
                                    const iBL = ny * W + nx
                                    const iBR = ny * W + nx + 1
                                    const iTL = (ny + 1) * W + nx
                                    const iTR = (ny + 1) * W + nx + 1

                                    if ((D.flags[iBL] & 8) && (D.flags[iBR] & 8) &&
                                        (D.flags[iTL] & 8) && (D.flags[iTR] & 8) &&
                                        !rompMap.has(iBL) && !rompMap.has(iBR) &&
                                        !rompMap.has(iTL) && !rompMap.has(iTR)) {
                                        break check_loop // goto out_of_loop → no override
                                    }
                                }

                                // Romp or cliff → override ground texture.
                                // Resolve via RAWCODE stored at model placement time —
                                // never rely on per-corner D.cliffTexture indices which
                                // may point to a different cliff type at that tilepoint.
                                const hasRomp = rompMap.has(nIdx)
                                const hasCell = isCliff && cellMap.has(nIdx)
                                if (hasRomp || hasCell) {
                                    const rawcode = hasRomp ? rompMap.get(nIdx) : cellMap.get(nIdx)

                                    // Check if this tilepoint is at the peak of the cliff cell
                                    // and if an upperTile override exists.
                                    let tex = -1
                                    if (hasCell && nx < cellW && ny < cellH) {
                                        const iBL = ny * W + nx
                                        const iBR = ny * W + nx + 1
                                        const iTL = (ny + 1) * W + nx
                                        const iTR = (ny + 1) * W + nx + 1
                                        const peak = Math.max(
                                            D.layerHeight[iBL], D.layerHeight[iBR],
                                            D.layerHeight[iTL], D.layerHeight[iTR]
                                        )
                                        if (D.layerHeight[idx] === peak) {
                                            tex = _rawcodeToUpper(rawcode)
                                        }
                                    }
                                    if (tex < 0) tex = _rawcodeToGround(rawcode)
                                    if (tex >= 0) arr[idx] = tex
                                    break check_loop // first match wins (return in HiveWE)
                                }
                            }
                        }
                    }
                }

                return arr
            }

            _cliffGroundOverride = computeCliffGroundOverride()

            // ── Colour fallback texture ──────────────────────────
            // (W-1)×(H-1) cells rendered with the same transition
            // algorithm as tile textures, but filled with palette colours.
            const cellsX = W - 1
            const cellsY = H - 1

            // ── Transition shape polygons ────────────────────────
            // Each of the 16 sub-tiles is defined as a polygon in
            // normalised [0..1] cell coordinates:
            //
            //   A(0,0)──B(1,0)     Edge cut points at 25%:
            //    │       │          Top:    AB(.25,0)   BA(.75,0)
            //   D(0,1)──C(1,1)     Right:  BC(1,.25)   CB(1,.75)
            //                       Bottom: CD(.75,1)   DC(.25,1)
            //                       Left:   DA(0,.75)   AD(0,.25)
            //
            //  1: ABCD    2: C       3: D       4: CD
            //  5: B       6: BC      7: BD      8: BCD
            //  9: A      10: AC     11: AD     12: ACD
            // 13: AB     14: ABC    15: ABD    16: ABCD
            const TRANSITION_SHAPES = [
                null,                                                    // 0: unused
                [[0,0],[1,0],[1,1],[0,1]],                               // 1: ABCD
                [[1,.75],[1,1],[.75,1]],                                 // 2: C
                [[.25,1],[0,1],[0,.75]],                                 // 3: D
                [[0,.75],[0,1],[1,1],[1,.75]],                           // 4: CD
                [[.75,0],[1,0],[1,.25]],                                 // 5: B
                [[.75,0],[1,0],[1,1],[.75,1]],                           // 6: BC
                [[.75,0],[1,0],[1,.25],[.25,1],[0,1],[0,.75]],           // 7: BD
                [[.25,0],[1,0],[1,1],[0,1],[0,.25]],                     // 8: BCD
                [[0,0],[.25,0],[0,.25]],                                 // 9: A
                [[0,0],[.25,0],[1,.75],[1,1],[.75,1],[0,.25]],           // 10: AC
                [[0,0],[.25,0],[.25,1],[0,1]],                           // 11: AD
                [[0,0],[.75,0],[1,.25],[1,1],[0,1]],                     // 12: ACD
                [[0,0],[1,0],[1,.25],[0,.25]],                           // 13: AB
                [[0,0],[1,0],[1,1],[.25,1],[0,.75]],                     // 14: ABC
                [[0,0],[1,0],[1,.75],[.75,1],[0,1]],                     // 15: ABD
                [[0,0],[1,0],[1,1],[0,1]],                               // 16: ABCD
            ]

            const COLOR_CPX = 32
            const colorCanvas = document.createElement('canvas')
            colorCanvas.width = cellsX * COLOR_CPX
            colorCanvas.height = cellsY * COLOR_CPX
            const colorTex = new THREE.CanvasTexture(colorCanvas)
            colorTex.magFilter = THREE.LinearFilter
            colorTex.minFilter = THREE.LinearFilter

            // ── Cliff cell geometry removal ────────────────────
            // Instead of transparency tricks, we remove cliff cell
            // triangles from the terrain mesh by making them degenerate
            // (zero-area) in the index buffer — matching HiveWE's
            // ground_exists = false approach (terrain.vert line 62).
            // Cliff models provide the geometry for those cells.
            // The texture atlas still has data for cliff cells (for
            // bilinear filtering at cell boundaries).
            const _origIdx = geo.index.array.constructor === Uint32Array
                ? new Uint32Array(geo.index.array) : new Uint16Array(geo.index.array)

            // Corners with ramp transition models placed on them (HiveWE "romp" flag).
            // These cells have their terrain hidden, just like regular cliff cells.
            // Map stores cornerIndex → cliff rawcode (e.g. "CIsn") of the cell that placed the romp.
            let _romp = new Map()
            // Per cliff-cell: BL corner index → cliff rawcode resolved at model placement time.
            let _cliffCellRawcode = new Map()

            function _updateCliffCells() {
                const indices = geo.index.array
                // Restore original indices first
                indices.set(_origIdx)
                if (showCliffs) {
                    const gridX = W - 1
                    const gridY = H - 1
                    for (let iy = 0; iy < gridY; iy++) {
                        for (let ix = 0; ix < gridX; ix++) {
                            // Map geometry cell (ix,iy) to data cell
                            // geo iy=0 → top (data cy = H-2), iy=gridY-1 → bottom (data cy = 0)
                            const cy = gridY - 1 - iy
                            const cx = ix
                            const iBL = cy * W + cx
                            const iBR = cy * W + cx + 1
                            const iTL = (cy + 1) * W + cx
                            const iTR = (cy + 1) * W + cx + 1
                            const lBL = D.layerHeight[iBL]
                            const lBR = D.layerHeight[iBR]
                            const lTL = D.layerHeight[iTL]
                            const lTR = D.layerHeight[iTR]

                            // HiveWE ground_exists (terrain.ixx line 940):
                            //   ground_exists = !((cliff || romp) && !ramp_entrance)
                            const isCliff = !(lBL === lBR && lBR === lTL && lTL === lTR)
                            const isRomp = _romp.has(iBL)
                            if (!isCliff && !isRomp) continue

                            // Ramp entrance: all 4 corners have ramp flag AND
                            // heights are NOT diagonally symmetric → keep terrain
                            // visible (HiveWE terrain.ixx is_corner_ramp_entrance
                            // + update_ground_exists line 940).
                            const fBL = D.flags[iBL], fBR = D.flags[iBR],
                                  fTL = D.flags[iTL], fTR = D.flags[iTR]
                            const isRampEntrance = (fBL & 8) && (fBR & 8) && (fTL & 8) && (fTR & 8) &&
                                !(lBL === lTR && lTL === lBR)
                            if (isRampEntrance) continue

                            // Cliff/romp cell → make its 2 triangles degenerate (all same vertex)
                            const off = (iy * gridX + ix) * 6
                            const v0 = indices[off]
                            for (let k = 0; k < 6; k++) indices[off + k] = v0
                        }
                    }
                }
                geo.index.needsUpdate = true
            }

            function applyColors() {
                const ctx = colorCanvas.getContext('2d')
                ctx.clearRect(0, 0, colorCanvas.width, colorCanvas.height)

                for (let cy = 0; cy < cellsY; cy++) {
                    for (let cx = 0; cx < cellsX; cx++) {
                        const iBL = cy * W + cx
                        const iBR = cy * W + cx + 1
                        const iTL = (cy + 1) * W + cx
                        const iTR = (cy + 1) * W + cx + 1

                        // Read ground texture, applying cliff groundTile override.
                        // Cliff cells are always drawn (not skipped) so that
                        // bilinear texture filtering at the terrain–cliff
                        // boundary blends matching colours instead of mixing
                        // with transparent black → eliminates the dark seam.
                        const ov = showCliffs ? _cliffGroundOverride : null
                        const bl = ov && ov[iBL] >= 0 ? ov[iBL] : D.groundTexture[iBL]
                        const br = ov && ov[iBR] >= 0 ? ov[iBR] : D.groundTexture[iBR]
                        const tl = ov && ov[iTL] >= 0 ? ov[iTL] : D.groundTexture[iTL]
                        const tr = ov && ov[iTR] >= 0 ? ov[iTR] : D.groundTexture[iTR]

                        const unique = [...new Set([bl, br, tl, tr])].sort((a, b) => a - b)

                        const dstX = cx * COLOR_CPX
                        const dstY = (cellsY - 1 - cy) * COLOR_CPX

                        for (let li = 0; li < unique.length; li++) {
                            const L = unique[li]
                            const col = palette[L] || [0.5, 0.5, 0.5]

                            let mask = 0
                            if (li === 0 && unique.length > 1) {
                                mask = 15
                            } else {
                                if (bl === L) mask |= 2
                                if (br === L) mask |= 1
                                if (tl === L) mask |= 8
                                if (tr === L) mask |= 4
                            }

                            if (mask === 0) continue

                            const subtile = mask === 15
                                ? 1
                                : [0,2,3,4,5,6,7,8,9,10,11,12,13,14,15,1][mask]
                            const shape = TRANSITION_SHAPES[subtile]
                            if (!shape) continue

                            ctx.fillStyle = 'rgb(' + Math.round(col[0] * 255) + ',' +
                                Math.round(col[1] * 255) + ',' + Math.round(col[2] * 255) + ')'
                            ctx.beginPath()
                            ctx.moveTo(dstX + shape[0][0] * COLOR_CPX,
                                       dstY + shape[0][1] * COLOR_CPX)
                            for (let p = 1; p < shape.length; p++) {
                                ctx.lineTo(dstX + shape[p][0] * COLOR_CPX,
                                           dstY + shape[p][1] * COLOR_CPX)
                            }
                            ctx.closePath()
                            ctx.fill()
                        }

                        // Per-cell flag overlays (bottom-left point flags)
                        const fi = cy * W + cx
                        const ff = D.flags[fi]
                        if (showWater && (ff & 1)) {
                            ctx.fillStyle = 'rgba(0,60,200,0.4)'
                            ctx.fillRect(dstX, dstY, COLOR_CPX, COLOR_CPX)
                        }
                        if (showBlight && (ff & 4)) {
                            ctx.fillStyle = 'rgba(180,0,0,0.3)'
                            ctx.fillRect(dstX, dstY, COLOR_CPX, COLOR_CPX)
                        }
                        if (showRamp && (ff & 8)) {
                            ctx.fillStyle = 'rgba(200,200,0,0.3)'
                            ctx.fillRect(dstX, dstY, COLOR_CPX, COLOR_CPX)
                        }
                        if (showBoundary && (ff & 2)) {
                            ctx.fillStyle = 'rgba(0,0,0,0.6)'
                            ctx.fillRect(dstX, dstY, COLOR_CPX, COLOR_CPX)
                        }
                    }
                }

                colorTex.needsUpdate = true
            }

            applyColors()
            _updateCliffCells()

            // ── Tile colour picker → update terrain ─────────────
            document.addEventListener('color-change', e => {
                const {index, color} = e.detail
                if (index >= 0 && index < palette.length) {
                    palette[index] = color
                    applyColors()
                }
            })

            const mat = new THREE.MeshPhongMaterial({map: colorTex, side: THREE.DoubleSide, specular: 0x000000, shininess: 0})
            mesh = new THREE.Mesh(geo, mat)
            scene.add(mesh)

            const TILE_TEXTURES = D.tileTextures || []
            let canvasTex = null
            let useTextures = true
            let _lastTileImages = null

            // ── Reusable sub-tile coordinate helper ──────────────
            const FILL_SQUARE = [1, 16]
            const FILL_RECT = []
            for (let f = 17; f <= 32; f++) FILL_RECT.push(f)
            FILL_RECT.push(1, 16)

            function subtileSrc(subtile, texW, texH) {
                const isRect = texW >= texH * 2
                const cellW = texW / (isRect ? 8 : 4)
                const cellH = texH / 4
                const n = subtile - 1 // 0-based
                let col, row
                if (isRect && n >= 16) {
                    // right half (sub-tiles 17-32)
                    const m = n - 16
                    col = 4 + (m % 4)
                    row = Math.floor(m / 4)
                } else {
                    // left half (sub-tiles 1-16) or square texture
                    col = n % 4
                    row = Math.floor(n / 4)
                }
                return {x: col * cellW, y: row * cellH, w: cellW, h: cellH}
            }

            // ── Build composited canvas from loaded tile images ──
            function buildComposited(tileImages) {
                _lastTileImages = tileImages
                // If no images loaded at all, fall back to palette mode
                const hasAnyImage = tileImages.some(function (img) { return img != null })
                if (!hasAnyImage) {
                    canvasTex = null
                    mat.map = colorTex
                    mat.transparent = false
                    mat.alphaTest = 0
                    mat.needsUpdate = true
                    return
                }

                const CPX = 32
                const c2 = document.createElement('canvas')
                c2.width = cellsX * CPX
                c2.height = cellsY * CPX
                const ctx = c2.getContext('2d')

                for (let cy = 0; cy < cellsY; cy++) {
                    for (let cx = 0; cx < cellsX; cx++) {
                        const iBL = cy * W + cx
                        const iBR = cy * W + cx + 1
                        const iTL = (cy + 1) * W + cx
                        const iTR = (cy + 1) * W + cx + 1

                        // Read ground texture, applying cliff groundTile override.
                        // Cliff cells are always drawn (not skipped) so that
                        // bilinear texture filtering at the terrain–cliff
                        // boundary blends matching colours instead of mixing
                        // with transparent black → eliminates the dark seam.
                        const ov = showCliffs ? _cliffGroundOverride : null
                        const bl = ov && ov[iBL] >= 0 ? ov[iBL] : D.groundTexture[iBL]
                        const br = ov && ov[iBR] >= 0 ? ov[iBR] : D.groundTexture[iBR]
                        const tl = ov && ov[iTL] >= 0 ? ov[iTL] : D.groundTexture[iTL]
                        const tr = ov && ov[iTR] >= 0 ? ov[iTR] : D.groundTexture[iTR]

                        const unique = [...new Set([bl, br, tl, tr])].sort((a, b) => a - b)

                        const dstX = cx * CPX
                        const dstY = (cellsY - 1 - cy) * CPX


                        for (let li = 0; li < unique.length; li++) {
                            const L = unique[li]
                            const img = tileImages[L]
                            if (!img) {
                                // Fallback: fill with palette colour
                                const col = palette[L] || [0.5, 0.5, 0.5]
                                ctx.fillStyle = 'rgb(' + Math.round(col[0] * 255) + ',' +
                                    Math.round(col[1] * 255) + ',' + Math.round(col[2] * 255) + ')'
                                ctx.fillRect(dstX, dstY, CPX, CPX)
                                continue
                            }

                            let mask = 0
                            if (li === 0 && unique.length > 1) {
                                mask = 15
                            } else {
                                if (bl === L) mask |= 2
                                if (br === L) mask |= 1
                                if (tl === L) mask |= 8
                                if (tr === L) mask |= 4
                            }

                            if (mask === 0) continue

                            const texW = img.naturalWidth
                            const texH = img.naturalHeight
                            const isRect = texW >= texH * 2
                            let subtile

                            if (mask === 15) {
                                const variation = D.groundVariation[iBL]
                                const pool = isRect ? FILL_RECT : FILL_SQUARE
                                subtile = pool[variation % pool.length]
                            } else {
                                subtile = [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 1][mask]
                            }

                            const src = subtileSrc(subtile, texW, texH)
                            ctx.drawImage(img, src.x, src.y, src.w, src.h,
                                dstX, dstY, CPX, CPX)
                        }

                        // Boundary darkening (same as applyColors but on composited canvas)
                        if (showBoundary) {
                            const fi = cy * W + cx
                            const ff = D.flags[fi]
                            if (ff & 2) {
                                ctx.fillStyle = 'rgba(0,0,0,0.6)'
                                ctx.fillRect(dstX, dstY, CPX, CPX)
                            }
                        }
                    }
                }

                // Use DataTexture instead of CanvasTexture to avoid
                // premultiplied-alpha issues: canvas 2D stores premultiplied
                // data, which can corrupt alpha when CanvasTexture uploads it.
                // getImageData() returns un-premultiplied RGBA → DataTexture
                // uploads it as straight RGBA, preserving alpha exactly.
                const imgData = ctx.getImageData(0, 0, c2.width, c2.height)
                canvasTex = new THREE.DataTexture(
                    new Uint8Array(imgData.data.buffer),
                    c2.width, c2.height,
                    THREE.RGBAFormat, THREE.UnsignedByteType
                )
                canvasTex.flipY = true
                canvasTex.magFilter = THREE.LinearFilter
                canvasTex.minFilter = THREE.LinearFilter
                canvasTex.needsUpdate = true
                if (useTextures) {
                    mat.map = canvasTex
                    // Keep terrain in opaque pass (transparent=false) to ensure it renders
                    // before water and writes to the depth buffer first — matching HiveWE
                    // render_ground() (opaque) → render_water() (depthMask=false) order.
                    // alphaTest still discards fully-transparent pixels (cliff cell edges).
                    mat.transparent = false
                    mat.alphaTest = 0.01
                    mat.needsUpdate = true
                }

                // ── Set tile previews on <tile-item> elements ────
                const tileItems = document.querySelectorAll('#tsGroundTiles tile-item')
                tileItems.forEach(el => {
                    const i = parseInt(el.getAttribute('index'), 10)
                    const img = tileImages[i]
                    if (!img) return
                    const src = subtileSrc(1, img.naturalWidth, img.naturalHeight)
                    const pc = document.createElement('canvas')
                    pc.width = src.w
                    pc.height = src.h
                    pc.getContext('2d').drawImage(img, src.x, src.y, src.w, src.h, 0, 0, src.w, src.h)
                    el.setAttribute('tile-preview', pc.toDataURL())
                })
            }

            // ── Load tile texture images and build composited canvas ──
            // Each ground tile texture is a 4×4 (or 8×4 for rectangular)
            // grid of sub-tiles. For each cell, we determine which textures
            // are present at the 4 corner points, sort them ascending,
            // and draw them bottom-to-top:
            //   1) The lowest texture always draws a full fill (base layer).
            //   2) Each subsequent texture draws a transition sub-tile
            //      covering only the corners that have exactly that texture.
            const _loadingBar = document.getElementById('globalLoadingBar')
            function _showLoading() { if (_loadingBar) _loadingBar.classList.add('active') }
            function _hideLoading() { if (_loadingBar) _loadingBar.classList.remove('active') }

             function loadAndComposite(textures) {
                 if (!textures || textures.length === 0) {
                     _hideLoading()
                     return
                 }
                 const tileImages = new Array(textures.length).fill(null)
                 let toLoad = 0
                 let loaded = 0

                 textures.forEach((entry, i) => {
                     if (!entry || !entry.dataUrl) return
                     toLoad++
                     const img = new Image()
                     img.onload = () => {
                         tileImages[i] = img
                         if (++loaded === toLoad) {
                             buildComposited(tileImages)
                             _hideLoading()
                         }
                     }
                     img.onerror = () => {
                         console.warn('[W3E TEX] image', i, 'load error')
                         if (++loaded === toLoad) {
                             buildComposited(tileImages)
                             _hideLoading()
                         }
                     }
                     img.src = entry.dataUrl
                 })

                 if (toLoad === 0) {
                     buildComposited(tileImages)
                     _hideLoading()
                 } else {
                     _showLoading()
                 }
             }

            // Initial load from render data
            loadAndComposite(TILE_TEXTURES)

            // Reload textures when game path changes — subscribe directly
             // to the 'status' source node for immediate reaction.
             // Fetches tile textures via HTTP (avoids large IPC payloads).
             W3E.onStatusChanged(function (status) {
                 const bs = DATA.binaryServer
                 const codes = DATA.groundTileCodes

                 if (!status || !status.hasPath) {
                     // Game path cleared → fall back to palette mode
                     loadAndComposite(codes ? codes.map(function () { return null }) : [])
                     return
                 }

                 if (bs && codes && codes.length > 0) {
                     // codes are Rawcode objects {raw, text} — extract text strings
                     const codeStrings = codes.map(function (c) { return typeof c === 'string' ? c : c.text })
                     const params = new URLSearchParams({token: bs.token, codes: codeStrings.join(',')})
                     if (DATA.archivePath) params.set('archive', DATA.archivePath)
                     const url = 'http://127.0.0.1:' + bs.port + '/mapEditor/tileTextures?' + params
                     _showLoading()
                     fetch(url)
                     .then(function (resp) {
                         return resp.ok ? resp.json() : null
                     })
                     .then(function (textures) {
                         if (textures) {
                             loadAndComposite(textures)
                         } else {
                             console.warn('[W3E TEX] no textures in response')
                             _hideLoading()
                         }
                     })
                     .catch(function (e) {
                         console.error('[W3E TEX] fetch error:', e)
                         _hideLoading()
                     })
                 } else {
                     console.warn('[W3E TEX] no binary server or no codes, cannot fetch textures')
                 }
             })

            // ── Wireframe grid ───────────────────────────────────
            // Two-level wireframe:
            //   fine   — every cell edge (white, low opacity)
            //   coarse — every BLOCK-th edge (yellow, higher opacity)
            // This matches the WC3 editor grid where every 4th line
            // is highlighted (BLOCK = 4 → 512 world units apart).
            const BLOCK = 4
            const gpos = geo.attributes.position

            function buildWireArrays() {
                const fine = [], coarse = []
                for (let j = 0; j < H; j++) {
                    const arr = j % BLOCK === 0 ? coarse : fine
                    for (let i = 0; i < W - 1; i++) {
                        const a = j * W + i, b = a + 1
                        arr.push(gpos.getX(a), gpos.getY(a), gpos.getZ(a), gpos.getX(b), gpos.getY(b), gpos.getZ(b))
                    }
                }
                for (let i = 0; i < W; i++) {
                    const arr = i % BLOCK === 0 ? coarse : fine
                    for (let j = 0; j < H - 1; j++) {
                        const a = j * W + i, b = (j + 1) * W + i
                        arr.push(gpos.getX(a), gpos.getY(a), gpos.getZ(a), gpos.getX(b), gpos.getY(b), gpos.getZ(b))
                    }
                }
                return {fine, coarse}
            }

            let wireData = buildWireArrays()
            const fineGeo = new THREE.BufferGeometry()
            fineGeo.setAttribute('position', new THREE.Float32BufferAttribute(wireData.fine, 3))
            const fineMesh = new THREE.LineSegments(fineGeo, new THREE.LineBasicMaterial({
                color: 0xffffff,
                transparent: true,
                opacity: 0.12
            }))
            fineMesh.visible = true
            scene.add(fineMesh)

            const coarseGeo = new THREE.BufferGeometry()
            coarseGeo.setAttribute('position', new THREE.Float32BufferAttribute(wireData.coarse, 3))
            const coarseMesh = new THREE.LineSegments(coarseGeo, new THREE.LineBasicMaterial({
                color: 0xffff00,
                transparent: true,
                opacity: 0.5
            }))
            coarseMesh.visible = true
            scene.add(coarseMesh)

            function rebuildWireframe() {
                wireData = buildWireArrays()
                fineGeo.setAttribute('position', new THREE.Float32BufferAttribute(wireData.fine, 3))
                coarseGeo.setAttribute('position', new THREE.Float32BufferAttribute(wireData.coarse, 3))
            }

            // ── Ramp diamond overlay ────────────────────────────
            // Draws a flat diamond (rhombus) outline at each tilepoint
            // that has the ramp flag set. Visible when cbRamp is checked.
            let rampDiamondMesh = null

            function buildRampDiamonds() {
                if (rampDiamondMesh) {
                    scene.remove(rampDiamondMesh)
                    rampDiamondMesh.geometry.dispose()
                    rampDiamondMesh = null
                }
                if (!showRamp) return

                const HALF = TILE * 0.25
                const LIFT = 8  // slight Z lift above terrain surface
                const verts = []
                const pos = geo.attributes.position

                for (let sy = 0; sy < H; sy++) {
                    for (let sx = 0; sx < W; sx++) {
                        const idx = sy * W + sx
                        if (!(D.flags[idx] & 8)) continue

                        const gj = H - 1 - sy
                        const vi = gj * W + sx
                        const x = pos.getX(vi)
                        const y = pos.getY(vi)
                        const z = pos.getZ(vi) + LIFT

                        // Diamond: 4 line segments forming a rhombus
                        verts.push(x, y + HALF, z,  x + HALF, y, z)  // top → right
                        verts.push(x + HALF, y, z,  x, y - HALF, z)  // right → bottom
                        verts.push(x, y - HALF, z,  x - HALF, y, z)  // bottom → left
                        verts.push(x - HALF, y, z,  x, y + HALF, z)  // left → top
                    }
                }

                if (verts.length === 0) return

                const dGeo = new THREE.BufferGeometry()
                dGeo.setAttribute('position', new THREE.Float32BufferAttribute(verts, 3))
                rampDiamondMesh = new THREE.LineSegments(dGeo,
                    new THREE.LineBasicMaterial({
                        color: 0xffff00,
                        depthTest: false,
                        transparent: true,
                        opacity: 0.85
                    }))
                rampDiamondMesh.renderOrder = 998
                scene.add(rampDiamondMesh)
            }

            // ── Checkbox state persistence ──────────────────────
            const savedState = (vscode && vscode.getState()) || {}
            const cbState = savedState.terrainChecks || {}

            function saveCbState() {
                if (!vscode) return
                const st = vscode.getState() || {}
                const checks = {};
                ['cbWater', 'cbBoundary', 'cbBlight', 'cbRamp', 'cbWireframe', 'cbTextures', 'cbDeformation', 'cbSlopes', 'cbCliffs', 'cbObjects'].forEach(id => {
                    const el = document.getElementById(id)
                    if (el) checks[id] = el.checked
                })
                st.terrainChecks = checks
                vscode.setState(st)
            }

            ['cbWater', 'cbBoundary', 'cbBlight', 'cbRamp', 'cbWireframe', 'cbTextures', 'cbDeformation', 'cbSlopes', 'cbCliffs', 'cbObjects'].forEach(id => {
                const el = document.getElementById(id)
                if (el && cbState[id] != null) el.checked = cbState[id]
            })

            const cbWaterEl = document.getElementById('cbWater')
            const cbBoundaryEl = document.getElementById('cbBoundary')
            const cbBlightEl = document.getElementById('cbBlight')
            const cbRampEl = document.getElementById('cbRamp')
            const cbWireframeEl = document.getElementById('cbWireframe')
            const cbTexturesEl = document.getElementById('cbTextures')
            const cbDeformationEl = document.getElementById('cbDeformation')
            const cbSlopesEl = document.getElementById('cbSlopes')

            if (cbWaterEl && cbWaterEl.checked) showWater = true
            if (cbBoundaryEl && cbBoundaryEl.checked) showBoundary = true
            if (cbBlightEl && cbBlightEl.checked) showBlight = true
            if (cbRampEl && cbRampEl.checked) showRamp = true
            if (cbWireframeEl) {
                if (cbWireframeEl.checked) {
                    fineMesh.visible = true
                    coarseMesh.visible = true
                } else {
                    fineMesh.visible = false
                    coarseMesh.visible = false
                }
            }
            if (cbTexturesEl) {
                useTextures = cbTexturesEl.checked
            }
            if (cbDeformationEl && !cbDeformationEl.checked) {
                showDeformation = false
                applyHeights()
                rebuildWireframe()
            }
            if (cbSlopesEl && !cbSlopesEl.checked) {
                showSlopes = false
                applyHeights()
                rebuildWireframe()
            }
            applyColors()
            if (showRamp) buildRampDiamonds()
            if (useTextures && canvasTex) {
                mat.map = canvasTex
                mat.transparent = false // opaque pass — see comment in buildComposited
                mat.alphaTest = 0.01
                mat.needsUpdate = true
            }

            const cb = (id, fn) => {
                const el = document.getElementById(id)
                if (el) el.addEventListener('change', fn)
            }
            cb('cbWater', e => {
                showWater = e.target.checked
                applyColors()
                _buildWaterMesh()
                saveCbState()
            })
            cb('cbBoundary', e => {
                showBoundary = e.target.checked
                applyColors()
                if (_lastTileImages) buildComposited(_lastTileImages)
                _updateInstanceBoundaryColors()
                saveCbState()
            })
            cb('cbBlight', e => {
                showBlight = e.target.checked
                applyColors()
                saveCbState()
            })
            cb('cbRamp', e => {
                showRamp = e.target.checked
                applyColors()
                buildRampDiamonds()
                saveCbState()
            })
            cb('cbWireframe', e => {
                fineMesh.visible = e.target.checked
                coarseMesh.visible = e.target.checked
                saveCbState()
            })
            cb('cbTextures', e => {
                useTextures = e.target.checked
                const useTex = !!(useTextures && canvasTex)
                mat.map = useTex ? canvasTex : colorTex
                mat.transparent = false // always opaque — terrain must render before water
                mat.alphaTest = useTex ? 0.01 : 0
                mat.needsUpdate = true
                saveCbState()
            })
            cb('cbDeformation', e => {
                showDeformation = e.target.checked
                applyHeights()
                rebuildWireframe()
                buildRampDiamonds()
                _updateTerrainHeightTexture()
                _buildWaterMesh()
                saveCbState()
            })
            cb('cbSlopes', e => {
                showSlopes = e.target.checked
                applyHeights()
                rebuildWireframe()
                buildRampDiamonds()
                _updateTerrainHeightTexture()
                _buildWaterMesh()
                saveCbState()
            })

            const raycaster = new THREE.Raycaster()
            const mouseNdc = new THREE.Vector2()
            const infoEl = document.getElementById('cursor-info')
            // halfGridW/H convert geometry coords (centered at 0,0) to
            // game coords (bottom-left origin):
            //   gameX = D.offsetX + vx + halfGridW
            //   gameY = D.offsetY + vy + halfGridH
            const halfGridW = (W - 1) * TILE / 2
            const halfGridH = (H - 1) * TILE / 2

            // ── Terrain height texture for per-vertex cliff deformation ──────
            // Each texel stores the final terrain height at that tilepoint.
            // Cliff vertex shaders sample this texture to conform to terrain.
            let _terrainHeightTex = null

            function _buildTerrainHeightData() {
                // HiveWE cliff.vert binds ground_height_buffer (deformation only,
                // no layer height) — see terrain.ixx line 578 + 885.
                // The layer contribution is already in the instance Z offset
                // and baked into the cliff model geometry (BAAA, ABBA, etc.).
                const data = new Float32Array(W * H * 4) // RGBA float
                for (let j = 0; j < H; j++) {
                    for (let i = 0; i < W; i++) {
                        const idx = j * W + i
                        let h = 0
                        if (showDeformation) {
                            h = (D.groundHeight[idx] - H_ZERO) / H_SCALE
                        }
                        data[idx * 4] = h
                    }
                }
                return data
            }

            function _initTerrainHeightTexture() {
                const data = _buildTerrainHeightData()
                _terrainHeightTex = new THREE.DataTexture(data, W, H, THREE.RGBAFormat, THREE.FloatType)
                // LinearFilter → GPU bilinear interpolation between texels.
                // This makes cliff vertex heights smoothly blend between tilepoints,
                // matching the terrain PlaneGeometry's linear face interpolation
                // and eliminating visible gaps at cliff-terrain boundaries.
                _terrainHeightTex.magFilter = THREE.LinearFilter
                _terrainHeightTex.minFilter = THREE.LinearFilter
                _terrainHeightTex.needsUpdate = true
            }

            function _updateTerrainHeightTexture() {
                if (!_terrainHeightTex) return
                const data = _buildTerrainHeightData()
                _terrainHeightTex.image.data.set(data)
                _terrainHeightTex.needsUpdate = true
            }

            _initTerrainHeightTexture()

            // ── Point marker (inverted pyramid at hovered grid vertex) ───────
            // Tip at origin (terrain point), square base above at z = h
            const MARKER_S = TILE * 0.12
            const MARKER_H = TILE * 0.3
            const markerGeo = new THREE.BufferGeometry()
            // 8 edges = 16 vertices (each edge is a pair for LineSegments)
            // prettier-ignore
            markerGeo.setAttribute('position', new THREE.Float32BufferAttribute([
                // 4 edges from tip to base corners
                0, 0, 0,  -MARKER_S, -MARKER_S, MARKER_H,
                0, 0, 0,   MARKER_S, -MARKER_S, MARKER_H,
                0, 0, 0,   MARKER_S,  MARKER_S, MARKER_H,
                0, 0, 0,  -MARKER_S,  MARKER_S, MARKER_H,
                // 4 base edges
                -MARKER_S, -MARKER_S, MARKER_H,   MARKER_S, -MARKER_S, MARKER_H,
                 MARKER_S, -MARKER_S, MARKER_H,   MARKER_S,  MARKER_S, MARKER_H,
                 MARKER_S,  MARKER_S, MARKER_H,  -MARKER_S,  MARKER_S, MARKER_H,
                -MARKER_S,  MARKER_S, MARKER_H,  -MARKER_S, -MARKER_S, MARKER_H,
            ], 3))
            const markerMesh = new THREE.LineSegments(markerGeo,
                new THREE.LineBasicMaterial({color: 0x00ff00, depthTest: false, depthWrite: false, transparent: true}))
            markerMesh.renderOrder = 999
            markerMesh.visible = false
            scene.add(markerMesh)

            // Horizontal plane fallback for raycasting — always catches the ray
            // even when the terrain mesh has holes (cliff cells) and cliff models miss.
            const _hoverPlane = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0)
            const _hoverTarget = new THREE.Vector3()

            canvas.addEventListener('mousemove', e => {
                const rect = canvas.getBoundingClientRect()
                mouseNdc.x = ((e.clientX - rect.left) / rect.width) * 2 - 1
                mouseNdc.y = -((e.clientY - rect.top) / rect.height) * 2 + 1
                raycaster.setFromCamera(mouseNdc, ctrl.camera)
                let pt = null
                const hits = raycaster.intersectObject(mesh)
                if (hits.length > 0) {
                    pt = hits[0].point
                } else if (cliffGroup && cliffGroup.visible) {
                    const cliffHits = raycaster.intersectObjects(cliffGroup.children, false)
                    if (cliffHits.length > 0) pt = cliffHits[0].point
                }
                // Fallback: intersect horizontal plane at z=0 (covers holes under cliffs)
                if (!pt && raycaster.ray.intersectPlane(_hoverPlane, _hoverTarget)) {
                    // Check if the point is within terrain bounds
                    const hx = _hoverTarget.x, hy = _hoverTarget.y
                    if (hx >= -worldW / 2 - TILE && hx <= worldW / 2 + TILE &&
                        hy >= -worldH / 2 - TILE && hy <= worldH / 2 + TILE) {
                        pt = _hoverTarget
                    }
                }
                if (pt) {

                    // Snap to nearest data point (grid vertex).
                    // Math.round ensures we pick the closest vertex,
                    // not the one below-left (Math.floor).
                    //   sx = round((pt.x + worldW/2) / TILE)  → 0..W-1
                    //   sy = round((pt.y + worldH/2) / TILE)  → 0..H-1
                    //   sy=0 is bottom, sy=H-1 is top row.
                    const sx = Math.max(0, Math.min(W - 1, Math.round((pt.x + worldW / 2) / TILE)))
                    const sy = Math.max(0, Math.min(H - 1, Math.round((pt.y + worldH / 2) / TILE)))
                    const idx = sy * W + sx

                    // Geometry vertex index: gj = H-1-sy (flip Y),
                    // vi = gj * W + gi, stride = W.
                    const vi = (H - 1 - sy) * W + sx
                    const gpos = geo.attributes.position
                    const vx = gpos.getX(vi)
                    const vy = gpos.getY(vi)
                    const vz = gpos.getZ(vi)

                    // Position marker at the snapped vertex
                    markerMesh.position.set(vx, vy, vz)
                    markerMesh.visible = true

                    // Info bar — all values derived from the same snapped point
                    const gameX = D.offsetX + vx + halfGridW
                    const gameY = D.offsetY + vy + halfGridH

                    // Resolve ground tile rawcode
                    const gtIdx = D.groundTexture[idx]
                    const groundCodes = DATA.groundTileCodes || []
                    const gtCode = gtIdx < groundCodes.length
                        ? (typeof groundCodes[gtIdx] === 'string' ? groundCodes[gtIdx] : groundCodes[gtIdx].text || '') : ''

                    // Resolve cliff tile rawcode
                    const ctIdx = D.cliffTexture[idx]
                    const cliffCodes = DATA.cliffTileCodes || []
                    const ctCode = ctIdx < 15 && ctIdx < cliffCodes.length
                        ? (typeof cliffCodes[ctIdx] === 'string' ? cliffCodes[ctIdx] : cliffCodes[ctIdx].text || '') : ''

                    // Flags
                    const fl = []
                    const cf = D.flags[idx]
                    if (cf & 1) fl.push('💧water')
                    if (cf & 2) fl.push('🚧boundary')
                    if (cf & 4) fl.push('☠blight')
                    if (cf & 8) fl.push('📐ramp')

                    // Raw ground height
                    const rawGH = D.groundHeight[idx]
                    const deformation = ((rawGH - H_ZERO) / H_SCALE).toFixed(1)

                    const parts = []
                    parts.push('<span class="ci-label">Point</span> ' + sx + ', ' + sy)
                    parts.push('<span class="ci-label">World</span> ' + gameX.toFixed(0) + ', ' + gameY.toFixed(0) + ', ' + vz.toFixed(1))
                    parts.push('<span class="ci-label">Layer</span> ' + D.layerHeight[idx])
                    parts.push('<span class="ci-label">Deform</span> ' + deformation)
                    parts.push('<span class="ci-label">Ground</span> ' + gtIdx + (gtCode ? ' <code>' + gtCode + '</code>' : '') +
                        ' <span class="ci-dim">var ' + D.groundVariation[idx] + '</span>')
                    if (ctIdx < 15) {
                        parts.push('<span class="ci-label">Cliff</span> ' + ctIdx + (ctCode ? ' <code>' + ctCode + '</code>' : '') +
                            ' <span class="ci-dim">var ' + D.cliffVariation[idx] + '</span>')
                    }

                    // Show cliff ground override diagnostic
                    if (_cliffGroundOverride && _cliffGroundOverride[idx] >= 0) {
                        const ovIdx = _cliffGroundOverride[idx]
                        const ovCode = ovIdx < groundCodes.length
                            ? (typeof groundCodes[ovIdx] === 'string' ? groundCodes[ovIdx] : groundCodes[ovIdx].text || '') : ''
                        // Also show which neighbor (di,dj) triggered the override
                        let ovSrc = ''
                        const cellW2 = W - 1, cellH2 = H - 1
                        for (let di2 = -1; di2 <= 0; di2++) {
                            for (let dj2 = -1; dj2 <= 0; dj2++) {
                                const nx2 = sx + di2, ny2 = sy + dj2
                                if (nx2 < 0 || nx2 >= W || ny2 < 0 || ny2 >= H) continue
                                const nIdx2 = ny2 * W + nx2
                                const hasCliffCell = _cliffCellRawcode && _cliffCellRawcode.has(nIdx2)
                                const hasRompCorner = _romp && _romp.has(nIdx2)
                                if (hasRompCorner || hasCliffCell) {
                                    const rc = hasRompCorner ? _romp.get(nIdx2) : _cliffCellRawcode.get(nIdx2)
                                    ovSrc = ' src=(' + di2 + ',' + dj2 + ') <code>' + rc + '</code>' + (hasRompCorner ? ' romp' : '') + (hasCliffCell ? ' cliff' : '')
                                    break
                                }
                            }
                            if (ovSrc) break
                        }
                        parts.push('<span class="ci-label">Override</span> ' + ovIdx + (ovCode ? ' <code>' + ovCode + '</code>' : '') + ' <span class="ci-dim">' + ovSrc + '</span>')
                    }

                    if (fl.length) parts.push(fl.join(' '))
                    if ((cf & 1) && D.waterHeight) {
                        const wz = _waterZ(idx).toFixed(1)
                        parts.push('<span class="ci-label">Water</span> ' + wz)
                    }

                    infoEl.innerHTML = parts.join('<span class="ci-sep">│</span>')
                    return
                }
                markerMesh.visible = false
                infoEl.textContent = ''
            })
            canvas.addEventListener('mouseleave', () => {
                markerMesh.visible = false
                document.getElementById('cursor-info').textContent = ''
            })

            // ── Click on object → highlight in Placed window ────
            let _clickStartX = 0, _clickStartY = 0
            canvas.addEventListener('pointerdown', function (e) {
                _clickStartX = e.clientX
                _clickStartY = e.clientY
            })
            canvas.addEventListener('click', function (e) {
                // Ignore if it was a drag (orbit/pan)
                let dx = e.clientX - _clickStartX, dy = e.clientY - _clickStartY
                if (dx * dx + dy * dy > 9) return
                if (e.target.closest('float-window') || e.target.closest('.menubar')) return

                let rect = canvas.getBoundingClientRect()
                let ndc = new THREE.Vector2(
                    ((e.clientX - rect.left) / rect.width) * 2 - 1,
                    -((e.clientY - rect.top) / rect.height) * 2 + 1
                )
                let rc = new THREE.Raycaster()
                rc.setFromCamera(ndc, ctrl.camera)

                // Check cliff models first (if visible)
                if (cliffGroup.visible) {
                    let cliffHits = rc.intersectObjects(cliffGroup.children, false)
                    if (cliffHits.length > 0) {
                        let cliffHit = cliffHits[0]
                        let cliffObj = cliffHit.object
                        if (cliffObj.userData && cliffObj.userData._items && cliffHit.instanceId != null) {
                            let cliffItem = cliffObj.userData._items[cliffHit.instanceId]
                            if (cliffItem && cliffItem.path && vscode) {
                                let cmd = {command: 'openModel', path: cliffItem.path}
                                if (cliffItem.cliffTex) {
                                    cmd.cliffTex = cliffItem.cliffTex
                                }
                                vscode.postMessage(cmd)
                            }
                        }
                        return
                    }
                }

                // Check placed objects (doodads/units)
                if (!objectGroup.visible) return
                let hits = rc.intersectObjects(objectGroup.children, false)
                if (hits.length > 0) {
                    let hit = hits[0]
                    let obj = hit.object
                    if (obj.userData && obj.userData._items && hit.instanceId != null) {
                        let item = obj.userData._items[hit.instanceId]
                        if (item && item.i != null) {
                            W3E.highlightPlacedDoodad(item.i)
                        }
                    }
                }
            })

            // ── Map objects (doodads & units) on terrain ─────────
            const objectGroup = new THREE.Group()
            scene.add(objectGroup)

            // ── Cliff models group ────────────────────────────
            const cliffGroup = new THREE.Group()
            scene.add(cliffGroup)

            let _doodFileMap = DATA.doodadFileMap || {}
            let _destFileMap = DATA.destructableFileMap || {}
            let _unitFileMap = DATA.unitFileMap || {}
            let _cliffTypeMap = DATA.cliffTypeMap || {}
            let _doodItems = DATA.doodadPlacements || []
            let _unitItems = DATA.unitPlacements || []

             const _modelCache = {} // path → [{geometry, material}]
             const _pendingItems = {} // path → [items]
             const _textureLoader = new THREE.TextureLoader()
             _textureLoader.crossOrigin = 'anonymous'

             // ── Team Color / Team Glow texture generation (matches MdlVis) ──
            const _TEAM_GLOW_ALPHA = [
                1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,1,
                1,1,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
                1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
                1,1,1,1,1,1,1,1,2,2,2,2,3,3,3,3,3,3,3,2,2,2,1,1,1,1,0,0,0,0,0,1,
                1,1,1,1,1,1,1,1,1,2,2,3,4,5,6,6,6,6,5,4,3,2,2,1,2,2,1,0,0,0,0,0,
                1,1,1,1,1,1,1,1,1,1,3,4,6,7,9,9,10,9,8,7,5,3,2,1,3,2,2,1,0,0,0,0,
                1,1,1,1,1,1,1,1,3,4,6,8,10,13,14,15,17,16,15,12,10,7,6,5,4,3,2,1,0,0,0,0,
                1,1,1,1,1,1,1,1,7,8,10,13,16,18,20,22,24,23,21,18,15,12,10,9,4,3,2,1,0,0,0,0,
                0,0,1,1,0,1,3,4,5,9,15,20,25,30,35,38,38,36,34,31,26,20,13,9,9,6,2,1,0,1,1,0,
                0,0,1,1,0,1,3,5,10,15,21,28,35,41,47,50,51,49,46,41,36,28,20,15,10,7,3,1,0,1,1,0,
                0,0,1,1,1,2,4,7,15,20,28,38,47,55,62,67,69,67,62,56,47,37,28,21,12,9,4,1,1,1,1,0,
                0,0,1,1,1,3,6,9,16,23,33,45,57,68,78,83,87,83,77,69,58,45,33,25,15,11,6,2,1,1,1,0,
                0,0,1,1,1,4,8,11,19,27,39,53,67,81,92,99,103,99,91,81,68,53,39,30,18,13,7,3,1,1,1,1,
                0,0,1,0,1,5,9,13,24,32,46,61,77,92,105,112,116,112,104,93,78,61,45,35,20,16,9,4,1,1,1,1,
                0,0,0,0,2,5,11,14,27,36,50,67,84,100,113,120,124,120,112,100,84,66,49,39,23,17,10,4,1,1,1,1,
                0,0,0,0,2,6,11,15,28,36,51,68,85,102,115,123,127,122,114,102,86,67,50,40,24,18,11,4,1,1,1,1,
                1,1,1,1,2,5,11,15,25,36,51,67,82,97,112,121,123,118,110,98,83,66,49,39,22,17,10,4,2,1,0,0,
                1,1,1,1,2,5,10,14,24,34,48,63,77,90,104,113,116,111,103,92,78,61,46,36,20,16,9,4,1,1,0,0,
                1,1,1,1,1,4,9,12,22,30,43,56,68,80,92,99,104,99,92,82,69,54,39,30,18,14,8,3,1,1,0,0,
                1,1,1,1,1,3,7,10,18,25,35,47,58,69,78,84,88,84,78,69,58,45,33,25,16,12,6,3,1,1,0,0,
                0,1,1,1,1,2,5,8,13,18,27,37,47,56,64,68,70,67,62,55,47,37,26,20,12,9,5,2,1,1,0,0,
                0,1,1,1,0,1,4,6,9,13,19,27,36,43,48,51,52,50,46,41,35,28,20,14,10,7,3,1,1,1,0,0,
                0,1,1,1,0,1,3,4,7,9,13,19,25,30,33,34,36,34,32,29,25,20,14,9,8,5,2,1,1,1,0,0,
                0,1,1,1,0,0,2,4,6,7,9,13,18,21,23,23,27,25,23,21,19,15,9,6,6,4,2,0,0,1,0,0,
                1,1,1,1,1,1,1,1,4,5,6,8,10,12,13,14,16,15,14,12,10,8,7,6,1,1,1,1,1,1,1,1,
                1,1,1,1,1,1,1,1,2,3,4,6,7,8,10,10,10,9,8,7,5,4,3,2,1,1,1,1,1,1,1,1,
                1,1,1,1,1,1,1,1,1,1,2,2,3,4,5,5,5,4,4,3,2,1,1,0,1,1,1,1,1,1,1,1,
                1,1,1,1,1,1,1,1,0,0,0,1,1,1,2,2,3,3,2,2,2,2,1,1,1,1,1,1,1,1,1,1,
                1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,0,1,1,1,1,1,2,2,2,1,1,1,1,1,1,1,1,
                1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,
                1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,
                1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,
            ]

            // Team Color (replaceableId=1): 8×8 solid red, full alpha
            const _teamColorTex = (function () {
                const c = document.createElement('canvas')
                c.width = 8; c.height = 8
                const ctx = c.getContext('2d')
                ctx.fillStyle = '#ff0000'
                ctx.fillRect(0, 0, 8, 8)
                const t = new THREE.CanvasTexture(c)
                t.wrapS = THREE.RepeatWrapping
                t.wrapT = THREE.RepeatWrapping
                t.magFilter = THREE.LinearFilter
                t.minFilter = THREE.LinearFilter
                return t
            })()

            // Team Glow (replaceableId=2): 32×32 pre-multiplied alpha glow
            const _teamGlowTex = (function () {
                const c = document.createElement('canvas')
                c.width = 32; c.height = 32
                const ctx = c.getContext('2d')
                const imgData = ctx.createImageData(32, 32)
                const d = imgData.data
                const rgb = [1, 0, 0] // default red
                for (let i = 0; i < 32 * 32; i++) {
                    const a = _TEAM_GLOW_ALPHA[i]
                    const off = i * 4
                    d[off]     = Math.round(rgb[0] * a * 2)
                    d[off + 1] = Math.round(rgb[1] * a * 2)
                    d[off + 2] = Math.round(rgb[2] * a * 2)
                    d[off + 3] = Math.min(255, a * 2)
                }
                ctx.putImageData(imgData, 0, 0)
                const t = new THREE.CanvasTexture(c)
                t.wrapS = THREE.RepeatWrapping
                t.wrapT = THREE.RepeatWrapping
                t.magFilter = THREE.LinearFilter
                t.minFilter = THREE.LinearFilter
                t.premultiplyAlpha = true
                return t
            })()

            // Red cube fallback for missing models
            const _FALLBACK_SIZE = TILE * 0.35
            const _fallbackGeo = new THREE.BoxGeometry(_FALLBACK_SIZE, _FALLBACK_SIZE, _FALLBACK_SIZE)
            const _fallbackMat = new THREE.MeshPhongMaterial({color: 0xff0000, flatShading: true})
            const _fallbackEntries = [{geometry: _fallbackGeo, material: _fallbackMat}]

            function _texUrl(texPath) {
                const bs = DATA.binaryServer
                if (!bs || !texPath) return null
                const params = new URLSearchParams({token: bs.token, path: texPath})
                if (DATA.archivePath) params.set('archive', DATA.archivePath)
                if (DATA.tileset) params.set('tileset', DATA.tileset)
                return 'http://127.0.0.1:' + bs.port + '/mdx/texture?' + params
            }

             // Load cliff tile texture previews into <tile-item> elements
             function _loadCliffTilePreviews() {
                 const items = document.querySelectorAll('#ctCliffSection tile-item')
                 if (!items.length) return
                 items.forEach(function (el) {
                     const texPath = el.getAttribute('tile-path')
                     if (!texPath) return
                     const url = _texUrl(texPath)
                     if (!url) return
                     const img = new Image()
                     img.crossOrigin = 'anonymous'
                     img.onload = function () {
                         const pc = document.createElement('canvas')
                         pc.width = 64
                         pc.height = 64
                         pc.getContext('2d').drawImage(img, 0, 0, img.naturalWidth, img.naturalHeight, 0, 0, 64, 64)
                         el.setAttribute('tile-preview', pc.toDataURL())
                     }
                     img.src = url
                 })
             }

            // ── Cliff per-vertex terrain height + normal shader ─────────────
            // Injects custom vertex code that:
            // 1. Samples terrain height at each vertex's world XY and offsets Z
            // 2. Computes terrain normal from 4 neighbor height samples
            // 3. Blends model normal with terrain normal
            // Matching HiveWE cliff.vert lines 20-41.
            function _applyCliffShader(material) {
                material.onBeforeCompile = function (shader) {
                    shader.uniforms.uTerrainHeight = {value: _terrainHeightTex}
                    shader.uniforms.uTerrainSize = {value: new THREE.Vector2(W, H)}
                    shader.uniforms.uHalfGrid = {value: new THREE.Vector2(halfGridW, halfGridH)}
                    shader.uniforms.uTileSize = {value: TILE}

                    shader.vertexShader =
                        'uniform sampler2D uTerrainHeight;\n' +
                        'uniform vec2 uTerrainSize;\n' +
                        'uniform vec2 uHalfGrid;\n' +
                        'uniform float uTileSize;\n' +
                        shader.vertexShader

                    shader.vertexShader = shader.vertexShader.replace(
                        '#include <project_vertex>',
                        [
                            'vec4 mvPosition = vec4(transformed, 1.0);',
                            '#ifdef USE_BATCHING',
                            '  mvPosition = batchingMatrix * mvPosition;',
                            '#endif',
                            '#ifdef USE_INSTANCING',
                            '  mvPosition = instanceMatrix * mvPosition;',
                            '#endif',
                            '',
                            '// Per-vertex terrain height (HiveWE cliff.vert line 22-25)',
                            '// Continuous tilepoint coordinate — NOT floored.',
                            '// With LinearFilter on the height texture, the GPU does',
                            '// bilinear interpolation between texels, matching the',
                            '// terrain PlaneGeometry linear face interpolation.',
                            '// This eliminates visible gaps at cliff-terrain seams.',
                            'vec2 _tp = clamp((mvPosition.xy + uHalfGrid) / uTileSize, vec2(0.0), uTerrainSize - 1.0);',
                            'vec2 _uv = (_tp + 0.5) / uTerrainSize;',
                            'float _h = texture2D(uTerrainHeight, _uv).r;',
                            '',
                            '// Terrain normal from neighbor samples (HiveWE cliff.vert lines 27-33)',
                            'vec2 _fl = floor(_tp);',
                            'float _hL = texture2D(uTerrainHeight, (vec2(max(_fl.x - 1.0, 0.0), _fl.y) + 0.5) / uTerrainSize).r;',
                            'float _hR = texture2D(uTerrainHeight, (vec2(min(_fl.x + 1.0, uTerrainSize.x - 1.0), _fl.y) + 0.5) / uTerrainSize).r;',
                            'float _hD = texture2D(uTerrainHeight, (vec2(_fl.x, max(_fl.y - 1.0, 0.0)) + 0.5) / uTerrainSize).r;',
                            'float _hU = texture2D(uTerrainHeight, (vec2(_fl.x, min(_fl.y + 1.0, uTerrainSize.y - 1.0)) + 0.5) / uTerrainSize).r;',
                            'vec3 _terrainN = normalize(vec3(_hL - _hR, _hD - _hU, 2.0 * uTileSize));',
                            '',
                            '// Blend model normal with terrain normal (HiveWE cliff.vert lines 40-41)',
                            'vec3 _instNorm = objectNormal;',
                            '#ifdef USE_INSTANCING',
                            '  _instNorm = mat3(instanceMatrix) * _instNorm;',
                            '#endif',
                            'vec3 _blendedN = normalize(vec3(_instNorm.xy + _terrainN.xy, _instNorm.z * _terrainN.z));',
                            'vNormal = normalize(normalMatrix * _blendedN);',
                            '',
                            '// Apply terrain height offset',
                            'mvPosition.z += _h;',
                            'mvPosition = modelViewMatrix * mvPosition;',
                            'gl_Position = projectionMatrix * mvPosition;',
                        ].join('\n')
                    )
                }
            }

             function _buildModel(data, replaceableTextures, isCliff) {
                 const geosets = data.geosets || []
                 const textures = data.textures || []
                 const materials = data.materials || []
                 const entries = []

                 // Resolve texture path for a given layer
                 function _resolveLayerTexture(layer) {
                     if (!layer) return null
                     const texId = layer.texture_id
                     if (texId == null || texId >= textures.length) return null
                     const tex = textures[texId]
                     if (!tex) return null
                     let texPath = null
                     if (tex.replaceable_id && replaceableTextures) {
                         if (replaceableTextures._cliffTex !== undefined) {
                             texPath = replaceableTextures._cliffTex
                         } else if (replaceableTextures[tex.replaceable_id]) {
                             texPath = replaceableTextures[tex.replaceable_id]
                         }
                     } else if (tex.file_name && !tex.replaceable_id) {
                         texPath = tex.file_name
                     }
                     // Built-in replaceable textures: team color & team glow
                     if (!texPath && tex.replaceable_id === 1) return _teamColorTex
                     if (!texPath && tex.replaceable_id === 2) return _teamGlowTex
                     if (!texPath) return null
                     const url = _texUrl(texPath)
                     if (!url) return null
                     const t = _textureLoader.load(url)
                     t.wrapS = THREE.RepeatWrapping
                     t.wrapT = THREE.RepeatWrapping
                     t.magFilter = THREE.LinearFilter
                     t.minFilter = THREE.LinearMipmapLinearFilter
                     return t
                 }

                // Build material options for a single layer (matches model-viewer.js buildLayerMesh)
                function _buildLayerMaterial(layer, isCliff) {
                    const matOpts = {
                        side: THREE.DoubleSide,
                    }
                    // Cliff models: disable specular so adjacent instances
                    // have consistent diffuse-only lighting (no bright edge seams).
                    if (isCliff) {
                        matOpts.specular = 0x000000
                        matOpts.shininess = 0
                    }

                    const sf = layer ? (layer.shading_flags || 0) : 0
                    const fm = layer ? (layer.filter_mode || 0) : 0

                    // NoDepthTest (0x40)
                    if (sf & 0x40) matOpts.depthTest = false

                    // NoDepthSet (0x80)
                    if (sf & 0x80) matOpts.depthWrite = false

                    const tex = _resolveLayerTexture(layer)
                    if (tex) {
                        matOpts.map = tex
                        matOpts.color = 0xffffff
                    } else {
                        // No texture: modulate/modulate2x without texture is invisible
                        if (fm === 5 || fm === 6) {
                            matOpts.visible = false
                        }
                        matOpts.color = 0xcccccc
                    }

                    // Blending modes (WC3 / HiveWE reference):
                    //   0 None/Opaque:    no blending
                    //   1 Transparent:    alpha test ≥ 0.75
                    //   2 Blend:          GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA
                    //   3 Additive:       GL_SRC_ALPHA, GL_ONE
                    //   4 AddAlpha:       GL_SRC_ALPHA, GL_ONE
                    //   5 Modulate:       GL_ZERO, GL_SRC_COLOR
                    //   6 Modulate2x:     GL_DST_COLOR, GL_SRC_COLOR
                    if (fm === 0) {
                        matOpts.transparent = false
                    } else if (fm === 1) {
                        // Transparent/ColorAlpha — alpha test ≥ 0.75, rendered in opaque pass
                        // (MdlVis: glAlphaFunc(GL_GEQUAL,0.75) in opaque pass;
                        //  HiveWE: alpha_test=0.75, glBlendFunc(GL_ONE,GL_ZERO) — no blending)
                        matOpts.transparent = false
                        matOpts.alphaTest = 0.75
                    } else if (fm === 2) {
                        matOpts.transparent = true
                        matOpts.blending = THREE.NormalBlending
                        matOpts.depthWrite = false
                    } else if (fm === 3 || fm === 4) {
                        matOpts.transparent = true
                        matOpts.blending = THREE.CustomBlending
                        matOpts.blendSrc = THREE.SrcAlphaFactor
                        matOpts.blendDst = THREE.OneFactor
                        matOpts.depthWrite = false
                    } else if (fm === 5) {
                        matOpts.transparent = true
                        matOpts.premultipliedAlpha = true
                        matOpts.blending = THREE.CustomBlending
                        matOpts.blendEquation = THREE.AddEquation
                        matOpts.blendSrc = THREE.ZeroFactor
                        matOpts.blendDst = THREE.SrcColorFactor
                        matOpts.blendSrcAlpha = THREE.ZeroFactor
                        matOpts.blendDstAlpha = THREE.SrcAlphaFactor
                        matOpts.depthWrite = false
                    } else if (fm === 6) {
                        matOpts.transparent = true
                        matOpts.premultipliedAlpha = true
                        matOpts.blending = THREE.CustomBlending
                        matOpts.blendEquation = THREE.AddEquation
                        matOpts.blendSrc = THREE.DstColorFactor
                        matOpts.blendDst = THREE.SrcColorFactor
                        matOpts.blendSrcAlpha = THREE.ZeroFactor
                        matOpts.blendDstAlpha = THREE.SrcAlphaFactor
                        matOpts.depthWrite = false
                    }

                    // Layer alpha
                    if (layer && layer.alpha < 1.0) {
                        matOpts.transparent = true
                        matOpts.opacity = layer.alpha
                    }

                    // Render order: opaque layers first (0,1), then blend (2), then additive (3,4,5,6)
                    let renderOrder = 0
                    if (fm === 0 || fm === 1) renderOrder = 0
                    else if (fm === 2) renderOrder = 1
                    else renderOrder = 2

                    // Unshaded (0x01) → MeshBasicMaterial (no lighting), otherwise MeshPhongMaterial
                    let meshMat
                    if (sf & 0x01) {
                        delete matOpts.specular
                        delete matOpts.shininess
                        meshMat = new THREE.MeshBasicMaterial(matOpts)
                    } else {
                        meshMat = new THREE.MeshPhongMaterial(matOpts)
                    }
                    if (isCliff && _terrainHeightTex) _applyCliffShader(meshMat)
                    return {material: meshMat, renderOrder: renderOrder}
                }

                for (const g of geosets) {
                    if (!g.vertex_count || !g.face_count) continue
                    const verts = g.vertices instanceof Float32Array ? g.vertices : new Float32Array(0)
                    const norms = g.normals instanceof Float32Array ? g.normals : new Float32Array(0)
                    const faces = g.faces instanceof Uint16Array ? g.faces : new Uint16Array(0)
                    const uvs = g.uvs instanceof Float32Array ? g.uvs : new Float32Array(0)

                    const geo = new THREE.BufferGeometry()
                    geo.setAttribute('position', new THREE.BufferAttribute(verts, 3))
                    if (norms.length > 0) geo.setAttribute('normal', new THREE.BufferAttribute(norms, 3))
                    if (uvs.length > 0) geo.setAttribute('uv', new THREE.BufferAttribute(uvs, 2))
                    geo.setIndex(new THREE.BufferAttribute(faces, 1))
                    if (norms.length === 0) geo.computeVertexNormals()

                    // Get layers for this geoset's material
                    const layers = (g.material_id != null && g.material_id < materials.length)
                        ? (materials[g.material_id].layers || [])
                        : []

                    if (layers.length === 0) {
                        // No layers — fallback material
                        const {material, renderOrder} = _buildLayerMaterial(null, isCliff)
                        entries.push({geometry: geo, material: material, renderOrder: renderOrder})
                    } else {
                        // Render each layer as a separate pass (multi-pass rendering as in model-viewer)
                        for (const layer of layers) {
                            const {material, renderOrder} = _buildLayerMaterial(layer, isCliff)
                            entries.push({geometry: geo, material: material, renderOrder: renderOrder})
                        }
                    }
                }
                return entries
            }

            // Check if world coordinates (game space) fall in a boundary cell
            const _boundaryColor = new THREE.Color()
            function _isBoundaryAt(wx, wy) {
                const cx = Math.floor((wx - D.offsetX) / TILE)
                const cy = Math.floor((wy - D.offsetY) / TILE)
                if (cx < 0 || cy < 0 || cx >= W - 1 || cy >= H - 1) return true
                return !!(D.flags[cy * W + cx] & 2)
            }

            // Re-apply boundary darkening to all instanced meshes in both groups
            function _updateInstanceBoundaryColors() {
                const groups = [objectGroup, cliffGroup]
                for (const grp of groups) {
                    for (const child of grp.children) {
                        if (!child.isInstancedMesh || !child.userData._items) continue
                        const items = child.userData._items
                        for (let i = 0; i < items.length; i++) {
                            const it = items[i]
                            if (showBoundary && _isBoundaryAt(it.p[0], it.p[1])) {
                                _boundaryColor.setRGB(0.4, 0.4, 0.4)
                            } else {
                                _boundaryColor.setRGB(1, 1, 1)
                            }
                            child.setColorAt(i, _boundaryColor)
                        }
                        child.instanceColor.needsUpdate = true
                    }
                }
            }

            function _placeInstances(items, entries, group) {
                if (entries.length === 0 || items.length === 0) return
                const targetGroup = group || objectGroup
                const mat4 = new THREE.Matrix4()
                const pos = new THREE.Vector3()
                const quat = new THREE.Quaternion()
                const scl = new THREE.Vector3()
                const euler = new THREE.Euler()

                for (const entry of entries) {
                    const instMesh = new THREE.InstancedMesh(entry.geometry, entry.material, items.length)
                    if (entry.renderOrder) instMesh.renderOrder = entry.renderOrder
                    if (group) instMesh.frustumCulled = false // cliff shader moves verts
                    instMesh.userData._items = items
                    for (let i = 0; i < items.length; i++) {
                        const it = items[i]
                        pos.set(
                            it.p[0] - D.offsetX - halfGridW,
                            it.p[1] - D.offsetY - halfGridH,
                            it.p[2]
                        )
                        const rz = it.rz != null ? it.rz : (it.a || 0)
                        euler.set(it.rx || 0, it.ry || 0, rz, 'ZYX')
                        quat.setFromEuler(euler)
                        scl.set(it.s[0] || 1, it.s[1] || 1, it.s[2] || 1)
                        mat4.compose(pos, quat, scl)
                        instMesh.setMatrixAt(i, mat4)
                        // Boundary darkening: tint objects in boundary cells
                        if (showBoundary && _isBoundaryAt(it.p[0], it.p[1])) {
                            _boundaryColor.setRGB(0.4, 0.4, 0.4)
                        } else {
                            _boundaryColor.setRGB(1, 1, 1)
                        }
                        instMesh.setColorAt(i, _boundaryColor)
                    }
                    instMesh.instanceMatrix.needsUpdate = true
                    instMesh.instanceColor.needsUpdate = true
                    targetGroup.add(instMesh)
                }
            }

            // Bilinear interpolation of final ground height at fractional
            // tilepoint coordinates, matching HiveWE Terrain::interpolated_height.
            // Coordinates are in tile units (world / 128), NOT game units.
            function _interpolatedHeight(tx, ty) {
                tx = Math.max(0, Math.min(W - 1.01, tx))
                ty = Math.max(0, Math.min(H - 1.01, ty))
                const ix = Math.floor(tx), iy = Math.floor(ty)
                const ixc = Math.min(ix + 1, W - 1), iyc = Math.min(iy + 1, H - 1)

                // HiveWE: final_ground_height = (groundHeight - 8192) / 512 + layerHeight - 2
                //       = groundHeight / (4 * 128) - 16 + layerHeight - 2
                //       = groundHeight / (H_SCALE * TILE) + layerHeight - 18
                function _fgh(i, j) {
                    const idx = j * W + i
                    return D.groundHeight[idx] / (H_SCALE * TILE) + D.layerHeight[idx] - 18
                }
                const p1 = _fgh(ix, iy)
                const p2 = _fgh(ixc, iy)
                const p3 = _fgh(ix, iyc)
                const p4 = _fgh(ixc, iyc)
                const fx = tx - ix, fy = ty - iy
                const xx = p1 + (p2 - p1) * fx
                const yy = p3 + (p4 - p3) * fx
                return xx + (yy - xx) * fy
            }

            // Compute pitch (ry) and roll (rx) for a doodad, matching HiveWE
            // Doodad::update() (doodad.ixx lines 72-133).
            //
            // Coordinates in game (world) units; angle in radians.
            // maxPitch/maxRoll from SLK:
            //   < 0 → fixed tilt (value used directly as radians)
            //   = 0 → no tilt (default for missing SLK fields)
            //   > 0 → terrain-following, value = max angle in degrees
            // Returns {rx, ry} in radians (rx = roll around X, ry = -pitch around Y)
            // matching euler.set(rx, ry, a, 'ZYX') in _placeInstances.
            function _computeTerrainTilt(wx, wy, angle, maxPitch, maxRoll) {
                if (maxPitch === 0 && maxRoll === 0) return null

                // Sample radius in tile units (HiveWE: 32/128 = 0.25)
                const SR = 32 / TILE
                // Convert world position to tile coordinates
                const tx = (wx - D.offsetX) / TILE
                const ty = (wy - D.offsetY) / TILE

                // ── Pitch (rotation around local Y, negated) ──────────
                // HiveWE: negative → fixed (pitch = maxPitch, then rotation *= angleAxis(-pitch, Y))
                //         positive → terrain following clamped to ±maxPitch degrees
                let pitch = 0
                if (maxPitch < 0) {
                    pitch = maxPitch
                } else if (maxPitch > 0) {
                    const maxPitchRad = maxPitch * Math.PI / 180
                    const fwdX = tx + SR * Math.cos(angle)
                    const fwdY = ty + SR * Math.sin(angle)
                    const bwdX = tx - SR * Math.cos(angle)
                    const bwdY = ty - SR * Math.sin(angle)
                    const h1 = _interpolatedHeight(bwdX, bwdY)
                    const h2 = _interpolatedHeight(fwdX, fwdY)
                    pitch = Math.max(-maxPitchRad, Math.min(maxPitchRad,
                        Math.atan2(h2 - h1, SR * 2)))
                }

                // ── Roll (rotation around local X) ────────────────────
                // HiveWE: negative → fixed (roll = -maxRoll, then rotation *= angleAxis(roll, X))
                //         positive → terrain following clamped to ±maxRoll degrees
                let roll = 0
                if (maxRoll < 0) {
                    roll = -maxRoll
                } else if (maxRoll > 0) {
                    const maxRollRad = maxRoll * Math.PI / 180
                    const perpAngle = angle + Math.PI / 2
                    const fwdX = tx + SR * Math.cos(perpAngle)
                    const fwdY = ty + SR * Math.sin(perpAngle)
                    const bwdX = tx - SR * Math.cos(perpAngle)
                    const bwdY = ty - SR * Math.sin(perpAngle)
                    const h1 = _interpolatedHeight(bwdX, bwdY)
                    const h2 = _interpolatedHeight(fwdX, fwdY)
                    roll = Math.max(-maxRollRad, Math.min(maxRollRad,
                        Math.atan2(h2 - h1, SR * 2)))
                }

                if (pitch === 0 && roll === 0) return null
                // euler.set(rx=roll, ry=-pitch, rz=angle, 'ZYX') matches HiveWE's
                // quat composition: Rz(angle) * Ry(-pitch) * Rx(roll)
                return {rx: roll, ry: -pitch}
            }

            // Create centered items for missing cliff fallback cubes.
            // Normal cliff models get terrain deformation via the cliff shader,
            // but fallback cubes use plain material — add deformation manually.
            function _cliffCenteredItems(items) {
                const half = TILE / 2
                return items.map(function (it) {
                    let defZ = 0
                    if (showDeformation) {
                        let sx = Math.max(0, Math.min(W - 1, Math.round((it.p[0] - D.offsetX) / TILE)))
                        let sy = Math.max(0, Math.min(H - 1, Math.round((it.p[1] - D.offsetY) / TILE)))
                        defZ = (D.groundHeight[sy * W + sx] - H_ZERO) / H_SCALE
                    }
                    return {
                        path: it.path,
                        p: [it.p[0] + half, it.p[1] + half, it.p[2] + half + defZ],
                        s: [1, 1, 1],
                        a: 0,
                        cliffTex: it.cliffTex,
                    }
                })
            }

            const _rawModelData = {} // modelPath → raw msg data (shared across texture variants)

            // ── Cliff model collection ────────────────────────
            // For each cell where corners have different layerHeight,
            // derive the cliff/ramp model filename and collect placement items.
            //
            // Cliff models have a single material with Replaceable ID 11.
            // The actual texture path is {texDir}\{texFile}.blp from CliffTypes.slk
            // (HiveWE terrain.ixx lines 372-374: cliff_slk texdir + texfile).
            // The tileset-specific MPQ (e.g. L.mpq) is searched via the cascade
            // lookup when the `tileset` query parameter is passed to /mdx/texture.


            // Max variation index per cliff letter-pattern (from snapshot cliffVariations).
            // If cliff_variation > max, it is clamped (HiveWE terrain.ixx line 1080).
            // Populated from embedded Cliffs.slk / CityCliffs.slk via the game snapshot.

            function _collectCliffItems() {
                const _cliffVariations = (DATA.cliffVariations && DATA.cliffVariations.cliffs) || {}
                const _cityCliffVariations = (DATA.cliffVariations && DATA.cliffVariations.cityCliffs) || {}
                const items = [] // {path, p:[x,y,z], s:[1,1,1], a:0, cliffTex}
                _romp = new Map()
                _cliffCellRawcode = new Map()
                const cliffCodes = DATA.cliffTileCodes || []
                if (cliffCodes.length === 0 || Object.keys(_cliffTypeMap).length === 0) return items

                // Helper: resolve cliff texture path from a corner's cliffTexture index.
                // When ctIdx is invalid (>= 15 = default/unset, or out of range),
                // fall back to the first cliff type (index 0) — matching HiveWE
                // behaviour where cliff_texture=15 is remapped to 1 in
                // real_tile_texture and the cliff wall texture always gets a
                // valid index (terrain.ixx line 564).
                function _resolveCliffTex(ctIdx) {
                    let idx = ctIdx
                    if (idx >= 15 || idx >= cliffCodes.length) idx = 0
                    if (idx >= cliffCodes.length) return null
                    const rawcode = typeof cliffCodes[idx] === 'string'
                        ? cliffCodes[idx] : cliffCodes[idx].text || cliffCodes[idx].raw || ''
                    if (!rawcode) return null
                    const ct = _cliffTypeMap[rawcode]
                    if (!ct || !ct.texDir || !ct.texFile) return null
                    return ct.texDir + '\\' + ct.texFile + '.blp'
                }

                // Helper: resolve cliff_texture index → cliff rawcode string.
                // Falls back to index 0 when ctIdx >= 15 (unset/default).
                function _idxToRawcode(ctIdx) {
                    let idx = ctIdx
                    if (idx >= 15 || idx >= cliffCodes.length) idx = 0
                    if (idx >= cliffCodes.length) return ''
                    return typeof cliffCodes[idx] === 'string'
                        ? cliffCodes[idx] : cliffCodes[idx].text || cliffCodes[idx].raw || ''
                }

                // Helper: compute ramp transition character (HiveWE terrain.ixx lines 999-1004)
                // Ramp corners use 'L' base with -4 per layer diff; non-ramp use 'A' with +1
                function _rampChar(isRamp, layerHeight, base) {
                    return String.fromCharCode(
                        (isRamp ? 76 : 65) + (layerHeight - base) * (isRamp ? -4 : 1)
                    )
                }

                // Three-phase cliff collection matching HiveWE terrain.ixx update_cliff_meshes
                // (lines 980-1083). Within one cell iteration:
                //   Phase 1: Try vertical ramp transition → if found, continue
                //   Phase 2: Try horizontal ramp transition → if found, continue
                //   Phase 3: Regular cliff model
                for (let cy = 0; cy < H - 1; cy++) {
                    for (let cx = 0; cx < W - 1; cx++) {
                        const iBL = cy * W + cx
                        const iBR = cy * W + cx + 1
                        const iTL = (cy + 1) * W + cx
                        const iTR = (cy + 1) * W + cx + 1

                        const lBL = D.layerHeight[iBL]
                        const lBR = D.layerHeight[iBR]
                        const lTL = D.layerHeight[iTL]
                        const lTR = D.layerHeight[iTR]

                        const rBL = !!(D.flags[iBL] & 8)
                        const rBR = !!(D.flags[iBR] & 8)
                        const rTL = !!(D.flags[iTL] & 8)
                        const rTR = !!(D.flags[iTR] & 8)

                        // ── Phase 1: Vertical ramp transition (HiveWE lines 987-1021) ──
                        // Spans 2 cells vertically. One side (left or right) has ramp,
                        // the other doesn't. Uses CliffTrans models with H/L letters.
                        if (cy < H - 2) {
                            const iTTL = (cy + 2) * W + cx
                            const iTTR = (cy + 2) * W + cx + 1
                            const lTTL = D.layerHeight[iTTL]
                            const lTTR = D.layerHeight[iTTR]
                            const rTTL = !!(D.flags[iTTL] & 8)
                            const rTTR = !!(D.flags[iTTR] & 8)

                            const ae = Math.min(lBL, lTTL)
                            const cf = Math.min(lBR, lTTR)

                            if (lTL === ae && lTR === cf) {
                                const rampBase = Math.min(ae, cf)
                                if (rBL === rTL && rBL === rTTL &&
                                    rBR === rTR && rBR === rTTR &&
                                    rBL !== rBR) {

                                    // Skip if no height change — pattern would be all A/L,
                                    // model file doesn't exist (HiveWE: file_exists guard)
                                    if (lTTL !== rampBase || lTTR !== rampBase ||
                                        lBR !== rampBase || lBL !== rampBase) {

                                        const pattern = _rampChar(rTTL, lTTL, rampBase)
                                            + _rampChar(rTTR, lTTR, rampBase)
                                            + _rampChar(rBR, lBR, rampBase)
                                            + _rampChar(rBL, lBL, rampBase)

                                        const modelPath = 'Doodads\\Terrain\\CliffTrans\\CliffTrans' + pattern + '0.mdx'
                                        const cliffTex = _resolveCliffTex(D.cliffTexture[iBL])

                                        const wx = cx * TILE + D.offsetX
                                        const wy = cy * TILE + D.offsetY
                                        const wz = (rampBase - 2) * TILE

                                        items.push({
                                            path: modelPath,
                                            p: [wx, wy, wz],
                                            s: [1, 1, 1],
                                            a: -Math.PI / 2,
                                            cliffTex,
                                        })
                                        const _rc = _idxToRawcode(D.cliffTexture[iBL])
                                        _romp.set(iBL, _rc)
                                        _romp.set(iTL, _rc)
                                        continue
                                    }
                                }
                            }
                        }

                        // ── Phase 2: Horizontal ramp transition (HiveWE lines 1023-1058) ──
                        // Spans 2 cells horizontally. Top/bottom sides differ in ramp flag.
                        if (cx < W - 2) {
                            const iBRR = cy * W + cx + 2
                            const iTRR = (cy + 1) * W + cx + 2
                            const lBRR = D.layerHeight[iBRR]
                            const lTRR = D.layerHeight[iTRR]
                            const rBRR = !!(D.flags[iBRR] & 8)
                            const rTRR = !!(D.flags[iTRR] & 8)

                            const ae = Math.min(lBL, lBRR)
                            const bf = Math.min(lTL, lTRR)

                            if (lBR === ae && lTR === bf) {
                                const rampBase = Math.min(ae, bf)
                                if (rBL === rBR && rBL === rBRR &&
                                    rTL === rTR && rTL === rTRR &&
                                    rBL !== rTL) {

                                    // Skip if no height change — pattern would be all A/L,
                                    // model file doesn't exist (HiveWE: file_exists guard)
                                    if (lTL !== rampBase || lTRR !== rampBase ||
                                        lBRR !== rampBase || lBL !== rampBase) {

                                        const pattern = _rampChar(rTL, lTL, rampBase)
                                            + _rampChar(rTRR, lTRR, rampBase)
                                            + _rampChar(rBRR, lBRR, rampBase)
                                            + _rampChar(rBL, lBL, rampBase)

                                        const modelPath = 'Doodads\\Terrain\\CliffTrans\\CliffTrans' + pattern + '0.mdx'
                                        const cliffTex = _resolveCliffTex(D.cliffTexture[iBL])

                                        const wx = cx * TILE + D.offsetX
                                        const wy = cy * TILE + D.offsetY
                                        const wz = (rampBase - 2) * TILE

                                        items.push({
                                            path: modelPath,
                                            p: [wx, wy, wz],
                                            s: [1, 1, 1],
                                            a: -Math.PI / 2,
                                            cliffTex,
                                        })
                                        const _rc = _idxToRawcode(D.cliffTexture[iBL])
                                        _romp.set(iBL, _rc)
                                        _romp.set(iBR, _rc)
                                        continue
                                    }
                                }
                            }
                        }

                        // ── Phase 3: Regular cliff model (HiveWE lines 1060-1083) ──
                        const base = Math.min(lBL, lBR, lTL, lTR)
                        const peak = Math.max(lBL, lBR, lTL, lTR)
                        if (base === peak) continue // no cliff

                        // Skip cells where a ramp transition model was placed (romp)
                        if (_romp.has(iBL)) continue

                        // Skip ramp entrances — terrain mesh stays visible, no cliff model
                        // (HiveWE terrain.ixx is_corner_ramp_entrance line 803-815)
                        if (rBL && rBR && rTL && rTR && !(lBL === lTR && lTL === lBR)) continue

                        // Derive corner letters: 'A' + (layerHeight - base)
                        const dTL = lTL - base
                        const dTR = lTR - base
                        const dBR = lBR - base
                        const dBL = lBL - base

                        // Skip models with difference > 2 ('D' or higher) — they don't exist
                        if (dTL > 2 || dTR > 2 || dBR > 2 || dBL > 2) continue

                        const cTL = String.fromCharCode(65 + dTL) // A, B, C
                        const cTR = String.fromCharCode(65 + dTR)
                        const cBR = String.fromCharCode(65 + dBR)
                        const cBL = String.fromCharCode(65 + dBL)

                        // Skip 'AAAA' — not a valid cliff
                        if (dTL === 0 && dTR === 0 && dBR === 0 && dBL === 0) continue

                        // Resolve cliff type info; fall back to defaults when
                        // cliff_texture is unset (15) or out of range.
                        // HiveWE hardcodes "Cliffs" directory (terrain.ixx line 348)
                        // and never skips cliff model placement based on
                        // cliff_texture — the model is always placed.
                        const cellRawcode = _idxToRawcode(D.cliffTexture[iBL])
                        let modelDir = 'Cliffs'
                        let cliffTex = null
                        if (cellRawcode) {
                            const ct = _cliffTypeMap[cellRawcode]
                            if (ct) {
                                if (ct.cliffModelDir) modelDir = ct.cliffModelDir
                                if (ct.texDir && ct.texFile) {
                                    cliffTex = ct.texDir + '\\' + ct.texFile + '.blp'
                                }
                            }
                        }
                        // Fallback cliff wall texture from first cliff type
                        if (!cliffTex) cliffTex = _resolveCliffTex(D.cliffTexture[iBL])

                        // Store resolved cliff rawcode for ground-tile override
                        if (cellRawcode) _cliffCellRawcode.set(iBL, cellRawcode)

                        // Clamp variation to max available for this pattern
                        // (HiveWE terrain.ixx line 1080: std::clamp(cliff_variation, 0, cliff_variations[pattern]))
                        const pattern = cTL + cTR + cBR + cBL
                        const varMap = modelDir === 'CityCliffs' ? _cityCliffVariations : _cliffVariations
                        const maxVar = varMap[pattern] !== undefined ? varMap[pattern] : 0
                        const variation = Math.min(D.cliffVariation[iBL], maxVar)

                        const modelPath = 'Doodads\\Terrain\\' + modelDir + '\\' + modelDir +
                            pattern + variation + '.mdx'


                        // Position: bottom-left corner of cell, at base layer height.
                        // Per-vertex terrain height is applied in the cliff shader
                        // (see _applyCliffShader / HiveWE cliff.vert).
                        const wx = cx * TILE + D.offsetX
                        const wy = cy * TILE + D.offsetY
                        const wz = (base - 2) * TILE

                        items.push({
                            path: modelPath,
                            p: [wx, wy, wz],
                            s: [1, 1, 1],
                            a: -Math.PI / 2,  // WC3 cliff models are rotated 90° — unrotate (HiveWE: vec3(y, -x, z))
                            cliffTex,
                        })
                    }
                }
                return items
            }

            // Build replaceable texture map for a cache entry
            function _buildReplTex(info) {
                if (info._cliff) {
                    return info._cliffTex ? {_cliffTex: info._cliffTex} : null
                }
                if (info.texId && info.texFile) return {[info.texId]: info.texFile}
                return null
            }

            function _collectAndLoad() {
                // Clear existing objects
                while (objectGroup.children.length > 0) {
                    const c = objectGroup.children[0]
                    objectGroup.remove(c)
                    // Don't dispose shared fallback geometry/material
                    if (c.geometry && c.geometry !== _fallbackGeo) c.geometry.dispose()
                    if (c.material && c.material !== _fallbackMat) {
                        if (c.material.map) c.material.map.dispose()
                        c.material.dispose()
                    }
                }
                // Clear cliff models
                while (cliffGroup.children.length > 0) {
                    const c = cliffGroup.children[0]
                    cliffGroup.remove(c)
                    if (c.geometry && c.geometry !== _fallbackGeo) c.geometry.dispose()
                    if (c.material && c.material !== _fallbackMat) {
                        if (c.material.map) c.material.map.dispose()
                        c.material.dispose()
                    }
                }

                const byCacheKey = {} // cacheKey → {file, variation, resolved, items, texId, texFile}
                const _unmappedItems = [] // items with no rawcode→file mapping
                for (const item of _doodItems) {
                    const entry = _doodFileMap[item.r] || _destFileMap[item.r]
                    if (!entry) { _unmappedItems.push(item); continue }
                    const file = typeof entry === 'string' ? entry : entry.file
                    const texFile = typeof entry === 'object' ? (entry.texFile || '') : ''
                    const texId = typeof entry === 'object' ? (entry.texId || 0) : 0
                    const preResolved = typeof item.m === 'string' && item.m ? item.m : ''
                    const requestedFile = preResolved || file
                    const variation = preResolved ? 0 : (item.v || 0)

                    // fixedRot override: fixedRot >= 0 fully overrides DOO yaw.
                    // Keep original DOO angle untouched in item.a and place with item.rz.
                    const dooYaw = item.a || 0
                    const fixedRot = typeof entry === 'object' ? (entry.fixedRot != null ? entry.fixedRot : -1) : -1
                    const finalYaw = fixedRot >= 0 ? fixedRot * Math.PI / 180 : dooYaw
                    item.rz = finalYaw

                    // Compute terrain-following tilt from maxPitch/maxRoll using final yaw.
                    const mp = typeof entry === 'object' ? (entry.maxPitch != null ? entry.maxPitch : 0) : 0
                    const mr = typeof entry === 'object' ? (entry.maxRoll != null ? entry.maxRoll : 0) : 0
                    const tilt = _computeTerrainTilt(item.p[0], item.p[1], finalYaw, mp, mr)
                    if (tilt) { item.rx = tilt.rx; item.ry = tilt.ry }

                    const cacheKey = requestedFile.toLowerCase() + '|' + variation + (texFile ? '|' + texFile : '')
                    if (!byCacheKey[cacheKey]) {
                        byCacheKey[cacheKey] = {
                            file: requestedFile,
                            variation,
                            resolved: !!preResolved,
                            items: [],
                            texId,
                            texFile,
                        }
                    }
                    byCacheKey[cacheKey].items.push(item)
                }
                for (const item of _unitItems) {
                    const file = _unitFileMap[item.r]
                    if (!file) { _unmappedItems.push(item); continue }
                    const preResolved = typeof item.m === 'string' && item.m ? item.m : ''
                    const requestedFile = preResolved || file
                    const cacheKey = requestedFile.toLowerCase() + '|0'
                    if (!byCacheKey[cacheKey]) {
                        byCacheKey[cacheKey] = {
                            file: requestedFile,
                            variation: 0,
                            resolved: !!preResolved,
                            items: [],
                            texId: 0,
                            texFile: '',
                        }
                    }
                    byCacheKey[cacheKey].items.push(item)
                }

                // Collect cliff/ramp models from terrain data
                const cliffItems = _collectCliffItems()
                // Re-run cliff cell visibility now that _romp is populated
                _updateCliffCells()
                // Recompute cliff ground-tile override with romp data
                // (HiveWE real_tile_texture uses romp to spread cliff
                // ground texture to neighboring tilepoints)
                _cliffGroundOverride = computeCliffGroundOverride(_romp, _cliffCellRawcode)
                applyColors()
                if (canvasTex) buildComposited(_lastTileImages || [])
                for (const item of cliffItems) {
                    // Cache key includes texture path so different cliff types get separate entries
                    const texKey = item.cliffTex || ''
                    const cacheKey = item.path.toLowerCase() + '|0' + (texKey ? '|' + texKey : '')
                    if (!byCacheKey[cacheKey]) byCacheKey[cacheKey] = {
                        file: item.path, variation: 0, resolved: true, items: [],
                        _cliff: true,
                        _cliffTex: item.cliffTex,
                    }
                    byCacheKey[cacheKey].items.push(item)
                }


                // Place red cubes for items with no rawcode→file mapping
                if (_unmappedItems.length > 0) {
                    _placeInstances(_unmappedItems, _fallbackEntries)
                }

                // Place already-cached models; collect uncached for loading
                const toLoad = []
                const toLoadSet = new Set()
                for (const [cacheKey, info] of Object.entries(byCacheKey)) {
                    const grp = info._cliff ? cliffGroup : undefined
                    const rawKey = info.file.toLowerCase() + '|' + info.variation
                    if (_modelCache[cacheKey]) {
                        _placeInstances(info.items, _modelCache[cacheKey], grp)
                    } else if (_rawModelData[rawKey]) {
                        // Model data already loaded but not yet built for this texture variant
                        const replTex = _buildReplTex(info)
                        const entries = _buildModel(_rawModelData[rawKey], replTex, !!info._cliff)
                        _modelCache[cacheKey] = entries
                        _placeInstances(info.items, entries, grp)
                    } else {
                        _pendingItems[cacheKey] = info
                        if (!toLoadSet.has(rawKey)) {
                            toLoadSet.add(rawKey)
                            toLoad.push({path: info.file, variation: info.variation, resolved: !!info.resolved})
                        }
                    }
                }

                if (toLoad.length > 0 && vscode) {
                    vscode.postMessage({command: 'loadMapObjects', entries: toLoad})
                }
            }

            // Listen for model data coming back from the extension host
            window.addEventListener('message', function (e) {
                const msg = e.data

                if (msg && msg.command === 'mapObjectModel') {
                    const rawKey = msg.path.toLowerCase() + '|' + (msg.variation || 0)
                    _rawModelData[rawKey] = msg
                    // Build entries for each pending cache key that references this model path+variation
                    for (const [cacheKey, info] of Object.entries(_pendingItems)) {
                        if (info.file.toLowerCase() !== msg.path.toLowerCase() || info.variation !== (msg.variation || 0)) continue
                        const replTex = _buildReplTex(info)
                        const entries = _buildModel(msg, replTex, !!info._cliff)
                        _modelCache[cacheKey] = entries
                        _placeInstances(info.items, entries, info._cliff ? cliffGroup : undefined)
                        delete _pendingItems[cacheKey]
                    }
                } else if (msg && msg.command === 'mapObjectModelNotFound') {
                    // Model file could not be loaded — red cubes for all (cliffs centered)
                    for (const [cacheKey, info] of Object.entries(_pendingItems)) {
                        if (info.file.toLowerCase() !== msg.path.toLowerCase() || info.variation !== (msg.variation || 0)) continue
                        _modelCache[cacheKey] = _fallbackEntries
                        if (info._cliff) {
                            if (info.items && cliffGroup.visible) {
                                _placeInstances(_cliffCenteredItems(info.items), _fallbackEntries, cliffGroup)
                            }
                        } else {
                            if (info.items && objectGroup.visible) {
                                _placeInstances(info.items, _fallbackEntries)
                            }
                        }
                        delete _pendingItems[cacheKey]
                    }
                } else if (msg && msg.command === 'mapObjectsLoaded') {
                    // After all loading finishes, place red cubes for any remaining pending items
                    for (const [cacheKey, info] of Object.entries(_pendingItems)) {
                        if (!_modelCache[cacheKey]) {
                            _modelCache[cacheKey] = _fallbackEntries
                            if (info._cliff) {
                                if (cliffGroup.visible) {
                                    _placeInstances(_cliffCenteredItems(info.items), _fallbackEntries, cliffGroup)
                                }
                            } else {
                                if (objectGroup.visible) {
                                    _placeInstances(info.items, _fallbackEntries)
                                }
                            }
                        }
                    }
                    // Clear all pending
                    for (const key of Object.keys(_pendingItems)) delete _pendingItems[key]
                }
            })

            // Checkbox: toggle object visibility
            const cbObjectsEl = document.getElementById('cbObjects')
            if (cbObjectsEl && cbState.cbObjects != null) cbObjectsEl.checked = cbState.cbObjects
            if (cbObjectsEl && !cbObjectsEl.checked) objectGroup.visible = false

            cb('cbObjects', e => {
                objectGroup.visible = e.target.checked
                saveCbState()
            })

            // Checkbox: toggle cliff model visibility
            const cbCliffsEl = document.getElementById('cbCliffs')
            if (cbCliffsEl && cbState.cbCliffs != null) cbCliffsEl.checked = cbState.cbCliffs
            if (cbCliffsEl && !cbCliffsEl.checked) {
                cliffGroup.visible = false
                showCliffs = false
                _updateCliffCells()
            }

            cb('cbCliffs', e => {
                showCliffs = e.target.checked
                cliffGroup.visible = e.target.checked
                _updateCliffCells()
                applyColors()
                if (canvasTex) buildComposited(_lastTileImages || [])
                saveCbState()
            })

            // Initial load if SLK maps have data
            const _hasMaps = Object.keys(_doodFileMap).length > 0 || Object.keys(_destFileMap).length > 0 || Object.keys(_unitFileMap).length > 0 || Object.keys(_cliffTypeMap).length > 0
            if (_hasMaps && (_doodItems.length > 0 || _unitItems.length > 0 || Object.keys(_cliffTypeMap).length > 0)) {
                _collectAndLoad()
            }
            // Load cliff tile texture previews
            _loadCliffTilePreviews()

            // ── Water mesh ────────────────────────────────────────
            // A separate transparent mesh that sits at waterHeight,
            // matching HiveWE water.vert / water.frag rendering.
            //
            // Water is rendered per-cell: a cell has water if any of
            // its 4 corner tilepoints has the water flag set.
            //
            // Height formula (from terrain.md):
            //   waterLevel = (waterHeight - H_ZERO) / H_SCALE - waterZero
            // where waterZero = Water.slk "height" × TILE  (e.g. -0.7 × 128 = -89.6)
            //
            // Colour interpolation is based on depth = waterLevel - groundLevel,
            // using shallow and deep colour ranges from Water.slk.

            const waterGroup = new THREE.Group()
            scene.add(waterGroup)
            let _waterMesh = null

            // Water SLK parameters (from Rust via __W3E_DATA__)
            const _waterSlk = DATA.waterSlk
            // water_offset = height * TILE  (HiveWE: water_offset = water_slk.data<float>("height", ...))
            // In HiveWE, water_offset is added in tile units (height is already in tile units).
            // Our waterHeight is raw u16 like groundHeight, so we use the same formula.
            const WATER_OFFSET = _waterSlk ? _waterSlk.entry.height * TILE : -0.7 * TILE
            // Water animation params (from Water.slk)
            const _waterNumTex = _waterSlk ? _waterSlk.entry.numTex : 45
            const _waterTexRate = _waterSlk ? _waterSlk.entry.texRate : 15
            const _waterTexFile = _waterSlk ? _waterSlk.entry.texFile : 'ReplaceableTextures\\Water\\Water'

            // Water texture animation state
            const _waterTextures = []     // THREE.Texture array (loaded async)
             let _waterTexturesReady = false
             let _waterFrame = 0           // float, advances by _waterTexRate * dt
             let _waterLastFrame = -1      // last integer frame applied

            // Depth thresholds (in tile units, matching HiveWE water.vert)
            const W_MIN_DEPTH = 10 / 128
            const W_DEEP_LEVEL = 64 / 128
            const W_MAX_DEPTH = 72 / 128

            // Colour ranges from Water.slk (0-255 → 0-1)
            function _wc(r, g, b, a) { return [r / 255, g / 255, b / 255, a / 255] }
            const W_SMIN = _waterSlk ? _wc(_waterSlk.entry.sminR, _waterSlk.entry.sminG, _waterSlk.entry.sminB, _waterSlk.entry.sminA) : [1, 1, 1, 10/255]
            const W_SMAX = _waterSlk ? _wc(_waterSlk.entry.smaxR, _waterSlk.entry.smaxG, _waterSlk.entry.smaxB, _waterSlk.entry.smaxA) : [117/255, 117/255, 200/255, 219/255]
            const W_DMIN = _waterSlk ? _wc(_waterSlk.entry.dminR, _waterSlk.entry.dminG, _waterSlk.entry.dminB, _waterSlk.entry.dminA) : [117/255, 117/255, 200/255, 219/255]
            const W_DMAX = _waterSlk ? _wc(_waterSlk.entry.dmaxR, _waterSlk.entry.dmaxG, _waterSlk.entry.dmaxB, _waterSlk.entry.dmaxA) : [96/255, 96/255, 192/255, 250/255]

            function _waterColor(depthWorld) {
                // Convert depth from world units to tile units for threshold comparison
                // (HiveWE shader thresholds are in tile units: 10/128, 64/128, 72/128)
                const depth = depthWorld / TILE
                // Replicate HiveWE water.vert depth→colour interpolation
                let value = Math.max(0, Math.min(1, depth))
                let r, g, b, a
                if (value <= W_DEEP_LEVEL) {
                    const t = Math.max(0, value - W_MIN_DEPTH) / (W_DEEP_LEVEL - W_MIN_DEPTH)
                    r = W_SMIN[0] * (1 - t) + W_SMAX[0] * t
                    g = W_SMIN[1] * (1 - t) + W_SMAX[1] * t
                    b = W_SMIN[2] * (1 - t) + W_SMAX[2] * t
                    a = W_SMIN[3] * (1 - t) + W_SMAX[3] * t
                } else {
                    const t = Math.min(value - W_DEEP_LEVEL, W_MAX_DEPTH - W_DEEP_LEVEL) / (W_MAX_DEPTH - W_DEEP_LEVEL)
                    r = W_DMIN[0] * (1 - t) + W_DMAX[0] * t
                    g = W_DMIN[1] * (1 - t) + W_DMAX[1] * t
                    b = W_DMIN[2] * (1 - t) + W_DMAX[2] * t
                    a = W_DMIN[3] * (1 - t) + W_DMAX[3] * t
                }
                return [r, g, b, a]
            }

            // Compute final water height for a tilepoint (in world units)
            function _waterZ(idx) {
                return (D.waterHeight[idx] - H_ZERO) / H_SCALE + WATER_OFFSET
            }

            // Compute final ground height for a tilepoint (used for depth calc).
            // Uses ramp-adjusted layer height + deformation (matches terrain mesh).
            function _groundZ(idx, layer) {
                let h = (layer[idx] - 2) * TILE
                if (showDeformation) {
                    h += (D.groundHeight[idx] - H_ZERO) / H_SCALE
                }
                return h
            }

             // Load water textures asynchronously from the HTTP server
             // Path format: {texFile}{i:02}.blp (e.g. "ReplaceableTextures\Water\Water00.blp")
             function _loadWaterTextures() {
                 const bs = DATA.binaryServer
                 if (!bs || !_waterTexFile || _waterNumTex <= 0) return
                 _waterTexturesReady = false
                 _waterTextures.length = 0 // Clear old textures
                 let loaded = 0
                 for (let i = 0; i < _waterNumTex; i++) {
                     const texPath = _waterTexFile + String(i).padStart(2, '0') + '.blp'
                     const params = new URLSearchParams({token: bs.token, path: texPath})
                     if (DATA.archivePath) params.set('archive', DATA.archivePath)
                     if (DATA.tileset) params.set('tileset', DATA.tileset)
                     const url = 'http://127.0.0.1:' + bs.port + '/mdx/texture?' + params
                     const tex = _textureLoader.load(url, function () {
                         loaded++
                         if (loaded >= _waterNumTex) {
                             _waterTexturesReady = true
                             // Apply first frame immediately
                             if (_waterMesh && _waterTextures[0]) {
                                 _waterMesh.material.map = _waterTextures[0]
                                 _waterMesh.material.needsUpdate = true
                             }
                         }
                     })
                     tex.wrapS = THREE.ClampToEdgeWrapping
                     tex.wrapT = THREE.ClampToEdgeWrapping
                     tex.magFilter = THREE.LinearFilter
                     tex.minFilter = THREE.LinearMipmapLinearFilter
                     _waterTextures[i] = tex
                 }
             }
            _loadWaterTextures()

            // Advance water animation frame. Called from the render loop.
            function _updateWaterAnimation(dt) {
                if (!_waterTexturesReady || !_waterMesh || _waterNumTex <= 1) return
                _waterFrame += _waterTexRate * dt
                if (_waterFrame >= _waterNumTex) _waterFrame -= _waterNumTex * Math.floor(_waterFrame / _waterNumTex)
                const frame = Math.floor(_waterFrame) % _waterNumTex
                if (frame !== _waterLastFrame) {
                    _waterLastFrame = frame
                    const tex = _waterTextures[frame]
                    if (tex) {
                        _waterMesh.material.map = tex
                        _waterMesh.material.needsUpdate = true
                    }
                }
            }

            function _buildWaterMesh() {
                // Remove old water mesh
                if (_waterMesh) {
                    waterGroup.remove(_waterMesh)
                    _waterMesh.geometry.dispose()
                    _waterMesh.material.dispose()
                    _waterMesh = null
                }

                if (!showWater) return

                const layer = showSlopes ? computeRampLayerHeight() : D.layerHeight

                // Count water cells (cell has water if ANY of its 4 corners has water flag)
                const waterCells = []
                for (let cy = 0; cy < cellsY; cy++) {
                    for (let cx = 0; cx < cellsX; cx++) {
                        const iBL = cy * W + cx
                        const iBR = cy * W + cx + 1
                        const iTL = (cy + 1) * W + cx
                        const iTR = (cy + 1) * W + cx + 1

                        const hasWater = (D.flags[iBL] & 1) || (D.flags[iBR] & 1) ||
                                          (D.flags[iTL] & 1) || (D.flags[iTR] & 1)
                        if (hasWater) waterCells.push({cx, cy, iBL, iBR, iTL, iTR})
                    }
                }

                if (waterCells.length === 0) return

                // Build geometry: 2 triangles per water cell (TL→BR diagonal)
                const vertCount = waterCells.length * 4
                const faceCount = waterCells.length * 2
                const positions = new Float32Array(vertCount * 3)
                const colors = new Float32Array(vertCount * 4)
                const uvs = new Float32Array(vertCount * 2)
                const indices = new Uint32Array(faceCount * 3)

                let vi = 0, fi = 0
                for (const cell of waterCells) {
                    const {cx, cy, iBL, iBR, iTL, iTR} = cell

                    // Water heights at the 4 corners
                    const wBL = _waterZ(iBL)
                    const wBR = _waterZ(iBR)
                    const wTL = _waterZ(iTL)
                    const wTR = _waterZ(iTR)

                    // Ground heights at the 4 corners (for depth calculation)
                    const gBL = _groundZ(iBL, layer)
                    const gBR = _groundZ(iBR, layer)
                    const gTL = _groundZ(iTL, layer)
                    const gTR = _groundZ(iTR, layer)

                    // World XY positions of the 4 corners
                    // BL = (cx, cy), BR = (cx+1, cy), TL = (cx, cy+1), TR = (cx+1, cy+1)
                    // Convert to centered geometry coords:
                    const x0 = cx * TILE - halfGridW
                    const x1 = (cx + 1) * TILE - halfGridW
                    const y0 = cy * TILE - halfGridH
                    const y1 = (cy + 1) * TILE - halfGridH

                    // UV per vertex (matching HiveWE water.vert):
                    // BL → (0, 1), BR → (1, 1), TL → (0, 0), TR → (1, 0)
                    const base = vi
                    // BL
                    positions[vi * 3] = x0; positions[vi * 3 + 1] = y0; positions[vi * 3 + 2] = wBL
                    const cBL = _waterColor(wBL - gBL)
                    colors[vi * 4] = cBL[0]; colors[vi * 4 + 1] = cBL[1]; colors[vi * 4 + 2] = cBL[2]; colors[vi * 4 + 3] = cBL[3]
                    uvs[vi * 2] = 0; uvs[vi * 2 + 1] = 1
                    vi++
                    // BR
                    positions[vi * 3] = x1; positions[vi * 3 + 1] = y0; positions[vi * 3 + 2] = wBR
                    const cBR = _waterColor(wBR - gBR)
                    colors[vi * 4] = cBR[0]; colors[vi * 4 + 1] = cBR[1]; colors[vi * 4 + 2] = cBR[2]; colors[vi * 4 + 3] = cBR[3]
                    uvs[vi * 2] = 1; uvs[vi * 2 + 1] = 1
                    vi++
                    // TL
                    positions[vi * 3] = x0; positions[vi * 3 + 1] = y1; positions[vi * 3 + 2] = wTL
                    const cTL = _waterColor(wTL - gTL)
                    colors[vi * 4] = cTL[0]; colors[vi * 4 + 1] = cTL[1]; colors[vi * 4 + 2] = cTL[2]; colors[vi * 4 + 3] = cTL[3]
                    uvs[vi * 2] = 0; uvs[vi * 2 + 1] = 0
                    vi++
                    // TR
                    positions[vi * 3] = x1; positions[vi * 3 + 1] = y1; positions[vi * 3 + 2] = wTR
                    const cTR = _waterColor(wTR - gTR)
                    colors[vi * 4] = cTR[0]; colors[vi * 4 + 1] = cTR[1]; colors[vi * 4 + 2] = cTR[2]; colors[vi * 4 + 3] = cTR[3]
                    uvs[vi * 2] = 1; uvs[vi * 2 + 1] = 0
                    vi++

                    // Two triangles: TL-BL-BR diagonal (matching WC3 editor)
                    // Tri 1: TL, BL, BR
                    indices[fi * 3] = base + 2; indices[fi * 3 + 1] = base; indices[fi * 3 + 2] = base + 1
                    fi++
                    // Tri 2: TL, BR, TR
                    indices[fi * 3] = base + 2; indices[fi * 3 + 1] = base + 1; indices[fi * 3 + 2] = base + 3
                    fi++
                }

                const waterGeo = new THREE.BufferGeometry()
                waterGeo.setAttribute('position', new THREE.BufferAttribute(positions, 3))
                waterGeo.setAttribute('color', new THREE.BufferAttribute(colors, 4))
                waterGeo.setAttribute('uv', new THREE.BufferAttribute(uvs, 2))
                waterGeo.setIndex(new THREE.BufferAttribute(indices, 1))
                waterGeo.computeVertexNormals()

                const waterMat = new THREE.MeshBasicMaterial({
                    vertexColors: true,
                    transparent: true,
                    side: THREE.DoubleSide,
                    depthWrite: false,
                    opacity: 1.0,
                    map: (_waterTexturesReady && _waterTextures[0]) ? _waterTextures[0] : null,
                })

                _waterMesh = new THREE.Mesh(waterGeo, waterMat)
                // Keep water before transparent doodad layers (renderOrder 1/2),
                // otherwise non-depth-writing layers can appear below water.
                _waterMesh.renderOrder = 0
                waterGroup.add(_waterMesh)

                // Reset animation state
                _waterLastFrame = -1
            }

            // Initial water build
            _buildWaterMesh()

            // Expose water animation to the render loop
            _onAnimateWater = _updateWaterAnimation

            // Reload when game path changes (snapshot updated)
            W3E.onSnapshotChanged(function (snapshot, decorations, units) {
                // Update placement arrays from decorations/units placed items
                if (decorations && decorations.placed) {
                    _doodItems = decorations.placed.map((p, i) => ({
                        r: p.raw, t: p.text, v: p.variation,
                        m: p.modelPath || '',
                        i: i,
                        p: [p.position.x, p.position.y, p.position.z],
                        a: p.angle,
                        s: [p.scale.x, p.scale.y, p.scale.z]
                    }))
                    // Clear model cache so models are re-resolved with updated SLK paths
                    for (const key in _modelCache) delete _modelCache[key]
                }
                if (units && units.placed) {
                    _unitItems = units.placed.map(p => ({
                        r: p.raw, m: p.modelPath || '',
                        p: [p.position.x, p.position.y, p.position.z],
                        a: p.angle,
                        s: [p.scale.x, p.scale.y, p.scale.z]
                    }))
                }

                // Use DOOD/DEST data maps — they are rebuilt from the snapshot
                // which now includes w3d/w3b merges from the map archive.
                const doodDataMap = window._W3E_DOODADS ? window._W3E_DOODADS.getDataMap() : {}
                if (Object.keys(doodDataMap).length > 0) {
                    _doodFileMap = {}
                    for (const [rawId, d] of Object.entries(doodDataMap)) {
                        if (d.file) _doodFileMap[rawId] = {file: d.file, numVar: d.numVar || 1, maxPitch: d.maxPitch != null ? d.maxPitch : 0, maxRoll: d.maxRoll != null ? d.maxRoll : 0, fixedRot: d.fixedRot != null ? d.fixedRot : -1}
                    }
                }
                const destDataMap = window._W3E_DESTRUCTABLES ? window._W3E_DESTRUCTABLES.getDataMap() : {}
                if (Object.keys(destDataMap).length > 0) {
                    _destFileMap = {}
                    for (const [rawId, d] of Object.entries(destDataMap)) {
                        if (d.file) _destFileMap[rawId] = {file: d.file, numVar: d.numVar || 1, texId: d.texId || 0, texFile: d.texFile || '', maxPitch: d.maxPitch != null ? d.maxPitch : 0, maxRoll: d.maxRoll != null ? d.maxRoll : 0, fixedRot: d.fixedRot != null ? d.fixedRot : -1}
                    }
                }
                if (units && units.unitsMerged && units.unitsMerged.units) {
                    _unitFileMap = {}
                    for (const [rawId, u] of Object.entries(units.unitsMerged.units)) {
                        if (u.file) _unitFileMap[rawId] = u.file
                    }
                }
                if (snapshot.cliffTypesSlk && snapshot.cliffTypesSlk.cliffTypes) {
                    const prevMap = _cliffTypeMap
                    _cliffTypeMap = {}
                    for (const [id, ct] of Object.entries(snapshot.cliffTypesSlk.cliffTypes)) {
                        // Prefer per-map texSource (resolved with tileset MPQ) over snapshot's (tileset-agnostic)
                        const prevSource = prevMap[id] && prevMap[id].texSource ? prevMap[id].texSource : ''
                        _cliffTypeMap[id] = {cliffModelDir: ct.cliffModelDir || '', rampModelDir: ct.rampModelDir || '', texDir: ct.texDir || '', texFile: ct.texFile || '', texSource: prevSource || ct.texSource || '', groundTile: ct.groundTile || '', upperTile: ct.upperTile || ''}
                    }
                    // Update cliff ground-tile override and redraw terrain
                    DATA.cliffTypeMap = _cliffTypeMap
                    _cliffGroundOverride = computeCliffGroundOverride(_romp, _cliffCellRawcode)
                    applyColors()
                    if (canvasTex) buildComposited(_lastTileImages || [])
                    // Reload cliff tile texture previews
                    _loadCliffTilePreviews()
                }
                if (snapshot.cliffVariations) {
                    DATA.cliffVariations = snapshot.cliffVariations
                }
                const hasData = Object.keys(_doodFileMap).length > 0 || Object.keys(_destFileMap).length > 0 || Object.keys(_unitFileMap).length > 0 || Object.keys(_cliffTypeMap).length > 0
                if (hasData && (_doodItems.length > 0 || _unitItems.length > 0 || Object.keys(_cliffTypeMap).length > 0)) {
                    _collectAndLoad()
                }
            })
        }

        // ── Region overlays ─────────────────────────────────────────
        // Two layers per region:
        //   1. Fill   — semi-transparent mesh on terrain surface.
        //              depthTest:true + polygonOffset → occluded by models.
        //   2. Border — opaque outline along region edges.
        //              depthTest:false → shows through models, follows terrain.
        const _regionMeshes = {} // num → { fill, border, sx0, sy0, sx1, sy1, subW, subH }
        if (hasTerrain && mesh && DATA.w3rRegions && DATA.w3rRegions.length > 0) {
            const D = DATA.renderData
            const W = D.w, H = D.h
            const TILE = 128
            const halfGridW = (W - 1) * TILE / 2
            const halfGridH = (H - 1) * TILE / 2
            const terrainPos = mesh.geometry.attributes.position

            const fillGroup = new THREE.Group()
            fillGroup.renderOrder = 1
            scene.add(fillGroup)

            const borderGroup = new THREE.Group()
            borderGroup.renderOrder = 999
            scene.add(borderGroup)

            // Helper: scene coords + Z for a grid vertex (sx, sy)
            function _rgVert(sx, sy) {
                const gj = H - 1 - sy
                return [
                    -halfGridW + sx * TILE,
                    -halfGridH + sy * TILE,
                    terrainPos.getZ(gj * W + sx)
                ]
            }

            for (let i = 0; i < DATA.w3rRegions.length; i++) {
                const r = DATA.w3rRegions[i]
                const cr = r.color ? r.color.r : 0
                const cg = r.color ? r.color.g : 0
                const cb = r.color ? r.color.b : 0
                const rgbColor = new THREE.Color(cr / 255, cg / 255, cb / 255)

                // Convert region game-coord bounds → grid vertex indices
                let sx0 = (Math.min(r.left, r.right) - D.offsetX) / TILE
                let sx1 = (Math.max(r.left, r.right) - D.offsetX) / TILE
                let sy0 = (Math.min(r.bottom, r.top) - D.offsetY) / TILE
                let sy1 = (Math.max(r.bottom, r.top) - D.offsetY) / TILE

                sx0 = Math.max(0, Math.floor(sx0))
                sx1 = Math.min(W - 1, Math.ceil(sx1))
                sy0 = Math.max(0, Math.floor(sy0))
                sy1 = Math.min(H - 1, Math.ceil(sy1))

                const segX = sx1 - sx0
                const segY = sy1 - sy0
                if (segX < 1 || segY < 1) continue

                const subW = segX + 1
                const subH = segY + 1

                // ── Fill mesh ──────────────────────────────────────
                const fillPos = new Float32Array(subW * subH * 3)
                const fillIdx = []

                for (let dy = 0; dy < subH; dy++) {
                    for (let dx = 0; dx < subW; dx++) {
                        const v = _rgVert(sx0 + dx, sy0 + dy)
                        const vi = dy * subW + dx
                        fillPos[vi * 3    ] = v[0]
                        fillPos[vi * 3 + 1] = v[1]
                        fillPos[vi * 3 + 2] = v[2]
                    }
                }
                for (let dy = 0; dy < segY; dy++) {
                    for (let dx = 0; dx < segX; dx++) {
                        const a = dy * subW + dx
                        const b = a + 1
                        const c = a + subW
                        const d = c + 1
                        fillIdx.push(a, b, d)
                        fillIdx.push(a, d, c)
                    }
                }

                const fillGeo = new THREE.BufferGeometry()
                fillGeo.setAttribute('position', new THREE.BufferAttribute(fillPos, 3))
                fillGeo.setIndex(fillIdx)

                const fillMat = new THREE.MeshBasicMaterial({
                    color: rgbColor,
                    transparent: true,
                    opacity: 0.22,
                    side: THREE.DoubleSide,
                    depthWrite: false,
                    depthTest: true,
                    polygonOffset: true,
                    polygonOffsetFactor: -1,
                    polygonOffsetUnits: -1
                })
                const fillMesh = new THREE.Mesh(fillGeo, fillMat)
                fillMesh.renderOrder = 1
                fillMesh.visible = false
                fillGroup.add(fillMesh)

                // ── Border line (4 edges) ──────────────────────────
                const bv = []
                // Bottom: sy = sy0
                for (let dx = 0; dx < segX; dx++) {
                    const a = _rgVert(sx0 + dx, sy0), b = _rgVert(sx0 + dx + 1, sy0)
                    bv.push(a[0], a[1], a[2], b[0], b[1], b[2])
                }
                // Top: sy = sy1
                for (let dx = 0; dx < segX; dx++) {
                    const a = _rgVert(sx0 + dx, sy1), b = _rgVert(sx0 + dx + 1, sy1)
                    bv.push(a[0], a[1], a[2], b[0], b[1], b[2])
                }
                // Left: sx = sx0
                for (let dy = 0; dy < segY; dy++) {
                    const a = _rgVert(sx0, sy0 + dy), b = _rgVert(sx0, sy0 + dy + 1)
                    bv.push(a[0], a[1], a[2], b[0], b[1], b[2])
                }
                // Right: sx = sx1
                for (let dy = 0; dy < segY; dy++) {
                    const a = _rgVert(sx1, sy0 + dy), b = _rgVert(sx1, sy0 + dy + 1)
                    bv.push(a[0], a[1], a[2], b[0], b[1], b[2])
                }

                const borderGeo = new THREE.BufferGeometry()
                borderGeo.setAttribute('position', new THREE.Float32BufferAttribute(bv, 3))
                const borderLine = new THREE.LineSegments(borderGeo, new THREE.LineBasicMaterial({
                    color: rgbColor, depthTest: false, depthWrite: false
                }))
                borderLine.renderOrder = 999
                borderLine.visible = false
                borderGroup.add(borderLine)

                _regionMeshes[r.num] = {
                    fill: fillMesh, border: borderLine,
                    sx0, sy0, sx1, sy1, subW, subH
                }
            }

            // Sync heights with terrain on deformation/slopes change
            function _updateRegionMeshHeights() {
                for (const num in _regionMeshes) {
                    const rm = _regionMeshes[num]
                    const segX = rm.sx1 - rm.sx0, segY = rm.sy1 - rm.sy0

                    // Fill
                    const fPos = rm.fill.geometry.attributes.position
                    for (let dy = 0; dy < rm.subH; dy++) {
                        for (let dx = 0; dx < rm.subW; dx++) {
                            const vi = dy * rm.subW + dx
                            fPos.setZ(vi, _rgVert(rm.sx0 + dx, rm.sy0 + dy)[2])
                        }
                    }
                    fPos.needsUpdate = true

                    // Border
                    const bPos = rm.border.geometry.attributes.position
                    let bi = 0
                    for (let dx = 0; dx < segX; dx++) {
                        const a = _rgVert(rm.sx0 + dx, rm.sy0), b = _rgVert(rm.sx0 + dx + 1, rm.sy0)
                        bPos.setXYZ(bi++, a[0], a[1], a[2]); bPos.setXYZ(bi++, b[0], b[1], b[2])
                    }
                    for (let dx = 0; dx < segX; dx++) {
                        const a = _rgVert(rm.sx0 + dx, rm.sy1), b = _rgVert(rm.sx0 + dx + 1, rm.sy1)
                        bPos.setXYZ(bi++, a[0], a[1], a[2]); bPos.setXYZ(bi++, b[0], b[1], b[2])
                    }
                    for (let dy = 0; dy < segY; dy++) {
                        const a = _rgVert(rm.sx0, rm.sy0 + dy), b = _rgVert(rm.sx0, rm.sy0 + dy + 1)
                        bPos.setXYZ(bi++, a[0], a[1], a[2]); bPos.setXYZ(bi++, b[0], b[1], b[2])
                    }
                    for (let dy = 0; dy < segY; dy++) {
                        const a = _rgVert(rm.sx1, rm.sy0 + dy), b = _rgVert(rm.sx1, rm.sy0 + dy + 1)
                        bPos.setXYZ(bi++, a[0], a[1], a[2]); bPos.setXYZ(bi++, b[0], b[1], b[2])
                    }
                    bPos.needsUpdate = true
                }
            }

            // Initial visibility
            const initVis = W3E.getRegionVisibility()
            for (const num in _regionMeshes) {
                const vis = initVis[num] === true
                _regionMeshes[num].fill.visible = vis
                _regionMeshes[num].border.visible = vis
            }

            // React to visibility toggles
            W3E.onRegionVisibilityChanged(function (vis) {
                for (const num in _regionMeshes) {
                    const v = vis[num] === true
                    _regionMeshes[num].fill.visible = v
                    _regionMeshes[num].border.visible = v
                }
            })

            document.addEventListener('terrain-heights-changed', _updateRegionMeshHeights)
        }

        // ── FPS controls ────────────────────────────────────────────
        const ctrl = W3E.makeFpsControls(camera, canvas, maxDim, {zUp: true})
        ctrl.target.set(0, 0, 0)
        ctrl.saveInitState()

        const resetCameraBtn = document.getElementById('resetCameraBtn')
        if (resetCameraBtn) resetCameraBtn.addEventListener('click', function () { ctrl.reset() })

        function resize() {
            const cw = window.innerWidth, ch = window.innerHeight
            renderer.setSize(cw, ch)
            camera.aspect = cw / ch
            camera.updateProjectionMatrix()
        }

        resize()
        window.addEventListener('resize', resize);

        const _timer = new THREE.Timer();

        (function animate() {
            requestAnimationFrame(animate)
            _timer.update()
            const dt = _timer.getDelta()
            ctrl.update(dt)
            if (_onAnimateWater) _onAnimateWater(dt)
            renderer.render(scene, ctrl.camera)
        })()
    } catch (e) {
        console.error('Three.js init error:', e)
    }
})()
