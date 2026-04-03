'use strict';

// ── Tileset rebuilder ───────────────────────────────────────────────

window._W3E_TILESET = (function () {
    var U = window._W3E_UTILS;

    function rebuildTileset(slkData, groundTileCodes, cliffTileCodes) {
        const slkMap = {};
        let source = '';
        if (slkData && slkData.tiles) {
            source = slkData.source || '';
            for (const t of slkData.tiles) slkMap[t.tileId] = t;
        }

        const srcEl = document.getElementById('tsSlkSource');
        if (srcEl) {
            if (source) {
                srcEl.className = 'ts-source';
                srcEl.innerHTML = 'Terrain.slk: <span class="code">' + U.esc(source) + '</span>';
            } else {
                srcEl.className = 'ts-source ts-no-slk';
                srcEl.textContent = 'Terrain.slk not found \u2014 set Game Path';
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

        const cliffSection = document.getElementById('tsCliffSection');
        if (cliffSection) {
            cliffSection.innerHTML = '';
            if (cliffTileCodes.length > 0) {
                const title = document.createElement('div');
                title.className = 'tw-section-title';
                title.textContent = 'Cliff Tiles (' + cliffTileCodes.length + ')';
                cliffSection.appendChild(title);

                const container = document.createElement('div');
                container.className = 'legend';
                for (let i = 0; i < cliffTileCodes.length; i++) {
                    const el = document.createElement('tile-item');
                    el.setAttribute('index', String(i));
                    el.setAttribute('code', cliffTileCodes[i]);
                    container.appendChild(el);
                }
                cliffSection.appendChild(container);
            }
        }
    }

    return { rebuildTileset };
})();

