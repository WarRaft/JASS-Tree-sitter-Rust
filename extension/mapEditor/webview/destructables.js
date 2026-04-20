'use strict';

// ── Destructables SLK: rebuild, filter, sort, detail ────────────────

window._W3E_DESTRUCTABLES = (function () {
    let U = window._W3E_UTILS;
    let S = window._W3E_STATE;

    let _destructableDataMap = {};
    let _allDestructables = [];
    let _filteredDestructables = [];
    let _destructablesSlkLoaded = false;
    let _currentDestId = null;
    let _detailHandlersAttached = false;

    /** @returns {boolean} true if original (from SLK, not in w3b) */
    function _isOriginal(d) { return !(d.baseId || d.w3bModified); }
    /** @returns {boolean} true if has actual field modifications in w3b */
    function _isModified(d) { return !!(d.defaults && Object.keys(d.defaults).length > 0); }

    function _makeSlkLink(slkPath) {
        const link = document.createElement('a');
        link.className = 'ts-slk-link';
        link.href = '#';
        link.textContent = slkPath;
        link.title = 'Open in side tab';
        link.addEventListener('click', function (e) {
            e.preventDefault();
            const vscode = S.getVscode();
            if (vscode) vscode.postMessage({command: 'openSlk', path: slkPath});
        });
        return link;
    }

    let _destCanvasList = null;

    function getDataMap() { return _destructableDataMap; }
    function getAllDestructables() { return _allDestructables; }
    function getFilteredDestructables() { return _filteredDestructables; }
    function isLoaded() { return _destructablesSlkLoaded; }

    // ── Sort state ────────────────────────────────────────────────
    let _destSort = {field: null, dir: 'asc'};

    function _saveDestSort() {
        S.patchWvState({_destSort: {field: _destSort.field, dir: _destSort.dir}});
    }

    function _saveDestFilters() {
        const uncheckedCats = [];
        document.querySelectorAll('.dt-cat-cb').forEach(cb => {
            if (!cb.checked) uncheckedCats.push(cb.getAttribute('data-cat'));
        });
        const uncheckedTs = [];
        document.querySelectorAll('.dt-ts-cb').forEach(cb => {
            if (!cb.checked) uncheckedTs.push(cb.getAttribute('data-ts'));
        });
        const uncheckedStatus = [];
        document.querySelectorAll('.dt-status-cb').forEach(cb => {
            if (!cb.checked) uncheckedStatus.push(cb.getAttribute('data-status'));
        });
        S.patchWvState({_destUncheckedCats: uncheckedCats, _destUncheckedTs: uncheckedTs, _destUncheckedStatus: uncheckedStatus});
    }

    function restoreDestFilters() {
        const s = S.getWvState();
        const uncheckedCats = s._destUncheckedCats || [];
        const uncheckedTs = s._destUncheckedTs || [];
        const uncheckedStatus = s._destUncheckedStatus || [];
        if (uncheckedCats.length) {
            document.querySelectorAll('.dt-cat-cb').forEach(cb => {
                if (uncheckedCats.includes(cb.getAttribute('data-cat'))) cb.checked = false;
            });
        }
        if (uncheckedTs.length) {
            document.querySelectorAll('.dt-ts-cb').forEach(cb => {
                if (uncheckedTs.includes(cb.getAttribute('data-ts'))) cb.checked = false;
            });
        }
        if (uncheckedStatus.length) {
            document.querySelectorAll('.dt-status-cb').forEach(cb => {
                if (uncheckedStatus.includes(cb.getAttribute('data-status'))) cb.checked = false;
            });
        }
    }

    function restoreDestSort() {
        const s = S.getWvState();
        if (s._destSort && s._destSort.field) {
            _destSort = {field: s._destSort.field, dir: s._destSort.dir || 'asc'};
        }
    }

    function _cycleDestSort(field) {
        if (_destSort.field !== field) {
            _destSort = {field, dir: 'asc'};
        } else if (_destSort.dir === 'asc') {
            _destSort.dir = 'desc';
        } else {
            _destSort = {field: null, dir: 'asc'};
        }
        _saveDestSort();
        updateSortButtons();
        filterAndRender();
    }

    function updateSortButtons() {
        document.querySelectorAll('.dt-sort-col').forEach(btn => {
            const f = btn.getAttribute('data-sort');
            btn.classList.remove('ds-sort-active', 'ds-sort-asc', 'ds-sort-desc');
            if (f === _destSort.field) {
                btn.classList.add('ds-sort-active', _destSort.dir === 'asc' ? 'ds-sort-asc' : 'ds-sort-desc');
            }
        });
    }

    function filterAndRender(saveState) {
        const enabledCats = new Set();
        document.querySelectorAll('.dt-cat-cb').forEach(cb => {
            if (cb.checked) enabledCats.add(cb.getAttribute('data-cat'));
        });
        const enabledTs = new Set();
        document.querySelectorAll('.dt-ts-cb').forEach(cb => {
            if (cb.checked) enabledTs.add(cb.getAttribute('data-ts'));
        });
        const enabledStatus = new Set();
        document.querySelectorAll('.dt-status-cb').forEach(cb => {
            if (cb.checked) enabledStatus.add(cb.getAttribute('data-status'));
        });
        if (saveState !== false) _saveDestFilters();

        const searchEl = document.getElementById('dtSearchInput');
        const q = searchEl ? searchEl.value.toLowerCase().trim() : '';

        const filtered = _allDestructables.filter(d => {
            if (q) {
                const rn = U.gsValue(d.name);
                const rs = U.gsValue(d.editorSuffix);
                const name = ((rn || '') + (rs ? ' ' + rs : '')).toLowerCase();
                const id = (d.destructableId || '').toLowerCase();
                const comment = (U.gsValue(d.comment) || '').toLowerCase();
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

        if (_destSort.field) {
            const f = _destSort.field;
            const mul = _destSort.dir === 'desc' ? -1 : 1;
            filtered.sort((a, b) => {
                const va = U.gsValue(a[f]).toLowerCase();
                const vb = U.gsValue(b[f]).toLowerCase();
                return va < vb ? -1 * mul : va > vb ? 1 * mul : 0;
            });
        }

        _filteredDestructables = filtered;
        if (_destCanvasList) {
            _destCanvasList.setData(filtered);
        }

        const cntEl = document.getElementById('dtDestCount');
        if (cntEl) cntEl.textContent = String(filtered.length);
    }

    function _rebuildSidebarCheckboxes() {
        const catSet = new Set();
        const tsSet = new Set();
        for (const d of _allDestructables) {
            if (d.category) catSet.add(d.category);
            if (d.tilesets) {
                for (const ch of d.tilesets) {
                    if (ch !== ',' && ch !== '*') tsSet.add(ch);
                }
            }
        }

        // Status checkboxes: type (Original / Custom) + flag (Modified)
        const statusCounts = {original: 0, custom: 0, modified: 0};
        for (const d of _allDestructables) {
            if (_isOriginal(d)) {
                statusCounts.original++;
            } else {
                statusCounts.custom++;
                if (_isModified(d)) statusCounts.modified++;
            }
        }
        const statusChecks = document.getElementById('dtStatusChecks');
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
                cb.className = 'dt-status-cb';
                cb.setAttribute('data-status', code);
                cb.checked = true;
                cb.addEventListener('change', filterAndRender);
                lbl.appendChild(cb);
                lbl.appendChild(document.createTextNode(' ' + icon + ' ' + label + ' (' + statusCounts[code] + ')'));
                statusChecks.appendChild(lbl);
            }
        }

        const catChecks = document.getElementById('dtCatChecks');
        if (catChecks) {
            catChecks.innerHTML = '';
            for (const code of Array.from(catSet).sort()) {
                const label = DESTRUCTABLE_CATEGORIES[code] || code;
                const lbl = document.createElement('label');
                lbl.className = 'menu-cb';
                const cb = document.createElement('input');
                cb.type = 'checkbox';
                cb.className = 'dt-cat-cb';
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

        const tsChecks = document.getElementById('dtTsChecks');
        if (tsChecks) {
            tsChecks.innerHTML = '';
            for (const code of Array.from(tsSet).sort()) {
                const label = TILESET_NAMES[code] || code;
                const lbl = document.createElement('label');
                lbl.className = 'menu-cb';
                const cb = document.createElement('input');
                cb.type = 'checkbox';
                cb.className = 'dt-ts-cb';
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
        restoreDestFilters();
    }

    function rebuild(slkData) {
        _destructablesSlkLoaded = true;
        let source = '';
        let w3bErrors = [];
        _allDestructables = [];
        _destructableDataMap = {};
        if (slkData && slkData.destructables) {
            source = slkData.source || '';
            w3bErrors = slkData.w3bErrors || [];
            _destructableDataMap = slkData.destructables;
            _allDestructables = Object.entries(slkData.destructables).map(function (e) { e[1]._rawKey = e[0]; return e[1]; });
        }


        const srcEl = document.getElementById('dtSlkSource');
        if (srcEl) {
            srcEl.innerHTML = '';
            if (source) {
                srcEl.className = 'ts-source';
                srcEl.appendChild(_makeSlkLink('Units\\DestructableData.slk'));
                const srcLine = document.createElement('div');
                srcLine.className = 'ts-slk-source-line';
                srcLine.textContent = source;
                srcEl.appendChild(srcLine);

                if (w3bErrors.length > 0) {
                    const errLink = document.createElement('a');
                    errLink.className = 'ts-slk-link';
                    errLink.href = '#';
                    errLink.textContent = '\u26a0 ' + w3bErrors.length + ' error' + (w3bErrors.length > 1 ? 's' : '') + ' reading Destructables';
                    errLink.style.color = 'var(--vscode-errorForeground, #f44)';
                    errLink.addEventListener('click', function (e) {
                        e.preventDefault();
                        _showW3bErrors(w3bErrors);
                    });
                    srcEl.appendChild(errLink);
                }
            } else {
                srcEl.className = 'ts-source ts-no-slk';
                srcEl.textContent = 'DestructableData.slk not found \u2014 set Game Path';
            }
        }

        const totalEl = document.getElementById('dtDestTotal');
        if (totalEl) totalEl.textContent = String(_allDestructables.length);

        _rebuildSidebarCheckboxes();
        restoreDestSort();
        updateSortButtons();
        filterAndRender(false);

        const searchEl = document.getElementById('dtSearchInput');
        if (searchEl && !searchEl._dtBound) {
            searchEl._dtBound = true;
            searchEl.addEventListener('input', filterAndRender);
        }

        document.querySelectorAll('.dt-sort-col').forEach(btn => {
            if (btn._dtSortBound) return;
            btn._dtSortBound = true;
            btn.addEventListener('click', () => _cycleDestSort(btn.getAttribute('data-sort')));
        });
    }

    // ── Detail window ─────────────────────────────────────────────
    const _DEST_GROUPS = [
        {
            title: '\ud83c\udff7 Identity', fields: [
                ['DestructableID', 'destructableId'], ['baseID', 'baseId'], ['Name', 'name'],
                ['EditorSuffix', 'editorSuffix'], ['comment', 'comment'],
                ['category', 'category'], ['doodClass', 'doodClass'],
                ['tilesets', 'tilesets'], ['tilesetSpecific', 'tilesetSpecific'],
            ]
        },
        {
            title: '\ud83c\udfa8 Model', modelFiles: true, fields: [
                ['texID', 'texId'], ['texFile', 'texFile'],
            ]
        },
        {
            title: '\ud83d\udee1 Combat', fields: [
                ['HP', 'hp'], ['armor', 'armor'], ['targType', 'targType'],
            ]
        },
        {
            title: '\ud83d\udcd0 Scale', fields: [
                ['minScale', 'minScale'], ['maxScale', 'maxScale'],
                ['canPlaceRandScale', 'canPlaceRandScale'],
            ]
        },
        {
            title: '\ud83d\udccd Placement', fields: [
                ['onCliffs', 'onCliffs'], ['onWater', 'onWater'],
                ['walkable', 'walkable'], ['canPlaceDead', 'canPlaceDead'],
                ['cliffHeight', 'cliffHeight'], ['fixedRot', 'fixedRot'],
                ['maxPitch', 'maxPitch'], ['maxRoll', 'maxRoll'],
                ['pathTex', 'pathTex'], ['pathTexDeath', 'pathTexDeath'],
                ['occH', 'occH'], ['flyH', 'flyH'],
            ]
        },
        {
            title: '\ud83d\udc46 Interaction', fields: [
                ['selSize', 'selSize'], ['useClickHelper', 'useClickHelper'],
                ['selectable', 'selectable'], ['selcircsize', 'selcircsize'],
                ['radius', 'radius'], ['fogRadius', 'fogRadius'],
                ['fogVis', 'fogVis'], ['lightweight', 'lightweight'],
                ['fatLOS', 'fatLos'],
            ]
        },
        {
            title: '\ud83d\udc41 Rendering', fields: [
                ['shadow', 'shadow'], ['deathSnd', 'deathSnd'],
                ['portraitmodel', 'portraitmodel'],
            ],
            color: {key: 'color', label: 'Tint Color'},
        },
        {
            title: '\ud83d\uddfa Minimap', fields: [
                ['showInMM', 'showInMm'], ['useMMColor', 'useMmColor'],
            ],
            color: {key: 'mmColor', label: 'Color'},
        },
        {
            title: '\ud83d\udee0 Economy', fields: [
                ['buildTime', 'buildTime'], ['repairTime', 'repairTime'],
                ['goldRep', 'goldRep'], ['lumberRep', 'lumberRep'],
            ]
        },
        {
            title: '\u2139 Meta', fields: [
                ['InBeta', 'inBeta'], ['version', 'version'],
            ]
        },
    ];

    function _getDestCollapseState() {
        return S.getWvState()._destCollapse || {};
    }

    function _setDestCollapseState(state) {
        S.patchWvState({_destCollapse: state});
    }

    if (U && typeof U.onModelVariantsResolved === 'function') {
        U.onModelVariantsResolved(function () {
            if (_currentDestId && _destructableDataMap[_currentDestId]) {
                showDetail(_currentDestId);
            }
        });
    }


    function showDetail(destId) {
        let vscode = S.getVscode();
        _currentDestId = destId;
        const d = _destructableDataMap[destId];
        if (!d) {
            const win = document.getElementById('destructableDetailWindow');
            const body = document.getElementById('destructableDetailBody');
            if (win && body) {
                body.innerHTML = '<div style="padding:1rem;color:var(--vscode-errorForeground,#f44);">'
                    + '<b>' + U.esc(String(destId)) + '</b> not found in DestructableData.slk<br>'
                    + '<small style="opacity:0.7;">Loaded destructables: ' + Object.keys(_destructableDataMap).length + '</small>'
                    + '</div>';
                win.setAttribute('title-text', '\ud83c\udfda ' + U.esc(String(destId)));
                win.show();
            }
            return;
        }

        const win = document.getElementById('destructableDetailWindow');
        if (!win) return;
        const body = document.getElementById('destructableDetailBody');
        if (!body) return;

        let html = '';
        const collapseState = _getDestCollapseState();

        for (const group of _DEST_GROUPS) {
            let rows = '';

            if (group.modelFiles) {
                const filePath = d.file;
                const numVar = d.numVar || 1;
                const dtTexId = d.texId || 0;
                const dtTexFile = d.texFile || '';
                if (filePath) {
                    rows += U.renderModelFileRow({filePath: filePath, numVar: numVar, defaults: d.defaults, vscode: vscode, includeTexAttrs: true, texId: dtTexId, texFile: dtTexFile});
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
                        let val = d[key];
                        if (val === undefined || val === '' || val === null) continue;
                        let display;
                        if (key === 'name' || key === 'editorSuffix' || key === 'comment') {
                            display = U.gsHtml(val);
                        } else if (key === 'pathTex' || key === 'pathTexDeath') {
                            display = '<a href="#" class="dd-pathtex-link" data-pathtex="' + U.esc(String(val)) + '">' + U.esc(String(val)) + '</a>';
                        } else {
                            display = U.esc(String(val));
                        }
                        if (key === 'category' && val) {
                            display = U.categoryBadge(val, DESTRUCTABLE_CATEGORIES);
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
        const titleName = U.gsValue(d.name) || d.destructableId;
        const titleSuffix = U.gsValue(d.editorSuffix);
        win.setAttribute('title-text', '\ud83c\udfda ' + titleName + (titleSuffix ? ' ' + titleSuffix : ''));
        win.show();

        if (!_detailHandlersAttached) {
            body.addEventListener('collapse-toggle', function (e) {
                const state = _getDestCollapseState();
                state[e.detail.title] = e.detail.open;
                _setDestCollapseState(state);
            });

            body.addEventListener('click', function (e) {
                let link = e.target.closest('.dd-model-link');
                if (link) {
                    e.preventDefault();
                    let cmd = {command: 'openModel', path: link.getAttribute('data-path')};
                    let tId = link.getAttribute('data-tex-id');
                    let tFile = link.getAttribute('data-tex-file');
                    if (tId) cmd.texId = parseInt(tId, 10);
                    if (tFile) cmd.texFile = tFile;
                    const curVscode = S.getVscode();
                    if (curVscode) curVscode.postMessage(cmd);
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

    // ── W3B errors window ──────────────────────────────────────
    function _showW3bErrors(errors) {
        const win = document.getElementById('destructableErrorsWindow');
        const body = document.getElementById('destructableErrorsBody');
        if (!win || !body) return;
        let html = '<div style="margin-bottom:8px;font-weight:bold;">\u26a0 ' + errors.length + ' error' + (errors.length > 1 ? 's' : '') + ' reading Destructables from war3map.w3b:</div>';
        html += '<div style="font-family:var(--vscode-editor-font-family,monospace);font-size:11px;">';
        for (let i = 0; i < errors.length; i++) {
            html += '<div style="padding:2px 0;border-bottom:1px solid var(--vscode-widget-border,rgba(128,128,128,0.2));">'
                + U.esc(errors[i]) + '</div>';
        }
        html += '</div>';
        body.innerHTML = html;
        win.setAttribute('title-text', '\u26a0 Destructable Errors (' + errors.length + ')');
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
        ctx.fillText(d.destructableId || '', x, mid);
        let catText = (typeof DESTRUCTABLE_CATEGORIES !== 'undefined' && DESTRUCTABLE_CATEGORIES[d.category]) || d.category || '';
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
        // Show baseId badge for custom destructables
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
            let rn = U.gsValue(d.name) || '';
            let rs = U.gsValue(d.editorSuffix);
            _clTruncText(ctx, rn + (rs ? ' ' + rs : ''), nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    // ── Canvas list lifecycle ─────────────────────────────────────
    function ensureCanvasList() {
        if (_destCanvasList) return;
        let el = document.getElementById('dtDestList');
        if (!el) return;
        _destCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: renderRow,
            onClick: function (item) {
                if (item._rawKey) showDetail(item._rawKey);
            }
        });
        if (_filteredDestructables.length) _destCanvasList.setData(_filteredDestructables);
    }

    function disposeCanvasList() {
        if (_destCanvasList) { _destCanvasList.dispose(); _destCanvasList = null; }
    }

    function cycleDestSort(field) { _cycleDestSort(field); }

    return {
        getDataMap,
        getAllDestructables,
        getFilteredDestructables,
        isLoaded,
        rebuild,
        showDetail,
        filterAndRender,
        restoreDestFilters,
        restoreDestSort,
        updateSortButtons,
        cycleDestSort,
        renderRow,
        ensureCanvasList,
        disposeCanvasList,
    };
})();

