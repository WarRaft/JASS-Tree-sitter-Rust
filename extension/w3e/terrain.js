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
            const W = D.w, H = D.h
            const TILE = 128
            const H_ZERO = 8192
            const H_SCALE = 4

            function indexToColor(index) {
                const golden = 137.508
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

            const worldW = (W - 1) * TILE
            const worldH = (H - 1) * TILE
            maxDim = Math.max(worldW, worldH)

            camera.far = maxDim * 20
            camera.position.set(0, -maxDim * 0.7, maxDim * 0.5)
            camera.lookAt(0, 0, 0)
            camera.updateProjectionMatrix()

            let showWater = false, showBoundary = false, showBlight = false, showRamp = false
            let showDeformation = true

            const geo = new THREE.PlaneGeometry(worldW, worldH, W - 1, H - 1)

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

            const cellsX = W - 1
            const cellsY = H - 1
            const texData = new Uint8Array(cellsX * cellsY * 4)
            const dataTex = new THREE.DataTexture(texData, cellsX, cellsY)
            dataTex.format = THREE.RGBAFormat
            dataTex.magFilter = THREE.NearestFilter
            dataTex.minFilter = THREE.NearestFilter

            function applyColors() {
                for (let cy = 0; cy < cellsY; cy++) {
                    for (let cx = 0; cx < cellsX; cx++) {
                        const idx = cy * W + cx
                        const ti = D.groundTexture[idx]
                        const col = palette[ti] || [0.5, 0.5, 0.5]
                        let r = col[0], g = col[1], b = col[2]
                        if (showWater && D.waterFlag[idx]) {
                            r *= 0.35
                            g *= 0.35
                            b = Math.min(1, b * 0.35 + 0.6)
                        }
                        if (showBlight && D.blightFlag[idx]) {
                            r = Math.min(1, r + 0.25)
                            g *= 0.5
                            b *= 0.5
                        }
                        if (showRamp && D.rampFlag[idx]) {
                            r = Math.min(1, r + 0.15)
                            g = Math.min(1, g + 0.15)
                            b *= 0.6
                        }
                        if (showBoundary && D.boundaryFlag[idx]) {
                            r *= 0.3
                            g *= 0.3
                            b *= 0.3
                        }
                        const pi = (cy * cellsX + cx) * 4
                        texData[pi] = Math.round(r * 255)
                        texData[pi + 1] = Math.round(g * 255)
                        texData[pi + 2] = Math.round(b * 255)
                        texData[pi + 3] = 255
                    }
                }
                dataTex.needsUpdate = true
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

            const mat = new THREE.MeshLambertMaterial({map: dataTex, side: THREE.DoubleSide})
            mesh = new THREE.Mesh(geo, mat)
            scene.add(mesh)

            const TILE_TEXTURES = D.tileTextures || []
            let canvasTex = null
            let useTextures = true

            // ── Load tile texture images and build composited canvas ──
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

                    // Fill tile pools: sub-tile indices used for full coverage (mask=15)
                    // Square textures:      [1, 16]            — 2 variants
                    // Rectangular textures: [1, 16..32]        — 18 variants
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
                    // Cell (cx, cy) corners:
                    //   BL = point(cx,   cy  )
                    //   BR = point(cx+1, cy  )
                    //   TL = point(cx,   cy+1)
                    //   TR = point(cx+1, cy+1)

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
                                    // Transition sub-tile: mask → subtile in 4×4 texture
                                    // Bits: 0(1)=BR/C  1(2)=BL/D  2(4)=TR/B  3(8)=TL/A
                                    // Texture layout (corners A=TL B=TR C=BR D=BL):
                                    //  1: ABCD   2: C      3: D      4: CD
                                    //  5: B      6: BC     7: BD     8: BCD
                                    //  9: A     10: AC    11: AD    12: ACD
                                    // 13: AB    14: ABC   15: ABD   16: ABCD
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
                mat.map = (useTextures && canvasTex) ? canvasTex : dataTex
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

                    // Snap to nearest data point (grid vertex)
                    const sx = Math.max(0, Math.min(W - 1, Math.round((pt.x + worldW / 2) / TILE)))
                    const sy = Math.max(0, Math.min(H - 1, Math.round((pt.y + worldH / 2) / TILE)))
                    const idx = sy * W + sx

                    // Geometry vertex for this data point
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

