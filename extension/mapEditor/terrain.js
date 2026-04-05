'use strict';

(async function () {
    const DATA = window.__W3E_DATA__
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
        unitDooItems: DATA.unitDooItems || []
    })

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
                    if (resp.ok) {
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
                    } else {
                        throw new Error('HTTP ' + resp.status)
                    }
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
            // When all 4 corners of a cell have the ramp flag AND
            // different layerHeight values, the higher corners are
            // lowered to the cell's minimum layerHeight. This turns
            // the vertical cliff step into a smooth slope.
            //
            // A point may be a corner of up to 4 cells; we take the
            // minimum adjusted value across all of them.
            function computeRampLayerHeight() {
                const adjusted = new Uint8Array(D.layerHeight)
                for (let cy = 0; cy < H - 1; cy++) {
                    for (let cx = 0; cx < W - 1; cx++) {
                        const iBL = cy * W + cx
                        const iBR = cy * W + cx + 1
                        const iTL = (cy + 1) * W + cx
                        const iTR = (cy + 1) * W + cx + 1

                        // All 4 corners must have the ramp flag (bit 3)
                        if (!((D.flags[iBL] & 8) && (D.flags[iBR] & 8) &&
                              (D.flags[iTL] & 8) && (D.flags[iTR] & 8))) continue

                        const lBL = D.layerHeight[iBL]
                        const lBR = D.layerHeight[iBR]
                        const lTL = D.layerHeight[iTL]
                        const lTR = D.layerHeight[iTR]

                        const minL = Math.min(lBL, lBR, lTL, lTR)
                        const maxL = Math.max(lBL, lBR, lTL, lTR)
                        if (minL === maxL) continue // no cliff in this cell

                        // Lower any corner above the minimum
                        if (lBL > minL) adjusted[iBL] = Math.min(adjusted[iBL], minL)
                        if (lBR > minL) adjusted[iBR] = Math.min(adjusted[iBR], minL)
                        if (lTL > minL) adjusted[iTL] = Math.min(adjusted[iTL], minL)
                        if (lTR > minL) adjusted[iTR] = Math.min(adjusted[iTR], minL)
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
            }

            applyHeights()

            // ── Cliff ground-tile override ────────────────────────
            // A tilepoint that is a corner of at least one cliff cell
            // has its ground texture replaced by the cliff type's
            // groundTile rawcode. Only the points that directly belong
            // to a cliff quad are affected — neighbouring flat-only
            // points keep their original texture.
            let _cliffGroundOverride = null // Int8Array, -1 = no override

            function computeCliffGroundOverride() {
                const arr = new Int8Array(W * H).fill(-1)
                const cliffCodes = DATA.cliffTileCodes || []
                const groundCodes = DATA.groundTileCodes || []
                const ctMap = DATA.cliffTypeMap || {}

                if (cliffCodes.length === 0 || groundCodes.length === 0 || Object.keys(ctMap).length === 0) return arr

                // Build groundTile rawcode → ground tile index
                const groundCodeIndex = {}
                for (let i = 0; i < groundCodes.length; i++) {
                    const c = typeof groundCodes[i] === 'string' ? groundCodes[i] : groundCodes[i].text || ''
                    if (c) groundCodeIndex[c] = i
                }

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

                        if (lBL === lBR && lBR === lTL && lTL === lTR) continue // flat, no cliff

                        // Cliff texture index from bottom-left corner
                        const ctIdx = D.cliffTexture[iBL]
                        if (ctIdx >= 15 || ctIdx >= cliffCodes.length) continue

                        const rawcode = typeof cliffCodes[ctIdx] === 'string'
                            ? cliffCodes[ctIdx]
                            : cliffCodes[ctIdx].text || cliffCodes[ctIdx].raw || ''
                        if (!rawcode) continue

                        const ct = ctMap[rawcode]
                        if (!ct || !ct.groundTile) continue

                        const gtIdx = groundCodeIndex[ct.groundTile]
                        if (gtIdx === undefined) continue

                        // Mark only the 4 corners of this cliff cell
                        arr[iBL] = gtIdx
                        arr[iBR] = gtIdx
                        arr[iTL] = gtIdx
                        arr[iTR] = gtIdx
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
                            if (lBL === lBR && lBR === lTL && lTL === lTR) continue

                            // Ramp entrance: all 4 corners have ramp flag AND
                            // heights are NOT diagonally symmetric → keep terrain
                            // visible (HiveWE terrain.ixx is_corner_ramp_entrance
                            // + update_ground_exists line 940).
                            const fBL = D.flags[iBL], fBR = D.flags[iBR],
                                  fTL = D.flags[iTL], fTR = D.flags[iTR]
                            if ((fBL & 8) && (fBR & 8) && (fTL & 8) && (fTR & 8) &&
                                !(lBL === lTR && lTL === lBR)) continue

                            // Cliff cell → make its 2 triangles degenerate (all same vertex)
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

            const mat = new THREE.MeshLambertMaterial({map: colorTex, side: THREE.DoubleSide})
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
                    }
                }

                canvasTex = new THREE.CanvasTexture(c2)
                canvasTex.magFilter = THREE.LinearFilter
                canvasTex.minFilter = THREE.LinearFilter
                if (useTextures) {
                    mat.map = canvasTex
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
                    const url = 'http://127.0.0.1:' + bs.port + '/w3e/tileTextures?' + params
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
            if (useTextures && canvasTex) {
                mat.map = canvasTex
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
                saveCbState()
            })
            cb('cbWireframe', e => {
                fineMesh.visible = e.target.checked
                coarseMesh.visible = e.target.checked
                saveCbState()
            })
            cb('cbTextures', e => {
                useTextures = e.target.checked
                mat.map = (useTextures && canvasTex) ? canvasTex : colorTex
                mat.needsUpdate = true
                saveCbState()
            })
            cb('cbDeformation', e => {
                showDeformation = e.target.checked
                applyHeights()
                rebuildWireframe()
                _updateTerrainHeightTexture()
                _buildWaterMesh()
                saveCbState()
            })
            cb('cbSlopes', e => {
                showSlopes = e.target.checked
                applyHeights()
                rebuildWireframe()
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
                _terrainHeightTex.magFilter = THREE.NearestFilter
                _terrainHeightTex.minFilter = THREE.NearestFilter
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

            canvas.addEventListener('mousemove', e => {
                const rect = canvas.getBoundingClientRect()
                mouseNdc.x = ((e.clientX - rect.left) / rect.width) * 2 - 1
                mouseNdc.y = -((e.clientY - rect.top) / rect.height) * 2 + 1
                raycaster.setFromCamera(mouseNdc, ctrl.camera)
                const hits = raycaster.intersectObject(mesh)
                if (hits.length > 0) {
                    const pt = hits[0].point

                    // Snap to nearest data point (grid vertex).
                    // Math.round ensures we pick the closest vertex,
                    // not the one below-left (Math.floor).
                    //   sx = round((pt.x + worldW/2) / TILE)  → 0..W-1
                    //   sy = round((pt.y + worldH/2) / TILE)  → 0..H-1
                    //   sy=0 is bottom, sy=H-1 is top.
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
            var _clickStartX = 0, _clickStartY = 0
            canvas.addEventListener('pointerdown', function (e) {
                _clickStartX = e.clientX
                _clickStartY = e.clientY
            })
            canvas.addEventListener('click', function (e) {
                // Ignore if it was a drag (orbit/pan)
                var dx = e.clientX - _clickStartX, dy = e.clientY - _clickStartY
                if (dx * dx + dy * dy > 9) return
                if (e.target.closest('float-window') || e.target.closest('.menubar')) return

                var rect = canvas.getBoundingClientRect()
                var ndc = new THREE.Vector2(
                    ((e.clientX - rect.left) / rect.width) * 2 - 1,
                    -((e.clientY - rect.top) / rect.height) * 2 + 1
                )
                var rc = new THREE.Raycaster()
                rc.setFromCamera(ndc, ctrl.camera)

                // Check cliff models first (if visible)
                if (cliffGroup.visible) {
                    var cliffHits = rc.intersectObjects(cliffGroup.children, false)
                    if (cliffHits.length > 0) {
                        var cliffHit = cliffHits[0]
                        var cliffObj = cliffHit.object
                        if (cliffObj.userData && cliffObj.userData._items && cliffHit.instanceId != null) {
                            var cliffItem = cliffObj.userData._items[cliffHit.instanceId]
                            if (cliffItem && cliffItem.path && vscode) {
                                var cmd = {command: 'openModel', path: cliffItem.path}
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
                var hits = rc.intersectObjects(objectGroup.children, false)
                if (hits.length > 0) {
                    var hit = hits[0]
                    var obj = hit.object
                    if (obj.userData && obj.userData._items && hit.instanceId != null) {
                        var item = obj.userData._items[hit.instanceId]
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
            const _doodItems = DATA.doodadPlacements || []
            const _unitItems = DATA.unitPlacements || []

            const _modelCache = {} // path → [{geometry, material}]
            const _pendingItems = {} // path → [items]
            const _textureLoader = new THREE.TextureLoader()
            _textureLoader.crossOrigin = 'anonymous'

            // Red cube fallback for missing models
            const _FALLBACK_SIZE = TILE * 0.35
            const _fallbackGeo = new THREE.BoxGeometry(_FALLBACK_SIZE, _FALLBACK_SIZE, _FALLBACK_SIZE)
            const _fallbackMat = new THREE.MeshPhongMaterial({color: 0xff0000, flatShading: true})
            const _fallbackEntries = [{geometry: _fallbackGeo, material: _fallbackMat}]

            function _b64f32(b64) {
                if (!b64) return new Float32Array(0)
                const bin = atob(b64), buf = new ArrayBuffer(bin.length), u8 = new Uint8Array(buf)
                for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i)
                return new Float32Array(buf)
            }

            function _b64u16(b64) {
                if (!b64) return new Uint16Array(0)
                const bin = atob(b64), buf = new ArrayBuffer(bin.length), u8 = new Uint8Array(buf)
                for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i)
                return new Uint16Array(buf)
            }

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
                            'vec2 _tp = clamp((mvPosition.xy + uHalfGrid) / uTileSize, vec2(0.0), uTerrainSize - 1.0);',
                            'vec2 _fl = floor(_tp);',
                            'vec2 _uv = (_fl + 0.5) / uTerrainSize;',
                            'float _h = texture2D(uTerrainHeight, _uv).r;',
                            '',
                            '// Terrain normal from neighbor samples (HiveWE cliff.vert lines 27-33)',
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

                for (const g of geosets) {
                    if (!g.vertex_count || !g.face_count) continue
                    const verts = _b64f32(g.vertices)
                    const norms = _b64f32(g.normals)
                    const faces = _b64u16(g.faces)
                    const uvs = _b64f32(g.uvs)

                    const geo = new THREE.BufferGeometry()
                    geo.setAttribute('position', new THREE.BufferAttribute(verts, 3))
                    if (norms.length > 0) geo.setAttribute('normal', new THREE.BufferAttribute(norms, 3))
                    if (uvs.length > 0) geo.setAttribute('uv', new THREE.BufferAttribute(uvs, 2))
                    geo.setIndex(new THREE.BufferAttribute(faces, 1))
                    if (norms.length === 0) geo.computeVertexNormals()

                    const matOpts = {
                        color: 0xcccccc,
                        side: THREE.DoubleSide,
                        flatShading: false,
                    }
                    // Cliff models: disable specular so adjacent instances
                    // have consistent diffuse-only lighting (no bright edge seams).
                    if (isCliff) {
                        matOpts.specular = 0x000000
                        matOpts.shininess = 0
                    }

                    // Look up texture via material_id → material → layer → texture
                    if (g.material_id != null && g.material_id < materials.length) {
                        const mat = materials[g.material_id]
                        const layers = mat.layers || []
                        if (layers.length > 0) {
                            const layer = layers[0]
                            const texId = layer.texture_id
                            if (texId < textures.length) {
                                const tex = textures[texId]
                                // Determine texture path: use replaceable texture override if available
                                var texPath = null
                                if (tex && tex.replaceable_id && replaceableTextures) {
                                    if (replaceableTextures._cliffTex !== undefined) {
                                        // Cliff model: single material with Replaceable ID 11,
                                        // texture = texDir\texFile from CliffTypes.slk
                                        texPath = replaceableTextures._cliffTex
                                    } else if (replaceableTextures[tex.replaceable_id]) {
                                        texPath = replaceableTextures[tex.replaceable_id]
                                    }
                                } else if (tex && tex.file_name && !tex.replaceable_id) {
                                    texPath = tex.file_name
                                }
                                if (texPath) {
                                    const url = _texUrl(texPath)
                                    if (url) {
                                        const t = _textureLoader.load(url)
                                        t.wrapS = THREE.RepeatWrapping
                                        t.wrapT = THREE.RepeatWrapping
                                        t.magFilter = THREE.LinearFilter
                                        t.minFilter = THREE.LinearMipmapLinearFilter
                                        matOpts.map = t
                                        matOpts.color = 0xffffff
                                    }
                                }
                            }
                            const fm = layer.filter_mode
                            if (fm === 1) {
                                matOpts.transparent = true
                                matOpts.alphaTest = 0.5
                            } else if (fm === 2 || fm === 3) {
                                matOpts.transparent = true
                                matOpts.blending = fm === 3 ? THREE.AdditiveBlending : THREE.NormalBlending
                                matOpts.depthWrite = false
                            }
                            if (layer.alpha < 1.0) {
                                matOpts.transparent = true
                                matOpts.opacity = layer.alpha
                            }
                        }
                    }

                    const meshMat = new THREE.MeshPhongMaterial(matOpts)
                    if (isCliff && _terrainHeightTex) _applyCliffShader(meshMat)
                    entries.push({geometry: geo, material: meshMat})
                }
                return entries
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
                    if (group) instMesh.frustumCulled = false // cliff shader moves verts
                    instMesh.userData._items = items
                    for (let i = 0; i < items.length; i++) {
                        const it = items[i]
                        pos.set(
                            it.p[0] - D.offsetX - halfGridW,
                            it.p[1] - D.offsetY - halfGridH,
                            it.p[2]
                        )
                        euler.set(it.rx || 0, it.ry || 0, it.a || 0)
                        quat.setFromEuler(euler)
                        scl.set(it.s[0] || 1, it.s[1] || 1, it.s[2] || 1)
                        mat4.compose(pos, quat, scl)
                        instMesh.setMatrixAt(i, mat4)
                    }
                    instMesh.instanceMatrix.needsUpdate = true
                    targetGroup.add(instMesh)
                }
            }

            // Resolve the model file path for a doodad, applying variation logic
            function _resolveModelPath(baseFile, numVar, variation) {
                var lastSlash = Math.max(baseFile.lastIndexOf('/'), baseFile.lastIndexOf('\\'))
                var dotIdx = baseFile.lastIndexOf('.')
                var hasExt = dotIdx > lastSlash && dotIdx >= 0
                var base = hasExt ? baseFile.substring(0, dotIdx) : baseFile
                var ext = hasExt ? baseFile.substring(dotIdx) : '.mdx'
                if (numVar <= 1) return base + ext
                var idx = (variation || 0) % numVar
                return base + idx + ext
            }

            // Create centered items for missing cliff fallback cubes.
            // Normal cliff models get terrain deformation via the cliff shader,
            // but fallback cubes use plain material — add deformation manually.
            function _cliffCenteredItems(items) {
                const half = TILE / 2
                return items.map(function (it) {
                    var defZ = 0
                    if (showDeformation) {
                        var sx = Math.max(0, Math.min(W - 1, Math.round((it.p[0] - D.offsetX) / TILE)))
                        var sy = Math.max(0, Math.min(H - 1, Math.round((it.p[1] - D.offsetY) / TILE)))
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
                const cliffCodes = DATA.cliffTileCodes || []
                if (cliffCodes.length === 0 || Object.keys(_cliffTypeMap).length === 0) return items

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

                        const base = Math.min(lBL, lBR, lTL, lTR)
                        const peak = Math.max(lBL, lBR, lTL, lTR)
                        if (base === peak) continue // no cliff

                        // Cliff texture index from bottom-left corner
                        const ctIdx = D.cliffTexture[iBL]
                        if (ctIdx >= 15 || ctIdx >= cliffCodes.length) continue

                        const rawcode = typeof cliffCodes[ctIdx] === 'string'
                            ? cliffCodes[ctIdx]
                            : cliffCodes[ctIdx].text || cliffCodes[ctIdx].raw || ''
                        if (!rawcode) continue

                        const ct = _cliffTypeMap[rawcode]
                        if (!ct) continue

                        // Determine if this cell is a ramp (all 4 corners have ramp flag)
                        const isRamp = (D.flags[iBL] & 8) && (D.flags[iBR] & 8) &&
                                       (D.flags[iTL] & 8) && (D.flags[iTR] & 8)

                        // Ramp entrance: all 4 ramp AND heights NOT diagonally
                        // symmetric → terrain mesh stays visible, no cliff model
                        // (HiveWE terrain.ixx is_corner_ramp_entrance line 803-815)
                        if (isRamp && !(lBL === lTR && lTL === lBR)) continue

                        const modelDir = isRamp ? ct.rampModelDir : ct.cliffModelDir
                        if (!modelDir) continue

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

                        // Clamp variation to max available for this pattern
                        // (HiveWE terrain.ixx line 1080: std::clamp(cliff_variation, 0, cliff_variations[pattern]))
                        const pattern = cTL + cTR + cBR + cBL
                        const varMap = modelDir === 'CityCliffs' ? _cityCliffVariations : _cliffVariations
                        const maxVar = varMap[pattern] !== undefined ? varMap[pattern] : 0
                        const variation = Math.min(D.cliffVariation[iBL], maxVar)

                        const modelPath = 'Doodads\\Terrain\\' + modelDir + '\\' + modelDir +
                            pattern + variation + '.mdx'

                        // Cliff texture: derived from CliffTypes.slk texDir/texFile
                        // (HiveWE terrain.ixx lines 372-374: texdir + texfile)
                        const cliffTex = (ct.texDir && ct.texFile)
                            ? ct.texDir + '\\' + ct.texFile + '.blp'
                            : null

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

                const pathsNeeded = {} // modelPath → true (for loading dedup)
                const byCacheKey = {} // cacheKey → {path, items, texId, texFile}
                const _unmappedItems = [] // items with no rawcode→file mapping
                for (const item of _doodItems) {
                    const entry = _doodFileMap[item.r] || _destFileMap[item.r]
                    if (!entry) { _unmappedItems.push(item); continue }
                    const file = typeof entry === 'string' ? entry : entry.file
                    const numVar = typeof entry === 'object' ? (entry.numVar || 1) : 1
                    const resolved = _resolveModelPath(file, numVar, item.v)
                    const texFile = typeof entry === 'object' ? (entry.texFile || '') : ''
                    const texId = typeof entry === 'object' ? (entry.texId || 0) : 0
                    const cacheKey = texFile ? (resolved + '|' + texFile) : resolved
                    if (!byCacheKey[cacheKey]) byCacheKey[cacheKey] = {path: resolved, items: [], texId, texFile}
                    byCacheKey[cacheKey].items.push(item)
                    pathsNeeded[resolved] = true
                }
                for (const item of _unitItems) {
                    const file = _unitFileMap[item.r]
                    if (!file) { _unmappedItems.push(item); continue }
                    const cacheKey = file
                    if (!byCacheKey[cacheKey]) byCacheKey[cacheKey] = {path: file, items: [], texId: 0, texFile: ''}
                    byCacheKey[cacheKey].items.push(item)
                    pathsNeeded[file] = true
                }

                // Collect cliff/ramp models from terrain data
                const cliffItems = _collectCliffItems()
                for (const item of cliffItems) {
                    // Cache key includes texture path so different cliff types get separate entries
                    const texKey = item.cliffTex || ''
                    const cacheKey = texKey ? (item.path + '|' + texKey) : item.path
                    if (!byCacheKey[cacheKey]) byCacheKey[cacheKey] = {
                        path: item.path, items: [],
                        _cliff: true,
                        _cliffTex: item.cliffTex,
                    }
                    byCacheKey[cacheKey].items.push(item)
                    pathsNeeded[item.path] = true
                }


                // Place red cubes for items with no rawcode→file mapping
                if (_unmappedItems.length > 0) {
                    _placeInstances(_unmappedItems, _fallbackEntries)
                }

                // Place already-cached models; collect uncached for loading
                const toLoad = []
                for (const [cacheKey, info] of Object.entries(byCacheKey)) {
                    const grp = info._cliff ? cliffGroup : undefined
                    if (_modelCache[cacheKey]) {
                        _placeInstances(info.items, _modelCache[cacheKey], grp)
                    } else if (_rawModelData[info.path]) {
                        // Model data already loaded but not yet built for this texture variant
                        const replTex = _buildReplTex(info)
                        const entries = _buildModel(_rawModelData[info.path], replTex, !!info._cliff)
                        _modelCache[cacheKey] = entries
                        _placeInstances(info.items, entries, grp)
                    } else {
                        _pendingItems[cacheKey] = info
                        if (!toLoad.includes(info.path)) toLoad.push(info.path)
                    }
                }

                if (toLoad.length > 0 && vscode) {
                    vscode.postMessage({command: 'loadMapObjects', paths: toLoad})
                }
            }

            // Listen for model data coming back from the extension host
            window.addEventListener('message', function (e) {
                const msg = e.data
                if (msg && msg.command === 'mapObjectModel') {
                    _rawModelData[msg.path] = msg
                    // Build entries for each pending cache key that references this model path
                    for (const [cacheKey, info] of Object.entries(_pendingItems)) {
                        if (info.path !== msg.path) continue
                        const replTex = _buildReplTex(info)
                        const entries = _buildModel(msg, replTex, !!info._cliff)
                        _modelCache[cacheKey] = entries
                        const grp = info._cliff ? cliffGroup : objectGroup
                        if (info.items && grp.visible) {
                            _placeInstances(info.items, entries, info._cliff ? cliffGroup : undefined)
                        }
                        delete _pendingItems[cacheKey]
                    }
                } else if (msg && msg.command === 'mapObjectModelNotFound') {
                    // Model file could not be loaded — red cubes for all (cliffs centered)
                    for (const [cacheKey, info] of Object.entries(_pendingItems)) {
                        if (info.path !== msg.path) continue
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
                _waterMesh.renderOrder = 10  // render after terrain
                waterGroup.add(_waterMesh)

                // Reset animation state
                _waterLastFrame = -1
            }

            // Initial water build
            _buildWaterMesh()

            // Expose water animation to the render loop
            _onAnimateWater = _updateWaterAnimation

            // Reload when game path changes (snapshot updated)
            W3E.onSnapshotChanged(function (snapshot) {
                if (snapshot.doodadsSlk && snapshot.doodadsSlk.doodads) {
                    _doodFileMap = {}
                    for (const [rawId, d] of Object.entries(snapshot.doodadsSlk.doodads)) {
                        if (d.file) _doodFileMap[rawId] = {file: d.file, numVar: d.numVar || 1}
                    }
                }
                if (snapshot.destructablesSlk && snapshot.destructablesSlk.destructables) {
                    _destFileMap = {}
                    for (const [rawId, d] of Object.entries(snapshot.destructablesSlk.destructables)) {
                        if (d.file) _destFileMap[rawId] = {file: d.file, numVar: d.numVar || 1, texId: d.texId || 0, texFile: d.texFile || ''}
                    }
                }
                if (snapshot.unitsSlk && snapshot.unitsSlk.units) {
                    _unitFileMap = {}
                    for (const [rawId, u] of Object.entries(snapshot.unitsSlk.units)) {
                        if (u.file) _unitFileMap[rawId] = u.file
                    }
                }
                if (snapshot.cliffTypesSlk && snapshot.cliffTypesSlk.cliffTypes) {
                    const prevMap = _cliffTypeMap
                    _cliffTypeMap = {}
                    for (const [id, ct] of Object.entries(snapshot.cliffTypesSlk.cliffTypes)) {
                        // Prefer per-map texSource (resolved with tileset MPQ) over snapshot's (tileset-agnostic)
                        const prevSource = prevMap[id] && prevMap[id].texSource ? prevMap[id].texSource : ''
                        _cliffTypeMap[id] = {cliffModelDir: ct.cliffModelDir || '', rampModelDir: ct.rampModelDir || '', texDir: ct.texDir || '', texFile: ct.texFile || '', texSource: prevSource || ct.texSource || '', groundTile: ct.groundTile || ''}
                    }
                    // Update cliff ground-tile override and redraw terrain
                    DATA.cliffTypeMap = _cliffTypeMap
                    _cliffGroundOverride = computeCliffGroundOverride()
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

        // ── Orbit controls ──────────────────────────────────────
        const ctrl = W3E.makeOrbitControls(camera, canvas, maxDim, {zUp: true})
        ctrl.target.set(0, 0, 0)

        function resize() {
            const cw = window.innerWidth, ch = window.innerHeight
            renderer.setSize(cw, ch)
            camera.aspect = cw / ch
            camera.updateProjectionMatrix()
        }

        resize()
        window.addEventListener('resize', resize);

        const _clock = new THREE.Clock();

        (function animate() {
            requestAnimationFrame(animate)
            const dt = _clock.getDelta()
            ctrl.update()
            if (_onAnimateWater) _onAnimateWater(dt)
            renderer.render(scene, ctrl.camera)
        })()
    } catch (e) {
        console.error('Three.js init error:', e)
    }
})()

