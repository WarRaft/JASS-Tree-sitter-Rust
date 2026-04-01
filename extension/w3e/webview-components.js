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

    // Dynamic bottom-bar height: reads the actual rendered height of #cursor-info
    get _BOTTOM_BAR() {
        const el = document.getElementById('cursor-info');
        return el ? el.offsetHeight : 0;
    }

    constructor() {
        super();

        const shadow = this.attachShadow({mode: 'open'});
        shadow.innerHTML = `
<style>
:host {
    position: absolute;
    z-index: 10;
    min-width: 200px;
    min-height: 120px;
    max-height: calc(100vh - 65px);
    background: rgba(37, 37, 38, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    display: flex;
    flex-direction: column;
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
    flex-shrink: 0;
}
.title:active { cursor: grabbing; }
.title-label { flex: 1; }
.title-right { display: flex; align-items: center; gap: 2px; }
.close, .maximize {
    background: none; border: none;
    color: var(--vscode-editor-foreground, #ccc);
    cursor: pointer; font-size: 14px; line-height: 1;
    padding: 0 4px; border-radius: 3px; opacity: 0.6;
}
.close:hover, .maximize:hover { opacity: 1; background: rgba(255, 255, 255, 0.1); }
.maximize { font-size: 12px; }
:host([maximized]) {
    border-radius: 0;
    max-height: none;
}
:host([maximized]) .resize-grip { display: none; }
.body {
    padding: 10px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
}
:host([no-padding]) .body { padding: 0; }
.body::-webkit-scrollbar { width: 6px; }
.body::-webkit-scrollbar-track { background: transparent; }
.body::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.15); border-radius: 3px; }
.body::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,0.25); }
.resize-grip {
    position: absolute;
    right: 0; bottom: 0;
    width: 16px; height: 16px;
    cursor: nwse-resize;
    z-index: 1;
}
.resize-grip::after {
    content: '';
    position: absolute;
    right: 3px; bottom: 3px;
    width: 8px; height: 8px;
    border-right: 2px solid rgba(255,255,255,0.25);
    border-bottom: 2px solid rgba(255,255,255,0.25);
    border-radius: 0 0 2px 0;
}
.resize-grip:hover::after {
    border-color: rgba(255,255,255,0.5);
}
.loading-bar {
    height: 0; overflow: hidden; position: relative; flex-shrink: 0;
}
:host([loading]) .loading-bar { height: 2px; }
.loading-bar::after {
    content: '';
    position: absolute; top: 0; left: -40%;
    width: 40%; height: 100%;
    background: var(--vscode-progressBar-background, #0e70c0);
    animation: loading-slide 1.2s ease-in-out infinite;
}
@keyframes loading-slide {
    0% { left: -40%; }
    100% { left: 100%; }
}
</style>
<div class="title" id="titleBar">
    <span class="title-label" id="titleText"></span>
    <div class="title-right">
        <slot name="actions"></slot>
        <button class="maximize" id="maxBtn" title="Maximize">\u25a1</button>
        <button class="close" id="closeBtn">\u00d7</button>
    </div>
</div>
<div class="loading-bar"></div>
<div class="body"><slot></slot></div>
<div class="resize-grip" id="resizeGrip"></div>`;

        this._titleEl = shadow.getElementById('titleText');
        this._titleBar = shadow.getElementById('titleBar');
        this._resizeGrip = shadow.getElementById('resizeGrip');
        this._maxBtn = shadow.getElementById('maxBtn');
        this._savedRect = null; // saved geometry before maximize

        // Close
        shadow.getElementById('closeBtn').addEventListener('click', () => this.hide());

        // Maximize / Restore
        this._maxBtn.addEventListener('click', () => this._toggleMaximize());

        // Double-click title bar to maximize/restore
        this._titleBar.addEventListener('dblclick', (e) => {
            if (e.target.closest('button')) return;
            this._toggleMaximize();
        });

        // Drag
        let dragging = false, sx = 0, sy = 0, ox = 0, oy = 0;
        this._titleBar.addEventListener('pointerdown', e => {
            if (e.target.closest('button')) return;
            if (this.hasAttribute('maximized')) return;
            dragging = true;
            sx = e.clientX; sy = e.clientY;
            ox = this.offsetLeft; oy = this.offsetTop;
            this._titleBar.setPointerCapture(e.pointerId);
            e.preventDefault();
            this._bringToFront();
        });
        this._titleBar.addEventListener('pointermove', e => {
            if (!dragging) return;
            let newLeft = ox + e.clientX - sx;
            let newTop = oy + e.clientY - sy;
            const vb = window.innerHeight - this._BOTTOM_BAR;
            const vw = window.innerWidth;
            newTop = Math.max(0, Math.min(newTop, vb - 40));
            newLeft = Math.max(40 - this.offsetWidth, Math.min(newLeft, vw - 40));
            this.style.left = newLeft + 'px';
            this.style.top = newTop + 'px';
            this.style.right = 'auto';
        });
        this._titleBar.addEventListener('pointerup', e => {
            dragging = false;
            this._titleBar.releasePointerCapture(e.pointerId);
            this._clampToViewport();
        });

        // Resize
        let resizing = false, rsx = 0, rsy = 0, rw = 0, rh = 0;
        this._resizeGrip.addEventListener('pointerdown', e => {
            if (this.hasAttribute('maximized')) return;
            resizing = true;
            rsx = e.clientX; rsy = e.clientY;
            rw = this.offsetWidth; rh = this.offsetHeight;
            this._resizeGrip.setPointerCapture(e.pointerId);
            e.preventDefault();
            e.stopPropagation();
            this._bringToFront();
        });
        this._resizeGrip.addEventListener('pointermove', e => {
            if (!resizing) return;
            const vb = window.innerHeight - this._BOTTOM_BAR;
            const vw = window.innerWidth;
            const elLeft = this.offsetLeft;
            const elTop = this.offsetTop;
            const maxW = Math.max(200, vw - elLeft);
            const maxH = Math.max(120, vb - elTop);
            const newW = Math.min(maxW, Math.max(200, rw + e.clientX - rsx));
            const newH = Math.min(maxH, Math.max(120, rh + e.clientY - rsy));
            this.style.width = newW + 'px';
            this.style.height = newH + 'px';
        });
        this._resizeGrip.addEventListener('pointerup', e => {
            resizing = false;
            this._resizeGrip.releasePointerCapture(e.pointerId);
        });
    }

    attributeChangedCallback(name, old, val) {
        if (name === 'title-text') this._titleEl.textContent = val || '';
    }

    get open() { return !this.hasAttribute('hidden'); }

    get loading() { return this.hasAttribute('loading'); }
    set loading(v) {
        if (v) this.setAttribute('loading', '');
        else this.removeAttribute('loading');
    }

    toggle() { if (this.open) this.hide(); else this.show(); }

    show() {
        this.removeAttribute('hidden');
        this._bringToFront();
        if (this.hasAttribute('maximized')) {
            // Re-apply maximize geometry (viewport may have changed)
            const menubar = document.getElementById('menubar');
            const menuW = menubar ? menubar.offsetWidth : 0;
            const vb = window.innerHeight - this._BOTTOM_BAR;
            this.style.left = menuW + 'px';
            this.style.top = '0px';
            this.style.right = 'auto';
            this.style.width = (window.innerWidth - menuW) + 'px';
            this.style.height = vb + 'px';
        } else {
            this._clampToViewport();
        }
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

    _clampToViewport() {
        if (this.hasAttribute('maximized')) return;
        const vb = window.innerHeight - this._BOTTOM_BAR;
        const vw = window.innerWidth;
        const rect = this.getBoundingClientRect();

        // Clamp bottom edge (above bottom bar)
        if (rect.bottom > vb) {
            const maxH = vb - rect.top;
            if (maxH >= 120) {
                this.style.height = maxH + 'px';
            } else {
                this.style.top = Math.max(0, vb - 120) + 'px';
                this.style.height = Math.min(120, vb) + 'px';
            }
        }

        // Clamp right edge
        if (rect.right > vw) {
            const maxW = vw - rect.left;
            if (maxW >= 200) {
                this.style.width = maxW + 'px';
            } else {
                this.style.left = Math.max(0, vw - 200) + 'px';
                this.style.width = Math.min(200, vw) + 'px';
                this.style.right = 'auto';
            }
        }
    }

    _toggleMaximize() {
        if (this.hasAttribute('maximized')) {
            this._restoreFromMaximize();
        } else {
            this._applyMaximize();
        }
    }

    _applyMaximize() {
        // Save current geometry
        this._savedRect = {
            left: this.style.left,
            top: this.style.top,
            right: this.style.right,
            width: this.style.width,
            height: this.style.height,
        };
        // Compute menubar width
        const menubar = document.getElementById('menubar');
        const menuW = menubar ? menubar.offsetWidth : 0;
        const vb = window.innerHeight - this._BOTTOM_BAR;
        this.style.left = menuW + 'px';
        this.style.top = '0px';
        this.style.right = 'auto';
        this.style.width = (window.innerWidth - menuW) + 'px';
        this.style.height = vb + 'px';
        this.setAttribute('maximized', '');
        this._maxBtn.textContent = '\u29c9'; // ⧉ restore icon
        this._maxBtn.title = 'Restore';
        this._bringToFront();
    }

    _restoreFromMaximize() {
        this.removeAttribute('maximized');
        if (this._savedRect) {
            this.style.left = this._savedRect.left;
            this.style.top = this._savedRect.top;
            this.style.right = this._savedRect.right;
            this.style.width = this._savedRect.width;
            this.style.height = this._savedRect.height;
            this._savedRect = null;
        }
        this._maxBtn.textContent = '\u25a1'; // □ maximize icon
        this._maxBtn.title = 'Maximize';
        this._clampToViewport();
    }

    _notifyToggle() {
        document.dispatchEvent(new CustomEvent('float-toggled', {detail: {id: this.id}}));
    }
}

customElements.define('float-window', FloatWindow);


// ── <reload-button> Custom Element ───────────────────────────────────
// Usage:
//   <reload-button slot="actions"></reload-button>
// API: .loading (getter/setter)
// Fires: 'reload' event (bubbles, composed) on click when not loading.

class ReloadButton extends HTMLElement {
    constructor() {
        super();
        const shadow = this.attachShadow({mode: 'open'});
        shadow.innerHTML = `
<style>
:host { display: inline-flex; align-items: center; }
button {
    background: none; border: none;
    color: var(--vscode-editor-foreground, #ccc);
    cursor: pointer; font-size: 14px; line-height: 1;
    padding: 0 4px; border-radius: 3px; opacity: 0.6;
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
}
button:hover { opacity: 1; background: rgba(255, 255, 255, 0.1); }
button[disabled] { cursor: default; opacity: 0.3; pointer-events: none; }
.icon { display: inline-block; }
@keyframes spin { to { transform: rotate(360deg); } }
:host([loading]) .icon { animation: spin 0.8s linear infinite; }
</style>
<button id="btn" title="Reload"><span class="icon">\u27f3</span></button>`;

        this._btn = shadow.getElementById('btn');
        this._btn.addEventListener('click', () => {
            if (this.loading) return;
            this.dispatchEvent(new CustomEvent('reload', {bubbles: true, composed: true}));
        });
    }

    get loading() { return this.hasAttribute('loading'); }
    set loading(v) {
        if (v) this.setAttribute('loading', '');
        else this.removeAttribute('loading');
        this._btn.disabled = !!v;
    }
}

customElements.define('reload-button', ReloadButton);


// ── <collapse-group> Custom Element ─────────────────────────────────
// Usage:
//   <collapse-group group-title="🏷 Title" open>…content…</collapse-group>
// API: standard <details> open attribute via reflection.

class CollapseGroup extends HTMLElement {
    static get observedAttributes() { return ['group-title', 'open']; }

    constructor() {
        super();
        const s = this.attachShadow({mode: 'open'});
        s.innerHTML = `
<style>
:host {
    display: block;
    margin-bottom: 2px;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 4px;
    overflow: hidden;
}
:host + :host { margin-top: 2px; }
summary {
    font-size: 11px;
    font-weight: 600;
    padding: 4px 8px;
    cursor: pointer;
    color: var(--vscode-foreground, #ccc);
    background: rgba(255, 255, 255, 0.04);
    user-select: none;
    list-style: none;
    display: flex;
    align-items: center;
}
summary::-webkit-details-marker { display: none; }
summary::before {
    content: '▶';
    display: inline-block;
    width: 12px;
    font-size: 9px;
    margin-right: 4px;
    transition: transform 0.15s;
    flex-shrink: 0;
}
details[open] > summary::before { transform: rotate(90deg); }
summary:hover { background: rgba(255, 255, 255, 0.08); }
.body {
    padding: 2px 8px 4px;
}
</style>
<details id="d">
    <summary id="s"></summary>
    <div class="body"><slot></slot></div>
</details>`;
        this._details = s.getElementById('d');
        this._summary = s.getElementById('s');
    }

    connectedCallback() {
        this._sync();
        this._details.addEventListener('toggle', () => {
            this.dispatchEvent(new CustomEvent('collapse-toggle', {
                bubbles: true,
                detail: {title: this.getAttribute('group-title') || '', open: this._details.open}
            }));
        });
    }
    attributeChangedCallback() { this._sync(); }

    _sync() {
        this._summary.textContent = this.getAttribute('group-title') || '';
        this._details.open = this.hasAttribute('open');
    }

    get open() { return this._details.open; }
    set open(v) {
        if (v) this.setAttribute('open', '');
        else this.removeAttribute('open');
    }
}

customElements.define('collapse-group', CollapseGroup);


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

// ── Doodad / tileset label constants ─────────────────────────────────

const TILESET_NAMES = {
    A: 'Ashenvale', B: 'Barrens', K: 'Black Citadel', Y: 'Cityscape',
    X: 'Dalaran', J: 'Dalaran Ruins', D: 'Dungeon', C: 'Felwood',
    I: 'Icecrown Glacier', F: 'Lordaeron Fall', L: 'Lordaeron Summer',
    W: 'Lordaeron Winter', N: 'Northrend', O: 'Outland',
    Z: 'Sunken Ruins', G: 'Underground', V: 'Village', Q: 'Village Fall',
};

const DOODAD_CATEGORIES = {
    C: 'Cliffs/Terrain',
    E: 'Environment',
    O: 'Props',
    S: 'Structures',
    W: 'Water',
    Z: 'Cinematic',
};

// ── Doodad item ──────────────────────────────────────────────────────

class DoodadItem extends HTMLElement {
    constructor() {
        super();
        const s = this.attachShadow({mode: 'open'});
        s.innerHTML = `
        <style>
            :host {
                display: flex;
                align-items: center;
                gap: 6px;
                padding: 3px 6px;
                font-size: 12px;
                border-bottom: 1px solid var(--vscode-editorWidget-border, #333);
                cursor: pointer;
            }
            :host(:hover) {
                background: var(--vscode-list-hoverBackground, rgba(255,255,255,.06));
            }
            .id { font-family: var(--vscode-editor-font-family, monospace); color: var(--vscode-textLink-foreground, #3794ff); min-width: 40px; flex-shrink: 0; }
            .name { flex: 1; color: var(--vscode-foreground, #ccc); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
            .tilesets { display: flex; gap: 2px; flex-shrink: 0; flex-wrap: nowrap; }
            .ts-badge {
                display: inline-block;
                width: 16px;
                height: 16px;
                line-height: 16px;
                text-align: center;
                font-size: 10px;
                font-family: var(--vscode-editor-font-family, monospace);
                font-weight: 600;
                border-radius: 3px;
                background: rgba(255, 255, 255, 0.08);
                color: var(--vscode-descriptionForeground, #999);
                flex-shrink: 0;
            }
            .ts-badge.ts-all {
                background: rgba(78, 154, 241, 0.25);
                color: var(--vscode-textLink-foreground, #3794ff);
                width: 16px;
            }
            .cat { color: var(--vscode-descriptionForeground, #888); font-size: 11px; flex-shrink: 0; min-width: 80px; text-align: right; padding-right: 4px; }
        </style>
        <span class="id" id="doodId"></span>
        <span class="name" id="name"></span>
        <span class="tilesets" id="tilesets"></span>
        <span class="cat" id="cat"></span>`;
    }

    connectedCallback() { this._render(); }
    static get observedAttributes() { return ['dood-id', 'dood-name', 'category', 'tilesets']; }
    attributeChangedCallback() { if (this.shadowRoot) this._render(); }

    _render() {
        const s = this.shadowRoot;
        s.getElementById('doodId').textContent = this.getAttribute('dood-id') || '';
        const name = this.getAttribute('dood-name') || '';
        const nameEl = s.getElementById('name');
        nameEl.textContent = name;
        nameEl.title = this.getAttribute('comment') || '';
        const cat = this.getAttribute('category') || '';
        const catEl = s.getElementById('cat');
        const catLabel = (typeof DOODAD_CATEGORIES !== 'undefined' && DOODAD_CATEGORIES[cat]) || cat;
        catEl.textContent = catLabel;
        catEl.title = cat ? cat + ' \u2014 ' + catLabel : '';

        // Tileset badges
        const ts = this.getAttribute('tilesets') || '';
        const tsEl = s.getElementById('tilesets');
        tsEl.innerHTML = '';
        if (ts === '*') {
            const badge = document.createElement('span');
            badge.className = 'ts-badge ts-all';
            badge.textContent = '*';
            badge.title = 'All tilesets';
            tsEl.appendChild(badge);
        } else if (ts) {
            const chars = ts.split(',').filter(Boolean);
            for (const ch of chars) {
                const badge = document.createElement('span');
                badge.className = 'ts-badge';
                badge.textContent = ch;
                if (typeof TILESET_NAMES !== 'undefined' && TILESET_NAMES[ch]) {
                    badge.title = TILESET_NAMES[ch];
                }
                tsEl.appendChild(badge);
            }
        }
    }
}

customElements.define('doodad-item', DoodadItem);

// ── Unit item ────────────────────────────────────────────────────────

class UnitItem extends HTMLElement {
    constructor() {
        super();
        const s = this.attachShadow({mode: 'open'});
        s.innerHTML = `
        <style>
            :host {
                display: flex;
                align-items: baseline;
                gap: 6px;
                padding: 3px 6px;
                font-size: 12px;
                border-bottom: 1px solid var(--vscode-editorWidget-border, #333);
                cursor: pointer;
            }
            :host(:hover) {
                background: var(--vscode-list-hoverBackground, rgba(255,255,255,.06));
            }
            .id { font-family: var(--vscode-editor-font-family, monospace); color: var(--vscode-textLink-foreground, #3794ff); min-width: 40px; }
            .comment { flex: 1; color: var(--vscode-foreground, #ccc); }
            .race { color: var(--vscode-descriptionForeground, #888); font-size: 11px; min-width: 50px; }
            .move { color: var(--vscode-descriptionForeground, #888); font-size: 11px; min-width: 36px; }
            .pts { color: var(--vscode-descriptionForeground, #888); font-size: 11px; font-family: var(--vscode-editor-font-family, monospace); min-width: 30px; text-align: right; }
            .file-link {
                color: var(--vscode-textLink-foreground, #3794ff);
                font-size: 10px;
                font-family: var(--vscode-editor-font-family, monospace);
                opacity: 0.7;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
                max-width: 200px;
            }
            .file-link:hover { opacity: 1; text-decoration: underline; }
        </style>
        <span class="id" id="unitId"></span>
        <span class="comment" id="comment"></span>
        <span class="race" id="race"></span>
        <span class="move" id="move"></span>
        <span class="pts" id="pts"></span>
        <span class="file-link" id="fileLink"></span>`;
    }

    connectedCallback() { this._render(); }
    static get observedAttributes() { return ['unit-id', 'comment', 'race', 'move-tp', 'points', 'file']; }
    attributeChangedCallback() { if (this.shadowRoot) this._render(); }

    _render() {
        const s = this.shadowRoot;
        s.getElementById('unitId').textContent = this.getAttribute('unit-id') || '';
        s.getElementById('comment').textContent = this.getAttribute('comment') || '';
        const race = this.getAttribute('race') || '';
        s.getElementById('race').textContent = race;
        s.getElementById('move').textContent = this.getAttribute('move-tp') || '';
        const pts = this.getAttribute('points') || '';
        s.getElementById('pts').textContent = pts && pts !== '0' ? pts + 'pt' : '';
        const file = this.getAttribute('file') || '';
        const fileLink = s.getElementById('fileLink');
        if (file) {
            const shortName = file.replace(/\\/g, '/').split('/').pop() || file;
            fileLink.textContent = shortName;
            fileLink.title = file;
        } else {
            fileLink.textContent = '';
        }
    }
}

customElements.define('unit-item', UnitItem);


// ── W3E application logic ────────────────────────────────────────────
window.W3E = (function () {
    const _gamePathHandlers = [];
    let _vscode = null;

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

    // ── Doodads SLK rebuilder ────────────────────────────────
    // Stores the full doodad array for the detail window.
    let _doodadDataMap = {};
    let _allDoodads = [];

    // ── Doodads state persistence helpers ─────────────────────
    function _getWvState() {
        if (!_vscode) return {};
        try { return _vscode.getState() || {}; } catch (_) { return {}; }
    }

    function _patchWvState(patch) {
        if (!_vscode) return;
        try {
            const s = _getWvState();
            Object.assign(s, patch);
            _vscode.setState(s);
        } catch (_) { /* ignore */ }
    }

    function _saveDoodSort() {
        _patchWvState({_doodSort: {field: _doodSort.field, dir: _doodSort.dir}});
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
        _patchWvState({_doodUncheckedCats: uncheckedCats, _doodUncheckedTs: uncheckedTs});
    }

    function _restoreDoodFilters() {
        const s = _getWvState();
        const uncheckedCats = s._doodUncheckedCats || [];
        const uncheckedTs = s._doodUncheckedTs || [];
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
    }

    function _restoreDoodSort() {
        const s = _getWvState();
        if (s._doodSort && s._doodSort.field) {
            _doodSort = {field: s._doodSort.field, dir: s._doodSort.dir || 'asc'};
        }
    }

    // Sort state: { field: 'doodId'|'name'|'category'|null, dir: 'asc'|'desc' }
    let _doodSort = {field: null, dir: 'asc'};

    function _cycleDoodSort(field) {
        if (_doodSort.field !== field) {
            // First click on a new field → asc
            _doodSort = {field, dir: 'asc'};
        } else if (_doodSort.dir === 'asc') {
            // Second click → desc
            _doodSort.dir = 'desc';
        } else {
            // Third click → off
            _doodSort = {field: null, dir: 'asc'};
        }
        _saveDoodSort();
        _updateSortButtons();
        _filterAndRenderDoodads();
    }

    function _updateSortButtons() {
        document.querySelectorAll('.ds-sort-col').forEach(btn => {
            const f = btn.getAttribute('data-sort');
            btn.classList.remove('ds-sort-active', 'ds-sort-asc', 'ds-sort-desc');
            if (f === _doodSort.field) {
                btn.classList.add('ds-sort-active', _doodSort.dir === 'asc' ? 'ds-sort-asc' : 'ds-sort-desc');
            }
        });
    }

    function _filterAndRenderDoodads(saveState) {
        // Collect enabled categories
        const enabledCats = new Set();
        document.querySelectorAll('.ds-cat-cb').forEach(cb => {
            if (cb.checked) enabledCats.add(cb.getAttribute('data-cat'));
        });
        // Collect enabled tilesets
        const enabledTs = new Set();
        document.querySelectorAll('.ds-ts-cb').forEach(cb => {
            if (cb.checked) enabledTs.add(cb.getAttribute('data-ts'));
        });

        // Persist filter state when triggered by user action
        if (saveState !== false) _saveDoodFilters();

        // Search text
        const searchEl = document.getElementById('dsSearchInput');
        const q = searchEl ? searchEl.value.toLowerCase().trim() : '';

        const filtered = _allDoodads.filter(d => {
            // Search filter: match name or rawcode
            if (q) {
                const name = (d.name || '').toLowerCase();
                const id = (d.doodId || '').toLowerCase();
                const comment = (d.comment || '').toLowerCase();
                if (!name.includes(q) && !id.includes(q) && !comment.includes(q)) return false;
            }
            // Category filter
            if (d.category && !enabledCats.has(d.category)) return false;
            // Tileset filter: show if '*' or if at least one tileset char is enabled
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

        // Apply sorting
        if (_doodSort.field) {
            const f = _doodSort.field;
            const mul = _doodSort.dir === 'desc' ? -1 : 1;
            filtered.sort((a, b) => {
                const va = (a[f] || '').toLowerCase();
                const vb = (b[f] || '').toLowerCase();
                return va < vb ? -1 * mul : va > vb ? 1 * mul : 0;
            });
        }

        const listEl = document.getElementById('dsDoodadList');
        if (listEl) {
            listEl.innerHTML = '';
            for (const d of filtered) {
                const el = document.createElement('doodad-item');
                el.setAttribute('dood-id', d.doodId || '');
                el.setAttribute('dood-name', d.name || '');
                el.setAttribute('comment', d.comment || '');
                el.setAttribute('category', d.category || '');
                el.setAttribute('tilesets', d.tilesets || '');
                listEl.appendChild(el);
            }
        }

        const cntEl = document.getElementById('dsDoodadCount');
        if (cntEl) cntEl.textContent = String(filtered.length);
    }

    function _rebuildDoodadSidebarCheckboxes() {
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
                cb.addEventListener('change', _filterAndRenderDoodads);
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
                cb.addEventListener('change', _filterAndRenderDoodads);
                lbl.appendChild(cb);
                const badge = document.createElement('span');
                badge.className = 'ds-ts-badge';
                badge.textContent = code;
                lbl.appendChild(badge);
                lbl.appendChild(document.createTextNode(' ' + label));
                tsChecks.appendChild(lbl);
            }
        }
        _restoreDoodFilters();
    }

    function rebuildDoodads(slkData) {
        let source = '';
        _allDoodads = [];
        _doodadDataMap = {};
        if (slkData && slkData.doodads) {
            source = slkData.source || '';
            _allDoodads = slkData.doodads;
        }

        // Build lookup map
        for (const d of _allDoodads) {
            _doodadDataMap[d.doodId] = d;
        }

        const srcEl = document.getElementById('dsSlkSource');
        if (srcEl) {
            if (source) {
                srcEl.className = 'ts-source';
                srcEl.innerHTML = 'Doodads.slk: <span class="code">' + esc(source) + '</span>';
            } else {
                srcEl.className = 'ts-source ts-no-slk';
                srcEl.textContent = 'Doodads.slk not found \u2014 set Game Path';
            }
        }

        const totalEl = document.getElementById('dsDoodadTotal');
        if (totalEl) totalEl.textContent = String(_allDoodads.length);

        _rebuildDoodadSidebarCheckboxes();
        _restoreDoodSort();
        _updateSortButtons();
        _filterAndRenderDoodads(false);

        // Bind search input
        const searchEl = document.getElementById('dsSearchInput');
        if (searchEl && !searchEl._dsBound) {
            searchEl._dsBound = true;
            searchEl.addEventListener('input', _filterAndRenderDoodads);
        }

        // Bind sort column headers
        document.querySelectorAll('.ds-sort-col').forEach(btn => {
            if (btn._dsSortBound) return;
            btn._dsSortBound = true;
            btn.addEventListener('click', () => _cycleDoodSort(btn.getAttribute('data-sort')));
        });
    }

    // ── Doodad detail window populator ────────────────────────

    // Field grouping definition for doodad detail view.
    const _DOOD_GROUPS = [
        {
            title: '🏷 Identity', fields: [
                'doodID', 'Name', 'comment', 'category', 'doodClass',
                'tilesets', 'tilesetSpecific',
            ]
        },
        {
            title: '🎨 Model', modelFiles: true, fields: [
                'soundLoop',
            ]
        },
        {
            title: '📐 Scale', fields: [
                'defScale', 'minScale', 'maxScale', 'canPlaceRandScale',
            ]
        },
        {
            title: '📍 Placement', fields: [
                'onCliffs', 'onWater', 'floats', 'walkable', 'fixedRot',
                'maxPitch', 'maxRoll', 'pathTex',
            ]
        },
        {
            title: '👆 Interaction', fields: [
                'selSize', 'useClickHelper', 'ignoreModelClick', 'visRadius',
            ]
        },
        {
            title: '👁 Rendering', fields: [
                'shadow', 'showInFog', 'animInFog',
            ]
        },
        {
            title: '🗺 Minimap', fields: [
                'showInMM', 'useMMColor',
            ],
            color: {r: 'MMRed', g: 'MMGreen', b: 'MMBlue', label: 'Color'},
        },
        {
            title: '🌈 Vertex Colors', vertexColors: true,
        },
        {
            title: 'ℹ Meta', fields: [
                'InBeta', 'version',
            ]
        },
    ];

    function _colorBadge(r, g, b) {
        return '<span class="dd-color-badge" style="background:rgb(' + r + ',' + g + ',' + b + ')" title="rgb(' + r + ',' + g + ',' + b + ')"></span>';
    }

    // Collapsed state persistence for doodad detail view
    function _getDoodCollapseState() {
        const s = _getWvState();
        return s._doodCollapse || {};
    }

    function _setDoodCollapseState(state) {
        _patchWvState({_doodCollapse: state});
    }

    /**
     * Build model file paths based on the base path and number of variations.
     * numVar=1 → [basePath.mdx] (appends .mdx if no extension)
     * numVar>1 → [basePath0.mdx, basePath1.mdx, …]
     */
    function _buildModelPaths(filePath, numVar) {
        // Detect whether the path already has a file extension
        const lastSlash = Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\'));
        const dotIdx = filePath.lastIndexOf('.');
        const hasExt = dotIdx > lastSlash && dotIdx >= 0;

        const base = hasExt ? filePath.substring(0, dotIdx) : filePath;
        const ext = hasExt ? filePath.substring(dotIdx) : '.mdx';

        if (numVar <= 1) return [base + ext];

        const paths = [];
        for (let i = 0; i < numVar; i++) {
            paths.push(base + i + ext);
        }
        return paths;
    }

    function showDoodadDetail(doodId) {
        const d = _doodadDataMap[doodId];
        if (!d) return;

        const win = document.getElementById('doodadDetailWindow');
        if (!win) return;

        const body = document.getElementById('doodadDetailBody');
        if (!body) return;

        const raw = d.raw || {};
        const used = new Set();
        let html = '';
        const collapseState = _getDoodCollapseState();

        for (const group of _DOOD_GROUPS) {
            let rows = '';

            if (group.vertexColors) {
                // Collect vertex color variations 01..10
                for (let i = 1; i <= 10; i++) {
                    const idx = String(i).padStart(2, '0');
                    const rk = 'vertR' + idx, gk = 'vertG' + idx, bk = 'vertB' + idx;
                    used.add(rk); used.add(gk); used.add(bk);
                    if (raw[rk] === undefined && raw[gk] === undefined && raw[bk] === undefined) continue;
                    const rv = raw[rk] ?? '255', gv = raw[gk] ?? '255', bv = raw[bk] ?? '255';
                    rows += '<tr><td class="key">Variation ' + idx + '</td><td>'
                        + esc(rv) + ',' + esc(gv) + ',' + esc(bv) + ' '
                        + _colorBadge(rv, gv, bv) + '</td></tr>';
                }
            } else if (group.modelFiles) {
                // Model files with variation links
                used.add('file'); used.add('numVar');
                const filePath = raw.file;
                const numVar = parseInt(raw.numVar, 10) || 1;
                if (filePath) {
                    const paths = _buildModelPaths(filePath, numVar);
                    const links = paths.map(p =>
                        '<a href="#" class="dd-model-link" data-path="' + esc(p) + '">' + esc(p) + '</a>'
                    ).join('');
                    rows += '<tr><td class="key">file</td><td>' + links + '</td></tr>';
                }
                rows += '<tr><td class="key">numVar</td><td>' + esc(String(raw.numVar ?? '')) + '</td></tr>';
                // Regular fields in this group
                if (group.fields) {
                    for (const k of group.fields) {
                        used.add(k);
                        if (raw[k] === undefined) continue;
                        rows += '<tr><td class="key">' + esc(k) + '</td><td>' + esc(String(raw[k] ?? '')) + '</td></tr>';
                    }
                }
            } else {
                // Regular fields
                if (group.fields) {
                    for (const k of group.fields) {
                        used.add(k);
                        if (raw[k] === undefined) continue;
                        let val = esc(String(raw[k] ?? ''));
                        // Decode category code
                        if (k === 'category' && raw[k] && DOODAD_CATEGORIES[raw[k]]) {
                            val = esc(raw[k]) + ' — ' + esc(DOODAD_CATEGORIES[raw[k]]);
                        }
                        // Decode tileset codes
                        if (k === 'tilesets' && raw[k]) {
                            const ts = raw[k];
                            if (ts === '*') {
                                val = '* — All';
                            } else {
                                const decoded = ts.replace(/,/g, '').split('').map(function (ch) {
                                    return TILESET_NAMES[ch] ? ch + ' ' + TILESET_NAMES[ch] : ch;
                                }).join(', ');
                                val = esc(decoded);
                            }
                        }
                        rows += '<tr><td class="key">' + esc(k) + '</td><td>' + val + '</td></tr>';
                    }
                }
                // Single color row (e.g. minimap)
                if (group.color) {
                    const c = group.color;
                    used.add(c.r); used.add(c.g); used.add(c.b);
                    const rv = raw[c.r], gv = raw[c.g], bv = raw[c.b];
                    if (rv !== undefined || gv !== undefined || bv !== undefined) {
                        const rn = rv ?? '0', gn = gv ?? '0', bn = bv ?? '0';
                        rows += '<tr><td class="key">' + esc(c.label) + '</td><td>'
                            + esc(rn) + ',' + esc(gn) + ',' + esc(bn) + ' '
                            + _colorBadge(rn, gn, bn) + '</td></tr>';
                    }
                }
            }

            if (!rows) continue;

            // Default: open; respect saved state if present
            const isOpen = collapseState.hasOwnProperty(group.title) ? collapseState[group.title] : true;
            html += '<collapse-group group-title="' + esc(group.title) + '"' + (isOpen ? ' open' : '') + '>'
                + '<table class="info">' + rows + '</table>'
                + '</collapse-group>';
        }

        // Catch any fields not covered by groups
        const remaining = Object.keys(raw).filter(k => !used.has(k)).sort();
        if (remaining.length) {
            let rows = '';
            for (const k of remaining) {
                rows += '<tr><td class="key">' + esc(k) + '</td><td>' + esc(String(raw[k] ?? '')) + '</td></tr>';
            }
            const otherTitle = '❓ Other';
            const isOtherOpen = collapseState.hasOwnProperty(otherTitle) ? collapseState[otherTitle] : true;
            html += '<collapse-group group-title="' + esc(otherTitle) + '"' + (isOtherOpen ? ' open' : '') + '>'
                + '<table class="info">' + rows + '</table>'
                + '</collapse-group>';
        }

        body.innerHTML = html;
        win.setAttribute('title-text', '\ud83c\udf33 ' + (d.name || d.doodId));
        win.show();

        // Listen for collapse-group toggle events and persist state
        body.addEventListener('collapse-toggle', function (e) {
            const state = _getDoodCollapseState();
            state[e.detail.title] = e.detail.open;
            _setDoodCollapseState(state);
        });

        // Bind model file links
        body.querySelectorAll('.dd-model-link').forEach(function (link) {
            link.addEventListener('click', function (e) {
                e.preventDefault();
                if (_vscode) _vscode.postMessage({command: 'openModel', path: link.getAttribute('data-path')});
            });
        });
    }

    // ── Units SLK rebuilder ──────────────────────────────────
    function rebuildUnits(slkData) {
        let source = '';
        let units = [];
        if (slkData && slkData.units) {
            source = slkData.source || '';
            units = slkData.units;
        }

        const srcEl = document.getElementById('usSlkSource');
        if (srcEl) {
            if (source) {
                srcEl.className = 'ts-source';
                srcEl.innerHTML = 'UnitData.slk: <span class="code">' + esc(source) + '</span>';
            } else {
                srcEl.className = 'ts-source ts-no-slk';
                srcEl.textContent = 'UnitData.slk not found \u2014 set Game Path';
            }
        }

        const cntEl = document.getElementById('usUnitCount');
        if (cntEl) cntEl.textContent = String(units.length);

        const listEl = document.getElementById('usUnitList');
        if (listEl) {
            listEl.innerHTML = '';
            for (const u of units) {
                const el = document.createElement('unit-item');
                el.setAttribute('unit-id', u.unitId || '');
                el.setAttribute('comment', u.comment || '');
                el.setAttribute('race', u.race || '');
                el.setAttribute('move-tp', u.moveTp || '');
                el.setAttribute('threat', String(u.threat || 0));
                el.setAttribute('points', String(u.points || 0));
                el.setAttribute('file', u.file || '');
                listEl.appendChild(el);
            }
        }
    }

    // ── Shared orbit controls (used by terrain & model viewer) ──
    function makeOrbitControls(cam, domEl, maxD, opts) {
        const skipGuards = opts && opts.skipGuards;
        const target = new THREE.Vector3();
        const sph = new THREE.Spherical();
        const sphDelta = new THREE.Spherical();
        const panOff = new THREE.Vector3();
        let zoomFactor = 1;
        const ROTATE_SPEED = 0.005, PAN_SPEED = 1.0;
        let rotating = false, panning = false, px = 0, py = 0;

        domEl.addEventListener('pointerdown', function (e) {
            if (!skipGuards && (e.target.closest('float-window') || e.target.closest('.menubar'))) return;
            if (e.button === 0) rotating = true;
            else if (e.button === 1 || e.button === 2) panning = true;
            px = e.clientX;
            py = e.clientY;
            domEl.setPointerCapture(e.pointerId);
        });
        domEl.addEventListener('pointermove', function (e) {
            var dx = e.clientX - px, dy = e.clientY - py;
            px = e.clientX;
            py = e.clientY;
            if (rotating) {
                sphDelta.theta -= dx * ROTATE_SPEED;
                sphDelta.phi -= dy * ROTATE_SPEED;
            }
            if (panning) {
                var v = new THREE.Vector3();
                var factor = cam.position.distanceTo(target) * Math.tan(cam.fov / 2 * Math.PI / 180) * 2 / domEl.clientHeight;
                v.setFromMatrixColumn(cam.matrix, 0);
                panOff.addScaledVector(v, -dx * factor * PAN_SPEED);
                v.setFromMatrixColumn(cam.matrix, 1);
                panOff.addScaledVector(v, dy * factor * PAN_SPEED);
            }
        });
        domEl.addEventListener('pointerup', function (e) {
            rotating = false;
            panning = false;
            try { domEl.releasePointerCapture(e.pointerId); } catch (_) {}
        });
        domEl.addEventListener('wheel', function (e) {
            if (!skipGuards && e.target.closest('float-window')) return;
            e.preventDefault();
            zoomFactor *= e.deltaY > 0 ? 1.1 : 0.9;
        }, {passive: false});
        domEl.addEventListener('contextmenu', function (e) { e.preventDefault(); });

        var ctrl = {
            target: target,
            maxDist: maxD,
            update: function () {
                var off = cam.position.clone().sub(target);
                sph.setFromVector3(off);
                sph.theta += sphDelta.theta;
                sph.phi += sphDelta.phi;
                sph.phi = Math.max(0.01, Math.min(Math.PI - 0.01, sph.phi));
                sph.radius *= zoomFactor;
                sph.radius = Math.max(1, Math.min(ctrl.maxDist * 5, sph.radius));
                target.add(panOff);
                off.setFromSpherical(sph);
                cam.position.copy(target).add(off);
                cam.lookAt(target);
                sphDelta.set(0, 0, 0);
                panOff.set(0, 0, 0);
                zoomFactor = 1;
            }
        };
        return ctrl;
    }

    // ── Embedded model viewer ─────────────────────────────
    function _initModelViewer() {
        const win = document.getElementById('modelViewerWindow');
        const container = document.getElementById('modelCanvasContainer');
        const canvas = document.getElementById('modelCanvas');
        const infoEl = document.getElementById('modelInfo');
        const nameEl = document.getElementById('modelName');
        if (!win || !container || !canvas) return {load() {}};

        const renderer = new THREE.WebGLRenderer({canvas, antialias: true, alpha: false});
        renderer.setPixelRatio(window.devicePixelRatio);
        renderer.setClearColor(0x1e1e1e);

        const scene = new THREE.Scene();
        const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 10000);
        camera.position.set(300, 200, 300);
        camera.lookAt(0, 50, 0);

        scene.add(new THREE.AmbientLight(0x606060));
        const dirLight = new THREE.DirectionalLight(0xffffff, 0.8);
        dirLight.position.set(200, 400, 300);
        scene.add(dirLight);
        const dirLight2 = new THREE.DirectionalLight(0x4488ff, 0.3);
        dirLight2.position.set(-200, 100, -300);
        scene.add(dirLight2);

        const gridHelper = new THREE.GridHelper(500, 20, 0x444444, 0x333333);
        scene.add(gridHelper);
        const axesHelper = new THREE.AxesHelper(100);
        scene.add(axesHelper);

        const COLORS = [
            0x4fc3f7, 0xab47bc, 0x66bb6a, 0xffa726,
            0xef5350, 0x26c6da, 0xd4e157, 0xec407a,
        ];

        const rootGroup = new THREE.Group();
        rootGroup.rotation.x = -Math.PI / 2;
        scene.add(rootGroup);

        const meshGroup = new THREE.Group();
        const wireframeGroup = new THREE.Group();
        rootGroup.add(meshGroup);
        rootGroup.add(wireframeGroup);

        let defaultCamTarget = new THREE.Vector3();
        let maxDim = 100;

        // Orbit controls — same as terrain editor
        var ctrl = makeOrbitControls(camera, canvas, maxDim, {skipGuards: true});

        // Toolbar — sidebar buttons
        const wireBtn = document.getElementById('mvWireBtn');
        const axesBtn = document.getElementById('mvAxesBtn');
        const gridBtn = document.getElementById('mvGridBtn');
        const resetBtn = document.getElementById('mvResetCamera');
        const geosetBtn = document.getElementById('mvGeosetBtn');
        const geosetsPanel = document.getElementById('mvGeosetsPanel');
        const geosetList = document.getElementById('mvGeosetList');
        const materialBtn = document.getElementById('mvMaterialBtn');
        const materialsPanel = document.getElementById('mvMaterialsPanel');
        const materialList = document.getElementById('mvMaterialList');

        let wireOn = false, axesOn = true, gridOn = true;

        function toggleSbBtn(btn, on) {
            if (on) btn.classList.add('active');
            else btn.classList.remove('active');
        }

        if (wireBtn) wireBtn.addEventListener('click', function () {
            wireOn = !wireOn;
            toggleSbBtn(wireBtn, wireOn);
            wireframeGroup.children.forEach(function (m, i) {
                var mainMesh = meshGroup.children[i];
                m.visible = wireOn && (!mainMesh || mainMesh.visible);
            });
        });
        if (axesBtn) axesBtn.addEventListener('click', function () {
            axesOn = !axesOn;
            toggleSbBtn(axesBtn, axesOn);
            axesHelper.visible = axesOn;
        });
        if (gridBtn) gridBtn.addEventListener('click', function () {
            gridOn = !gridOn;
            toggleSbBtn(gridBtn, gridOn);
            gridHelper.visible = gridOn;
        });
        if (resetBtn) resetBtn.addEventListener('click', function () {
            ctrl.target.copy(defaultCamTarget);
            const d2 = new THREE.Vector3(maxDim * 0.7, maxDim * 0.5, maxDim * 0.7);
            camera.position.copy(defaultCamTarget).add(d2);
            camera.lookAt(defaultCamTarget);
        });

        // Geoset panel toggle
        if (geosetBtn && geosetsPanel) {
            geosetBtn.addEventListener('click', function () {
                const show = geosetsPanel.hidden;
                geosetsPanel.hidden = !show;
                toggleSbBtn(geosetBtn, show);
                // Hide materials panel if opening geosets
                if (show && materialsPanel && !materialsPanel.hidden) {
                    materialsPanel.hidden = true;
                    toggleSbBtn(materialBtn, false);
                }
            });
        }

        // Material panel toggle
        if (materialBtn && materialsPanel) {
            materialBtn.addEventListener('click', function () {
                const show = materialsPanel.hidden;
                materialsPanel.hidden = !show;
                toggleSbBtn(materialBtn, show);
                // Hide geosets panel if opening materials
                if (show && geosetsPanel && !geosetsPanel.hidden) {
                    geosetsPanel.hidden = true;
                    toggleSbBtn(geosetBtn, false);
                }
            });
        }

        // ── Panel resize handles ────────────────────────────────
        document.querySelectorAll('.mv-panel-resize-handle').forEach(function (handle) {
            handle.addEventListener('mousedown', function (e) {
                e.preventDefault();
                e.stopPropagation();
                var panel = handle.parentElement;
                if (!panel) return;
                var startX = e.clientX;
                var startW = panel.offsetWidth;
                handle.classList.add('active');

                function onMove(ev) {
                    ev.preventDefault();
                    var delta = startX - ev.clientX;
                    var newW = Math.max(120, Math.min(panel.parentElement.clientWidth * 0.8, startW + delta));
                    panel.style.width = newW + 'px';
                }
                function onUp() {
                    handle.classList.remove('active');
                    document.removeEventListener('mousemove', onMove);
                    document.removeEventListener('mouseup', onUp);
                }
                document.addEventListener('mousemove', onMove);
                document.addEventListener('mouseup', onUp);
            });
        });

        // Resize
        function onResize() {
            const w = container.clientWidth;
            const h = container.clientHeight;
            if (w === 0 || h === 0) return;
            renderer.setSize(w, h);
            camera.aspect = w / h;
            camera.updateProjectionMatrix();
        }
        const resizeObs = new ResizeObserver(onResize);
        resizeObs.observe(container);

        // Animation loop
        let animating = false;
        function animate() {
            if (!animating) return;
            requestAnimationFrame(animate);
            ctrl.update();
            renderer.render(scene, camera);
        }

        // Show/hide animation
        new MutationObserver(function () {
            if (win.open) {
                animating = true;
                onResize();
                animate();
            } else {
                animating = false;
            }
        }).observe(win, {attributes: true, attributeFilter: ['hidden']});

        function b64ToFloat32(b64) {
            if (!b64) return new Float32Array(0);
            const bin = atob(b64);
            const buf = new ArrayBuffer(bin.length);
            const u8 = new Uint8Array(buf);
            for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i);
            return new Float32Array(buf);
        }

        function b64ToUint16(b64) {
            if (!b64) return new Uint16Array(0);
            const bin = atob(b64);
            const buf = new ArrayBuffer(bin.length);
            const u8 = new Uint8Array(buf);
            for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i);
            return new Uint16Array(buf);
        }

        // Filter mode names
        var FILTER_MODE_NAMES = [
            'None', 'Transparent', 'Blend', 'Additive',
            'AddAlpha', 'Modulate', 'Modulate2x'
        ];

        /** Build texture URL for the HTTP server. */
        function textureUrl(bs, archivePath, texPath) {
            if (!bs || !texPath) return null;
            var params = new URLSearchParams({
                token: bs.token,
                path: texPath,
            });
            if (archivePath) params.set('archive', archivePath);
            return 'http://127.0.0.1:' + bs.port + '/mdx/texture?' + params;
        }

        function load(msg) {
            // Clear old meshes
            meshGroup.clear();
            wireframeGroup.clear();

            if (nameEl) nameEl.textContent = msg.name || 'Model';

            const geosets = msg.geosets || [];
            const textures = msg.textures || [];
            const materials = msg.materials || [];
            const bs = msg.binaryServer || window.__W3E_DATA__.binaryServer || null;
            const archivePath = msg.archivePath || window.__W3E_DATA__.archivePath || null;

            if (geosets.length === 0) {
                if (infoEl) infoEl.textContent = 'No geosets';
                win.show();
                return;
            }

            // ── Pre-load textures via HTTP server ─────────────────
            var loadedTextures = new Array(textures.length).fill(null);
            var textureLoader = new THREE.TextureLoader();
            textureLoader.crossOrigin = 'anonymous';

            /** Look up the THREE.Texture for a geoset by its materialId. */
            function getTextureForMaterial(materialId) {
                if (materialId < materials.length) {
                    var mat = materials[materialId];
                    var layers = mat.layers || [];
                    if (layers.length > 0) {
                        var texId = layers[0].texture_id;
                        if (texId < loadedTextures.length && loadedTextures[texId]) {
                            return {texture: loadedTextures[texId], layer: layers[0], texIndex: texId};
                        }
                    }
                }
                return null;
            }

            let totalVerts = 0, totalFaces = 0;

            geosets.forEach(function (g, idx) {
                if (!g.vertex_count || !g.face_count) return;
                const vertices = b64ToFloat32(g.vertices);
                const normals = b64ToFloat32(g.normals);
                const faces = b64ToUint16(g.faces);
                const uvs = b64ToFloat32(g.uvs);

                totalVerts += g.vertex_count;
                totalFaces += g.face_count;

                const geometry = new THREE.BufferGeometry();
                geometry.setAttribute('position', new THREE.BufferAttribute(vertices, 3));
                if (normals.length > 0) geometry.setAttribute('normal', new THREE.BufferAttribute(normals, 3));
                if (uvs.length > 0) geometry.setAttribute('uv', new THREE.BufferAttribute(uvs, 2));
                geometry.setIndex(new THREE.BufferAttribute(faces, 1));
                if (normals.length === 0) geometry.computeVertexNormals();

                const color = COLORS[idx % COLORS.length];
                var texInfo = getTextureForMaterial(g.material_id);

                var matOpts = {
                    side: THREE.DoubleSide,
                    flatShading: false,
                };
                if (texInfo) {
                    matOpts.map = texInfo.texture;
                    var fm = texInfo.layer.filter_mode;
                    if (fm === 1) {
                        matOpts.transparent = true;
                        matOpts.alphaTest = 0.5;
                    } else if (fm === 2 || fm === 3) {
                        matOpts.transparent = true;
                        matOpts.blending = fm === 3 ? THREE.AdditiveBlending : THREE.NormalBlending;
                        matOpts.depthWrite = false;
                    } else {
                        matOpts.transparent = false;
                    }
                    if (texInfo.layer.alpha < 1.0) {
                        matOpts.transparent = true;
                        matOpts.opacity = texInfo.layer.alpha;
                    }
                } else {
                    matOpts.color = color;
                    matOpts.transparent = true;
                    matOpts.opacity = 0.95;
                }
                const material = new THREE.MeshPhongMaterial(matOpts);
                material.userData = {hasTexture: !!texInfo, fallbackColor: color, materialId: g.material_id};
                const mesh = new THREE.Mesh(geometry, material);
                mesh.userData.geoIndex = idx;
                mesh.userData.materialId = g.material_id;
                meshGroup.add(mesh);

                const wireMat = new THREE.MeshBasicMaterial({
                    color: 0xffffff, wireframe: true, transparent: true, opacity: 0.15,
                });
                const wireMesh = new THREE.Mesh(geometry, wireMat);
                wireMesh.visible = wireOn;
                wireframeGroup.add(wireMesh);
            });

            if (infoEl) {
                infoEl.textContent = geosets.length + ' geoset(s) | ' + totalVerts + ' verts | ' + totalFaces + ' faces';
            }

            // ── Start loading textures now that meshes exist ──────
            if (bs) {
                textures.forEach(function (tex, i) {
                    if (!tex || !tex.file_name || tex.replaceable_id) return;
                    var url = textureUrl(bs, archivePath, tex.file_name);
                    if (!url) return;

                    var threeTex = textureLoader.load(url, function () {
                        // Texture loaded — update all meshes that reference it
                        meshGroup.children.forEach(function (m) {
                            var matId = m.userData.materialId;
                            var info = getTextureForMaterial(matId);
                            if (info && info.texIndex === i) {
                                m.material.map = threeTex;
                                m.material.color.set(0xffffff);
                                m.material.needsUpdate = true;
                            }
                        });
                        // Update material panel texture thumbnails
                        var imgs = document.querySelectorAll('[data-mv-tex-index="' + i + '"]');
                        imgs.forEach(function (img) {
                            img.src = url;
                            img.style.display = '';
                        });
                    });
                    threeTex.wrapS = THREE.RepeatWrapping;
                    threeTex.wrapT = THREE.RepeatWrapping;
                    threeTex.magFilter = THREE.LinearFilter;
                    threeTex.minFilter = THREE.LinearMipmapLinearFilter;
                    loadedTextures[i] = threeTex;
                });
            }

            // ── Populate geosets panel ─────────────────────────────
            if (geosetList) {
                geosetList.innerHTML = '';
                geosets.forEach(function (g, idx) {
                    if (!g.vertex_count || !g.face_count) return;
                    const color = COLORS[idx % COLORS.length];
                    const r = (color >> 16) & 0xff;
                    const gv = (color >> 8) & 0xff;
                    const b = color & 0xff;
                    const row = document.createElement('div');
                    row.className = 'mv-mat-row';
                    row.innerHTML =
                        '<div class="mv-mat-swatch" style="background:rgb(' + r + ',' + gv + ',' + b + ')"></div>' +
                        '<span class="mv-mat-label">Geoset ' + idx + ' <span style="opacity:.5;font-size:11px">' + (g.vertex_count || 0) + 'v / ' + (g.face_count || 0) + 'f' + (g.material_id !== undefined ? ' mat=' + g.material_id : '') + '</span></span>' +
                        '<span class="mv-mat-eye">\ud83d\udc41</span>';
                    row.addEventListener('click', function () {
                        const mesh = meshGroup.children[idx];
                        const wire = wireframeGroup.children[idx];
                        if (!mesh) return;
                        const vis = !mesh.visible;
                        mesh.visible = vis;
                        if (wire) wire.visible = vis && wireOn;
                        row.classList.toggle('mv-hidden', !vis);
                    });
                    geosetList.appendChild(row);
                });
            }

            // ── Populate materials panel ──────────────────────────
            if (materialList) {
                materialList.innerHTML = '';
                materials.forEach(function (mat, i) {
                    var item = document.createElement('div');
                    item.className = 'mv-mat-item';

                    var header = document.createElement('div');
                    header.className = 'mv-mat-item-header';
                    var headerText = 'Material #' + i;
                    if (mat.priority_plane) headerText += ' (plane: ' + mat.priority_plane + ')';
                    if (mat.flags) headerText += ' [0x' + mat.flags.toString(16) + ']';
                    header.textContent = headerText;
                    item.appendChild(header);

                    var layers = mat.layers || [];
                    layers.forEach(function (layer, li) {
                        var layerDiv = document.createElement('div');
                        layerDiv.className = 'mv-mat-layer';

                        var fmName = FILTER_MODE_NAMES[layer.filter_mode] || 'Unknown(' + layer.filter_mode + ')';

                        var layerHtml =
                            '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Layer #' + li + '</span></div>' +
                            '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Filter:</span> <span>' + fmName + '</span></div>' +
                            '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Shading:</span> <span>0x' + layer.shading_flags.toString(16) + '</span></div>' +
                            '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Texture:</span> <span title="' + (tex && tex.file_name ? tex.file_name.replace(/"/g, '&quot;') : '') + '">#' + layer.texture_id;

                        var tex = textures[layer.texture_id];
                        if (tex && tex.file_name) {
                            layerHtml += ' — ' + tex.file_name.replace(/\\\\/g, '/');
                        }
                        layerHtml += '</span></div>';
                        layerHtml += '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Alpha:</span> <span>' + (layer.alpha !== undefined ? layer.alpha.toFixed(2) : '1.00') + '</span></div>';
                        layerDiv.innerHTML = layerHtml;

                        // Texture thumbnail
                        if (tex && tex.file_name && !tex.replaceable_id && bs) {
                            var thumbUrl = textureUrl(bs, archivePath, tex.file_name);
                            if (thumbUrl) {
                                var thumb = document.createElement('img');
                                thumb.className = 'mv-mat-thumb';
                                thumb.src = thumbUrl;
                                thumb.alt = tex.file_name;
                                thumb.setAttribute('data-mv-tex-index', layer.texture_id);
                                thumb.onerror = function () {
                                    thumb.style.display = 'none';
                                    var ph = document.createElement('div');
                                    ph.className = 'mv-mat-thumb-placeholder';
                                    ph.textContent = 'Texture not found';
                                    thumb.parentNode.replaceChild(ph, thumb);
                                };
                                layerDiv.appendChild(thumb);
                            }
                        } else if (tex && tex.replaceable_id) {
                            var ph = document.createElement('div');
                            ph.className = 'mv-mat-thumb-placeholder';
                            ph.textContent = 'Replaceable (ID ' + tex.replaceable_id + ')';
                            layerDiv.appendChild(ph);
                        }

                        item.appendChild(layerDiv);
                    });

                    materialList.appendChild(item);
                });

                if (materials.length === 0) {
                    materialList.innerHTML = '<div style="padding:8px;opacity:.5">No materials</div>';
                }
            }

            // Auto-fit camera
            const box = new THREE.Box3();
            meshGroup.children.forEach(function (m) {
                m.geometry.computeBoundingBox();
                const cb = m.geometry.boundingBox.clone();
                cb.applyMatrix4(m.matrixWorld);

                box.union(cb);
            });

            const tempGroup = new THREE.Group();
            tempGroup.rotation.x = -Math.PI / 2;
            tempGroup.updateMatrixWorld(true);

            const center = new THREE.Vector3();
            box.getCenter(center);
            center.applyMatrix4(tempGroup.matrixWorld);

            const size = new THREE.Vector3();
            box.getSize(size);
            maxDim = Math.max(size.x, size.y, size.z) || 100;
            ctrl.maxDist = maxDim;

            const dist = maxDim * 1.5;
            ctrl.target.copy(center);
            defaultCamTarget = center.clone();

            const d2 = new THREE.Vector3().set(dist * 0.7, dist * 0.5, dist * 0.7);
            camera.position.copy(center).add(d2);
            camera.lookAt(center);

            camera.near = maxDim * 0.001;
            camera.far = maxDim * 20;
            camera.updateProjectionMatrix();

            win.show();
            onResize();
        }

        function showUnsupported(msg) {
            meshGroup.clear();
            wireframeGroup.clear();
            if (geosetList) geosetList.innerHTML = '';
            if (materialList) materialList.innerHTML = '';
            if (nameEl) nameEl.textContent = msg.name || 'Model';
            if (infoEl) infoEl.textContent = '\u26a0 ' + (msg.reason || 'Unsupported format');
            win.show();
        }

        return {load, showUnsupported};
    }

    // ── init() — main entry point ────────────────────────────
    function init(config) {
        const vscode = config.vscode;
        _vscode = vscode;
        const groundTileCodes = config.groundTileCodes || [];
        const cliffTileCodes = config.cliffTileCodes || [];
        const isArchive = !!config.isArchive;

        // ── Populate initial doodad data map for detail window ──
        if (config.initialDoodadsSlk && config.initialDoodadsSlk.doodads) {
            _allDoodads = config.initialDoodadsSlk.doodads;
            for (const d of _allDoodads) {
                _doodadDataMap[d.doodId] = d;
            }
            // Restore saved filter state
            _restoreDoodFilters();
            // Bind initial filter checkbox events
            document.querySelectorAll('.ds-cat-cb').forEach(cb => {
                cb.addEventListener('change', _filterAndRenderDoodads);
            });
            document.querySelectorAll('.ds-ts-cb').forEach(cb => {
                cb.addEventListener('change', _filterAndRenderDoodads);
            });
            // Bind initial sort column headers
            document.querySelectorAll('.ds-sort-col').forEach(btn => {
                if (btn._dsSortBound) return;
                btn._dsSortBound = true;
                btn.addEventListener('click', () => _cycleDoodSort(btn.getAttribute('data-sort')));
            });
            // Bind initial search input
            const searchEl = document.getElementById('dsSearchInput');
            if (searchEl && !searchEl._dsBound) {
                searchEl._dsBound = true;
                searchEl.addEventListener('input', _filterAndRenderDoodads);
            }
            // Restore saved sort state and re-render
            _restoreDoodSort();
            _updateSortButtons();
            _filterAndRenderDoodads(false);
        }

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

        // ── Loading state ──────────────────────────────────────
        function setLoading(v) {
            document.querySelectorAll('reload-button').forEach(btn => {
                btn.loading = v;
                const win = btn.closest('float-window');
                if (win) win.loading = v;
            });
        }

        // ── Reload button click → re-fetch all game-path data ──
        document.addEventListener('reload', () => {
            setLoading(true);
            if (vscode) vscode.postMessage({command: 'reloadGamePath'});
        });

        // ── Game Path ────────────────────────────────────────
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

        onGamePathChanged(data => {
            if (!data.status) return;
            const gpBody = document.getElementById('gpBody');
            if (!gpBody) return;
            gpBody.innerHTML = renderGpBody(data.status);
            bindGpButtons();
        });

        // ── Tileset ──────────────────────────────────────────
        onGamePathChanged(data => rebuildTileset(data.terrainSlk, groundTileCodes, cliffTileCodes));

        // ── Doodads ──────────────────────────────────────────
        onGamePathChanged(data => rebuildDoodads(data.doodadsSlk));

        // ── Units ────────────────────────────────────────────
        onGamePathChanged(data => rebuildUnits(data.unitsSlk));

        // ── Doodad item click → show detail window ──────────────
        {
            const dsList = document.getElementById('dsDoodadList');
            if (dsList) {
                dsList.addEventListener('click', function (e) {
                    const item = e.target.closest('doodad-item');
                    if (!item) return;
                    const id = item.getAttribute('dood-id') || '';
                    if (!id) return;
                    showDoodadDetail(id);
                });
            }
        }

        // ── Placed doodad rawcode click → show detail window ────
        {
            const dooWin = document.getElementById('doodadDooWindow');
            if (dooWin) {
                dooWin.addEventListener('click', function (e) {
                    const link = e.target.closest('.doo-id-link');
                    if (!link) return;
                    e.preventDefault();
                    const id = link.getAttribute('data-dood-id') || '';
                    if (!id) return;
                    showDoodadDetail(id);
                });
            }
        }

        if (vscode) {
            // ── Unit item click → open model ─────────────────────
            const usList = document.getElementById('usUnitList');
            if (usList) {
                usList.addEventListener('click', function (e) {
                    const item = e.target.closest('unit-item');
                    if (!item) return;
                    const file = item.getAttribute('file') || '';
                    if (!file) return;
                    vscode.postMessage({command: 'openModel', path: file});
                });
            }
        }

        // ── Model viewer (embedded float-window) ─────────────
        const _modelViewer = _initModelViewer();

        // ── Message router ───────────────────────────────────
        window.addEventListener('message', e => {
            const msg = e.data;
            if (msg && msg.command === 'gamePathChanged') {
                for (const fn of _gamePathHandlers) fn(msg);
                setLoading(false);
            }
            if (msg && msg.command === 'loadingDone') {
                setLoading(false);
            }
            if (msg && msg.command === 'loadingStart') {
                setLoading(true);
            }
            if (msg && msg.command === 'modelData') {
                _modelViewer.load(msg);
            }
            if (msg && msg.command === 'modelUnsupported') {
                _modelViewer.showUnsupported(msg);
            }
        });

        // ── Archive file interactions ────────────────────────
        if (isArchive && vscode) {
            // ── Custom context menu ──────────────────────────
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

                // position, keep on-screen
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

    // ── Highlight a placed doodad by DOO index ──────────────
    function highlightPlacedDoodad(dooIndex) {
        const win = document.getElementById('doodadDooWindow');
        if (!win) return;
        // Open the window
        win.show();
        // Find the row by data-doo-idx attribute
        const row = win.querySelector('tr[data-doo-idx="' + dooIndex + '"]');
        if (!row) return;
        // Remove previous highlight
        win.querySelectorAll('.doo-highlight').forEach(function (el) {
            el.classList.remove('doo-highlight');
        });
        // Highlight and scroll
        row.classList.add('doo-highlight');
        row.scrollIntoView({behavior: 'smooth', block: 'center'});
    }

    return {init, onGamePathChanged, indexToRgb, syncMenuActive, makeOrbitControls, highlightPlacedDoodad};
})();

