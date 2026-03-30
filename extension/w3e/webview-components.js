'use strict';

// ── <float-window> Custom Element ────────────────────────────────────
// Usage:
//   <float-window id="myWin" title-text="Title" hidden style="left:100px;top:16px;">
//     <button slot="actions" class="float-action">📁</button>  <!-- optional -->
//     ...body content...
//   </float-window>
// API: .toggle(), .show(), .hide(), .open (getter)

class FloatWindow extends HTMLElement {
    static get observedAttributes() { return ['title-text']; }

    constructor() {
        super();
        const shadow = this.attachShadow({mode: 'open'});
        shadow.innerHTML = `
<style>
:host {
    position: absolute;
    z-index: 10;
    min-width: 260px;
    max-width: 500px;
    background: rgba(37, 37, 38, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    overflow: hidden;
    display: block;
    font-family: var(--vscode-font-family, 'Segoe UI', sans-serif);
    font-size: 13px;
    color: var(--vscode-editor-foreground, #ccc);
}
:host([hidden]) { display: none !important; }
.title {
    display: flex; align-items: center;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.04);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    cursor: grab; user-select: none;
    font-size: 12px; font-weight: 600;
}
.title:active { cursor: grabbing; }
.title-label { flex: 1; }
.title-right { display: flex; align-items: center; gap: 2px; }
.close {
    background: none; border: none;
    color: var(--vscode-editor-foreground, #ccc);
    cursor: pointer; font-size: 14px; line-height: 1;
    padding: 0 4px; border-radius: 3px; opacity: 0.6;
}
.close:hover { opacity: 1; background: rgba(255, 255, 255, 0.1); }
.body {
    padding: 10px;
    max-height: 60vh;
    overflow-y: auto;
}
:host([no-padding]) .body { padding: 0; }
.body::-webkit-scrollbar { width: 6px; }
.body::-webkit-scrollbar-track { background: transparent; }
.body::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.15); border-radius: 3px; }
.body::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,0.25); }
</style>
<div class="title" id="titleBar">
    <span class="title-label" id="titleText"></span>
    <div class="title-right">
        <slot name="actions"></slot>
        <button class="close" id="closeBtn">\u00d7</button>
    </div>
</div>
<div class="body"><slot></slot></div>`;

        this._titleEl = shadow.getElementById('titleText');
        this._titleBar = shadow.getElementById('titleBar');

        // Close
        shadow.getElementById('closeBtn').addEventListener('click', () => this.hide());

        // Drag
        let dragging = false, sx = 0, sy = 0, ox = 0, oy = 0;
        this._titleBar.addEventListener('pointerdown', e => {
            if (e.target.closest('button')) return;
            dragging = true;
            sx = e.clientX; sy = e.clientY;
            ox = this.offsetLeft; oy = this.offsetTop;
            this._titleBar.setPointerCapture(e.pointerId);
            e.preventDefault();
            this._bringToFront();
        });
        this._titleBar.addEventListener('pointermove', e => {
            if (!dragging) return;
            this.style.left = (ox + e.clientX - sx) + 'px';
            this.style.top = (oy + e.clientY - sy) + 'px';
            this.style.right = 'auto';
        });
        this._titleBar.addEventListener('pointerup', e => {
            dragging = false;
            this._titleBar.releasePointerCapture(e.pointerId);
        });
    }

    attributeChangedCallback(name, old, val) {
        if (name === 'title-text') this._titleEl.textContent = val || '';
    }

    get open() { return !this.hasAttribute('hidden'); }

    toggle() { if (this.open) this.hide(); else this.show(); }

    show() {
        this.removeAttribute('hidden');
        this._bringToFront();
        this._notifyToggle();
    }

    hide() {
        this.setAttribute('hidden', '');
        this._notifyToggle();
    }

    _bringToFront() {
        document.querySelectorAll('float-window').forEach(w => { w.style.zIndex = '10'; });
        this.style.zIndex = '11';
    }

    _notifyToggle() {
        document.dispatchEvent(new CustomEvent('float-toggled', {detail: {id: this.id}}));
    }
}

customElements.define('float-window', FloatWindow);


// ── <tile-item> Custom Element ───────────────────────────────────────
// Usage:
//   <tile-item index="0" code="Ldrt" tile-name="Dirt"
//              tile-path="TerrainArt\...\file.blp"
//              swatch-color="128,200,50"></tile-item>
//
// Fires: 'color-change' event with {index, color} when the user picks a colour.

class TileItem extends HTMLElement {
    static get observedAttributes() {
        return ['index', 'code', 'tile-name', 'tile-path', 'swatch-color', 'tile-preview'];
    }

    constructor() {
        super();
        const shadow = this.attachShadow({mode: 'open'});
        shadow.innerHTML = `
<style>
:host {
    display: flex; align-items: flex-start; gap: 8px;
    padding: 6px 8px; border-radius: 4px;
}
:host(:hover) { background: rgba(255, 255, 255, 0.05); }

/* ── tile preview square with color badge ── */
.preview {
    position: relative;
    width: 40px; height: 40px;
    flex-shrink: 0;
    border-radius: 4px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    background: rgba(255, 255, 255, 0.06);
    overflow: visible;
}
.color-badge {
    position: absolute;
    right: -4px; bottom: -4px;
    width: 16px; height: 16px;
    border-radius: 50%;
    border: 2px solid rgba(30, 30, 30, 0.9);
    cursor: pointer;
    box-shadow: 0 0 0 1px rgba(255,255,255,0.15);
}
.color-badge[hidden] { display: none; }
.color-input {
    position: absolute;
    top: 0; left: 0;
    width: 100%; height: 100%;
    opacity: 0;
    cursor: pointer;
    border: none; padding: 0;
}

/* ── text block ── */
.info {
    display: flex; flex-direction: column; gap: 1px;
    min-width: 0;
    padding-top: 2px;
}
.code {
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: 11px; color: var(--vscode-textLink-foreground, #3794ff);
}
.name {
    font-size: 11px;
    color: var(--vscode-descriptionForeground, #999);
}
.name:empty { display: none; }
.path {
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: 10px;
    color: var(--vscode-descriptionForeground, #666);
    word-break: break-all;
}
.path:empty { display: none; }
</style>
<div class="preview" id="preview">
    <div class="color-badge" id="badge">
        <input type="color" class="color-input" id="colorInput" title="Change map colour">
    </div>
</div>
<div class="info">
    <span class="code" id="code"></span>
    <span class="name" id="name"></span>
    <span class="path" id="path"></span>
</div>`;

        this._colorInput = shadow.getElementById('colorInput');
        this._badge = shadow.getElementById('badge');

        this._colorInput.addEventListener('input', () => {
            const hex = this._colorInput.value;
            const r = parseInt(hex.slice(1, 3), 16);
            const g = parseInt(hex.slice(3, 5), 16);
            const b = parseInt(hex.slice(5, 7), 16);
            this._badge.style.background = hex;
            this.setAttribute('swatch-color', r + ',' + g + ',' + b);
            this.dispatchEvent(new CustomEvent('color-change', {
                bubbles: true,
                detail: {index: parseInt(this.getAttribute('index') || '0', 10), color: [r / 255, g / 255, b / 255]}
            }));
        });
    }

    connectedCallback() { this._render(); }
    attributeChangedCallback() { if (this.isConnected) this._render(); }

    _render() {
        const s = this.shadowRoot;
        const idx = this.getAttribute('index') || '';
        const code = this.getAttribute('code') || '';
        const color = this.getAttribute('swatch-color') || '';
        const preview = this.getAttribute('tile-preview') || '';

        const previewEl = s.getElementById('preview');
        if (preview) {
            previewEl.style.backgroundImage = 'url(' + preview + ')';
            previewEl.style.backgroundSize = 'cover';
        } else {
            previewEl.style.backgroundImage = '';
            previewEl.style.backgroundSize = '';
        }

        const badge = s.getElementById('badge');
        if (color) {
            const bg = 'rgb(' + color + ')';
            badge.style.background = bg;
            badge.removeAttribute('hidden');
            // sync color input value
            const parts = color.split(',').map(Number);
            if (parts.length === 3) {
                const hex = '#' + parts.map(v => v.toString(16).padStart(2, '0')).join('');
                this._colorInput.value = hex;
            }
        } else {
            badge.setAttribute('hidden', '');
        }

        s.getElementById('code').textContent = idx + ': ' + code;
        s.getElementById('name').textContent = this.getAttribute('tile-name') || '';
        s.getElementById('path').textContent = this.getAttribute('tile-path') || '';
    }
}

customElements.define('tile-item', TileItem);


// ── W3E application logic ────────────────────────────────────────────
window.W3E = (function () {
    const _gamePathHandlers = [];

    function esc(s) {
        return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    }

    function indexToRgb(index) {
        const golden = 137.508;
        const hue = (index * golden) % 360;
        const sat = 0.55 + 0.15 * ((index % 3) / 2);
        const lum = 0.45 + 0.10 * ((index % 5) / 4);
        const cc = (1 - Math.abs(2 * lum - 1)) * sat;
        const xx = cc * (1 - Math.abs((hue / 60 % 2) - 1));
        const mm = lum - cc / 2;
        let r, g, b;
        if (hue < 60) { r = cc; g = xx; b = 0; }
        else if (hue < 120) { r = xx; g = cc; b = 0; }
        else if (hue < 180) { r = 0; g = cc; b = xx; }
        else if (hue < 240) { r = 0; g = xx; b = cc; }
        else if (hue < 300) { r = xx; g = 0; b = cc; }
        else { r = cc; g = 0; b = xx; }
        return [Math.round((r + mm) * 255), Math.round((g + mm) * 255), Math.round((b + mm) * 255)];
    }

    function onGamePathChanged(fn) { _gamePathHandlers.push(fn); }

    function syncMenuActive() {
        document.querySelectorAll('[data-action="toggleWindow"]').forEach(btn => {
            const target = btn.getAttribute('data-target');
            if (!target) return;
            const win = document.getElementById(target);
            btn.classList.toggle('active', !!(win && win.open));
        });
    }

    // ── Game Path body builder ────────────────────────────────
    const REQUIRED_MPQ = ['War3.mpq', 'War3x.mpq', 'War3xLocal.mpq', 'War3Patch.mpq'];

    function renderGpBody(status) {
        const gp = status.gamePath || '';
        const has = !!gp;
        let h = '<div class="gp-hint">Path to Warcraft III installation folder.</div>';
        h += has
            ? '<div class="gp-path">' + esc(gp) + '</div>'
            : '<div class="gp-no-path">Not selected</div>';
        if (has && status.mpqStatus) {
            h += '<div class="gp-mpq-list">';
            for (const f of REQUIRED_MPQ) {
                const ok = status.mpqStatus[f];
                h += '<div class="gp-mpq-row ' + (ok ? 'gp-ok' : 'gp-missing') + '">'
                    + '<span>' + (ok ? '\u2705' : '\u274c') + '</span> '
                    + '<span>' + esc(f) + '</span></div>';
            }
            h += '</div>';
        }
        h += '<div class="gp-actions">'
            + '<button class="gp-browse" id="gamePathBrowse">\ud83d\udcc2 Browse\u2026</button>';
        if (has) h += '<button class="gp-clear" id="gamePathClear">\u2715 Clear</button>';
        h += '</div>';
        return h;
    }

    // ── Tileset rebuilder ────────────────────────────────────
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
                srcEl.innerHTML = 'Terrain.slk: <span class="code">' + esc(source) + '</span>';
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
                const rgb = indexToRgb(i);
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

    // ── init() — main entry point ────────────────────────────
    function init(config) {
        const vscode = config.vscode;
        const groundTileCodes = config.groundTileCodes || [];
        const cliffTileCodes = config.cliffTileCodes || [];
        const isArchive = !!config.isArchive;

        // ── Menu sync ────────────────────────────────────────
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

        // ── Game Path ────────────────────────────────────────
        function bindGpButtons() {
            const b = document.getElementById('gamePathBrowse');
            if (b && vscode) b.addEventListener('click', () => vscode.postMessage({command: 'browseGamePath'}));
            const c = document.getElementById('gamePathClear');
            if (c && vscode) c.addEventListener('click', () => vscode.postMessage({command: 'setGamePath', value: ''}));
        }

        bindGpButtons();

        onGamePathChanged(data => {
            if (!data.status) return;
            const gpBody = document.getElementById('gpBody');
            if (!gpBody) return;
            gpBody.innerHTML = renderGpBody(data.status);
            bindGpButtons();
        });

        // ── Tileset ──────────────────────────────────────────
        onGamePathChanged(data => rebuildTileset(data.terrainSlk, groundTileCodes, cliffTileCodes));

        // ── Message router ───────────────────────────────────
        window.addEventListener('message', e => {
            const msg = e.data;
            if (msg && msg.command === 'gamePathChanged') {
                for (const fn of _gamePathHandlers) fn(msg);
            }
        });

        // ── Archive file interactions ────────────────────────
        if (isArchive && vscode) {
            document.querySelectorAll('.file-row').forEach(row => {
                row.addEventListener('click', () => {
                    const name = row.dataset.name;
                    if (!name) return;
                    if (name.replace(/\\/g, '/').toLowerCase() === 'war3map.w3e') {
                        const tw = document.getElementById('terrainWindow');
                        if (tw) { tw.show(); return; }
                    }
                    vscode.postMessage({command: 'openFile', name});
                });
            });

            const browseBtn = document.getElementById('browseBtn');
            if (browseBtn) {
                browseBtn.addEventListener('click', () => vscode.postMessage({command: 'browse'}));
            }

            document.querySelectorAll('.folder-row').forEach(row => {
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

    return {init, onGamePathChanged, indexToRgb, syncMenuActive};
})();

