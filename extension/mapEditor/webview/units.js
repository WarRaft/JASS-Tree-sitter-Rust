'use strict';

// ── Units SLK: rebuild, filter, sort, detail ────────────────────────

window._W3E_UNITS = (function () {
    let U = window._W3E_UTILS;
    let S = window._W3E_STATE;

    let _unitDataMap = {};
    let _allUnits = [];
    let _filteredUnits = [];

    let _unitCanvasList = null;

    function getDataMap() { return _unitDataMap; }
    function getAllUnits() { return _allUnits; }

    // ── Sort state ────────────────────────────────────────────────
    let _unitSort = {field: null, dir: 'asc'};

    function _saveUnitSort() {
        S.patchWvState({_unitSort: {field: _unitSort.field, dir: _unitSort.dir}});
    }

    function _saveUnitFilters() {
        const uncheckedRaces = [];
        document.querySelectorAll('.us-race-cb').forEach(cb => {
            if (!cb.checked) uncheckedRaces.push(cb.getAttribute('data-race'));
        });
        S.patchWvState({_unitUncheckedRaces: uncheckedRaces});
    }

    function restoreUnitFilters() {
        const s = S.getWvState();
        const uncheckedRaces = s._unitUncheckedRaces || [];
        if (uncheckedRaces.length) {
            document.querySelectorAll('.us-race-cb').forEach(cb => {
                if (uncheckedRaces.includes(cb.getAttribute('data-race'))) cb.checked = false;
            });
        }
    }

    function restoreUnitSort() {
        const s = S.getWvState();
        if (s._unitSort && s._unitSort.field) {
            _unitSort = {field: s._unitSort.field, dir: s._unitSort.dir || 'asc'};
        }
    }

    function _cycleUnitSort(field) {
        if (_unitSort.field !== field) {
            _unitSort = {field, dir: 'asc'};
        } else if (_unitSort.dir === 'asc') {
            _unitSort.dir = 'desc';
        } else {
            _unitSort = {field: null, dir: 'asc'};
        }
        _saveUnitSort();
        updateSortButtons();
        filterAndRender();
    }

    function updateSortButtons() {
        document.querySelectorAll('.us-sort-col').forEach(btn => {
            const f = btn.getAttribute('data-sort');
            btn.classList.remove('ds-sort-active', 'ds-sort-asc', 'ds-sort-desc');
            if (f === _unitSort.field) {
                btn.classList.add('ds-sort-active', _unitSort.dir === 'asc' ? 'ds-sort-asc' : 'ds-sort-desc');
            }
        });
    }

    function filterAndRender(saveState) {
        const enabledRaces = new Set();
        document.querySelectorAll('.us-race-cb').forEach(cb => {
            if (cb.checked) enabledRaces.add(cb.getAttribute('data-race'));
        });
        if (saveState !== false) _saveUnitFilters();

        const searchEl = document.getElementById('usSearchInput');
        const q = searchEl ? searchEl.value.toLowerCase().trim() : '';

        const filtered = _allUnits.filter(u => {
            if (q) {
                const name = (U.gsValue(u.name) || '').toLowerCase();
                const id = (u.unitId || '').toLowerCase();
                const comment = (u.comment || '').toLowerCase();
                if (!name.includes(q) && !id.includes(q) && !comment.includes(q)) return false;
            }
            if (u.race && !enabledRaces.has(u.race)) return false;
            return true;
        });

        if (_unitSort.field) {
            const f = _unitSort.field;
            const mul = _unitSort.dir === 'desc' ? -1 : 1;
            filtered.sort((a, b) => {
                let va, vb;
                if (f === 'name') {
                    va = (U.gsValue(a.name) || '').toLowerCase();
                    vb = (U.gsValue(b.name) || '').toLowerCase();
                } else {
                    va = (a[f] || '').toString().toLowerCase();
                    vb = (b[f] || '').toString().toLowerCase();
                }
                return va < vb ? -1 * mul : va > vb ? 1 * mul : 0;
            });
        }

        _filteredUnits = filtered;
        if (_unitCanvasList) {
            _unitCanvasList.setData(filtered);
        }

        const cntEl = document.getElementById('usUnitCount');
        if (cntEl) cntEl.textContent = String(filtered.length);
    }

    function _rebuildSidebarCheckboxes() {
        const raceSet = new Set();
        for (const u of _allUnits) {
            if (u.race) raceSet.add(u.race);
        }

        const UNIT_RACE_NAMES = {
            human: 'Human', orc: 'Orc', undead: 'Undead', nightelf: 'Night Elf',
            creeps: 'Creeps', commoner: 'Commoner', other: 'Other', demon: 'Demon',
            critters: 'Critters', naga: 'Naga',
        };

        const raceChecks = document.getElementById('usRaceChecks');
        if (raceChecks) {
            raceChecks.innerHTML = '';
            for (const code of Array.from(raceSet).sort()) {
                const label = UNIT_RACE_NAMES[code] || code;
                const lbl = document.createElement('label');
                lbl.className = 'menu-cb';
                const cb = document.createElement('input');
                cb.type = 'checkbox';
                cb.className = 'us-race-cb';
                cb.setAttribute('data-race', code);
                cb.checked = true;
                cb.addEventListener('change', filterAndRender);
                lbl.appendChild(cb);
                lbl.appendChild(document.createTextNode(' ' + label));
                raceChecks.appendChild(lbl);
            }
        }
        restoreUnitFilters();
    }

    function rebuild(slkData) {
        let source = '';
        _allUnits = [];
        _unitDataMap = {};
        let sources = [];
        if (slkData && slkData.units) {
            source = slkData.source || '';
            sources = slkData.sources || [];
            _unitDataMap = slkData.units;
            _allUnits = Object.entries(slkData.units).map(function (e) { e[1]._rawKey = e[0]; return e[1]; });
        }

        const srcEl = document.getElementById('usSlkSources');
        if (srcEl) {
            if (sources.length > 0) {
                srcEl.setAttribute('group-title', 'SLK Sources (' + sources.length + ')');
                srcEl.innerHTML = sources.map(function (s) {
                    return '<div class="ts-source" style="margin:1px 0;font-size:11px;">' + U.esc(s.source) + ' <span style="opacity:0.5;">(' + s.rows + ')</span></div>';
                }).join('');
            } else {
                srcEl.setAttribute('group-title', 'SLK Sources (0)');
                srcEl.innerHTML = '<div class="ts-source ts-no-slk">UnitData.slk not found \u2014 set Game Path</div>';
            }
        }

        const totalEl = document.getElementById('usUnitTotal');
        if (totalEl) totalEl.textContent = String(_allUnits.length);

        _rebuildSidebarCheckboxes();
        restoreUnitSort();
        updateSortButtons();
        filterAndRender(false);

        const searchEl = document.getElementById('usSearchInput');
        if (searchEl && !searchEl._usBound) {
            searchEl._usBound = true;
            searchEl.addEventListener('input', filterAndRender);
        }

        document.querySelectorAll('.us-sort-col').forEach(btn => {
            if (btn._usSortBound) return;
            btn._usSortBound = true;
            btn.addEventListener('click', () => _cycleUnitSort(btn.getAttribute('data-sort')));
        });
    }

    // ── Detail window ─────────────────────────────────────────────
    const _UNIT_GROUPS = [
        {
            title: '\ud83c\udff7 Identity', fields: [
                ['unitID', 'unitId'], ['Name', 'name'], ['comment', 'comment'],
                ['sort', 'sort'], ['race', 'race'], ['tilesets', 'tilesets'],
                ['level', 'level'], ['type', 'unitType'], ['isBldg', 'isBldg'],
            ]
        },
        {
            title: '\ud83c\udfa8 Model', modelFiles: true, fields: [
                ['modelScale', 'modelScale'], ['scale', 'scale'],
                ['scaleBull', 'scaleBull'], ['unitShadow', 'unitShadow'],
                ['buildingShadow', 'buildingShadow'], ['shadowOnWater', 'shadowOnWater'],
                ['special', 'special'], ['unitSound', 'unitSound'],
                ['unitClass', 'unitClass'],
            ],
            color: {key: '_tint', label: 'Tint Color'},
        },
        {
            title: '\u2764 Health & Mana', fields: [
                ['HP', 'hp'], ['realHP', 'realHp'], ['regenHP', 'regenHp'],
                ['regenType', 'regenType'], ['mana0', 'mana0'],
                ['manaN', 'manaN'], ['realM', 'realM'], ['regenMana', 'regenMana'],
            ]
        },
        {
            title: '\ud83d\udee1 Defence', fields: [
                ['def', 'def'], ['defType', 'defType'], ['defUp', 'defUp'],
                ['realdef', 'realDef'], ['targType', 'targType'],
                ['collision', 'collision'],
            ]
        },
        {
            title: '\u2694 Weapon 1', fields: [
                ['weapTp1', 'weapTp1'], ['weapType1', 'weapType1'],
                ['atkType1', 'atkType1'], ['dmgplus1', 'dmgplus1'],
                ['dice1', 'dice1'], ['sides1', 'sides1'],
                ['cool1', 'cool1'], ['rangeN1', 'rangeN1'],
                ['dmgPt1', 'dmgPt1'], ['backSw1', 'backSw1'],
                ['targs1', 'targs1'], ['splashTargs1', 'splashTargs1'],
                ['showUI1', 'showUi1'], ['minRange', 'minRange'],
                ['acquire', 'acquire'],
            ]
        },
        {
            title: '\u2694 Weapon 2', fields: [
                ['weapTp2', 'weapTp2'], ['weapType2', 'weapType2'],
                ['atkType2', 'atkType2'], ['dmgplus2', 'dmgplus2'],
                ['dice2', 'dice2'], ['sides2', 'sides2'],
                ['cool2', 'cool2'], ['rangeN2', 'rangeN2'],
                ['dmgPt2', 'dmgPt2'], ['backSw2', 'backSw2'],
                ['targs2', 'targs2'], ['splashTargs2', 'splashTargs2'],
                ['showUI2', 'showUi2'],
            ]
        },
        {
            title: '\ud83d\udcaa Stats', fields: [
                ['Primary', 'primary'], ['STR', 'str'], ['STR+', 'strPlus'],
                ['AGI', 'agi'], ['AGI+', 'agiPlus'],
                ['INT', 'int'], ['INT+', 'intPlus'],
            ]
        },
        {
            title: '\ud83d\udeb6 Movement', fields: [
                ['moveTp', 'moveTp'], ['spd', 'spd'],
                ['minSpd', 'minSpd'], ['maxSpd', 'maxSpd'],
                ['moveHeight', 'moveHeight'], ['moveFloor', 'moveFloor'],
                ['turnRate', 'turnRate'], ['propWin', 'propWin'],
            ]
        },
        {
            title: '\ud83d\udc41 Vision & Placement', fields: [
                ['sight', 'sight'], ['nsight', 'nsight'],
                ['pathTex', 'pathTex'], ['occH', 'occH'],
                ['selZ', 'selZ'], ['fogRad', 'fogRad'],
                ['uberSplat', 'uberSplat'], ['selCircOnWater', 'selCircOnWater'],
                ['maxPitch', 'maxPitch'], ['maxRoll', 'maxRoll'],
                ['elevPts', 'elevPts'], ['elevRad', 'elevRad'],
                ['fatLOS', 'fatLos'], ['inEditor', 'inEditor'],
                ['hiddenInEditor', 'hiddenInEditor'],
            ]
        },
        {
            title: '\ud83d\udee0 Economy', fields: [
                ['goldcost', 'goldCost'], ['lumbercost', 'lumberCost'],
                ['bldtm', 'bldTm'], ['reptm', 'repTm'],
                ['goldRep', 'goldRep'], ['lumberRep', 'lumberRep'],
                ['fmade', 'fmade'], ['fused', 'fused'],
                ['bountyDice', 'bountyDice'], ['bountySides', 'bountySides'],
                ['bountyPlus', 'bountyPlus'], ['points', 'points'],
            ]
        },
        {
            title: '\ud83d\udcdd Strings', fields: [
                ['Tip', 'tip'], ['Ubertip', 'ubertip'],
                ['Hotkey', 'hotkey'], ['Propernames', 'propernames'],
                ['Revivetip', 'revivetip'], ['Awakentip', 'awakentip'],
                ['EditorSuffix', 'editorSuffix'],
                ['CasterUpgradeName', 'casterUpgradeName'],
                ['CasterUpgradeTip', 'casterUpgradeTip'],
            ]
        },
        {
            title: '\u2139 Meta', fields: [
                ['InBeta', 'inBeta'], ['version', 'version'],
            ]
        },
    ];

    function _getUnitCollapseState() {
        return S.getWvState()._unitCollapse || {};
    }

    function _setUnitCollapseState(state) {
        S.patchWvState({_unitCollapse: state});
    }

    function showDetail(unitId) {
        let vscode = S.getVscode();
        const u = _unitDataMap[unitId];
        if (!u) {
            const win = document.getElementById('unitDetailWindow');
            const body = document.getElementById('unitDetailBody');
            if (win && body) {
                body.innerHTML = '<div style="padding:1rem;color:var(--vscode-errorForeground,#f44);">'
                    + '<b>' + U.esc(String(unitId)) + '</b> not found in UnitData.slk<br>'
                    + '<small style="opacity:0.7;">Loaded units: ' + Object.keys(_unitDataMap).length + '</small>'
                    + '</div>';
                win.setAttribute('title-text', '\ud83d\udde1 ' + U.esc(String(unitId)));
                win.show();
            }
            return;
        }

        const win = document.getElementById('unitDetailWindow');
        if (!win) return;
        const body = document.getElementById('unitDetailBody');
        if (!body) return;

        u._tint = {r: u.red || 255, g: u.green || 255, b: u.blue || 255};

        let html = '';
        const collapseState = _getUnitCollapseState();

        for (const group of _UNIT_GROUPS) {
            let rows = '';

            if (group.modelFiles) {
                const filePath = u.file;
                if (filePath) {
                    const link = '<a href="#" class="dd-model-link" data-path="' + U.esc(filePath) + '">' + U.esc(filePath) + '</a>';
                    rows += '<tr><td class="key">file</td><td>' + link + '</td></tr>';
                }
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        const val = u[key];
                        if (val === undefined || val === '' || val === null) continue;
                        rows += '<tr><td class="key">' + U.esc(label) + '</td><td>' + U.esc(String(val)) + '</td></tr>';
                    }
                }
                if (group.color) {
                    const c = u[group.color.key];
                    if (c) {
                        rows += '<tr><td class="key">' + U.esc(group.color.label) + '</td><td>'
                            + c.r + ',' + c.g + ',' + c.b + ' '
                            + U.colorBadge(c.r, c.g, c.b) + '</td></tr>';
                    }
                }
            } else {
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        const val = u[key];
                        if (val === undefined || val === '' || val === null) continue;
                        let display;
                        if (key === 'name') {
                            display = U.gsHtml(val);
                        } else if (key === 'pathTex') {
                            display = '<a href="#" class="dd-pathtex-link" data-pathtex="' + U.esc(String(val)) + '">' + U.esc(String(val)) + '</a>';
                        } else {
                            display = U.esc(String(val));
                        }
                        rows += '<tr><td class="key">' + U.esc(label) + '</td><td>' + display + '</td></tr>';
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
        win.setAttribute('title-text', '\ud83d\udde1 ' + (U.gsValue(u.name) || u.unitId));
        win.show();

        body.addEventListener('collapse-toggle', function (e) {
            const state = _getUnitCollapseState();
            state[e.detail.title] = e.detail.open;
            _setUnitCollapseState(state);
        });

        body.addEventListener('click', function (e) {
            let link = e.target.closest('.dd-model-link');
            if (link) {
                e.preventDefault();
                if (vscode) vscode.postMessage({command: 'openModel', path: link.getAttribute('data-path')});
                return;
            }
            let ptLink = e.target.closest('.dd-pathtex-link');
            if (ptLink) {
                e.preventDefault();
                window._W3E_PATH_TEX.showPathTex(ptLink.getAttribute('data-pathtex'));
            }
        });
    }

    // ── Canvas list row renderer ──────────────────────────────────
    function renderRow(ctx, u, x, y, w, h, c) {
        let mid = y + h / 2;
        ctx.textBaseline = 'middle';
        ctx.font = '11px ' + c.mono;
        ctx.fillStyle = c.link;
        ctx.fillText(u.unitId || '', x, mid);
        let raceText = u.race || '';
        ctx.font = '11px ' + c.font;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        ctx.fillText(raceText, x + w, mid);
        let raceW = raceText ? ctx.measureText(raceText).width + 8 : 0;
        ctx.textAlign = 'left';
        let nameX = x + 46;
        let nameEnd = x + w - raceW;
        let nameW = nameEnd - nameX;
        if (nameW > 10) {
            ctx.font = '12px ' + c.font;
            ctx.fillStyle = c.fg;
            _clTruncText(ctx, U.gsValue(u.name) || u.comment || '', nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    // ── Canvas list lifecycle ─────────────────────────────────────
    function ensureCanvasList() {
        if (_unitCanvasList) return;
        let el = document.getElementById('usUnitList');
        if (!el) return;
        _unitCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: renderRow,
            onClick: function (item) {
                if (item._rawKey) showDetail(item._rawKey);
            }
        });
        if (_filteredUnits.length) _unitCanvasList.setData(_filteredUnits);
    }

    function disposeCanvasList() {
        if (_unitCanvasList) { _unitCanvasList.dispose(); _unitCanvasList = null; }
    }

    function cycleUnitSort(field) { _cycleUnitSort(field); }

    return {
        getDataMap,
        getAllUnits,
        rebuild,
        showDetail,
        filterAndRender,
        restoreUnitFilters,
        restoreUnitSort,
        updateSortButtons,
        cycleUnitSort,
        renderRow,
        ensureCanvasList,
        disposeCanvasList,
    };
})();

