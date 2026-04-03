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

        scene.add(new THREE.AmbientLight(0xffffff, 0.4))
        const dirLight = new THREE.DirectionalLight(0xffffff, 0.8)
        dirLight.position.set(1, 2, 1.5).normalize()
        scene.add(dirLight)

        let maxDim = 10000
        let mesh = null

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

                        const bl = D.groundTexture[iBL]
                        const br = D.groundTexture[iBR]
                        const tl = D.groundTexture[iTL]
                        const tr = D.groundTexture[iTR]

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
                ['cbWater', 'cbBoundary', 'cbBlight', 'cbRamp', 'cbWireframe', 'cbTextures', 'cbDeformation', 'cbObjects'].forEach(id => {
                    const el = document.getElementById(id)
                    if (el) checks[id] = el.checked
                })
                st.terrainChecks = checks
                vscode.setState(st)
            }

            ['cbWater', 'cbBoundary', 'cbBlight', 'cbRamp', 'cbWireframe', 'cbTextures', 'cbDeformation', 'cbObjects'].forEach(id => {
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
                    const cf = D.flags[idx]
                    if (cf & 1) fl.push('water')
                    if (cf & 2) fl.push('boundary')
                    if (cf & 4) fl.push('blight')
                    if (cf & 8) fl.push('ramp')
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
                if (!objectGroup.visible) return
                var rect = canvas.getBoundingClientRect()
                var ndc = new THREE.Vector2(
                    ((e.clientX - rect.left) / rect.width) * 2 - 1,
                    -((e.clientY - rect.top) / rect.height) * 2 + 1
                )
                var rc = new THREE.Raycaster()
                rc.setFromCamera(ndc, camera)
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

            let _doodFileMap = DATA.doodadFileMap || {}
            let _destFileMap = DATA.destructableFileMap || {}
            let _unitFileMap = DATA.unitFileMap || {}
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
                return 'http://127.0.0.1:' + bs.port + '/mdx/texture?' + params
            }

            function _buildModel(data, replaceableTextures) {
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
                                if (tex && tex.replaceable_id && replaceableTextures && replaceableTextures[tex.replaceable_id]) {
                                    texPath = replaceableTextures[tex.replaceable_id]
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

                    entries.push({geometry: geo, material: new THREE.MeshPhongMaterial(matOpts)})
                }
                return entries
            }

            function _placeInstances(items, entries) {
                if (entries.length === 0 || items.length === 0) return
                const mat4 = new THREE.Matrix4()
                const pos = new THREE.Vector3()
                const quat = new THREE.Quaternion()
                const scl = new THREE.Vector3()
                const euler = new THREE.Euler()

                for (const entry of entries) {
                    const instMesh = new THREE.InstancedMesh(entry.geometry, entry.material, items.length)
                    instMesh.userData._items = items
                    for (let i = 0; i < items.length; i++) {
                        const it = items[i]
                        pos.set(
                            it.p[0] - D.offsetX - halfGridW,
                            it.p[1] - D.offsetY - halfGridH,
                            it.p[2]
                        )
                        euler.set(0, 0, it.a || 0)
                        quat.setFromEuler(euler)
                        scl.set(it.s[0] || 1, it.s[1] || 1, it.s[2] || 1)
                        mat4.compose(pos, quat, scl)
                        instMesh.setMatrixAt(i, mat4)
                    }
                    instMesh.instanceMatrix.needsUpdate = true
                    objectGroup.add(instMesh)
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

            const _rawModelData = {} // modelPath → raw msg data (shared across texture variants)

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

                // Place red cubes for items with no rawcode→file mapping
                if (_unmappedItems.length > 0) {
                    _placeInstances(_unmappedItems, _fallbackEntries)
                }

                // Place already-cached models; collect uncached for loading
                const toLoad = []
                for (const [cacheKey, info] of Object.entries(byCacheKey)) {
                    if (_modelCache[cacheKey]) {
                        _placeInstances(info.items, _modelCache[cacheKey])
                    } else if (_rawModelData[info.path]) {
                        // Model data already loaded but not yet built for this texture variant
                        const replTex = info.texId && info.texFile ? {[info.texId]: info.texFile} : null
                        const entries = _buildModel(_rawModelData[info.path], replTex)
                        _modelCache[cacheKey] = entries
                        _placeInstances(info.items, entries)
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
                        const replTex = info.texId && info.texFile ? {[info.texId]: info.texFile} : null
                        const entries = _buildModel(msg, replTex)
                        _modelCache[cacheKey] = entries
                        if (info.items && objectGroup.visible) {
                            _placeInstances(info.items, entries)
                        }
                        delete _pendingItems[cacheKey]
                    }
                } else if (msg && msg.command === 'mapObjectModelNotFound') {
                    // Model file could not be loaded — place red cubes
                    for (const [cacheKey, info] of Object.entries(_pendingItems)) {
                        if (info.path !== msg.path) continue
                        _modelCache[cacheKey] = _fallbackEntries
                        if (info.items && objectGroup.visible) {
                            _placeInstances(info.items, _fallbackEntries)
                        }
                        delete _pendingItems[cacheKey]
                    }
                } else if (msg && msg.command === 'mapObjectsLoaded') {
                    // After all loading finishes, place red cubes for any remaining pending items
                    for (const [cacheKey, info] of Object.entries(_pendingItems)) {
                        if (!_modelCache[cacheKey]) {
                            _modelCache[cacheKey] = _fallbackEntries
                            if (objectGroup.visible) {
                                _placeInstances(info.items, _fallbackEntries)
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

            // Initial load if SLK maps have data
            const _hasMaps = Object.keys(_doodFileMap).length > 0 || Object.keys(_destFileMap).length > 0 || Object.keys(_unitFileMap).length > 0
            if (_hasMaps && (_doodItems.length > 0 || _unitItems.length > 0)) {
                _collectAndLoad()
            }

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
                const hasData = Object.keys(_doodFileMap).length > 0 || Object.keys(_destFileMap).length > 0 || Object.keys(_unitFileMap).length > 0
                if (hasData && (_doodItems.length > 0 || _unitItems.length > 0)) {
                    _collectAndLoad()
                }
            })
        }

        // ── Orbit controls ──────────────────────────────────────
        const ctrl = W3E.makeOrbitControls(camera, canvas, maxDim)
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
    } catch (e) {
        console.error('Three.js init error:', e)
    }
})()

