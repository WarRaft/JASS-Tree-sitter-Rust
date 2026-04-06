'use strict';

// ── Tileset rebuilder ───────────────────────────────────────────────

window._W3E_TILESET = (function () {
    let U = window._W3E_UTILS;
    let S = window._W3E_STATE;

    function _makeSlkLink(slkPath) {
        let link = document.createElement('a');
        link.className = 'ts-slk-link';
        link.href = '#';
        link.textContent = slkPath;
        link.title = 'Open in side tab';
        link.addEventListener('click', function (e) {
            e.preventDefault();
            let vscode = S.getVscode();
            if (vscode) vscode.postMessage({command: 'openSlk', path: slkPath});
        });
        return link;
    }

    function rebuildTileset(slkData, groundTileCodes) {
        const slkMap = {};
        let source = '';
        if (slkData && slkData.tiles) {
            source = slkData.source || '';
            for (const t of slkData.tiles) slkMap[t.tileId] = t;
        }

        const srcEl = document.getElementById('tsSlkSource');
        if (srcEl) {
            srcEl.innerHTML = '';
            if (source) {
                srcEl.className = 'ts-source';
                srcEl.appendChild(_makeSlkLink('TerrainArt\\Terrain.slk'));
                const srcLine = document.createElement('div');
                srcLine.className = 'ts-slk-source-line';
                srcLine.textContent = source;
                srcEl.appendChild(srcLine);
            } else {
                srcEl.className = 'ts-source ts-no-slk';
                srcEl.textContent = 'TerrainArt\\Terrain.slk \u2014 not found, set Game Path';
            }
        }

        const groundCnt = document.getElementById('tsGroundCount');
        if (groundCnt) groundCnt.textContent = String(groundTileCodes.length);

        const groundEl = document.getElementById('tsGroundTiles');
        if (groundEl) {
            groundEl.innerHTML = '';
            for (let i = 0; i < groundTileCodes.length; i++) {
                const code = groundTileCodes[i];
                const rgb = U.indexToRgb(i);
                const info = slkMap[code];
                const el = document.createElement('tile-item');
                el.setAttribute('index', String(i));
                el.setAttribute('code', code);
                el.setAttribute('swatch-color', rgb.join(','));
                if (info) {
                    if (info.comment) el.setAttribute('tile-name', info.comment);
                    if (info.dir && info.file) {
                        el.setAttribute('tile-path', info.dir + '\\' + info.file + (info.ext || ''));
                    }
                }
                groundEl.appendChild(el);
            }
        }
    }

    function rebuildCliffs(cliffTypesSlk, cliffTileCodes) {
        const srcEl = document.getElementById('ctSlkSource');
        if (srcEl) {
            srcEl.innerHTML = '';
            const source = (cliffTypesSlk && cliffTypesSlk.source) || '';
            if (source) {
                srcEl.className = 'ts-source';
                srcEl.appendChild(_makeSlkLink('TerrainArt\\CliffTypes.slk'));
                const srcLine = document.createElement('div');
                srcLine.className = 'ts-slk-source-line';
                srcLine.textContent = source;
                srcEl.appendChild(srcLine);
            } else {
                srcEl.className = 'ts-source ts-no-slk';
                srcEl.textContent = 'TerrainArt\\CliffTypes.slk \u2014 not found, set Game Path';
            }
        }

        const cliffSection = document.getElementById('ctCliffSection');
        if (cliffSection) {
            cliffSection.innerHTML = '';
            if (cliffTileCodes.length > 0) {
                const cliffTypeMap = {};
                if (cliffTypesSlk && cliffTypesSlk.cliffTypes) {
                    for (const [id, ct] of Object.entries(cliffTypesSlk.cliffTypes)) {
                        cliffTypeMap[id] = ct;
                    }
                }

                const title = document.createElement('div');
                title.className = 'tw-section-title';
                title.textContent = 'Cliff Tiles (' + cliffTileCodes.length + ')';
                cliffSection.appendChild(title);

                const container = document.createElement('div');
                container.className = 'legend';
                for (let i = 0; i < cliffTileCodes.length; i++) {
                    const code = cliffTileCodes[i];
                    const el = document.createElement('tile-item');
                    el.setAttribute('index', String(i));
                    el.setAttribute('code', code);
                    const ct = cliffTypeMap[code];
                    if (ct) {
                        const parts = [];
                        if (ct.cliffModelDir) parts.push(ct.cliffModelDir);
                        if (ct.cliffClass) parts.push(ct.cliffClass);
                        if (parts.length > 0) el.setAttribute('tile-name', parts.join(' \u2014 '));
                        if (ct.texDir && ct.texFile) {
                            el.setAttribute('tile-path', ct.texDir + '\\' + ct.texFile + '.blp');
                        }
                        if (ct.texSource) {
                            el.setAttribute('tile-source', ct.texSource);
                        }
                    }
                    container.appendChild(el);
                }
                cliffSection.appendChild(container);
            }
        }
    }

    return { rebuildTileset, rebuildCliffs };
})();

