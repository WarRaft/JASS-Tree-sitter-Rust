'use strict';

// ── W3E application entry point ─────────────────────────────────────
// Assembles sub-modules loaded via separate <script> tags into window.W3E.

window.W3E = (function () {
    let U = window._W3E_UTILS;
    let S = window._W3E_STATE;
    let TILESET = window._W3E_TILESET;
    let DOOD = window._W3E_DOODADS;
    let DEST = window._W3E_DESTRUCTABLES;
    let UNITS = window._W3E_UNITS;
    let PLACED = window._W3E_PLACED;
    let GP = window._W3E_GAME_PATH;

    // ── Status / snapshot listeners ──────────────────────────────
    let _statusListeners = [];
    let _snapshotListeners = [];
    let _groundTileCodes = [];
    let _cliffTileCodes = [];
    // Per-map texSource for cliff types (resolved with tileset MPQ).
    // Snapshot data is tileset-agnostic, so we preserve these.
    let _cliffTexSourceMap = {};

    function onStatusChanged(fn) { _statusListeners.push(fn); }
    function onSnapshotChanged(fn) { _snapshotListeners.push(fn); }

    function _applyGamePathChanged(status, snapshot) {
        for (let i = 0; i < _statusListeners.length; i++) {
            try { _statusListeners[i](status); } catch (_) {}
        }
        if (!snapshot) return;

        U.setWestrings(snapshot.westrings);
        TILESET.rebuildTileset(snapshot.terrainSlk, _groundTileCodes);
        // Merge per-map texSource into snapshot cliff types (snapshot is tileset-agnostic)
        if (snapshot.cliffTypesSlk && snapshot.cliffTypesSlk.cliffTypes) {
            for (const [id, ct] of Object.entries(snapshot.cliffTypesSlk.cliffTypes)) {
                if (_cliffTexSourceMap[id]) {
                    ct.texSource = _cliffTexSourceMap[id];
                }
            }
        }
        TILESET.rebuildCliffs(snapshot.cliffTypesSlk, _cliffTileCodes);

        // The snapshot now includes w3d/w3b merges when an archive path
        // is provided, so always rebuild from the snapshot.
        if (snapshot.doodadsSlk) {
            DOOD.rebuild(snapshot.doodadsSlk);
        }
        if (snapshot.destructablesSlk) {
            DEST.rebuild(snapshot.destructablesSlk);
        }
        UNITS.rebuild(snapshot.unitsSlk);
        PLACED.updatePlacedNames();

        for (let j = 0; j < _snapshotListeners.length; j++) {
            try { _snapshotListeners[j](snapshot); } catch (_) {}
        }
    }

    // ── Orbit controls — delegate to _W3E_ORBIT ────────────────
    let makeOrbitControls = window._W3E_ORBIT.makeOrbitControls;

    // ── Menu sync ────────────────────────────────────────────────
    function syncMenuActive() {
        document.querySelectorAll('[data-action="toggleWindow"]').forEach(btn => {
            const target = btn.getAttribute('data-target');
            if (!target) return;
            const win = document.getElementById(target);
            btn.classList.toggle('active', !!(win && win.open));
        });
    }

    // ── init() — main entry point ────────────────────────────────
    function init(config) {
        const vscode = config.vscode;
        S.setVscode(vscode);
        _groundTileCodes = config.groundTileCodes || [];
        _cliffTileCodes = config.cliffTileCodes || [];
        const isArchive = !!config.isArchive;

        if (config.doodadDooItems) PLACED.setDoodadDooItems(config.doodadDooItems);
        if (config.unitDooItems) PLACED.setUnitDooItems(config.unitDooItems);

        // ── Initial doodads SLK ──────────────────────────────────
        if (config.initialDoodadsSlk && config.initialDoodadsSlk.doodads) {
            DOOD.rebuild(config.initialDoodadsSlk);
        }

        // ── Initial destructables SLK ────────────────────────────
        if (config.initialDestructablesSlk && config.initialDestructablesSlk.destructables) {
            DEST.rebuild(config.initialDestructablesSlk);
        }

        // ── Initial units SLK ────────────────────────────────────
        if (config.initialUnitsSlk && config.initialUnitsSlk.units) {
            UNITS.rebuild(config.initialUnitsSlk);
        }

        // ── Initial cliff types SLK ─────────────────────────────
        if (config.initialCliffTypesSlk) {
            // Store per-map texSource (resolved with tileset MPQ)
            if (config.initialCliffTypesSlk.cliffTypes) {
                for (const [id, ct] of Object.entries(config.initialCliffTypesSlk.cliffTypes)) {
                    if (ct.texSource) _cliffTexSourceMap[id] = ct.texSource;
                }
            }
            TILESET.rebuildCliffs(config.initialCliffTypesSlk, _cliffTileCodes);
        }

        // ── Resolve placed object names from initial SLK data ────
        PLACED.updatePlacedNames();

        // ── Menu sync ────────────────────────────────────────────
        document.addEventListener('float-toggled', syncMenuActive);
        document.querySelectorAll('[data-action="toggleWindow"]').forEach(btn => {
            btn.addEventListener('click', () => {
                const target = btn.getAttribute('data-target');
                if (!target) return;
                const win = document.getElementById(target);
                if (win && win.toggle) win.toggle();
            });
        });
        syncMenuActive();

        // ── Open BLP from tile-item click ────────────────────────
        document.addEventListener('open-blp', function (e) {
            let p = e.detail && e.detail.path;
            if (p && vscode) vscode.postMessage({command: 'openBlp', path: p});
        });

        // ── Loading state ────────────────────────────────────────
        function setLoading(v) {
            document.querySelectorAll('reload-button').forEach(btn => {
                btn.loading = v;
                const win = btn.closest('float-window');
                if (win) win.loading = v;
            });
        }

        document.addEventListener('reload', () => {
            setLoading(true);
            if (vscode) vscode.postMessage({command: 'reloadGamePath'});
        });

        // ── Game Path ────────────────────────────────────────────
        function bindGpButtons() {
            const b = document.getElementById('gamePathBrowse');
            if (b && vscode) b.addEventListener('click', () => {
                vscode.postMessage({command: 'browseGamePath'});
            });
            const c = document.getElementById('gamePathClear');
            if (c && vscode) c.addEventListener('click', () => {
                vscode.postMessage({command: 'setGamePath', value: ''});
            });
        }

        bindGpButtons();

        onStatusChanged(function (status) {
            if (!status) return;
            let gpBody = document.getElementById('gpBody');
            if (!gpBody) return;
            gpBody.innerHTML = GP.renderBody(status);
            bindGpButtons();
        });

        // ── Canvas list lifecycle ────────────────────────────────
        document.addEventListener('float-toggled', function (evt) {
            let id = evt.detail && evt.detail.id;
            let win = id ? document.getElementById(id) : null;
            if (!win) return;
            if (id === 'doodadsSlkWindow') {
                if (win.open) { DOOD.ensureCanvasList(); DOOD.filterAndRender(false); }
                else DOOD.disposeCanvasList();
            } else if (id === 'destructablesSlkWindow') {
                if (win.open) { DEST.ensureCanvasList(); DEST.filterAndRender(false); }
                else DEST.disposeCanvasList();
            } else if (id === 'unitsSlkWindow') {
                if (win.open) { UNITS.ensureCanvasList(); UNITS.filterAndRender(false); }
                else UNITS.disposeCanvasList();
            } else if (id === 'unitDooWindow') {
                if (win.open) PLACED.ensureUnitDooCanvasList();
                else PLACED.disposeUnitDooCanvasList();
            } else if (id === 'doodadDooWindow') {
                if (win.open) PLACED.ensureDoodadDooCanvasList();
                else PLACED.disposeDoodadDooCanvasList();
            } else if (id === 'destructableDooWindow') {
                if (win.open) PLACED.ensureDestDooCanvasList();
                else PLACED.disposeDestDooCanvasList();
            }
        });

        // ── Create canvas lists for already-open windows ─────────
        let _unitDooWin = document.getElementById('unitDooWindow');
        if (_unitDooWin && _unitDooWin.open) PLACED.ensureUnitDooCanvasList();
        let _doodadDooWin = document.getElementById('doodadDooWindow');
        if (_doodadDooWin && _doodadDooWin.open) PLACED.ensureDoodadDooCanvasList();
        let _destDooWin = document.getElementById('destructableDooWindow');
        if (_destDooWin && _destDooWin.open) PLACED.ensureDestDooCanvasList();

        // ── Model viewer ─────────────────────────────────────────
        const _modelViewer = window._W3E_MODEL_VIEWER.init();

        // ── BLP viewer ───────────────────────────────────────────
        const _blpViewer = (function () {
            const win = document.getElementById('blpViewerWindow');
            const body = document.getElementById('blpMipmaps');
            const empty = document.getElementById('blpEmpty');
            const checkerToggle = document.getElementById('blpCheckerToggle');
            const bgColorPicker = document.getElementById('blpBgColor');

            // ── BLP viewer settings (localStorage) ──
            let checkerOn = localStorage.getItem('blpChecker') === '1';
            let savedBg = localStorage.getItem('blpBgColor');
            if (checkerToggle) checkerToggle.checked = checkerOn;
            if (bgColorPicker && savedBg) bgColorPicker.value = savedBg;

            function updateWrappers() {
                if (!body) return;
                body.querySelectorAll('.blp-img-wrap').forEach(function (w) {
                    w.classList.toggle('checker', checkerOn);
                    if (!checkerOn) w.style.backgroundColor = bgColorPicker ? bgColorPicker.value : '';
                    else w.style.backgroundColor = '';
                });
            }

            if (checkerToggle) checkerToggle.addEventListener('change', function () {
                checkerOn = checkerToggle.checked;
                localStorage.setItem('blpChecker', checkerOn ? '1' : '0');
                updateWrappers();
            });
            if (bgColorPicker) bgColorPicker.addEventListener('input', function () {
                localStorage.setItem('blpBgColor', bgColorPicker.value);
                updateWrappers();
            });

            // ── Alpha Test window (WebGL) ──
            const atWin = document.getElementById('blpAlphaTestWindow');
            const atWrap = document.getElementById('blpAtCanvasWrap');
            const atSlider = document.getElementById('blpAtSlider');
            const atValue = document.getElementById('blpAtValue');
            const atChecker = document.getElementById('blpAtChecker');
            const atBgColor = document.getElementById('blpAtBgColor');

            let atCheckerOn = localStorage.getItem('blpAtChecker') === '1';
            let atSavedBg = localStorage.getItem('blpAtBgColor');
            let atAlphaVal = parseFloat(localStorage.getItem('blpAlphaTest'));
            if (isNaN(atAlphaVal)) atAlphaVal = 0.75;

            if (atChecker) atChecker.checked = atCheckerOn;
            if (atBgColor && atSavedBg) atBgColor.value = atSavedBg;
            if (atSlider) atSlider.value = String(atAlphaVal);
            if (atValue) atValue.textContent = atAlphaVal.toFixed(2);

            let atGl = null;
            let atCanvas = null;
            let atProgram = null;
            let atAlphaLoc = null;
            let atTexture = null;

            const AT_VS = [
                'attribute vec2 a_pos;',
                'varying vec2 v_uv;',
                'void main(){',
                '  v_uv = a_pos * 0.5 + 0.5;',
                '  v_uv.y = 1.0 - v_uv.y;',
                '  gl_Position = vec4(a_pos, 0.0, 1.0);',
                '}'
            ].join('\n');

            const AT_FS = [
                'precision mediump float;',
                'varying vec2 v_uv;',
                'uniform sampler2D u_tex;',
                'uniform float u_alpha;',
                'void main(){',
                '  vec4 c = texture2D(u_tex, v_uv);',
                '  if(c.a <= u_alpha) discard;',
                '  gl_FragColor = c;',
                '}'
            ].join('\n');

            function atInitGL(canvas) {
                let gl = canvas.getContext('webgl', {alpha: true, premultipliedAlpha: false, preserveDrawingBuffer: true});
                if (!gl) return null;

                function compile(type, src) {
                    let s = gl.createShader(type);
                    gl.shaderSource(s, src);
                    gl.compileShader(s);
                    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
                        console.error('Shader compile error:', gl.getShaderInfoLog(s));
                    }
                    return s;
                }
                let vs = compile(gl.VERTEX_SHADER, AT_VS);
                let fs = compile(gl.FRAGMENT_SHADER, AT_FS);
                let prog = gl.createProgram();
                gl.attachShader(prog, vs);
                gl.attachShader(prog, fs);
                gl.linkProgram(prog);
                if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
                    console.error('Program link error:', gl.getProgramInfoLog(prog));
                }
                gl.useProgram(prog);

                // fullscreen quad: two triangles
                let buf = gl.createBuffer();
                gl.bindBuffer(gl.ARRAY_BUFFER, buf);
                gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1, 1,-1, -1,1, 1,1]), gl.STATIC_DRAW);
                let loc = gl.getAttribLocation(prog, 'a_pos');
                gl.enableVertexAttribArray(loc);
                gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);

                gl.enable(gl.BLEND);
                gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);

                atProgram = prog;
                atAlphaLoc = gl.getUniformLocation(prog, 'u_alpha');
                atGl = gl;
                return gl;
            }

            function atUploadTexture(gl, img) {
                if (atTexture) gl.deleteTexture(atTexture);
                let tex = gl.createTexture();
                gl.bindTexture(gl.TEXTURE_2D, tex);
                gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
                gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
                gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
                gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
                gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
                gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
                gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
                atTexture = tex;
            }

            function redrawAtCanvas() {
                if (!atGl || !atTexture) return;
                let gl = atGl;
                gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight);
                gl.clearColor(0, 0, 0, 0);
                gl.clear(gl.COLOR_BUFFER_BIT);
                gl.useProgram(atProgram);
                gl.activeTexture(gl.TEXTURE0);
                gl.bindTexture(gl.TEXTURE_2D, atTexture);
                gl.uniform1f(atAlphaLoc, atAlphaVal);
                gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
            }

            function updateAtWrapBg() {
                if (!atWrap) return;
                atWrap.classList.toggle('checker', atCheckerOn);
                if (!atCheckerOn) atWrap.style.backgroundColor = atBgColor ? atBgColor.value : '';
                else atWrap.style.backgroundColor = '';
            }
            updateAtWrapBg();

            if (atChecker) atChecker.addEventListener('change', function () {
                atCheckerOn = atChecker.checked;
                localStorage.setItem('blpAtChecker', atCheckerOn ? '1' : '0');
                updateAtWrapBg();
            });
            if (atBgColor) atBgColor.addEventListener('input', function () {
                localStorage.setItem('blpAtBgColor', atBgColor.value);
                updateAtWrapBg();
            });
            if (atSlider) atSlider.addEventListener('input', function () {
                atAlphaVal = parseFloat(atSlider.value);
                localStorage.setItem('blpAlphaTest', String(atAlphaVal));
                if (atValue) atValue.textContent = atAlphaVal.toFixed(2);
                redrawAtCanvas();
            });

            function openAlphaTest(dataUrl, w, h, label) {
                if (!atWin || !atWrap) return;
                // Create a fresh WebGL canvas
                let canvas = document.createElement('canvas');
                canvas.width = w;
                canvas.height = h;
                atWrap.innerHTML = '';
                atWrap.appendChild(canvas);
                atCanvas = canvas;

                let gl = atInitGL(canvas);
                if (!gl) return;

                let img = new Image();
                img.onload = function () {
                    atUploadTexture(gl, img);
                    redrawAtCanvas();
                };
                img.src = dataUrl;
                atWin.setAttribute('title-text', '\u03b1T ' + label);
                atWin.show();
            }

            return {
                load: function (msg) {
                    if (!body || !win) return;
                    if (empty) empty.style.display = 'none';
                    win.setAttribute('title-text', '\ud83d\uddbc ' + (msg.name || 'BLP'));

                    body.innerHTML = '';
                    let mipmaps = msg.mipmaps || [];

                    for (let i = 0; i < mipmaps.length; i++) {
                        let mip = mipmaps[i];

                        let div = document.createElement('div');
                        div.className = 'blp-mipmap';

                        // ── Meta bar ──
                        let meta = document.createElement('div');
                        meta.className = 'blp-mip-meta';

                        let sizeSpan = document.createElement('span');
                        sizeSpan.className = 'blp-mip-size';
                        sizeSpan.textContent = mip.width + ' \u00d7 ' + mip.height;
                        meta.appendChild(sizeSpan);

                        let actions = document.createElement('span');
                        actions.className = 'blp-mip-actions';

                        if (mip.image_data_url) {
                            let alphaBtn = document.createElement('button');
                            alphaBtn.className = 'blp-alpha-btn';
                            alphaBtn.textContent = '\u03b1T';
                            alphaBtn.title = 'Alpha Test';
                            (function (url, w, h, idx) {
                                alphaBtn.addEventListener('click', function () {
                                    openAlphaTest(url, w, h, w + '\u00d7' + h + ' #' + (idx + 1));
                                });
                            })(mip.image_data_url, mip.width, mip.height, i);
                            actions.appendChild(alphaBtn);
                        }

                        let indexSpan = document.createElement('span');
                        indexSpan.textContent = '#' + (i + 1);
                        actions.appendChild(indexSpan);

                        meta.appendChild(actions);
                        div.appendChild(meta);

                        if (mip.image_data_url) {
                            let wrap = document.createElement('div');
                            wrap.className = 'blp-img-wrap' + (checkerOn ? ' checker' : '');
                            if (!checkerOn && bgColorPicker) wrap.style.backgroundColor = bgColorPicker.value;
                            let img = document.createElement('img');
                            img.src = mip.image_data_url;
                            img.alt = mip.width + 'x' + mip.height;
                            wrap.appendChild(img);
                            div.appendChild(wrap);
                        } else {
                            let noImg = document.createElement('div');
                            noImg.className = 'blp-no-image';
                            noImg.textContent = 'No image';
                            div.appendChild(noImg);
                        }

                        body.appendChild(div);
                    }
                    win.show();
                }
            };
        })();

        // ── Message router ───────────────────────────────────────
        window.addEventListener('message', e => {
            const msg = e.data;
            if (msg && msg.command === 'gamePathChanged') {
                try { _applyGamePathChanged(msg.status, msg.snapshot); } catch (_) {}
                setLoading(false);
            }
            if (msg && msg.command === 'loadingDone') setLoading(false);
            if (msg && msg.command === 'loadingStart') setLoading(true);
            if (msg && msg.command === 'modelData') _modelViewer.load(msg);
            if (msg && msg.command === 'modelUnsupported') _modelViewer.showUnsupported(msg);
            if (msg && msg.command === 'blpData') _blpViewer.load(msg);
        });

        // ── Archive file interactions ────────────────────────────
        if (isArchive && vscode) {
            const ctxMenu = document.createElement('div');
            ctxMenu.className = 'ctx-menu';
            ctxMenu.hidden = true;
            document.body.appendChild(ctxMenu);

            let _ctxName = '';

            function hideCtx() { ctxMenu.hidden = true; }

            function showCtx(x, y, name) {
                _ctxName = name;
                ctxMenu.innerHTML =
                    '<div class="ctx-item" data-act="extractHere">\ud83d\udce4 Extract Here</div>' +
                    '<div class="ctx-item" data-act="extractTo">\ud83d\udcc2 Extract To\u2026</div>' +
                    '<div class="ctx-sep"></div>' +
                    '<div class="ctx-item" data-act="copyPath">\ud83d\udccb Copy Path</div>';

                ctxMenu.hidden = false;
                const rect = ctxMenu.getBoundingClientRect();
                const mx = Math.min(x, window.innerWidth - rect.width - 4);
                const my = Math.min(y, window.innerHeight - rect.height - 4);
                ctxMenu.style.left = Math.max(0, mx) + 'px';
                ctxMenu.style.top = Math.max(0, my) + 'px';

                ctxMenu.querySelectorAll('.ctx-item').forEach(function (item) {
                    item.addEventListener('click', function () {
                        const act = item.dataset.act;
                        if (act === 'copyPath') {
                            if (navigator.clipboard) navigator.clipboard.writeText(_ctxName);
                        } else {
                            vscode.postMessage({command: act, name: _ctxName});
                        }
                        hideCtx();
                    });
                });
            }

            document.addEventListener('click', function (e) {
                if (!ctxMenu.contains(e.target)) hideCtx();
            });
            document.addEventListener('keydown', function (e) {
                if (e.key === 'Escape') hideCtx();
            });
            document.addEventListener('scroll', hideCtx, true);

            document.querySelectorAll('.file-row').forEach(row => {
                row.addEventListener('contextmenu', function (e) {
                    e.preventDefault();
                    e.stopPropagation();
                    showCtx(e.clientX, e.clientY, row.dataset.name);
                });
                row.addEventListener('click', () => {
                    const name = row.dataset.name;
                    if (!name) return;
                    if (name.replace(/\\/g, '/').toLowerCase() === 'war3map.w3e') {
                        const tw = document.getElementById('terrainWindow');
                        if (tw) { tw.show(); return; }
                    }
                    if (name.replace(/\\/g, '/').toLowerCase() === 'war3map.w3i') {
                        const w = document.getElementById('w3iWindow');
                        if (w) { w.show(); return; }
                    }
                    if (name.replace(/\\/g, '/').toLowerCase() === 'war3map.doo') {
                        const w = document.getElementById('doodadDooWindow');
                        if (w) { w.show(); return; }
                    }
                    if (name.replace(/\\/g, '/').toLowerCase() === 'war3mapunits.doo') {
                        const w = document.getElementById('unitDooWindow');
                        if (w) { w.show(); return; }
                    }
                    if (name.replace(/\\/g, '/').toLowerCase() === 'war3map.w3r') {
                        const w = document.getElementById('regionsWindow');
                        if (w) { w.show(); return; }
                    }
                    const ext = (name.split('.').pop() || '').toLowerCase();
                    if (ext === 'mdx' || ext === 'mdl') {
                        vscode.postMessage({command: 'openModel', path: name});
                        return;
                    }
                    if (ext === 'blp') {
                        vscode.postMessage({command: 'openBlp', path: name});
                        return;
                    }
                    if (ext === 'slk') {
                        vscode.postMessage({command: 'openSlk', path: name});
                        return;
                    }
                    vscode.postMessage({command: 'openFile', name});
                });
            });

            const browseBtn = document.getElementById('browseBtn');
            if (browseBtn) {
                browseBtn.addEventListener('click', () => vscode.postMessage({command: 'browse'}));
            }
            const browseMpqBtn = document.getElementById('browseMpqBtn');
            if (browseMpqBtn) {
                browseMpqBtn.addEventListener('click', () => vscode.postMessage({command: 'browse'}));
            }

            document.querySelectorAll('.folder-row').forEach(row => {
                row.addEventListener('contextmenu', function (e) {
                    e.preventDefault();
                    e.stopPropagation();
                    const p = row.dataset.path;
                    if (p) showCtx(e.clientX, e.clientY, p);
                });
                row.addEventListener('click', () => {
                    row.classList.toggle('collapsed');
                    const children = row.nextElementSibling;
                    if (children && children.classList.contains('folder-children')) {
                        children.classList.toggle('collapsed');
                    }
                });
            });

            const filterInput = document.getElementById('fileFilter');
            if (filterInput) {
                function applyFileFilters() {
                    const q = filterInput.value.toLowerCase();
                    const filesList = document.getElementById('filesList');

                    // Determine which sources are enabled
                    const hiddenSources = new Set();
                    document.querySelectorAll('.file-source-cb').forEach(cb => {
                        if (!cb.checked) hiddenSources.add(cb.dataset.source);
                    });

                    if (!q && hiddenSources.size === 0) {
                        filesList.querySelectorAll('.file-row').forEach(r => r.style.display = '');
                        filesList.querySelectorAll('.folder-row').forEach(r => { r.style.display = ''; r.classList.remove('collapsed'); });
                        filesList.querySelectorAll('.folder-children').forEach(r => { r.style.display = ''; r.classList.remove('collapsed'); });
                        return;
                    }
                    filesList.querySelectorAll('.file-row').forEach(r => {
                        const source = r.dataset.source || 'listfile';
                        const matchesText = !q || (r.dataset.name || '').toLowerCase().includes(q);
                        const matchesSource = !hiddenSources.has(source);
                        r.style.display = matchesText && matchesSource ? '' : 'none';
                    });
                    const folders = filesList.querySelectorAll('.folder-children');
                    for (let i = folders.length - 1; i >= 0; i--) {
                        const fc = folders[i];
                        const hasVisible = fc.querySelector('.file-row:not([style*="display: none"]), .folder-row:not([style*="display: none"])');
                        fc.style.display = hasVisible ? '' : 'none';
                        fc.classList.remove('collapsed');
                        const fr = fc.previousElementSibling;
                        if (fr && fr.classList.contains('folder-row')) {
                            fr.style.display = hasVisible ? '' : 'none';
                            fr.classList.remove('collapsed');
                        }
                    }
                }

                filterInput.addEventListener('input', applyFileFilters);

                document.querySelectorAll('.file-source-cb').forEach(cb => {
                    cb.addEventListener('change', applyFileFilters);
                });
            }
        }
    }

    return {
        init,
        onStatusChanged,
        onSnapshotChanged,
        indexToRgb: U.indexToRgb,
        syncMenuActive,
        makeOrbitControls,
        highlightPlacedDoodad: function (idx) { PLACED.highlightPlacedDoodad(idx); },
    };
})();

