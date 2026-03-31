'use strict';

(function () {
    const DATA = window.__W3E_DATA__
    if (!DATA) return

    const vscode = (typeof acquireVsCodeApi === 'function') ? acquireVsCodeApi() : null

    W3E.init({
        vscode: vscode,
        groundTileCodes: DATA.groundTileCodes,
        cliffTileCodes: DATA.cliffTileCodes,
        isArchive: DATA.isArchive
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

        scene.add(new THREE.AmbientLight(0xffffff, 0.4))
        const dirLight = new THREE.DirectionalLight(0xffffff, 0.8)
        dirLight.position.set(1, 2, 1.5).normalize()
        scene.add(dirLight)

        let maxDim = 10000
        let mesh = null

        if (hasTerrain) {
            const D = DATA.renderData
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

            let showWater = false, showBoundary = false, showBlight = false, showRamp = false
            let showDeformation = true

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

            // ── Height formula ──────────────────────────────────
            // Base height:   (layerHeight - 2) * TILE
            // Deformation:   (groundHeight - 8192) / 4
            // Final Z:       base + deformation (if enabled)
            function applyHeights() {
                const pos = geo.attributes.position
                for (let gj = 0; gj < H; gj++) {
                    for (let gi = 0; gi < W; gi++) {
                        const vi = gj * W + gi
                        const idx = (H - 1 - gj) * W + gi
                        let h = (D.layerHeight[idx] - 2) * TILE
                        if (showDeformation) {
                            h += (D.groundHeight[idx] - H_ZERO) / H_SCALE
                        }
                        pos.setZ(vi, h)
                    }
                }
                pos.needsUpdate = true
                geo.computeVertexNormals()
            }

            applyHeights()

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
                [[.75,0],[1,0],[1,.25],[0,.75],[0,1],[.25,1]],           // 7: BD
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

            function applyColors() {
                const ctx = colorCanvas.getContext('2d')
                ctx.clearRect(0, 0, colorCanvas.width, colorCanvas.height)

                for (let cy = 0; cy < cellsY; cy++) {
                    for (let cx = 0; cx < cellsX; cx++) {
                        const iBL = cy * W + cx
                        const iBR = cy * W + cx + 1
                        const iTL = (cy + 1) * W + cx
                        const iTR = (cy + 1) * W + cx + 1

                        const bl = D.groundTexture[iBL]
                        const br = D.groundTexture[iBR]
                        const tl = D.groundTexture[iTL]
                        const tr = D.groundTexture[iTR]

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
                        if (showWater && D.waterFlag[fi]) {
                            ctx.fillStyle = 'rgba(0,60,200,0.4)'
                            ctx.fillRect(dstX, dstY, COLOR_CPX, COLOR_CPX)
                        }
                        if (showBlight && D.blightFlag[fi]) {
                            ctx.fillStyle = 'rgba(180,0,0,0.3)'
                            ctx.fillRect(dstX, dstY, COLOR_CPX, COLOR_CPX)
                        }
                        if (showRamp && D.rampFlag[fi]) {
                            ctx.fillStyle = 'rgba(200,200,0,0.3)'
                            ctx.fillRect(dstX, dstY, COLOR_CPX, COLOR_CPX)
                        }
                        if (showBoundary && D.boundaryFlag[fi]) {
                            ctx.fillStyle = 'rgba(0,0,0,0.6)'
                            ctx.fillRect(dstX, dstY, COLOR_CPX, COLOR_CPX)
                        }
                    }
                }

                colorTex.needsUpdate = true
            }

            applyColors()

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

            // ── Load tile texture images and build composited canvas ──
            // Each ground tile texture is a 4×4 (or 8×4 for rectangular)
            // grid of sub-tiles. For each cell, we determine which textures
            // are present at the 4 corner points, sort them ascending,
            // and draw them bottom-to-top:
            //   1) The lowest texture always draws a full fill (base layer).
            //   2) Each subsequent texture draws a transition sub-tile
            //      covering only the corners that have exactly that texture.
            if (TILE_TEXTURES.length > 0) {
                const tileImages = new Array(TILE_TEXTURES.length).fill(null)
                let toLoad = 0
                let loaded = 0

                TILE_TEXTURES.forEach((entry, i) => {
                    if (!entry || !entry.dataUrl) return
                    toLoad++
                    const img = new Image()
                    img.onload = () => {
                        tileImages[i] = img
                        if (++loaded === toLoad) buildComposited()
                    }
                    img.onerror = () => {
                        if (++loaded === toLoad) buildComposited()
                    }
                    img.src = entry.dataUrl
                })

                if (toLoad === 0) buildComposited()

                function buildComposited() {
                    const CPX = 32
                    const c2 = document.createElement('canvas')
                    c2.width = cellsX * CPX
                    c2.height = cellsY * CPX
                    const ctx = c2.getContext('2d')

                    // Fill tile pools: sub-tile indices used for full coverage (mask=15).
                    // When all 4 corners have the same texture (or it's the base
                    // layer), we pick a variation from this pool using
                    // groundVariation[iBL] to avoid tiling repetition.
                    //
                    // Square textures (4×4 = 16 sub-tiles):
                    //   only sub-tiles 1 and 16 are full-fill → 2 variants
                    //
                    // Rectangular textures (8×4 = 32 sub-tiles):
                    //   sub-tiles 17..32 (right half) + 1 and 16 → 18 variants
                    const FILL_SQUARE = [1, 16]
                    const FILL_RECT = []
                    for (let f = 17; f <= 32; f++) FILL_RECT.push(f)
                    FILL_RECT.push(1, 16)

                    // Convert a 1-based sub-tile index to pixel coordinates
                    // in the texture image.
                    //
                    // Square (4×4):
                    //   1  2  3  4
                    //   5  6  7  8
                    //   9 10 11 12
                    //  13 14 15 16
                    //
                    // Rectangular (two 4×4 halves side-by-side):
                    //   1  2  3  4 | 17 18 19 20
                    //   5  6  7  8 | 21 22 23 24
                    //   9 10 11 12 | 25 26 27 28
                    //  13 14 15 16 | 29 30 31 32
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

                    // W×H points → (W-1)×(H-1) cells.
                    //
                    // Cell (cx, cy) uses four corner tilepoints:
                    //   TL ── TR       TL = point(cx,   cy+1)
                    //    │    │        TR = point(cx+1, cy+1)
                    //   BL ── BR       BL = point(cx,   cy  )
                    //                  BR = point(cx+1, cy  )
                    //
                    // Example: corners have textures
                    //   A=6  B=3       (TL=6, TR=3)
                    //   D=2  C=0       (BL=2, BR=0)
                    //
                    // Sorted unique: [0, 2, 3, 6]
                    // Rendering order (bottom → top):
                    //   1) tex 0 → full fill (base, because multiple textures)
                    //   2) tex 2 → D only   (BL===2)  → subtile 3
                    //   3) tex 3 → B only   (TR===3)  → subtile 5
                    //   4) tex 6 → A only   (TL===6)  → subtile 9

                    for (let cy = 0; cy < cellsY; cy++) {
                        for (let cx = 0; cx < cellsX; cx++) {
                            const iBL = cy * W + cx
                            const iBR = cy * W + cx + 1
                            const iTL = (cy + 1) * W + cx
                            const iTR = (cy + 1) * W + cx + 1

                            const bl = D.groundTexture[iBL]
                            const br = D.groundTexture[iBR]
                            const tl = D.groundTexture[iTL]
                            const tr = D.groundTexture[iTR]

                            // Unique layers sorted ascending (lower index = lower layer)
                            const unique = [...new Set([bl, br, tl, tr])].sort((a, b) => a - b)

                            // Canvas Y is flipped: canvas row 0 = terrain top
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

                                // 4-bit mask: which corners have exactly this texture
                                let mask = 0
                                if (li === 0 && unique.length > 1) {
                                    // Lowest layer with multiple textures → full fill as base
                                    mask = 15
                                } else {
                                    if (bl === L) mask |= 2 // bit 1 = BL
                                    if (br === L) mask |= 1 // bit 0 = BR
                                    if (tl === L) mask |= 8 // bit 3 = TL
                                    if (tr === L) mask |= 4 // bit 2 = TR
                                }

                                if (mask === 0) continue

                                const texW = img.naturalWidth
                                const texH = img.naturalHeight
                                const isRect = texW >= texH * 2
                                let subtile

                                if (mask === 15) {
                                    // Full fill — select from fill pool using variation
                                    const variation = D.groundVariation[iBL]
                                    const pool = isRect ? FILL_RECT : FILL_SQUARE
                                    subtile = pool[variation % pool.length]
                                } else {
                                    // ── Transition sub-tile selection ──
                                    //
                                    // Cell corners (as in geometry):
                                    //   A B      A = TL (top-left)
                                    //   D C      B = TR (top-right)
                                    //            C = BR (bottom-right)
                                    //            D = BL (bottom-left)
                                    //
                                    // Mask bits:
                                    //   bit 0 (1) = BR = C
                                    //   bit 1 (2) = BL = D
                                    //   bit 2 (4) = TR = B
                                    //   bit 3 (8) = TL = A
                                    //
                                    // Sub-tile layout in the texture (4×4 grid):
                                    //   1: ABCD    2: C       3: D       4: CD
                                    //   5: B       6: BC      7: BD      8: BCD
                                    //   9: A      10: AC     11: AD     12: ACD
                                    //  13: AB     14: ABC    15: ABD    16: ABCD
                                    //
                                    // Lookup: mask → subtile index
                                    //   mask  1 (C)    → 2      mask  9 (AC)   → 10
                                    //   mask  2 (D)    → 3      mask 10 (AD)   → 11
                                    //   mask  3 (CD)   → 4      mask 11 (ACD)  → 12
                                    //   mask  4 (B)    → 5      mask 12 (AB)   → 13
                                    //   mask  5 (BC)   → 6      mask 13 (ABC)  → 14
                                    //   mask  6 (BD)   → 7      mask 14 (ABD)  → 15
                                    //   mask  7 (BCD)  → 8      mask 15 (ABCD) → 1
                                    //   mask  8 (A)    → 9
                                    //
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
            }

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
                ['cbWater', 'cbBoundary', 'cbBlight', 'cbRamp', 'cbWireframe', 'cbTextures', 'cbDeformation'].forEach(id => {
                    const el = document.getElementById(id)
                    if (el) checks[id] = el.checked
                })
                st.terrainChecks = checks
                vscode.setState(st)
            }

            ['cbWater', 'cbBoundary', 'cbBlight', 'cbRamp', 'cbWireframe', 'cbTextures', 'cbDeformation'].forEach(id => {
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

            // ── Point marker (cube at hovered grid vertex) ───────
            const MARKER_SIZE = TILE * 0.2
            const markerGeo = new THREE.BoxGeometry(MARKER_SIZE, MARKER_SIZE, MARKER_SIZE)
            const markerEdges = new THREE.EdgesGeometry(markerGeo)
            const markerMesh = new THREE.LineSegments(markerEdges,
                new THREE.LineBasicMaterial({color: 0x00ff00, depthTest: false}))
            markerMesh.renderOrder = 999
            markerMesh.visible = false
            scene.add(markerMesh)

            canvas.addEventListener('mousemove', e => {
                const rect = canvas.getBoundingClientRect()
                mouseNdc.x = ((e.clientX - rect.left) / rect.width) * 2 - 1
                mouseNdc.y = -((e.clientY - rect.top) / rect.height) * 2 + 1
                raycaster.setFromCamera(mouseNdc, camera)
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
                    const fl = []
                    if (D.waterFlag[idx]) fl.push('water')
                    if (D.boundaryFlag[idx]) fl.push('boundary')
                    if (D.blightFlag[idx]) fl.push('blight')
                    if (D.rampFlag[idx]) fl.push('ramp')
                    infoEl.textContent = 'X: ' + gameX.toFixed(2) + '  Y: ' + gameY.toFixed(2) +
                        '  Z: ' + vz.toFixed(2) + '  Tex: ' + D.groundTexture[idx] +
                        '  Cliff: ' + D.cliffVariation[idx] + '/' + D.cliffTexture[idx] +
                        '  Layer: ' + D.layerHeight[idx] +
                        (fl.length ? ' [' + fl.join(', ') + ']' : '')
                    return
                }
                markerMesh.visible = false
                infoEl.textContent = ''
            })
            canvas.addEventListener('mouseleave', () => {
                markerMesh.visible = false
                document.getElementById('cursor-info').textContent = ''
            })
        }

        // ── Orbit controls ──────────────────────────────────────
        const ctrl = makeOrbitControls(camera, canvas, maxDim)
        ctrl.target.set(0, 0, 0)

        function resize() {
            const cw = window.innerWidth, ch = window.innerHeight
            renderer.setSize(cw, ch)
            camera.aspect = cw / ch
            camera.updateProjectionMatrix()
        }

        resize()
        window.addEventListener('resize', resize);

        (function animate() {
            requestAnimationFrame(animate)
            ctrl.update()
            renderer.render(scene, camera)
        })()

        function makeOrbitControls(cam, domEl, maxD) {
            const target = new THREE.Vector3()
            const sph = new THREE.Spherical()
            const sphDelta = new THREE.Spherical()
            const panOff = new THREE.Vector3()
            let zoomFactor = 1
            const ROTATE_SPEED = 0.005, PAN_SPEED = 1.0
            let rotating = false, panning = false, px = 0, py = 0

            domEl.addEventListener('pointerdown', e => {
                if (e.target.closest('float-window') || e.target.closest('.menubar')) return
                if (e.button === 0) rotating = true
                else if (e.button === 1 || e.button === 2) panning = true
                px = e.clientX
                py = e.clientY
                domEl.setPointerCapture(e.pointerId)
            })
            domEl.addEventListener('pointermove', e => {
                const dx = e.clientX - px, dy = e.clientY - py
                px = e.clientX
                py = e.clientY
                if (rotating) {
                    sphDelta.theta -= dx * ROTATE_SPEED
                    sphDelta.phi -= dy * ROTATE_SPEED
                }
                if (panning) {
                    const v = new THREE.Vector3()
                    const factor = cam.position.distanceTo(target) * Math.tan(cam.fov / 2 * Math.PI / 180) * 2 / domEl.clientHeight
                    v.setFromMatrixColumn(cam.matrix, 0)
                    panOff.addScaledVector(v, -dx * factor * PAN_SPEED)
                    v.setFromMatrixColumn(cam.matrix, 1)
                    panOff.addScaledVector(v, dy * factor * PAN_SPEED)
                }
            })
            domEl.addEventListener('pointerup', e => {
                rotating = false
                panning = false
                try {
                    domEl.releasePointerCapture(e.pointerId)
                } catch (_) {
                }
            })
            domEl.addEventListener('wheel', e => {
                if (e.target.closest('float-window')) return
                e.preventDefault()
                zoomFactor *= e.deltaY > 0 ? 1.1 : 0.9
            }, {passive: false})
            domEl.addEventListener('contextmenu', e => e.preventDefault())

            return {
                target,
                update() {
                    const off = cam.position.clone().sub(target)
                    sph.setFromVector3(off)
                    sph.theta += sphDelta.theta
                    sph.phi += sphDelta.phi
                    sph.phi = Math.max(0.01, Math.min(Math.PI - 0.01, sph.phi))
                    sph.radius *= zoomFactor
                    sph.radius = Math.max(1, Math.min(maxD * 5, sph.radius))
                    target.add(panOff)
                    off.setFromSpherical(sph)
                    cam.position.copy(target).add(off)
                    cam.lookAt(target)
                    sphDelta.set(0, 0, 0)
                    panOff.set(0, 0, 0)
                    zoomFactor = 1
                }
            }
        }
    } catch (e) {
        console.error('Three.js init error:', e)
    }
})()

