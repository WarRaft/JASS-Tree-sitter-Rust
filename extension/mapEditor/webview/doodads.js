'use strict';

// ── Doodads SLK: rebuild, filter, sort, detail ──────────────────────

window._W3E_DOODADS = (function () {
    let U = window._W3E_UTILS;
    let S = window._W3E_STATE;

    let _doodadDataMap = {};
    let _allDoodads = [];
    let _filteredDoodads = [];
    let _doodadsSlkLoaded = false;
    let _currentDoodId = null;
    let _detailHandlersAttached = false;

    /** @returns {boolean} true if original (from SLK, not in w3d) */
    function _isOriginal(d) { return !(d.baseId || d.w3dModified); }
    /** @returns {boolean} true if has actual field modifications in w3d */
    function _isModified(d) { return !!(d.defaults && Object.keys(d.defaults).length > 0); }

    // ── Canvas list instance ──────────────────────────────────────
    let _doodadCanvasList = null;

    function getDataMap() { return _doodadDataMap; }
    function getAllDoodads() { return _allDoodads; }
    function getFilteredDoodads() { return _filteredDoodads; }
    function isLoaded() { return _doodadsSlkLoaded; }

    // ── Sort state ────────────────────────────────────────────────
    let _doodSort = {field: null, dir: 'asc'};

    function _saveDoodSort() {
        S.patchWvState({_doodSort: {field: _doodSort.field, dir: _doodSort.dir}});
    }

    function _saveDoodFilters() {
        const uncheckedCats = [];
        document.querySelectorAll('.ds-cat-cb').forEach(cb => {
            if (!cb.checked) uncheckedCats.push(cb.getAttribute('data-cat'));
        });
        const uncheckedTs = [];
        document.querySelectorAll('.ds-ts-cb').forEach(cb => {
            if (!cb.checked) uncheckedTs.push(cb.getAttribute('data-ts'));
        });
        const uncheckedStatus = [];
        document.querySelectorAll('.ds-status-cb').forEach(cb => {
            if (!cb.checked) uncheckedStatus.push(cb.getAttribute('data-status'));
        });
        S.patchWvState({_doodUncheckedCats: uncheckedCats, _doodUncheckedTs: uncheckedTs, _doodUncheckedStatus: uncheckedStatus});
    }

    function restoreDoodFilters() {
        const s = S.getWvState();
        const uncheckedCats = s._doodUncheckedCats || [];
        const uncheckedTs = s._doodUncheckedTs || [];
        const uncheckedStatus = s._doodUncheckedStatus || [];
        if (uncheckedCats.length) {
            document.querySelectorAll('.ds-cat-cb').forEach(cb => {
                if (uncheckedCats.includes(cb.getAttribute('data-cat'))) cb.checked = false;
            });
        }
        if (uncheckedTs.length) {
            document.querySelectorAll('.ds-ts-cb').forEach(cb => {
                if (uncheckedTs.includes(cb.getAttribute('data-ts'))) cb.checked = false;
            });
        }
        if (uncheckedStatus.length) {
            document.querySelectorAll('.ds-status-cb').forEach(cb => {
                if (uncheckedStatus.includes(cb.getAttribute('data-status'))) cb.checked = false;
            });
        }
    }

    function restoreDoodSort() {
        const s = S.getWvState();
        if (s._doodSort && s._doodSort.field) {
            _doodSort = {field: s._doodSort.field, dir: s._doodSort.dir || 'asc'};
        }
    }

    function _cycleDoodSort(field) {
        if (_doodSort.field !== field) {
            _doodSort = {field, dir: 'asc'};
        } else if (_doodSort.dir === 'asc') {
            _doodSort.dir = 'desc';
        } else {
            _doodSort = {field: null, dir: 'asc'};
        }
        _saveDoodSort();
        updateSortButtons();
        filterAndRender();
    }

    function updateSortButtons() {
        document.querySelectorAll('.ds-sort-col').forEach(btn => {
            const f = btn.getAttribute('data-sort');
            btn.classList.remove('ds-sort-active', 'ds-sort-asc', 'ds-sort-desc');
            if (f === _doodSort.field) {
                btn.classList.add('ds-sort-active', _doodSort.dir === 'asc' ? 'ds-sort-asc' : 'ds-sort-desc');
            }
        });
    }

    function filterAndRender(saveState) {
        const enabledCats = new Set();
        document.querySelectorAll('.ds-cat-cb').forEach(cb => {
            if (cb.checked) enabledCats.add(cb.getAttribute('data-cat'));
        });
        const enabledTs = new Set();
        document.querySelectorAll('.ds-ts-cb').forEach(cb => {
            if (cb.checked) enabledTs.add(cb.getAttribute('data-ts'));
        });
        const enabledStatus = new Set();
        document.querySelectorAll('.ds-status-cb').forEach(cb => {
            if (cb.checked) enabledStatus.add(cb.getAttribute('data-status'));
        });
        if (saveState !== false) _saveDoodFilters();

        const searchEl = document.getElementById('dsSearchInput');
        const q = searchEl ? searchEl.value.toLowerCase().trim() : '';

        const filtered = _allDoodads.filter(d => {
            if (q) {
                const name = U.gsValue(d.name).toLowerCase();
                const id = (d.doodId || '').toLowerCase();
                const comment = (d.comment || '').toLowerCase();
                if (!name.includes(q) && !id.includes(q) && !comment.includes(q)) return false;
            }
            // Type filter: original / custom; flag: modified
            const original = _isOriginal(d);
            if (original && !enabledStatus.has('original')) return false;
            if (!original && !enabledStatus.has('custom')) return false;
            if (!original && _isModified(d) && !enabledStatus.has('modified')) return false;
            if (d.category && !enabledCats.has(d.category)) return false;
            if (d.tilesets) {
                if (d.tilesets === '*') return true;
                const chars = d.tilesets.replace(/,/g, '');
                if (chars.length > 0) {
                    let match = false;
                    for (const ch of chars) {
                        if (enabledTs.has(ch)) { match = true; break; }
                    }
                    if (!match) return false;
                }
            }
            return true;
        });

        if (_doodSort.field) {
            const f = _doodSort.field;
            const mul = _doodSort.dir === 'desc' ? -1 : 1;
            filtered.sort((a, b) => {
                const va = U.gsValue(a[f]).toLowerCase();
                const vb = U.gsValue(b[f]).toLowerCase();
                return va < vb ? -1 * mul : va > vb ? 1 * mul : 0;
            });
        }

        _filteredDoodads = filtered;
        if (_doodadCanvasList) {
            _doodadCanvasList.setData(filtered);
        }

        const cntEl = document.getElementById('dsDoodadCount');
        if (cntEl) cntEl.textContent = String(filtered.length);
    }

    function _rebuildSidebarCheckboxes() {
        const catSet = new Set();
        const tsSet = new Set();
        for (const d of _allDoodads) {
            if (d.category) catSet.add(d.category);
            if (d.tilesets) {
                for (const ch of d.tilesets) {
                    if (ch !== ',' && ch !== '*') tsSet.add(ch);
                }
            }
        }

        // Status checkboxes: type (Original / Custom) + flag (Modified)
        const statusCounts = {original: 0, custom: 0, modified: 0};
        for (const d of _allDoodads) {
            if (_isOriginal(d)) {
                statusCounts.original++;
            } else {
                statusCounts.custom++;
                if (_isModified(d)) statusCounts.modified++;
            }
        }
        const statusChecks = document.getElementById('dsStatusChecks');
        if (statusChecks) {
            statusChecks.innerHTML = '';
            const statuses = [
                ['original', '\u26aa', 'Original'],
                ['custom', '\ud83d\udd35', 'Custom'],
                ['modified', '\ud83d\udfe1', 'Modified'],
            ];
            for (const [code, icon, label] of statuses) {
                const lbl = document.createElement('label');
                lbl.className = 'menu-cb';
                const cb = document.createElement('input');
                cb.type = 'checkbox';
                cb.className = 'ds-status-cb';
                cb.setAttribute('data-status', code);
                cb.checked = true;
                cb.addEventListener('change', filterAndRender);
                lbl.appendChild(cb);
                lbl.appendChild(document.createTextNode(' ' + icon + ' ' + label + ' (' + statusCounts[code] + ')'));
                statusChecks.appendChild(lbl);
            }
        }

        const catChecks = document.getElementById('dsCatChecks');
        if (catChecks) {
            catChecks.innerHTML = '';
            for (const code of Array.from(catSet).sort()) {
                const label = DOODAD_CATEGORIES[code] || code;
                const lbl = document.createElement('label');
                lbl.className = 'menu-cb';
                const cb = document.createElement('input');
                cb.type = 'checkbox';
                cb.className = 'ds-cat-cb';
                cb.setAttribute('data-cat', code);
                cb.checked = true;
                cb.addEventListener('change', filterAndRender);
                lbl.appendChild(cb);
                const badge = document.createElement('span');
                badge.className = 'ds-ts-badge';
                badge.textContent = code;
                lbl.appendChild(badge);
                lbl.appendChild(document.createTextNode(' ' + label));
                catChecks.appendChild(lbl);
            }
        }

        const tsChecks = document.getElementById('dsTsChecks');
        if (tsChecks) {
            tsChecks.innerHTML = '';
            for (const code of Array.from(tsSet).sort()) {
                const label = TILESET_NAMES[code] || code;
                const lbl = document.createElement('label');
                lbl.className = 'menu-cb';
                const cb = document.createElement('input');
                cb.type = 'checkbox';
                cb.className = 'ds-ts-cb';
                cb.setAttribute('data-ts', code);
                cb.checked = true;
                cb.addEventListener('change', filterAndRender);
                lbl.appendChild(cb);
                const badge = document.createElement('span');
                badge.className = 'ds-ts-badge';
                badge.textContent = code;
                lbl.appendChild(badge);
                lbl.appendChild(document.createTextNode(' ' + label));
                tsChecks.appendChild(lbl);
            }
        }
        restoreDoodFilters();
    }

    function rebuild(slkData) {
        _doodadsSlkLoaded = true;
        let source = '';
        let w3dErrors = [];
        _allDoodads = [];
        _doodadDataMap = {};
        if (slkData && slkData.doodads) {
            source = slkData.source || '';
            w3dErrors = slkData.w3dErrors || [];
            _doodadDataMap = slkData.doodads;
            _allDoodads = Object.entries(slkData.doodads).map(function (e) { e[1]._rawKey = e[0]; return e[1]; });
        }


        const srcEl = document.getElementById('dsSlkSource');
        if (srcEl) {
            srcEl.clear();
            if (source) {
                const vscode = S.getVscode();
                const openSlk = function (path) {
                    if (vscode) vscode.postMessage({command: 'openSlk', path: path});
                };
                srcEl.addPath(source, function () { openSlk('Doodads\\Doodads.slk'); });
                srcEl.addPath(source.replace(/Doodads\.slk$/i, 'DoodadMetaData.slk'), function () { openSlk('Doodads\\DoodadMetaData.slk'); });

                if (w3dErrors.length > 0) {
                    srcEl.addError(
                        '\u26a0 ' + w3dErrors.length + ' error' + (w3dErrors.length > 1 ? 's' : '') + ' reading Doodads',
                        function () { _showW3dErrors(w3dErrors); }
                    );
                }
            } else {
                srcEl.setNotFound('Doodads.slk not found \u2014 set Game Path');
            }
        }

        const totalEl = document.getElementById('dsDoodadTotal');
        if (totalEl) totalEl.textContent = String(_allDoodads.length);

        _rebuildSidebarCheckboxes();
        restoreDoodSort();
        updateSortButtons();
        filterAndRender(false);

        const searchEl = document.getElementById('dsSearchInput');
        if (searchEl && !searchEl._dsBound) {
            searchEl._dsBound = true;
            searchEl.addEventListener('input', filterAndRender);
        }

        document.querySelectorAll('.ds-sort-col').forEach(btn => {
            if (btn._dsSortBound) return;
            btn._dsSortBound = true;
            btn.addEventListener('click', () => _cycleDoodSort(btn.getAttribute('data-sort')));
        });
    }

    // ── Detail window ─────────────────────────────────────────────
    const _DOOD_GROUPS = [
        {
            title: '🏷 Identity', fields: [
                ['doodID', 'doodId'], ['baseID', 'baseId'], ['Name', 'name'], ['comment', 'comment'],
                ['category', 'category'], ['doodClass', 'doodClass'],
                ['tilesets', 'tilesets'], ['tilesetSpecific', 'tilesetSpecific'],
            ]
        },
        {
            title: '🎨 Model', modelFiles: true, fields: [
                ['soundLoop', 'soundLoop'],
            ]
        },
        {
            title: '📐 Scale', fields: [
                ['defScale', 'defScale'], ['minScale', 'minScale'],
                ['maxScale', 'maxScale'], ['canPlaceRandScale', 'canPlaceRandScale'],
            ]
        },
        {
            title: '📍 Placement', fields: [
                ['onCliffs', 'onCliffs'], ['onWater', 'onWater'],
                ['floats', 'floats'], ['walkable', 'walkable'],
                ['fixedRot', 'fixedRot'], ['maxPitch', 'maxPitch'],
                ['maxRoll', 'maxRoll'], ['pathTex', 'pathTex'],
            ]
        },
        {
            title: '👆 Interaction', fields: [
                ['selSize', 'selSize'], ['useClickHelper', 'useClickHelper'],
                ['ignoreModelClick', 'ignoreModelClick'], ['visRadius', 'visRadius'],
            ]
        },
        {
            title: '👁 Rendering', fields: [
                ['shadow', 'shadow'], ['showInFog', 'showInFog'],
                ['animInFog', 'animInFog'],
            ]
        },
        {
            title: '🗺 Minimap', fields: [
                ['showInMM', 'showInMm'], ['useMMColor', 'useMmColor'],
            ],
            color: {key: 'mmColor', label: 'Color'},
        },
        { title: '🌈 Vertex Colors', vertexColors: true },
        {
            title: 'ℹ Meta', fields: [
                ['InBeta', 'inBeta'], ['version', 'version'],
            ]
        },
    ];

    function _getDoodCollapseState() {
        return S.getWvState()._doodCollapse || {};
    }

    function _setDoodCollapseState(state) {
        S.patchWvState({_doodCollapse: state});
    }

    if (U && typeof U.onModelVariantsResolved === 'function') {
        U.onModelVariantsResolved(function () {
            if (_currentDoodId && _doodadDataMap[_currentDoodId]) {
                showDetail(_currentDoodId);
            }
        });
    }


    function showDetail(doodId) {
        let vscode = S.getVscode();
        _currentDoodId = doodId;
        const d = _doodadDataMap[doodId];
        if (!d) {
            const win = document.getElementById('doodadDetailWindow');
            const body = document.getElementById('doodadDetailBody');
            if (win && body) {
                body.innerHTML = '<div style="padding:1rem;color:var(--vscode-errorForeground,#f44);">'
                    + '<b>' + U.esc(String(doodId)) + '</b> not found in Doodads.slk<br>'
                    + '<small style="opacity:0.7;">Loaded doodads: ' + Object.keys(_doodadDataMap).length + '</small>'
                    + '</div>';
                win.setAttribute('title-text', '\ud83c\udf33 ' + U.esc(String(doodId)));
                win.show();
            }
            return;
        }

        const win = document.getElementById('doodadDetailWindow');
        if (!win) return;
        const body = document.getElementById('doodadDetailBody');
        if (!body) return;

        let html = '';
        const collapseState = _getDoodCollapseState();

        for (const group of _DOOD_GROUPS) {
            let rows = '';

            if (group.vertexColors) {
                if (d.vertColors && d.vertColors.length > 0) {
                    for (let i = 0; i < d.vertColors.length; i++) {
                        const c = d.vertColors[i];
                        const idx = String(i + 1).padStart(2, '0');
                        rows += '<tr><td class="key">Variation ' + idx + '</td><td>'
                            + c.r + ',' + c.g + ',' + c.b + ' '
                            + U.colorBadge(c.r, c.g, c.b) + '</td></tr>';
                    }
                }
            } else if (group.modelFiles) {
                const filePath = d.file;
                const numVar = d.numVar || 1;
                if (filePath) {
                    rows += U.renderModelFileRow({filePath: filePath, numVar: numVar, defaults: d.defaults, vscode: vscode, includeTexAttrs: false});
                }
                let numVarRow = '<tr><td class="key">numVar</td><td>' + numVar + '</td>';
                if (d.defaults && d.defaults['numVar'] !== undefined) {
                    numVarRow += '<td class="dd-default">' + U.esc(d.defaults['numVar']) + '</td>';
                }
                rows += numVarRow + '</tr>';
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        const val = d[key];
                        if (val === undefined || val === '' || val === null) continue;
                        let row = '<tr><td class="key">' + U.esc(label) + '</td><td>' + U.esc(String(val)) + '</td>';
                        if (d.defaults && d.defaults[key] !== undefined) {
                            row += '<td class="dd-default">' + U.esc(d.defaults[key]) + '</td>';
                        }
                        rows += row + '</tr>';
                    }
                }
            } else {
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        const val = d[key];
                        if (val === undefined || val === '' || val === null) continue;
                        let display;
                        if (key === 'name') {
                            display = U.gsHtml(val);
                        } else if (key === 'pathTex') {
                            display = '<a href="#" class="dd-pathtex-link" data-pathtex="' + U.esc(String(val)) + '">' + U.esc(String(val)) + '</a>';
                        } else {
                            display = U.esc(String(val));
                        }
                        if (key === 'category' && val) {
                            display = U.categoryBadge(val, DOODAD_CATEGORIES);
                        }
                        if (key === 'tilesets' && val) {
                            display = U.tilesetBadges(val);
                        }
                        let row = '<tr><td class="key">' + U.esc(label) + '</td><td>' + display + '</td>';
                        if (d.defaults && d.defaults[key] !== undefined) {
                            row += '<td class="dd-default">' + U.esc(d.defaults[key]) + '</td>';
                        }
                        rows += row + '</tr>';
                    }
                }
                if (group.color) {
                    const c = d[group.color.key];
                    if (c) {
                        let row = '<tr><td class="key">' + U.esc(group.color.label) + '</td><td>'
                            + c.r + ',' + c.g + ',' + c.b + ' '
                            + U.colorBadge(c.r, c.g, c.b) + '</td>';
                        if (d.defaults && d.defaults[group.color.key] !== undefined) {
                            row += '<td class="dd-default">' + U.esc(d.defaults[group.color.key]) + '</td>';
                        }
                        rows += row + '</tr>';
                    }
                }
            }

            if (!rows) continue;

            const isOpen = collapseState.hasOwnProperty(group.title) ? collapseState[group.title] : true;
            html += '<collapse-group group-title="' + U.esc(group.title) + '"' + (isOpen ? ' open' : '') + '>'
                + '<table class="info">' + rows + '</table>'
                + '</collapse-group>';
        }

        body.innerHTML = html;
        win.setAttribute('title-text', '\ud83c\udf33 ' + (U.gsValue(d.name) || d.doodId));
        win.show();

        if (!_detailHandlersAttached) {
            body.addEventListener('collapse-toggle', function (e) {
                const state = _getDoodCollapseState();
                state[e.detail.title] = e.detail.open;
                _setDoodCollapseState(state);
            });

            body.addEventListener('click', function (e) {
                let link = e.target.closest('.dd-model-link');
                if (link) {
                    e.preventDefault();
                    const curVscode = S.getVscode();
                    if (curVscode) curVscode.postMessage({command: 'openModel', path: link.getAttribute('data-path')});
                    return;
                }
                let ptLink = e.target.closest('.dd-pathtex-link');
                if (ptLink) {
                    e.preventDefault();
                    window._W3E_PATH_TEX.showPathTex(ptLink.getAttribute('data-pathtex'));
                }
            });

            _detailHandlersAttached = true;
        }
    }

    // ── W3D errors window ──────────────────────────────────────
    function _showW3dErrors(errors) {
        const win = document.getElementById('doodadErrorsWindow');
        const body = document.getElementById('doodadErrorsBody');
        if (!win || !body) return;
        let html = '<div style="margin-bottom:8px;font-weight:bold;">\u26a0 ' + errors.length + ' error' + (errors.length > 1 ? 's' : '') + ' reading Doodads from war3map.w3d:</div>';
        html += '<div style="font-family:var(--vscode-editor-font-family,monospace);font-size:11px;">';
        for (let i = 0; i < errors.length; i++) {
            html += '<div style="padding:2px 0;border-bottom:1px solid var(--vscode-widget-border,rgba(128,128,128,0.2));">'
                + U.esc(errors[i]) + '</div>';
        }
        html += '</div>';
        body.innerHTML = html;
        win.setAttribute('title-text', '\u26a0 Doodad Errors (' + errors.length + ')');
        win.show();
    }

    // ── Canvas list row renderer ──────────────────────────────────
    function renderRow(ctx, d, x, y, w, h, c) {
        // Highlight custom entries: modified = yellow, unmodified = blue
        if (!_isOriginal(d)) {
            if (_isModified(d)) {
                ctx.fillStyle = 'rgba(220,180,40,0.08)';
                ctx.fillRect(x, y, w, h);
                ctx.fillStyle = 'rgba(220,180,40,0.6)';
                ctx.fillRect(x - 5, y + 2, 3, h - 4);
            } else {
                ctx.fillStyle = 'rgba(70,130,230,0.08)';
                ctx.fillRect(x, y, w, h);
                ctx.fillStyle = 'rgba(70,130,230,0.6)';
                ctx.fillRect(x - 5, y + 2, 3, h - 4);
            }
        }
        let mid = y + h / 2;
        ctx.textBaseline = 'middle';
        ctx.font = '11px ' + c.mono;
        ctx.fillStyle = c.link;
        ctx.fillText(d.doodId || '', x, mid);
        let catText = (typeof DOODAD_CATEGORIES !== 'undefined' && DOODAD_CATEGORIES[d.category]) || d.category || '';
        ctx.font = '11px ' + c.font;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        ctx.fillText(catText, x + w, mid);
        let catW = catText ? ctx.measureText(catText).width + 8 : 0;
        ctx.textAlign = 'left';
        let bx = x + w - catW;
        let ts = d.tilesets || '';
        if (ts) {
            let chars = ts === '*' ? ['*'] : ts.split(',').filter(Boolean);
            for (let bi = chars.length - 1; bi >= 0; bi--) {
                bx -= 18;
                _clDrawBadge(ctx, chars[bi], bx, y, h, c, chars[bi] === '*');
            }
            bx -= 4;
        }
        // Show baseId badge for custom doodads
        if (d.baseId) {
            let badgeText = '\u2190' + d.baseId;
            ctx.font = '9px ' + c.mono;
            let bw = ctx.measureText(badgeText).width + 6;
            bx -= bw + 2;
            ctx.fillStyle = 'rgba(100,149,237,0.15)';
            ctx.fillRect(bx, y + 4, bw, h - 8);
            ctx.fillStyle = 'rgba(100,149,237,0.8)';
            ctx.fillText(badgeText, bx + 3, mid);
            bx -= 4;
            ctx.font = '11px ' + c.font;
        }
        let nameX = x + 46;
        let nameW = bx - nameX;
        if (nameW > 10) {
            ctx.font = '12px ' + c.font;
            ctx.fillStyle = c.fg;
            _clTruncText(ctx, U.gsValue(d.name) || '', nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    // ── Canvas list lifecycle ─────────────────────────────────────
    function ensureCanvasList() {
        if (_doodadCanvasList) return;
        let el = document.getElementById('dsDoodadList');
        if (!el) return;
        _doodadCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: renderRow,
            onClick: function (item) {
                if (item._rawKey) showDetail(item._rawKey);
            }
        });
        if (_filteredDoodads.length) _doodadCanvasList.setData(_filteredDoodads);
    }

    function disposeCanvasList() {
        if (_doodadCanvasList) { _doodadCanvasList.dispose(); _doodadCanvasList = null; }
    }

    function cycleDoodSort(field) { _cycleDoodSort(field); }

    return {
        getDataMap,
        getAllDoodads,
        getFilteredDoodads,
        isLoaded,
        rebuild,
        showDetail,
        filterAndRender,
        restoreDoodFilters,
        restoreDoodSort,
        updateSortButtons,
        cycleDoodSort,
        renderRow,
        ensureCanvasList,
        disposeCanvasList,
    };
})();
