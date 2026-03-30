const {esc, indexToRgb, TILESET_NAMES} = require('./utils.js')
const {renderHeaderContent, renderGamePathContent, renderFilesRows} = require('./panels.js')
const {editorStyles} = require('./styles.js')

/**
 * Build the full HTML page for the map editor webview.
 *
 * @param {Object|null} terrainData  — parsed w3e data, or null if unavailable
 * @param {string}      fname       — display file name
 * @param {string}      threeSrc    — webview URI to three.min.js
 * @param {string[]}    texUris     — webview URIs for texture-pack images
 * @param {Object}      mapInfo     — { mapName, binaries, currentFile, isArchive, isMap, archiveFiles, componentsSrc }
 */
function renderMapEditor(terrainData, fname, threeSrc, texUris, mapInfo) {
    const hasTerrain = !!terrainData

    let renderData = null
    let totalTiles = 0
    let totalCliffTiles = 0
    let tilesetName = ''
    let legendItems = ''
    let cliffLegendItems = ''
    let terrainSlkSource = ''
    let w = 0, h = 0

    // Build a tileID → SLK row lookup
    const slkMap = {}
    if (terrainData && terrainData._terrainSlk && terrainData._terrainSlk.tiles) {
        terrainSlkSource = terrainData._terrainSlk.source || ''
        for (const t of terrainData._terrainSlk.tiles) {
            slkMap[t.tileId] = t
        }
    }

    if (hasTerrain) {
        w = terrainData.map_width
        h = terrainData.map_height
        totalTiles = terrainData.ground_tiles ? terrainData.ground_tiles.length : 0
        totalCliffTiles = terrainData.cliff_tiles ? terrainData.cliff_tiles.length : 0
        tilesetName = TILESET_NAMES[terrainData.tileset] || terrainData.tileset

        if (terrainData.ground_tiles) {
            legendItems = terrainData.ground_tiles.map((code, i) => {
                const [r, g, b] = indexToRgb(i)
                const info = slkMap[code]
                const name = info ? info.comment : ''
                const tilePath = info && info.dir && info.file
                    ? info.dir + '\\' + info.file + (info.ext || '') : ''
                return `<tile-item index="${i}" code="${esc(code)}" tile-name="${esc(name)}" tile-path="${esc(tilePath)}" swatch-color="${r},${g},${b}"></tile-item>`
            }).join('\n')
        }

        if (terrainData.cliff_tiles) {
            cliffLegendItems = terrainData.cliff_tiles.map((code, i) => {
                return `<tile-item index="${i}" code="${esc(code)}"></tile-item>`
            }).join('\n')
        }

        renderData = {
            w, h, totalTiles,
            offsetX: terrainData.offset_x,
            offsetY: terrainData.offset_y,
            texUris: texUris || [],
            groundTexture: terrainData.points.map(p => p.ground_texture),
            groundHeight: terrainData.points.map(p => p.ground_height),
            waterFlag: terrainData.points.map(p => p.water ? 1 : 0),
            boundaryFlag: terrainData.points.map(p => p.boundary ? 1 : 0),
            blightFlag: terrainData.points.map(p => p.blight ? 1 : 0),
            rampFlag: terrainData.points.map(p => p.ramp ? 1 : 0),
            cliffVariation: terrainData.points.map(p => p.cliff_variation),
            cliffTexture: terrainData.points.map(p => p.cliff_texture),
            layerHeight: terrainData.points.map(p => p.layer_height),
        }
    }

    const headerContent = renderHeaderContent(mapInfo.archiveHeader)
    const gamePathContent = renderGamePathContent(mapInfo.gamePath, mapInfo.mpqStatus)
    const fileCount = mapInfo.archiveFiles ? mapInfo.archiveFiles.length : 0
    const filesRows = mapInfo.isArchive ? renderFilesRows(mapInfo.archiveFiles) : ''

    const nonce = mapInfo.nonce || ''
    const cspSource = mapInfo.cspSource || ''
    const componentsSrc = mapInfo.componentsSrc || ''

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${cspSource} data: blob:; script-src 'nonce-${nonce}'; style-src 'unsafe-inline'; font-src ${cspSource};" />
    <style>${editorStyles()}</style>
</head>
<body>
    <canvas id="terrain"></canvas>
    <div id="cursor-info" class="cursor-info"></div>

    <!-- ── Menu bar ───────────────────────────────────────────── -->
    <div class="menubar" id="menubar">
        <button class="menu-item" data-action="toggleWindow" data-target="gamePathWindow" title="Warcraft III installation path">\u2699 Game Path</button>
        <button class="menu-item${mapInfo.isArchive ? '' : ' disabled'}" ${mapInfo.isArchive ? 'data-action="toggleWindow" data-target="headerWindow"' : ''}
                title="${mapInfo.isArchive ? 'Archive header info' : 'Available only for archives (.w3x, .w3m, .w3n, .mpq)'}">\ud83d\udce6 Header</button>
        <button class="menu-item${hasTerrain ? '' : ' disabled'}" ${hasTerrain ? 'data-action="toggleWindow" data-target="terrainWindow"' : ''}
                title="${hasTerrain ? 'Terrain metadata' : 'No terrain data available'}">\ud83d\uddfa Terrain</button>
        <button class="menu-item${hasTerrain ? '' : ' disabled'}" ${hasTerrain ? 'data-action="toggleWindow" data-target="tilesetWindow"' : ''}
                title="${hasTerrain ? 'Tileset info' : 'No terrain data available'}">\ud83e\uddf1 Tileset</button>
        ${mapInfo.isArchive ? '<button class="menu-item" data-action="toggleWindow" data-target="filesWindow" title="Archive file list">\ud83d\udcc2 Files</button>' : ''}
    </div>

    <!-- ── Floating windows (Custom Elements) ─────────────────── -->

    <float-window id="gamePathWindow" title-text="\u2699 Game Path" hidden style="left:140px;top:16px;">
        <div id="gpBody">${gamePathContent}</div>
    </float-window>

    ${mapInfo.isArchive ? `
    <float-window id="headerWindow" title-text="\ud83d\udce6 Header \u2014 ${esc(fname)}" hidden style="left:140px;top:16px;">
        ${headerContent}
    </float-window>
    ` : ''}

    ${hasTerrain ? `
    <float-window id="terrainWindow" title-text="\ud83d\uddfa Terrain" hidden style="left:140px;top:16px;">
        <table class="info">
            <tr><td class="key">Magic</td><td><code>${esc(terrainData.magic)}</code></td></tr>
            <tr><td class="key">Version</td><td>${terrainData.version}</td></tr>
            <tr><td class="key">Tileset</td><td>${esc(terrainData.tileset)} \u2014 ${esc(tilesetName)}</td></tr>
            <tr><td class="key">Custom</td><td>${terrainData.custom_tileset ? 'Yes' : 'No'}</td></tr>
            <tr><td class="key">Size</td><td>${w} \u00d7 ${h} (${w * h} pts)</td></tr>
            <tr><td class="key">Offset</td><td>X: ${terrainData.offset_x.toFixed(2)}, Y: ${terrainData.offset_y.toFixed(2)}</td></tr>
        </table>
        <div class="tw-section-title">Layers</div>
        <div class="terrain-checks">
            <label class="menu-cb"><input type="checkbox" id="cbWater" /> Water</label>
            <label class="menu-cb"><input type="checkbox" id="cbBoundary" /> Boundary</label>
            <label class="menu-cb"><input type="checkbox" id="cbBlight" /> Blight</label>
            <label class="menu-cb"><input type="checkbox" id="cbRamp" /> Ramp</label>
            <label class="menu-cb"><input type="checkbox" id="cbWireframe" /> Wireframe</label>
            <label class="menu-cb"><input type="checkbox" id="cbTextures" /> Textures</label>
            <label class="menu-cb"><input type="checkbox" id="cbLayerHeight" /> Layer Height</label>
        </div>
    </float-window>
    ` : ''}

    ${hasTerrain ? `
    <float-window id="tilesetWindow" title-text="\ud83e\uddf1 Tileset" hidden style="left:140px;top:16px;">
        <div id="tsSlkSource" class="${terrainSlkSource ? 'ts-source' : 'ts-source ts-no-slk'}">${terrainSlkSource ? 'Terrain.slk: <span class="code">' + esc(terrainSlkSource) + '</span>' : 'Terrain.slk not found \u2014 set Game Path'}</div>
        <div class="tw-section-title">Ground Tiles (<span id="tsGroundCount">${totalTiles}</span>)</div>
        <div class="legend" id="tsGroundTiles">${legendItems}</div>
        <div id="tsCliffSection">${totalCliffTiles > 0 ? '<div class="tw-section-title">Cliff Tiles (' + totalCliffTiles + ')</div><div class="legend">' + cliffLegendItems + '</div>' : ''}</div>
    </float-window>
    ` : ''}

    ${mapInfo.isArchive ? `
    <float-window id="filesWindow" title-text="\ud83d\udcc2 Files (${fileCount})" no-padding hidden style="right:16px;top:16px;left:auto;">
        <button slot="actions" class="float-action" id="browseBtn" title="Mount as workspace folder">\ud83d\udcc1</button>
        <input type="text" id="fileFilter" placeholder="Filter files\u2026" class="file-filter" />
        <div class="files-list" id="filesList">${filesRows}</div>
    </float-window>
    ` : ''}

    <script nonce="${nonce}" src="${componentsSrc}"></script>
    <script nonce="${nonce}" src="${threeSrc}"></script>
    <script nonce="${nonce}">
    (function() {
        'use strict';

        const vscode = (typeof acquireVsCodeApi === 'function') ? acquireVsCodeApi() : null;

        W3E.init({
            vscode: vscode,
            groundTileCodes: ${hasTerrain && terrainData.ground_tiles ? JSON.stringify(terrainData.ground_tiles) : '[]'},
            cliffTileCodes: ${hasTerrain && terrainData.cliff_tiles ? JSON.stringify(terrainData.cliff_tiles) : '[]'},
            isArchive: ${!!mapInfo.isArchive}
        });

        // ── Three.js setup ──────────────────────────────────────
        try {
        const hasTerrain = ${hasTerrain};
        const canvas = document.getElementById('terrain');
        const renderer = new THREE.WebGLRenderer({canvas, antialias: true});
        renderer.setPixelRatio(window.devicePixelRatio);

        const scene = new THREE.Scene();
        scene.background = new THREE.Color(0x1e1e1e);

        const camera = new THREE.PerspectiveCamera(50, 1, 1, 100000);
        camera.position.set(0, -5000, 3500);
        camera.lookAt(0, 0, 0);

        scene.add(new THREE.AmbientLight(0xffffff, 0.4));
        const dirLight = new THREE.DirectionalLight(0xffffff, 0.8);
        dirLight.position.set(1, 2, 1.5).normalize();
        scene.add(dirLight);

        let maxDim = 10000;
        let mesh = null;

        if (hasTerrain) {
            const D = ${renderData ? JSON.stringify(renderData) : 'null'};
            const W = D.w, H = D.h;
            const TILE = 128;
            const H_ZERO = 8192;
            const H_SCALE = 4;

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

            const worldW = W * TILE;
            const worldH = H * TILE;
            maxDim = Math.max(worldW, worldH);

            camera.far = maxDim * 20;
            camera.position.set(0, -maxDim * 0.7, maxDim * 0.5);
            camera.lookAt(0, 0, 0);
            camera.updateProjectionMatrix();

            function cornerHeight(ci, cj) {
                let sum = 0, cnt = 0;
                for (let dx = -1; dx <= 0; dx++) {
                    for (let dy = -1; dy <= 0; dy++) {
                        const sx = ci + dx, sy = cj + dy;
                        if (sx >= 0 && sx < W && sy >= 0 && sy < H) {
                            if (showLayerHeight) {
                                sum += D.layerHeight[sy * W + sx] * TILE;
                            } else {
                                sum += D.groundHeight[sy * W + sx];
                            }
                            cnt++;
                        }
                    }
                }
                if (showLayerHeight) {
                    return cnt > 0 ? sum / cnt : 0;
                }
                return cnt > 0 ? (sum / cnt - H_ZERO) / H_SCALE : 0;
            }

            let showWater = false, showBoundary = false, showBlight = false, showRamp = false;
            let showLayerHeight = false;

            const geo = new THREE.PlaneGeometry(worldW, worldH, W, H);

            function applyHeights() {
                const pos = geo.attributes.position;
                for (let gj = 0; gj <= H; gj++) {
                    for (let gi = 0; gi <= W; gi++) {
                        const vi = gj * (W + 1) + gi;
                        pos.setZ(vi, cornerHeight(gi, H - gj));
                    }
                }
                pos.needsUpdate = true;
                geo.computeVertexNormals();
            }
            applyHeights();

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
                        if (showWater && D.waterFlag[idx]) { r *= 0.35; g *= 0.35; b = Math.min(1, b * 0.35 + 0.6); }
                        if (showBlight && D.blightFlag[idx]) { r = Math.min(1, r + 0.25); g *= 0.5; b *= 0.5; }
                        if (showRamp && D.rampFlag[idx]) { r = Math.min(1, r + 0.15); g = Math.min(1, g + 0.15); b *= 0.6; }
                        if (showBoundary && D.boundaryFlag[idx]) { r *= 0.3; g *= 0.3; b *= 0.3; }
                        const pi = idx * 4;
                        texData[pi] = Math.round(r * 255);
                        texData[pi + 1] = Math.round(g * 255);
                        texData[pi + 2] = Math.round(b * 255);
                        texData[pi + 3] = 255;
                    }
                }
                dataTex.needsUpdate = true;
            }
            applyColors();

            // ── Tile colour picker → update terrain ─────────────
            document.addEventListener('color-change', e => {
                const {index, color} = e.detail;
                if (index >= 0 && index < palette.length) {
                    palette[index] = color;
                    applyColors();
                }
            });

            const mat = new THREE.MeshLambertMaterial({map: dataTex, side: THREE.DoubleSide});
            mesh = new THREE.Mesh(geo, mat);
            scene.add(mesh);

            const TEX_URIS = D.texUris;
            let canvasTex = null;
            let useTextures = false;

            if (TEX_URIS.length > 0) {
                const tileTexMap = {};
                for (let i = 0; i < D.totalTiles; i++) tileTexMap[i] = TEX_URIS[(i * 7 + 3) % TEX_URIS.length];
                const needed = [...new Set(Object.values(tileTexMap))];
                const images = {};
                let loaded = 0;
                needed.forEach(url => {
                    const img = new Image();
                    img.crossOrigin = 'anonymous';
                    img.onload = () => { images[url] = img; if (++loaded === needed.length) buildCTex(); };
                    img.onerror = () => { if (++loaded === needed.length) buildCTex(); };
                    img.src = url;
                });
                function buildCTex() {
                    const CPX = 16;
                    const c2 = document.createElement('canvas');
                    c2.width = W * CPX; c2.height = H * CPX;
                    const ctx = c2.getContext('2d');
                    for (let sy = 0; sy < H; sy++) for (let sx = 0; sx < W; sx++) {
                        const img = images[tileTexMap[D.groundTexture[sy * W + sx]]];
                        if (img) ctx.drawImage(img, 0, 0, img.width, img.height, sx * CPX, (H-1-sy) * CPX, CPX, CPX);
                        else { ctx.fillStyle = '#888'; ctx.fillRect(sx * CPX, (H-1-sy) * CPX, CPX, CPX); }
                    }
                    canvasTex = new THREE.CanvasTexture(c2);
                    canvasTex.magFilter = THREE.NearestFilter;
                    canvasTex.minFilter = THREE.LinearFilter;
                    if (useTextures) mat.map = canvasTex;
                }
            }

            const BLOCK = 4;
            const gpos = geo.attributes.position;

            function buildWireArrays() {
                const fine = [], coarse = [];
                for (let j = 0; j <= H; j++) {
                    const arr = j % BLOCK === 0 ? coarse : fine;
                    for (let i = 0; i < W; i++) {
                        const a = j*(W+1)+i, b = a+1;
                        arr.push(gpos.getX(a),gpos.getY(a),gpos.getZ(a), gpos.getX(b),gpos.getY(b),gpos.getZ(b));
                    }
                }
                for (let i = 0; i <= W; i++) {
                    const arr = i % BLOCK === 0 ? coarse : fine;
                    for (let j = 0; j < H; j++) {
                        const a = j*(W+1)+i, b = (j+1)*(W+1)+i;
                        arr.push(gpos.getX(a),gpos.getY(a),gpos.getZ(a), gpos.getX(b),gpos.getY(b),gpos.getZ(b));
                    }
                }
                return {fine, coarse};
            }

            let wireData = buildWireArrays();
            const fineGeo = new THREE.BufferGeometry();
            fineGeo.setAttribute('position', new THREE.Float32BufferAttribute(wireData.fine, 3));
            const fineMesh = new THREE.LineSegments(fineGeo, new THREE.LineBasicMaterial({color:0xffffff, transparent:true, opacity:0.12}));
            fineMesh.visible = false;
            scene.add(fineMesh);

            const coarseGeo = new THREE.BufferGeometry();
            coarseGeo.setAttribute('position', new THREE.Float32BufferAttribute(wireData.coarse, 3));
            const coarseMesh = new THREE.LineSegments(coarseGeo, new THREE.LineBasicMaterial({color:0xffff00, transparent:true, opacity:0.5}));
            coarseMesh.visible = false;
            scene.add(coarseMesh);

            function rebuildWireframe() {
                wireData = buildWireArrays();
                fineGeo.setAttribute('position', new THREE.Float32BufferAttribute(wireData.fine, 3));
                coarseGeo.setAttribute('position', new THREE.Float32BufferAttribute(wireData.coarse, 3));
            }

            // ── Checkbox state persistence ──────────────────────
            const savedState = (vscode && vscode.getState()) || {};
            const cbState = savedState.terrainChecks || {};

            function saveCbState() {
                if (!vscode) return;
                const st = vscode.getState() || {};
                const checks = {};
                ['cbWater','cbBoundary','cbBlight','cbRamp','cbWireframe','cbTextures','cbLayerHeight'].forEach(id => {
                    const el = document.getElementById(id);
                    if (el) checks[id] = el.checked;
                });
                st.terrainChecks = checks;
                vscode.setState(st);
            }

            ['cbWater','cbBoundary','cbBlight','cbRamp','cbWireframe','cbTextures','cbLayerHeight'].forEach(id => {
                const el = document.getElementById(id);
                if (el && cbState[id] != null) el.checked = cbState[id];
            });

            const cbWaterEl = document.getElementById('cbWater');
            const cbBoundaryEl = document.getElementById('cbBoundary');
            const cbBlightEl = document.getElementById('cbBlight');
            const cbRampEl = document.getElementById('cbRamp');
            const cbWireframeEl = document.getElementById('cbWireframe');
            const cbTexturesEl = document.getElementById('cbTextures');
            const cbLayerHeightEl = document.getElementById('cbLayerHeight');

            if (cbWaterEl && cbWaterEl.checked) showWater = true;
            if (cbBoundaryEl && cbBoundaryEl.checked) showBoundary = true;
            if (cbBlightEl && cbBlightEl.checked) showBlight = true;
            if (cbRampEl && cbRampEl.checked) showRamp = true;
            if (cbWireframeEl && cbWireframeEl.checked) { fineMesh.visible = true; coarseMesh.visible = true; }
            if (cbTexturesEl && cbTexturesEl.checked) { useTextures = true; }
            if (cbLayerHeightEl && cbLayerHeightEl.checked) { showLayerHeight = true; applyHeights(); rebuildWireframe(); }
            applyColors();
            if (useTextures && canvasTex) { mat.map = canvasTex; mat.needsUpdate = true; }

            const cb = (id, fn) => { const el = document.getElementById(id); if (el) el.addEventListener('change', fn); };
            cb('cbWater', e => { showWater = e.target.checked; applyColors(); saveCbState(); });
            cb('cbBoundary', e => { showBoundary = e.target.checked; applyColors(); saveCbState(); });
            cb('cbBlight', e => { showBlight = e.target.checked; applyColors(); saveCbState(); });
            cb('cbRamp', e => { showRamp = e.target.checked; applyColors(); saveCbState(); });
            cb('cbWireframe', e => { fineMesh.visible = e.target.checked; coarseMesh.visible = e.target.checked; saveCbState(); });
            cb('cbTextures', e => {
                useTextures = e.target.checked;
                mat.map = (useTextures && canvasTex) ? canvasTex : dataTex;
                mat.needsUpdate = true;
                saveCbState();
            });
            cb('cbLayerHeight', e => {
                showLayerHeight = e.target.checked;
                applyHeights();
                rebuildWireframe();
                saveCbState();
            });

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
                    const gameX = D.offsetX + pt.x + halfGridW;
                    const gameY = D.offsetY + pt.y + halfGridH;
                    const sx = Math.max(0, Math.min(W-1, Math.floor((pt.x + worldW/2) / TILE)));
                    const sy = Math.max(0, Math.min(H-1, H-1-Math.floor((worldH/2 - pt.y) / TILE)));
                    const idx = sy * W + sx;
                    const fl = [];
                    if (D.waterFlag[idx]) fl.push('water');
                    if (D.boundaryFlag[idx]) fl.push('boundary');
                    if (D.blightFlag[idx]) fl.push('blight');
                    if (D.rampFlag[idx]) fl.push('ramp');
                    infoEl.textContent = 'X: ' + gameX.toFixed(2) + '  Y: ' + gameY.toFixed(2) +
                        '  Z: ' + pt.z.toFixed(2) + '  Tex: ' + D.groundTexture[idx] +
                        '  Cliff: ' + D.cliffVariation[idx] + '/' + D.cliffTexture[idx] +
                        '  Layer: ' + D.layerHeight[idx] +
                        (fl.length ? ' [' + fl.join(', ') + ']' : '');
                    return;
                }
                infoEl.textContent = '';
            });
            canvas.addEventListener('mouseleave', () => { document.getElementById('cursor-info').textContent = ''; });
        }

        // ── Orbit controls ──────────────────────────────────────
        const ctrl = makeOrbitControls(camera, canvas, maxDim);
        ctrl.target.set(0, 0, 0);

        function resize() {
            const cw = window.innerWidth, ch = window.innerHeight;
            renderer.setSize(cw, ch);
            camera.aspect = cw / ch;
            camera.updateProjectionMatrix();
        }
        resize();
        window.addEventListener('resize', resize);

        (function animate() {
            requestAnimationFrame(animate);
            ctrl.update();
            renderer.render(scene, camera);
        })();

        function makeOrbitControls(cam, domEl, maxD) {
            const target = new THREE.Vector3();
            const sph = new THREE.Spherical();
            const sphDelta = new THREE.Spherical();
            const panOff = new THREE.Vector3();
            let zoomFactor = 1;
            const ROTATE_SPEED = 0.005, PAN_SPEED = 1.0;
            let rotating = false, panning = false, px = 0, py = 0;

            domEl.addEventListener('pointerdown', e => {
                if (e.target.closest('float-window') || e.target.closest('.menubar')) return;
                if (e.button === 0) rotating = true;
                else if (e.button === 1 || e.button === 2) panning = true;
                px = e.clientX; py = e.clientY;
                domEl.setPointerCapture(e.pointerId);
            });
            domEl.addEventListener('pointermove', e => {
                const dx = e.clientX - px, dy = e.clientY - py;
                px = e.clientX; py = e.clientY;
                if (rotating) { sphDelta.theta -= dx * ROTATE_SPEED; sphDelta.phi -= dy * ROTATE_SPEED; }
                if (panning) {
                    const v = new THREE.Vector3();
                    const factor = cam.position.distanceTo(target) * Math.tan(cam.fov/2*Math.PI/180)*2/domEl.clientHeight;
                    v.setFromMatrixColumn(cam.matrix, 0); panOff.addScaledVector(v, -dx * factor * PAN_SPEED);
                    v.setFromMatrixColumn(cam.matrix, 1); panOff.addScaledVector(v, dy * factor * PAN_SPEED);
                }
            });
            domEl.addEventListener('pointerup', e => {
                rotating = false; panning = false;
                try { domEl.releasePointerCapture(e.pointerId); } catch(_) {}
            });
            domEl.addEventListener('wheel', e => {
                if (e.target.closest('float-window')) return;
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
                    sph.radius = Math.max(1, Math.min(maxD * 5, sph.radius));
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
        } catch(e) { console.error('Three.js init error:', e); }
    })();
    </script>
</body>
</html>`
}

module.exports = {renderMapEditor}

