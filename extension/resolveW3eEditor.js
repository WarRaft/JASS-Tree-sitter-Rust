// noinspection NpmUsedModulesInstalled
const {Uri} = require('vscode')
const fs = require('fs')
const path = require('path')

/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('vscode-languageclient').LanguageClient} client
 * @param {import('vscode').Uri} extensionUri
 */
async function resolveW3eEditor(document, webviewPanel, _token, client, extensionUri) {
    /** @type {Object} */
    const result = await client.sendRequest('w3e/render', {
        uri: document.uri.toString()
    })

    if (result.error) {
        webviewPanel.webview.html = errorHtml(result.error.message)
        return
    }

    const fname = document.uri.path.split('/').pop() || 'w3e'

    const threeUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'vendor', 'three.min.js')
    )

    // Collect .jpg textures from TexturePack
    const texDir = path.join(extensionUri.fsPath, 'extension', 'assets', 'TexturePack')
    let texUris = []
    try {
        texUris = fs.readdirSync(texDir)
            .filter(f => f.toLowerCase().endsWith('.jpg'))
            .map(f => webviewPanel.webview.asWebviewUri(
                Uri.joinPath(extensionUri, 'extension', 'assets', 'TexturePack', f)
            ).toString())
    } catch (_) { /* pack missing — colours only */ }

    webviewPanel.webview.html = renderW3e(result, fname, threeUri.toString(), texUris)
}

function errorHtml(msg) {
    return `<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"/></head>
<body style="background:var(--vscode-editor-background);color:var(--vscode-errorForeground);font-family:var(--vscode-font-family);padding:2rem;">
<h2>⚠ Error</h2><pre>${esc(msg)}</pre>
</body></html>`
}

function esc(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

/**
 * Generate a visually distinct colour for a given index.
 * Uses golden-angle spacing in HSL to maximise contrast between adjacent indices.
 * Returns an [r, g, b] tuple (0-255).
 */
function indexToRgb(index) {
    const golden = 137.508
    const hue = (index * golden) % 360
    const sat = 0.55 + 0.15 * ((index % 3) / 2)
    const lum = 0.45 + 0.10 * ((index % 5) / 4)

    const c = (1 - Math.abs(2 * lum - 1)) * sat
    const x = c * (1 - Math.abs(((hue / 60) % 2) - 1))
    const m = lum - c / 2
    let r, g, b
    if (hue < 60) { r = c; g = x; b = 0 }
    else if (hue < 120) { r = x; g = c; b = 0 }
    else if (hue < 180) { r = 0; g = c; b = x }
    else if (hue < 240) { r = 0; g = x; b = c }
    else if (hue < 300) { r = x; g = 0; b = c }
    else { r = c; g = 0; b = x }
    return [
        Math.round((r + m) * 255),
        Math.round((g + m) * 255),
        Math.round((b + m) * 255),
    ]
}

function renderMeta(meta) {
    if (!meta) return ''
    if (meta.remaining === 0) {
        return `<div class="meta-banner ok">✓ All ${meta.total} bytes read</div>`
    }
    return `<div class="meta-banner warn">⚠ ${meta.remaining} of ${meta.total} bytes not read (parser stopped at 0x${meta.read.toString(16).toUpperCase()})</div>`
}

const TILESET_NAMES = {
    A: 'Ashenvale', B: 'Barrens', K: 'Black Citadel', Y: 'Cityscape',
    X: 'Dalaran', J: 'Dalaran Ruins', D: 'Dungeon', C: 'Felwood',
    I: 'Icecrown Glacier', F: 'Lordaeron Fall', L: 'Lordaeron Summer',
    W: 'Lordaeron Winter', N: 'Northrend', O: 'Outland',
    Z: 'Sunken Ruins', G: 'Underground', V: 'Village', Q: 'Village Fall',
}

function renderW3e(data, fname, threeSrc, texUris) {
    const w = data.map_width
    const h = data.map_height
    const totalTiles = data.ground_tiles ? data.ground_tiles.length : 0

    const tilesetName = TILESET_NAMES[data.tileset] || data.tileset
    const headerRows = [
        ['Magic', esc(data.magic)],
        ['Version', data.version],
        ['Tileset', `${esc(data.tileset)} — ${esc(tilesetName)}`],
        ['Custom Tileset', data.custom_tileset ? 'Yes' : 'No'],
        ['Map Size', `${w} × ${h} (${w * h} points)`],
        ['Offset', `X: ${data.offset_x.toFixed(2)}, Y: ${data.offset_y.toFixed(2)}`],
    ]

    const headerHtml = headerRows.map(([k, v]) =>
        `<tr><td class="key">${k}</td><td>${v}</td></tr>`
    ).join('')

    const metaHtml = renderMeta(data._meta)

    // Ground tiles legend
    let legendItems = ''
    if (data.ground_tiles) {
        legendItems = data.ground_tiles.map((code, i) => {
            const [r, g, b] = indexToRgb(i)
            return `<span class="legend-item">
                <span class="legend-swatch" style="background:rgb(${r},${g},${b})"></span>
                <span class="code">${i}: ${esc(code)}</span>
            </span>`
        }).join('')
    }

    // Cliff tiles legend
    let cliffLegendItems = ''
    if (data.cliff_tiles && data.cliff_tiles.length > 0) {
        cliffLegendItems = data.cliff_tiles.map((code, i) =>
            `<span class="legend-item"><span class="code">${i}: ${esc(code)}</span></span>`
        ).join('')
    }

    // Minimal data for the 3D renderer
    const renderData = {
        w, h, totalTiles,
        offsetX: data.offset_x,
        offsetY: data.offset_y,
        texUris: texUris || [],
        groundTexture: data.points.map(p => p.ground_texture),
        groundHeight: data.points.map(p => p.ground_height),
        waterFlag: data.points.map(p => p.water ? 1 : 0),
        boundaryFlag: data.points.map(p => p.boundary ? 1 : 0),
        blightFlag: data.points.map(p => p.blight ? 1 : 0),
        rampFlag: data.points.map(p => p.ramp ? 1 : 0),
    }

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <style>${sharedStyles()}</style>
</head>
<body>
    <h1>🗺 ${esc(fname)}</h1>
    ${metaHtml}
    <table class="info">${headerHtml}</table>

    <h2>🎨 Ground Tiles <span class="count">(${totalTiles})</span></h2>
    <div class="legend">${legendItems}</div>

    ${cliffLegendItems ? `<h2>🏔 Cliff Tiles <span class="count">(${data.cliff_tiles.length})</span></h2>
    <div class="legend">${cliffLegendItems}</div>` : ''}

    <h2>🌍 Terrain Preview</h2>
    <div class="toolbar">
        <label><input type="checkbox" id="cbWater" /> Water</label>
        <label><input type="checkbox" id="cbBoundary" /> Boundary</label>
        <label><input type="checkbox" id="cbBlight" /> Blight</label>
        <label><input type="checkbox" id="cbRamp" /> Ramp</label>
        <label><input type="checkbox" id="cbWireframe" /> Wireframe</label>
        <label><input type="checkbox" id="cbTextures" /> Textures</label>
    </div>
    <div class="canvas-wrap" id="canvasWrap">
        <canvas id="terrain"></canvas>
    </div>
    <div id="cursor-info" class="cursor-info"></div>

    <script src="${threeSrc}"></script>
    <script>
    (function() {
        'use strict';
        const D = ${JSON.stringify(renderData)};
        const W = D.w, H = D.h;
        const TILE = 128;           // distance between grid points
        const H_ZERO = 8192;        // zero height level
        const H_SCALE = 4;          // raw height units per game unit

        // ── Colour palette (0..1 floats) ────────────────────────────
        function indexToColor(index) {
            const golden = 137.508;
            const hue = (index * golden) % 360;
            const sat = 0.55 + 0.15 * ((index % 3) / 2);
            const lum = 0.45 + 0.10 * ((index % 5) / 4);
            const c = (1 - Math.abs(2 * lum - 1)) * sat;
            const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
            const m = lum - c / 2;
            let r, g, b;
            if (hue < 60)       { r = c; g = x; b = 0; }
            else if (hue < 120) { r = x; g = c; b = 0; }
            else if (hue < 180) { r = 0; g = c; b = x; }
            else if (hue < 240) { r = 0; g = x; b = c; }
            else if (hue < 300) { r = x; g = 0; b = c; }
            else                { r = c; g = 0; b = x; }
            return [r + m, g + m, b + m];
        }

        const palette = [];
        for (let i = 0; i < D.totalTiles; i++) palette.push(indexToColor(i));

        // ── Three.js setup ──────────────────────────────────────────
        const canvas = document.getElementById('terrain');
        const wrap = document.getElementById('canvasWrap');

        const renderer = new THREE.WebGLRenderer({canvas, antialias: true});
        renderer.setPixelRatio(window.devicePixelRatio);

        const scene = new THREE.Scene();
        scene.background = new THREE.Color(0x1e1e1e);

        // World dimensions — cells extend ½ tile beyond outermost grid centres
        const worldW = W * TILE;
        const worldH = H * TILE;
        const maxDim = Math.max(worldW, worldH);

        const camera = new THREE.PerspectiveCamera(50, 1, 1, maxDim * 20);

        // ── Lighting ────────────────────────────────────────────────
        scene.add(new THREE.AmbientLight(0xffffff, 0.4));
        const dirLight = new THREE.DirectionalLight(0xffffff, 0.8);
        dirLight.position.set(1, 2, 1.5).normalize();
        scene.add(dirLight);

        // ── Corner height helper ────────────────────────────────────
        // Corner (ci, cj) sits at the junction of up to 4 cells.
        // ci = 0..W, cj = 0..H  (cj = 0 is bottom in w3e space).
        function cornerHeight(ci, cj) {
            let sum = 0, cnt = 0;
            for (let dx = -1; dx <= 0; dx++) {
                for (let dy = -1; dy <= 0; dy++) {
                    const sx = ci + dx, sy = cj + dy;
                    if (sx >= 0 && sx < W && sy >= 0 && sy < H) {
                        sum += D.groundHeight[sy * W + sx];
                        cnt++;
                    }
                }
            }
            return cnt > 0 ? (sum / cnt - H_ZERO) / H_SCALE : 0;
        }

        // ── Build terrain geometry ──────────────────────────────────
        // PlaneGeometry with W×H segments → (W+1)×(H+1) corner vertices,
        // W×H quads.  Each quad = one w3e cell; grid points are face centres.
        let showWater = false, showBoundary = false, showBlight = false, showRamp = false;

        const geo = new THREE.PlaneGeometry(worldW, worldH, W, H);

        // Set Z at cell corners (averaged from neighbouring grid points)
        (function applyHeights() {
            const pos = geo.attributes.position;
            for (let gj = 0; gj <= H; gj++) {
                for (let gi = 0; gi <= W; gi++) {
                    const vi = gj * (W + 1) + gi;
                    pos.setZ(vi, cornerHeight(gi, H - gj));
                }
            }
            pos.needsUpdate = true;
            geo.computeVertexNormals();
        })();

        // ── DataTexture for per-cell flat colouring ─────────────────
        const texData = new Uint8Array(W * H * 4);
        const dataTex = new THREE.DataTexture(texData, W, H);
        dataTex.format = THREE.RGBAFormat;
        dataTex.magFilter = THREE.NearestFilter;
        dataTex.minFilter = THREE.NearestFilter;

        function applyColors() {
            for (let sy = 0; sy < H; sy++) {
                for (let sx = 0; sx < W; sx++) {
                    const idx = sy * W + sx;
                    const ti = D.groundTexture[idx];
                    const col = palette[ti] || [0.5, 0.5, 0.5];
                    let r = col[0], g = col[1], b = col[2];

                    if (showWater && D.waterFlag[idx]) {
                        r *= 0.35; g *= 0.35; b = Math.min(1, b * 0.35 + 0.6);
                    }
                    if (showBlight && D.blightFlag[idx]) {
                        r = Math.min(1, r + 0.25); g *= 0.5; b *= 0.5;
                    }
                    if (showRamp && D.rampFlag[idx]) {
                        r = Math.min(1, r + 0.15); g = Math.min(1, g + 0.15); b *= 0.6;
                    }
                    if (showBoundary && D.boundaryFlag[idx]) {
                        r *= 0.3; g *= 0.3; b *= 0.3;
                    }
                    const pi = idx * 4;
                    texData[pi]     = Math.round(r * 255);
                    texData[pi + 1] = Math.round(g * 255);
                    texData[pi + 2] = Math.round(b * 255);
                    texData[pi + 3] = 255;
                }
            }
            dataTex.needsUpdate = true;
        }
        applyColors();

        const mat = new THREE.MeshLambertMaterial({map: dataTex, side: THREE.DoubleSide});
        const mesh = new THREE.Mesh(geo, mat);
        scene.add(mesh);

        // ── Texture-pack mode ───────────────────────────────────────
        // Assign a random texture URI to each ground-tile index.
        const TEX_URIS = D.texUris;
        let canvasTex = null;       // THREE.CanvasTexture (built once images load)
        let useTextures = false;

        if (TEX_URIS.length > 0) {
            // deterministic shuffle based on tile count
            const tileTexMap = {};
            for (let i = 0; i < D.totalTiles; i++) {
                tileTexMap[i] = TEX_URIS[(i * 7 + 3) % TEX_URIS.length];
            }
            // unique URLs we actually need
            const needed = [...new Set(Object.values(tileTexMap))];
            const images = {};
            let loaded = 0;

            needed.forEach(url => {
                const img = new Image();
                img.crossOrigin = 'anonymous';
                img.onload = () => {
                    images[url] = img;
                    if (++loaded === needed.length) buildCanvasTexture(tileTexMap, images);
                };
                img.onerror = () => { if (++loaded === needed.length) buildCanvasTexture(tileTexMap, images); };
                img.src = url;
            });

            function buildCanvasTexture(map, imgs) {
                const CPX = 16;  // pixels per cell in the atlas
                const c = document.createElement('canvas');
                c.width  = W * CPX;
                c.height = H * CPX;
                const ctx = c.getContext('2d');
                for (let sy = 0; sy < H; sy++) {
                    for (let sx = 0; sx < W; sx++) {
                        const ti = D.groundTexture[sy * W + sx];
                        const img = imgs[map[ti]];
                        if (img) {
                            // tile the source image into the CPX×CPX cell
                            ctx.drawImage(img, 0, 0, img.width, img.height,
                                sx * CPX, (H - 1 - sy) * CPX, CPX, CPX);
                        } else {
                            ctx.fillStyle = '#888';
                            ctx.fillRect(sx * CPX, (H - 1 - sy) * CPX, CPX, CPX);
                        }
                    }
                }
                canvasTex = new THREE.CanvasTexture(c);
                canvasTex.magFilter = THREE.NearestFilter;
                canvasTex.minFilter = THREE.LinearFilter;
                canvasTex.needsUpdate = true;
                if (useTextures) mat.map = canvasTex;
            }
        }

        function switchMap(toTextures) {
            useTextures = toTextures;
            mat.map = (useTextures && canvasTex) ? canvasTex : dataTex;
            mat.needsUpdate = true;
        }

        // ── Quad wireframe ───────────────────────────────────────────
        // Fine grid (128) — white; coarse grid (512) — yellow.
        const BLOCK = 4;           // 512 / 128
        const fineArr = [], coarseArr = [];
        const gpos = geo.attributes.position;
        // horizontal lines (constant j, one per row of corners)
        for (let j = 0; j <= H; j++) {
            const arr = j % BLOCK === 0 ? coarseArr : fineArr;
            for (let i = 0; i < W; i++) {
                const a = j * (W + 1) + i, b = a + 1;
                arr.push(gpos.getX(a), gpos.getY(a), gpos.getZ(a),
                         gpos.getX(b), gpos.getY(b), gpos.getZ(b));
            }
        }
        // vertical lines (constant i, one per column of corners)
        for (let i = 0; i <= W; i++) {
            const arr = i % BLOCK === 0 ? coarseArr : fineArr;
            for (let j = 0; j < H; j++) {
                const a = j * (W + 1) + i, b = (j + 1) * (W + 1) + i;
                arr.push(gpos.getX(a), gpos.getY(a), gpos.getZ(a),
                         gpos.getX(b), gpos.getY(b), gpos.getZ(b));
            }
        }
        const fineGeo = new THREE.BufferGeometry();
        fineGeo.setAttribute('position', new THREE.Float32BufferAttribute(fineArr, 3));
        const fineMesh = new THREE.LineSegments(fineGeo,
            new THREE.LineBasicMaterial({color: 0xffffff, transparent: true, opacity: 0.12}));
        fineMesh.visible = false;
        scene.add(fineMesh);

        const coarseGeo = new THREE.BufferGeometry();
        coarseGeo.setAttribute('position', new THREE.Float32BufferAttribute(coarseArr, 3));
        const coarseMesh = new THREE.LineSegments(coarseGeo,
            new THREE.LineBasicMaterial({color: 0xffff00, transparent: true, opacity: 0.5}));
        coarseMesh.visible = false;
        scene.add(coarseMesh);

        // ── Camera position ─────────────────────────────────────────
        camera.position.set(0, -maxDim * 0.7, maxDim * 0.5);
        camera.lookAt(0, 0, 0);

        // ── Orbit controls ──────────────────────────────────────────
        const ctrl = makeOrbitControls(camera, canvas);
        ctrl.target.set(0, 0, 0);

        // ── Resize handling ─────────────────────────────────────────
        function resize() {
            const cw = wrap.clientWidth;
            const ch = wrap.clientHeight || 500;
            renderer.setSize(cw, ch);
            camera.aspect = cw / ch;
            camera.updateProjectionMatrix();
        }
        resize();
        new ResizeObserver(resize).observe(wrap);

        // ── Animation loop ──────────────────────────────────────────
        function animate() {
            requestAnimationFrame(animate);
            ctrl.update();
            renderer.render(scene, camera);
        }
        animate();

        // ── UI controls ─────────────────────────────────────────────
        document.getElementById('cbWater').addEventListener('change', e => { showWater = e.target.checked; applyColors(); });
        document.getElementById('cbBoundary').addEventListener('change', e => { showBoundary = e.target.checked; applyColors(); });
        document.getElementById('cbBlight').addEventListener('change', e => { showBlight = e.target.checked; applyColors(); });
        document.getElementById('cbRamp').addEventListener('change', e => { showRamp = e.target.checked; applyColors(); });
        document.getElementById('cbWireframe').addEventListener('change', e => {
            fineMesh.visible = e.target.checked;
            coarseMesh.visible = e.target.checked;
        });
        document.getElementById('cbTextures').addEventListener('change', e => {
            switchMap(e.target.checked);
        });

        // ── Raycast for cursor info (smooth coordinates) ────────────
        const raycaster = new THREE.Raycaster();
        const mouseNdc = new THREE.Vector2();
        const infoEl = document.getElementById('cursor-info');
        const halfGridW = (W - 1) * TILE / 2;
        const halfGridH = (H - 1) * TILE / 2;

        canvas.addEventListener('mousemove', e => {
            const rect = canvas.getBoundingClientRect();
            mouseNdc.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
            mouseNdc.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
            raycaster.setFromCamera(mouseNdc, camera);
            const hits = raycaster.intersectObject(mesh);
            if (hits.length > 0) {
                const pt = hits[0].point;
                // Continuous game-world coordinates
                const gameX = D.offsetX + pt.x + halfGridW;
                const gameY = D.offsetY + pt.y + halfGridH;
                const gameZ = pt.z;
                // Identify which cell the cursor is over
                const sx = Math.max(0, Math.min(W - 1,
                    Math.floor((pt.x + worldW / 2) / TILE)));
                const sy = Math.max(0, Math.min(H - 1,
                    H - 1 - Math.floor((worldH / 2 - pt.y) / TILE)));
                const idx = sy * W + sx;
                const gt = D.groundTexture[idx];
                const fl = [];
                if (D.waterFlag[idx]) fl.push('water');
                if (D.boundaryFlag[idx]) fl.push('boundary');
                if (D.blightFlag[idx]) fl.push('blight');
                if (D.rampFlag[idx]) fl.push('ramp');
                const fs = fl.length ? ' [' + fl.join(', ') + ']' : '';
                infoEl.textContent =
                    'X: ' + gameX.toFixed(2) +
                    '  Y: ' + gameY.toFixed(2) +
                    '  Z: ' + gameZ.toFixed(2) +
                    '  Texture: ' + gt + fs;
                return;
            }
            infoEl.textContent = '';
        });
        canvas.addEventListener('mouseleave', () => { infoEl.textContent = ''; });

        // ─────────────────────────────────────────────────────────────
        // Minimal OrbitControls (rotate, pan, zoom)
        // LMB = rotate, RMB / MMB = pan, wheel = zoom
        // ─────────────────────────────────────────────────────────────
        function makeOrbitControls(cam, domEl) {
            const target = new THREE.Vector3();
            const sph = new THREE.Spherical();
            const sphDelta = new THREE.Spherical();
            const panOff = new THREE.Vector3();
            let zoomFactor = 1;

            const ROTATE_SPEED = 0.005;
            const PAN_SPEED = 1.0;

            let rotating = false, panning = false;
            let px = 0, py = 0;

            domEl.addEventListener('pointerdown', e => {
                if (e.button === 0) rotating = true;
                else if (e.button === 1 || e.button === 2) panning = true;
                px = e.clientX; py = e.clientY;
                domEl.setPointerCapture(e.pointerId);
            });
            domEl.addEventListener('pointermove', e => {
                const dx = e.clientX - px, dy = e.clientY - py;
                px = e.clientX; py = e.clientY;
                if (rotating) {
                    sphDelta.theta -= dx * ROTATE_SPEED;
                    sphDelta.phi -= dy * ROTATE_SPEED;
                }
                if (panning) {
                    const v = new THREE.Vector3();
                    const dist = cam.position.distanceTo(target);
                    const factor = dist * Math.tan(cam.fov / 2 * Math.PI / 180) * 2 / domEl.clientHeight;
                    v.setFromMatrixColumn(cam.matrix, 0);
                    panOff.addScaledVector(v, -dx * factor * PAN_SPEED);
                    v.setFromMatrixColumn(cam.matrix, 1);
                    panOff.addScaledVector(v, dy * factor * PAN_SPEED);
                }
            });
            domEl.addEventListener('pointerup', e => {
                rotating = false; panning = false;
                domEl.releasePointerCapture(e.pointerId);
            });
            domEl.addEventListener('wheel', e => {
                e.preventDefault();
                zoomFactor *= e.deltaY > 0 ? 1.1 : 0.9;
            }, {passive: false});
            domEl.addEventListener('contextmenu', e => e.preventDefault());

            return {
                target,
                update() {
                    const off = cam.position.clone().sub(target);
                    sph.setFromVector3(off);
                    sph.theta += sphDelta.theta;
                    sph.phi += sphDelta.phi;
                    sph.phi = Math.max(0.01, Math.min(Math.PI - 0.01, sph.phi));
                    sph.radius *= zoomFactor;
                    sph.radius = Math.max(1, Math.min(maxDim * 5, sph.radius));
                    target.add(panOff);
                    off.setFromSpherical(sph);
                    cam.position.copy(target).add(off);
                    cam.lookAt(target);
                    sphDelta.set(0, 0, 0);
                    panOff.set(0, 0, 0);
                    zoomFactor = 1;
                }
            };
        }
    })();
    </script>
</body>
</html>`
}

function sharedStyles() {
    return `
        * { box-sizing: border-box; }
        body {
            background: var(--vscode-editor-background);
            color: var(--vscode-editor-foreground);
            font-family: var(--vscode-font-family), sans-serif;
            font-size: 13px;
            margin: 0;
            padding: 1rem 1.5rem;
        }
        h1 { font-size: 1.3em; margin: 0 0 0.75rem; }
        h2 {
            font-size: 1.1em;
            margin: 1.5rem 0 0.5rem;
            border-bottom: 1px solid var(--vscode-editorWidget-border);
            padding-bottom: 0.25rem;
        }
        .count { color: var(--vscode-descriptionForeground); font-weight: normal; }

        table.info { border-collapse: collapse; margin-bottom: 1rem; }
        table.info td { padding: 0.15rem 0.75rem 0.15rem 0; }
        table.info .key { color: var(--vscode-descriptionForeground); white-space: nowrap; }

        .code {
            font-family: var(--vscode-editor-font-family), monospace;
            font-size: 12px;
            color: var(--vscode-textLink-foreground);
        }
        .meta-banner {
            display: inline-flex; align-items: center; gap: 0.5rem;
            padding: 0.3rem 0.75rem; border-radius: 4px; font-size: 12px;
            margin-bottom: 0.75rem; font-variant-numeric: tabular-nums;
        }
        .meta-banner.ok {
            background: rgba(78, 201, 176, 0.12);
            color: #4ec9b0;
            border: 1px solid rgba(78, 201, 176, 0.3);
        }
        .meta-banner.warn {
            background: rgba(224, 108, 64, 0.12);
            color: #e06c40;
            border: 1px solid rgba(224, 108, 64, 0.3);
        }

        .legend {
            display: flex; flex-wrap: wrap; gap: 0.5rem 1rem;
            margin-bottom: 0.5rem;
        }
        .legend-item { display: inline-flex; align-items: center; gap: 0.3rem; }
        .legend-swatch {
            display: inline-block; width: 14px; height: 14px;
            border-radius: 2px; border: 1px solid var(--vscode-editorWidget-border);
        }

        .toolbar {
            display: flex; flex-wrap: wrap; gap: 0.75rem 1.5rem;
            margin-bottom: 0.75rem; align-items: center;
        }
        .toolbar label {
            display: inline-flex; align-items: center; gap: 0.3rem;
            cursor: pointer; font-size: 12px;
        }

        .canvas-wrap {
            border: 1px solid var(--vscode-editorWidget-border);
            border-radius: 4px;
            width: 100%;
            height: 65vh;
            min-height: 400px;
        }
        #terrain {
            display: block;
            width: 100%;
            height: 100%;
        }

        .cursor-info {
            font-family: var(--vscode-editor-font-family), monospace;
            font-size: 12px;
            margin-top: 0.5rem;
            min-height: 1.2em;
            color: var(--vscode-descriptionForeground);
        }
    `
}

module.exports = {
    resolveW3eEditor
}
