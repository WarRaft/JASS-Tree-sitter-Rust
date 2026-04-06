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
        DOOD.rebuild(snapshot.doodadsSlk);
        DEST.rebuild(snapshot.destructablesSlk);
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

            let checkerOn = false;

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
                updateWrappers();
            });
            if (bgColorPicker) bgColorPicker.addEventListener('input', function () {
                updateWrappers();
            });

            return {
                load: function (msg) {
                    if (!body || !win) return;
                    if (empty) empty.style.display = 'none';
                    win.setAttribute('title-text', '\ud83d\uddbc ' + (msg.name || 'BLP'));

                    let html = '';
                    let mipmaps = msg.mipmaps || [];
                    for (let i = 0; i < mipmaps.length; i++) {
                        let mip = mipmaps[i];
                        html += '<div class="blp-mipmap">';
                        html += '<div class="blp-mip-meta"><span class="blp-mip-size">' + mip.width + ' \u00d7 ' + mip.height + '</span><span>#' + (i + 1) + '</span></div>';
                        if (mip.image_data_url) {
                            html += '<div class="blp-img-wrap' + (checkerOn ? ' checker' : '') + '"' + (!checkerOn && bgColorPicker ? ' style="background-color:' + bgColorPicker.value + '"' : '') + '>';
                            html += '<img src="' + mip.image_data_url + '" alt="' + mip.width + 'x' + mip.height + '" />';
                            html += '</div>';
                        } else {
                            html += '<div class="blp-no-image">No image</div>';
                        }
                        html += '</div>';
                    }
                    body.innerHTML = html;
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
                    const ext = (name.split('.').pop() || '').toLowerCase();
                    if (ext === 'mdx' || ext === 'mdl') {
                        vscode.postMessage({command: 'openModel', path: name});
                        return;
                    }
                    if (ext === 'blp') {
                        vscode.postMessage({command: 'openBlp', path: name});
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
                filterInput.addEventListener('input', e => {
                    const q = e.target.value.toLowerCase();
                    const filesList = document.getElementById('filesList');
                    if (!q) {
                        filesList.querySelectorAll('.file-row').forEach(r => r.style.display = '');
                        filesList.querySelectorAll('.folder-row').forEach(r => { r.style.display = ''; r.classList.remove('collapsed'); });
                        filesList.querySelectorAll('.folder-children').forEach(r => { r.style.display = ''; r.classList.remove('collapsed'); });
                        return;
                    }
                    filesList.querySelectorAll('.file-row').forEach(r => {
                        r.style.display = (r.dataset.name || '').toLowerCase().includes(q) ? '' : 'none';
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

