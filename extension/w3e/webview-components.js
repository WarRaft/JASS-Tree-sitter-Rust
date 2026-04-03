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
    z-index: 5;
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

// ── Destructable item ────────────────────────────────────────────────

const DESTRUCTABLE_CATEGORIES = {
    B: 'Bridges/Ramps',
    D: 'Destructibles',
    P: 'Pathing Blockers',
};

class DestructableItem extends HTMLElement {
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
        <span class="id" id="destId"></span>
        <span class="name" id="name"></span>
        <span class="tilesets" id="tilesets"></span>
        <span class="cat" id="cat"></span>`;
    }

    connectedCallback() { this._render(); }
    static get observedAttributes() { return ['dest-id', 'dest-name', 'category', 'tilesets']; }
    attributeChangedCallback() { if (this.shadowRoot) this._render(); }

    _render() {
        const s = this.shadowRoot;
        s.getElementById('destId').textContent = this.getAttribute('dest-id') || '';
        const name = this.getAttribute('dest-name') || '';
        const nameEl = s.getElementById('name');
        nameEl.textContent = name;
        nameEl.title = this.getAttribute('comment') || '';
        const cat = this.getAttribute('category') || '';
        const catEl = s.getElementById('cat');
        const catLabel = (typeof DESTRUCTABLE_CATEGORIES !== 'undefined' && DESTRUCTABLE_CATEGORIES[cat]) || cat;
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

customElements.define('destructable-item', DestructableItem);

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


// ── CanvasList — Virtual-scroll canvas-based list ───────────────────
// Replaces heavy DOM lists (hundreds of shadow-DOM custom elements)
// with a single <canvas>. Only visible rows are drawn.
// Handles wheel-scroll, hover, click.

function _clRoundRect(ctx, x, y, w, h, r) {
    ctx.beginPath();
    ctx.moveTo(x + r, y);
    ctx.lineTo(x + w - r, y);
    ctx.quadraticCurveTo(x + w, y, x + w, y + r);
    ctx.lineTo(x + w, y + h - r);
    ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
    ctx.lineTo(x + r, y + h);
    ctx.quadraticCurveTo(x, y + h, x, y + h - r);
    ctx.lineTo(x, y + r);
    ctx.quadraticCurveTo(x, y, x + r, y);
    ctx.closePath();
}

function _clTruncText(ctx, text, x, y, maxW) {
    if (!text) return;
    if (ctx.measureText(text).width <= maxW) { ctx.fillText(text, x, y); return; }
    var ew = ctx.measureText('\u2026').width;
    var t = text;
    while (t.length > 0 && ctx.measureText(t).width + ew > maxW) t = t.slice(0, -1);
    ctx.fillText(t + '\u2026', x, y);
}

function _clDrawBadge(ctx, ch, x, rowY, rowH, c, isAll) {
    var bw = 16, bh = 16, by = rowY + (rowH - bh) / 2;
    ctx.fillStyle = isAll ? c.badgeAllBg : c.badgeBg;
    _clRoundRect(ctx, x, by, bw, bh, 3);
    ctx.fill();
    ctx.font = '600 10px ' + c.mono;
    ctx.fillStyle = isAll ? c.badgeAllFg : c.desc;
    ctx.textAlign = 'center';
    ctx.fillText(ch, x + bw / 2, rowY + rowH / 2);
    ctx.textAlign = 'left';
}

class CanvasList {
    constructor(container, options) {
        this._container = container;
        this._rh = options.rowHeight || 26;
        this._renderRow = options.renderRow;
        this._onClick = options.onClick || null;
        this._items = [];
        this._scrollY = 0;
        this._hover = -1;
        this._w = 0;
        this._h = 0;
        this._disposed = false;
        this._raf = 0;
        this._scrollDragging = false;
        this._scrollDragStartY = 0;
        this._scrollDragStartScrollY = 0;
        this._highlight = -1;
        this._highlightTimeout = 0;

        var cs = getComputedStyle(document.documentElement);
        var cv = function (n, fb) { return cs.getPropertyValue(n).trim() || fb; };
        this.C = {
            bg: cv('--vscode-editor-background', '#1e1e1e'),
            fg: cv('--vscode-editor-foreground', '#ccc'),
            hover: cv('--vscode-list-hoverBackground', 'rgba(255,255,255,0.06)'),
            border: cv('--vscode-editorWidget-border', '#333'),
            link: cv('--vscode-textLink-foreground', '#3794ff'),
            desc: cv('--vscode-descriptionForeground', '#999'),
            font: cv('--vscode-font-family', 'sans-serif'),
            mono: cv('--vscode-editor-font-family', 'monospace'),
            badgeBg: 'rgba(255,255,255,0.08)',
            badgeAllBg: 'rgba(78,154,241,0.25)',
            badgeAllFg: cv('--vscode-textLink-foreground', '#3794ff'),
        };

        this._canvas = document.createElement('canvas');
        this._canvas.style.cssText = 'display:block;width:100%;height:100%;cursor:default;';
        this._ctx = this._canvas.getContext('2d');

        container.style.overflow = 'hidden';
        container.innerHTML = '';
        container.appendChild(this._canvas);

        var self = this;
        this._handlers = {
            wheel: function (e) { self._onWheel(e); },
            move: function (e) { self._onMove(e); },
            leave: function () { self._onLeave(); },
            click: function (e) { self._onClickEvt(e); },
            down: function (e) { self._onPointerDown(e); },
            pmove: function (e) { self._onPointerMove(e); },
            up: function (e) { self._onPointerUp(e); },
        };
        this._canvas.addEventListener('wheel', this._handlers.wheel, {passive: false});
        this._canvas.addEventListener('mousemove', this._handlers.move);
        this._canvas.addEventListener('mouseleave', this._handlers.leave);
        this._canvas.addEventListener('click', this._handlers.click);
        this._canvas.addEventListener('pointerdown', this._handlers.down);
        this._canvas.addEventListener('pointermove', this._handlers.pmove);
        this._canvas.addEventListener('pointerup', this._handlers.up);

        this._ro = new ResizeObserver(function () { self._resize(); });
        this._ro.observe(container);
        this._resize();
    }

    setData(items) {
        this._items = items || [];
        this._clamp();
        this._schedule();
    }

    _resize() {
        var dpr = window.devicePixelRatio || 1;
        var w = this._container.clientWidth;
        var h = this._container.clientHeight;
        if (w <= 0 || h <= 0) return;
        this._canvas.width = w * dpr;
        this._canvas.height = h * dpr;
        this._ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        this._w = w;
        this._h = h;
        this._clamp();
        this._draw();
    }

    _clamp() {
        var max = Math.max(0, this._items.length * this._rh - this._h);
        if (this._scrollY > max) this._scrollY = max;
        if (this._scrollY < 0) this._scrollY = 0;
    }

    _schedule() {
        if (this._raf) return;
        var self = this;
        this._raf = requestAnimationFrame(function () { self._raf = 0; self._draw(); });
    }

    _scrollbarMetrics() {
        var total = this._items.length * this._rh;
        if (total <= this._h) return null;
        var thumbH = Math.max(20, this._h * this._h / total);
        var thumbY = (this._scrollY / total) * this._h;
        return {thumbH: thumbH, thumbY: thumbY, trackX: this._w - 10, trackW: 8};
    }

    _draw() {
        if (this._disposed) return;
        var ctx = this._ctx, w = this._w, h = this._h;
        if (!w || !h) return;
        ctx.clearRect(0, 0, w, h);

        var rh = this._rh;
        var first = Math.floor(this._scrollY / rh);
        var last = Math.min(this._items.length - 1, Math.ceil((this._scrollY + h) / rh));

        for (var i = first; i <= last; i++) {
            var y = i * rh - this._scrollY;
            if (i === this._highlight) {
                ctx.fillStyle = 'rgba(55, 148, 255, 0.2)';
                ctx.fillRect(0, y, w, rh);
            } else if (i === this._hover) {
                ctx.fillStyle = this.C.hover;
                ctx.fillRect(0, y, w, rh);
            }
            ctx.fillStyle = this.C.border;
            ctx.fillRect(0, y + rh - 1, w, 1);
            this._renderRow(ctx, this._items[i], 6, y, w - 20, rh, this.C);
        }

        var sb = this._scrollbarMetrics();
        if (sb) {
            ctx.fillStyle = 'rgba(255,255,255,0.15)';
            _clRoundRect(ctx, sb.trackX, sb.thumbY, sb.trackW, sb.thumbH, 3);
            ctx.fill();
        }
    }

    _idx(clientY) {
        var rect = this._canvas.getBoundingClientRect();
        var y = clientY - rect.top;
        var i = Math.floor((y + this._scrollY) / this._rh);
        return i >= 0 && i < this._items.length ? i : -1;
    }

    _onWheel(e) {
        e.preventDefault();
        this._scrollY += e.deltaY;
        this._clamp();
        this._hover = this._idx(e.clientY);
        this._schedule();
    }

    _onMove(e) {
        var i = this._idx(e.clientY);
        if (i !== this._hover) { this._hover = i; this._schedule(); }
    }

    _onLeave() {
        if (this._hover !== -1) { this._hover = -1; this._schedule(); }
    }

    _onClickEvt(e) {
        if (this._scrollDragging) return;
        var i = this._idx(e.clientY);
        if (i >= 0 && this._onClick) this._onClick(this._items[i], i);
    }

    _onPointerDown(e) {
        var sb = this._scrollbarMetrics();
        if (!sb) return;
        var rect = this._canvas.getBoundingClientRect();
        var mx = e.clientX - rect.left;
        var my = e.clientY - rect.top;
        if (mx >= sb.trackX && mx <= sb.trackX + sb.trackW && my >= sb.thumbY && my <= sb.thumbY + sb.thumbH) {
            this._scrollDragging = true;
            this._scrollDragStartY = my;
            this._scrollDragStartScrollY = this._scrollY;
            this._canvas.setPointerCapture(e.pointerId);
            e.preventDefault();
        }
    }

    _onPointerMove(e) {
        if (!this._scrollDragging) return;
        var rect = this._canvas.getBoundingClientRect();
        var my = e.clientY - rect.top;
        var dy = my - this._scrollDragStartY;
        var total = this._items.length * this._rh;
        this._scrollY = this._scrollDragStartScrollY + dy * total / this._h;
        this._clamp();
        this._schedule();
    }

    _onPointerUp(e) {
        if (this._scrollDragging) {
            this._scrollDragging = false;
            try { this._canvas.releasePointerCapture(e.pointerId); } catch (_) {}
        }
    }

    scrollToIndex(idx) {
        if (idx < 0 || idx >= this._items.length) return;
        var y = idx * this._rh;
        if (y < this._scrollY || y + this._rh > this._scrollY + this._h) {
            this._scrollY = Math.max(0, y - this._h / 2 + this._rh / 2);
            this._clamp();
        }
        this._highlight = idx;
        this._schedule();
        var self = this;
        clearTimeout(this._highlightTimeout);
        this._highlightTimeout = setTimeout(function () {
            self._highlight = -1;
            self._schedule();
        }, 2000);
    }

    dispose() {
        this._disposed = true;
        clearTimeout(this._highlightTimeout);
        if (this._raf) { cancelAnimationFrame(this._raf); this._raf = 0; }
        this._ro.disconnect();
        this._canvas.removeEventListener('wheel', this._handlers.wheel);
        this._canvas.removeEventListener('mousemove', this._handlers.move);
        this._canvas.removeEventListener('mouseleave', this._handlers.leave);
        this._canvas.removeEventListener('click', this._handlers.click);
        this._canvas.removeEventListener('pointerdown', this._handlers.down);
        this._canvas.removeEventListener('pointermove', this._handlers.pmove);
        this._canvas.removeEventListener('pointerup', this._handlers.up);
        if (this._canvas.parentNode) this._canvas.parentNode.removeChild(this._canvas);
        this._items = [];
    }
}


// ── W3E application logic ────────────────────────────────────────────
window.W3E = (function () {
    let _vscode = null;

    // ── Current snapshot & status ─────────────────────────────────
    // The extension host sends the full GameSnapshot in one message.
    // We store it here and call rebuild functions directly.
    var _snapshot = null;
    var _status = null;
    var _statusListeners = [];
    var _snapshotListeners = [];

    /** Register a callback for status (game path) changes. */
    function onStatusChanged(fn) { _statusListeners.push(fn); }

    /** Register a callback for snapshot changes (full data reload). */
    function onSnapshotChanged(fn) { _snapshotListeners.push(fn); }

    /** Apply a new snapshot + status from the extension host. */
    function _applyGamePathChanged(status, snapshot) {
        _status = status;
        _snapshot = snapshot;

        // Notify status listeners (e.g. terrain.js)
        for (var i = 0; i < _statusListeners.length; i++) {
            try { _statusListeners[i](status); } catch (_) {}
        }

        if (!snapshot) return;

        // Apply westrings
        _westringsMap = (snapshot.westrings && typeof snapshot.westrings === 'object')
            ? snapshot.westrings : {};

        // Rebuild all UI
        rebuildTileset(snapshot.terrainSlk, _groundTileCodes, _cliffTileCodes);
        rebuildDoodads(snapshot.doodadsSlk);
        rebuildDestructables(snapshot.destructablesSlk);
        rebuildUnits(snapshot.unitsSlk);
        _updatePlacedNames();

        // Notify snapshot listeners (e.g. terrain.js map objects)
        for (var j = 0; j < _snapshotListeners.length; j++) {
            try { _snapshotListeners[j](snapshot); } catch (_) {}
        }
    }

    // ── WESTRING resolution map ─────────────────────────────────
    var _westringsMap = {};

    function _resolveWestring(val) {
        if (!val || typeof val !== 'string') return val || '';
        var current = val;
        for (var i = 0; i < 3; i++) {
            if (!current.startsWith('WESTRING_')) break;
            var resolved = _westringsMap[current];
            if (resolved === undefined) break;
            current = resolved;
        }
        return current;
    }

    // ── GameString helpers ────────────────────────────────────────

    /** Extract the display text from a GameString (string or object). */
    function _gsValue(gs) {
        if (!gs) return '';
        if (typeof gs === 'object' && gs.value !== undefined) return gs.value;
        return String(gs);
    }

    /** Render a GameString as HTML — resolved values shown as clickable links. */
    function _gsHtml(gs) {
        if (!gs) return '';
        if (typeof gs === 'object' && gs.value !== undefined) {
            var v = esc(gs.value);
            if (gs.original && gs.original !== gs.value) {
                return '<a href="#" class="gs-resolved" data-gs-original="' + esc(gs.original) + '" data-gs-source="' + esc(gs.source || '') + '">' + v + '</a>';
            }
            return v;
        }
        return esc(String(gs));
    }

    /** Show the GameString info window with provenance details. */
    function _showGameStringInfo(value, original, source) {
        var win = document.getElementById('gameStringInfoWindow');
        if (!win) return;
        var body = win.querySelector('#gsInfoBody');
        if (!body) return;
        body.innerHTML =
            '<table class="info">' +
            '<tr><td class="key">value</td><td>' + esc(value) + '</td></tr>' +
            '<tr><td class="key">original</td><td><span class="code">' + esc(original) + '</span></td></tr>' +
            '<tr><td class="key">source</td><td>' + esc(source) + '</td></tr>' +
            '</table>';
        win.setAttribute('title-text', '\ud83d\udd17 ' + value);
        win.show();
    }

    /** Delegate click on .gs-resolved links to open the info window. */
    document.addEventListener('click', function (e) {
        var link = e.target.closest('.gs-resolved');
        if (!link) return;
        e.preventDefault();
        var original = link.getAttribute('data-gs-original') || '';
        var source = link.getAttribute('data-gs-source') || '';
        var value = link.textContent || '';
        _showGameStringInfo(value, original, source);
    });


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

    // ── Canvas list row renderers ──────────────────────────────────
    function _renderDoodadRow(ctx, d, x, y, w, h, c) {
        var mid = y + h / 2;
        ctx.textBaseline = 'middle';
        // ID
        ctx.font = '11px ' + c.mono;
        ctx.fillStyle = c.link;
        ctx.fillText(d.doodId || '', x, mid);
        // Category (right side)
        var catText = (typeof DOODAD_CATEGORIES !== 'undefined' && DOODAD_CATEGORIES[d.category]) || d.category || '';
        ctx.font = '11px ' + c.font;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        ctx.fillText(catText, x + w, mid);
        var catW = catText ? ctx.measureText(catText).width + 8 : 0;
        ctx.textAlign = 'left';
        // Tileset badges
        var bx = x + w - catW;
        var ts = d.tilesets || '';
        if (ts) {
            var chars = ts === '*' ? ['*'] : ts.split(',').filter(Boolean);
            for (var bi = chars.length - 1; bi >= 0; bi--) {
                bx -= 18;
                _clDrawBadge(ctx, chars[bi], bx, y, h, c, chars[bi] === '*');
            }
            bx -= 4;
        }
        // Name
        var nameX = x + 46;
        var nameW = bx - nameX;
        if (nameW > 10) {
            ctx.font = '12px ' + c.font;
            ctx.fillStyle = c.fg;
            _clTruncText(ctx, _gsValue(d.name) || '', nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    function _renderDestructableRow(ctx, d, x, y, w, h, c) {
        var mid = y + h / 2;
        ctx.textBaseline = 'middle';
        // ID
        ctx.font = '11px ' + c.mono;
        ctx.fillStyle = c.link;
        ctx.fillText(d.destructableId || '', x, mid);
        // Category (right side)
        var catText = (typeof DESTRUCTABLE_CATEGORIES !== 'undefined' && DESTRUCTABLE_CATEGORIES[d.category]) || d.category || '';
        ctx.font = '11px ' + c.font;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        ctx.fillText(catText, x + w, mid);
        var catW = catText ? ctx.measureText(catText).width + 8 : 0;
        ctx.textAlign = 'left';
        // Tileset badges
        var bx = x + w - catW;
        var ts = d.tilesets || '';
        if (ts) {
            var chars = ts === '*' ? ['*'] : ts.split(',').filter(Boolean);
            for (var bi = chars.length - 1; bi >= 0; bi--) {
                bx -= 18;
                _clDrawBadge(ctx, chars[bi], bx, y, h, c, chars[bi] === '*');
            }
            bx -= 4;
        }
        // Name
        var nameX = x + 46;
        var nameW = bx - nameX;
        if (nameW > 10) {
            ctx.font = '12px ' + c.font;
            ctx.fillStyle = c.fg;
            var rn = _gsValue(d.name) || '';
            var rs = _gsValue(d.editorSuffix);
            _clTruncText(ctx, rn + (rs ? ' ' + rs : ''), nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    function _renderUnitRow(ctx, u, x, y, w, h, c) {
        var mid = y + h / 2;
        ctx.textBaseline = 'middle';
        // ID
        ctx.font = '11px ' + c.mono;
        ctx.fillStyle = c.link;
        ctx.fillText(u.unitId || '', x, mid);
        // Race (right side)
        var raceText = u.race || '';
        ctx.font = '11px ' + c.font;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        ctx.fillText(raceText, x + w, mid);
        var raceW = raceText ? ctx.measureText(raceText).width + 8 : 0;
        ctx.textAlign = 'left';
        // Name
        var nameX = x + 46;
        var nameEnd = x + w - raceW;
        var nameW = nameEnd - nameX;
        if (nameW > 10) {
            ctx.font = '12px ' + c.font;
            ctx.fillStyle = c.fg;
            _clTruncText(ctx, _gsValue(u.name) || u.comment || '', nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    // ── Canvas list row renderers — placed DOO items ────────────
    function _renderPlacedDoodadRow(ctx, item, x, y, w, h, c) {
        var mid = y + h / 2;
        ctx.textBaseline = 'middle';
        // # index (right-aligned)
        ctx.font = '10px ' + c.mono;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        ctx.fillText(String(item.index + 1), x + 28, mid);
        ctx.textAlign = 'left';
        // Rawcode
        ctx.font = '11px ' + c.mono;
        ctx.fillStyle = item._error ? '#f44' : c.link;
        ctx.fillText(item.text || '', x + 34, mid);
        // Right side: angle
        ctx.font = '10px ' + c.mono;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        var angleDeg = item.angle != null ? (item.angle * 180 / Math.PI).toFixed(0) + '\u00b0' : '';
        ctx.fillText(angleDeg, x + w, mid);
        // Position
        var posText = _fmtPlacedF(item.position.x) + ', ' + _fmtPlacedF(item.position.y);
        ctx.fillText(posText, x + w - 42, mid);
        var posW = ctx.measureText(posText).width;
        ctx.textAlign = 'left';
        // Name
        var nameX = x + 78;
        var nameEnd = x + w - 42 - posW - 12;
        var nameW = nameEnd - nameX;
        if (nameW > 10) {
            ctx.font = '12px ' + c.font;
            ctx.fillStyle = c.fg;
            _clTruncText(ctx, item._name || '', nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    function _renderPlacedUnitRow(ctx, item, x, y, w, h, c) {
        var mid = y + h / 2;
        ctx.textBaseline = 'middle';
        // # index (right-aligned)
        ctx.font = '10px ' + c.mono;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        ctx.fillText(String(item.index + 1), x + 28, mid);
        ctx.textAlign = 'left';
        // Rawcode
        ctx.font = '11px ' + c.mono;
        ctx.fillStyle = c.link;
        ctx.fillText(item.text || '', x + 34, mid);
        // Right: player
        ctx.font = '10px ' + c.mono;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        if (item.player != null) {
            ctx.fillText('P' + item.player, x + w, mid);
        }
        // Angle
        var angleDeg = item.angle != null ? (item.angle * 180 / Math.PI).toFixed(0) + '\u00b0' : '';
        ctx.fillText(angleDeg, x + w - 28, mid);
        // Position
        var posText = _fmtPlacedF(item.position.x) + ', ' + _fmtPlacedF(item.position.y);
        ctx.fillText(posText, x + w - 68, mid);
        var posW = ctx.measureText(posText).width;
        ctx.textAlign = 'left';
        // Name/comment
        var nameX = x + 78;
        var nameEnd = x + w - 68 - posW - 12;
        var nameW = nameEnd - nameX;
        if (nameW > 10) {
            ctx.font = '12px ' + c.font;
            ctx.fillStyle = c.fg;
            _clTruncText(ctx, item._name || '', nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    // ── Canvas list instances & cached filtered data ─────────────
    var _doodadCanvasList = null;
    var _filteredDoodads = [];
    var _destCanvasList = null;
    var _filteredDestructables = [];
    var _unitCanvasList = null;
    var _allUnits = [];
    var _filteredUnits = [];

    function _ensureDoodadCanvasList() {
        if (_doodadCanvasList) return;
        var el = document.getElementById('dsDoodadList');
        if (!el) return;
        _doodadCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: _renderDoodadRow,
            onClick: function (item) {
                if (item._rawKey) showDoodadDetail(item._rawKey);
            }
        });
        if (_filteredDoodads.length) _doodadCanvasList.setData(_filteredDoodads);
    }
    function _disposeDoodadCanvasList() {
        if (_doodadCanvasList) { _doodadCanvasList.dispose(); _doodadCanvasList = null; }
    }

    function _ensureDestCanvasList() {
        if (_destCanvasList) return;
        var el = document.getElementById('dtDestList');
        if (!el) return;
        _destCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: _renderDestructableRow,
            onClick: function (item) {
                if (item._rawKey) showDestructableDetail(item._rawKey);
            }
        });
        if (_filteredDestructables.length) _destCanvasList.setData(_filteredDestructables);
    }
    function _disposeDestCanvasList() {
        if (_destCanvasList) { _destCanvasList.dispose(); _destCanvasList = null; }
    }

    function _ensureUnitCanvasList() {
        if (_unitCanvasList) return;
        var el = document.getElementById('usUnitList');
        if (!el) return;
        _unitCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: _renderUnitRow,
            onClick: function (item) {
                if (item._rawKey) showUnitDetail(item._rawKey);
            }
        });
        if (_filteredUnits.length) _unitCanvasList.setData(_filteredUnits);
    }
    function _disposeUnitCanvasList() {
        if (_unitCanvasList) { _unitCanvasList.dispose(); _unitCanvasList = null; }
    }

    // ── Placed DOO canvas list instances ─────────────────────────
    var _unitDooItems = [];
    var _destDooItems = [];
    var _unitDooCanvasList = null;
    var _doodadDooCanvasList = null;
    var _destDooCanvasList = null;

    function _ensureUnitDooCanvasList() {
        if (_unitDooCanvasList) return;
        var el = document.getElementById('unitDooList');
        if (!el) return;
        _unitDooCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: _renderPlacedUnitRow,
            onClick: function (item) {
                var rawKey = String(item.raw);
                if (_unitDataMap[rawKey]) {
                    showUnitDetail(rawKey);
                }
            }
        });
        if (_unitDooItems.length) _unitDooCanvasList.setData(_unitDooItems);
    }
    function _disposeUnitDooCanvasList() {
        if (_unitDooCanvasList) { _unitDooCanvasList.dispose(); _unitDooCanvasList = null; }
    }

    function _ensureDoodadDooCanvasList() {
        if (_doodadDooCanvasList) return;
        var el = document.getElementById('doodadDooList');
        if (!el) return;
        _doodadDooCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: _renderPlacedDoodadRow,
            onClick: function (item) {
                var rawKey = String(item.raw);
                if (_doodadDataMap[rawKey]) {
                    showDoodadDetail(rawKey);
                } else if (_destructableDataMap[rawKey]) {
                    showDestructableDetail(rawKey);
                }
            }
        });
        if (_doodadDooItems.length) _doodadDooCanvasList.setData(_doodadDooItems);
    }
    function _disposeDoodadDooCanvasList() {
        if (_doodadDooCanvasList) { _doodadDooCanvasList.dispose(); _doodadDooCanvasList = null; }
    }

    function _ensureDestDooCanvasList() {
        if (_destDooCanvasList) return;
        var el = document.getElementById('destructableDooList');
        if (!el) return;
        _destDooCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: _renderPlacedDoodadRow,
            onClick: function (item) {
                var rawKey = String(item.raw);
                if (_destructableDataMap[rawKey]) {
                    showDestructableDetail(rawKey);
                } else if (_doodadDataMap[rawKey]) {
                    showDoodadDetail(rawKey);
                }
            }
        });
        if (_destDooItems.length) _destDooCanvasList.setData(_destDooItems);
    }
    function _disposeDestDooCanvasList() {
        if (_destDooCanvasList) { _destDooCanvasList.dispose(); _destDooCanvasList = null; }
    }


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
    // _doodadDataMap is keyed by rawcode u32 (matching the HashMap from Rust).
    let _doodadDataMap = {};
    let _allDoodads = [];
    /** Whether doodadsSlk has been loaded at least once (even if empty). */
    var _doodadsSlkLoaded = false;

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
                const name = _gsValue(d.name).toLowerCase();
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
                const va = _gsValue(a[f]).toLowerCase();
                const vb = _gsValue(b[f]).toLowerCase();
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
        _doodadsSlkLoaded = true;
        let source = '';
        _allDoodads = [];
        _doodadDataMap = {};
        if (slkData && slkData.doodads) {
            source = slkData.source || '';
            // slkData.doodads is a HashMap<u32, Doodad> from Rust
            _doodadDataMap = slkData.doodads;
            _allDoodads = Object.entries(slkData.doodads).map(function (e) { e[1]._rawKey = e[0]; return e[1]; });
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
    // Each group contains `fields` mapping display labels to camelCase struct field names.
    const _DOOD_GROUPS = [
        {
            title: '🏷 Identity', fields: [
                ['doodID', 'doodId'],
                ['Name', 'name'],
                ['comment', 'comment'],
                ['category', 'category'],
                ['doodClass', 'doodClass'],
                ['tilesets', 'tilesets'],
                ['tilesetSpecific', 'tilesetSpecific'],
            ]
        },
        {
            title: '🎨 Model', modelFiles: true, fields: [
                ['soundLoop', 'soundLoop'],
            ]
        },
        {
            title: '📐 Scale', fields: [
                ['defScale', 'defScale'],
                ['minScale', 'minScale'],
                ['maxScale', 'maxScale'],
                ['canPlaceRandScale', 'canPlaceRandScale'],
            ]
        },
        {
            title: '📍 Placement', fields: [
                ['onCliffs', 'onCliffs'],
                ['onWater', 'onWater'],
                ['floats', 'floats'],
                ['walkable', 'walkable'],
                ['fixedRot', 'fixedRot'],
                ['maxPitch', 'maxPitch'],
                ['maxRoll', 'maxRoll'],
                ['pathTex', 'pathTex'],
            ]
        },
        {
            title: '👆 Interaction', fields: [
                ['selSize', 'selSize'],
                ['useClickHelper', 'useClickHelper'],
                ['ignoreModelClick', 'ignoreModelClick'],
                ['visRadius', 'visRadius'],
            ]
        },
        {
            title: '👁 Rendering', fields: [
                ['shadow', 'shadow'],
                ['showInFog', 'showInFog'],
                ['animInFog', 'animInFog'],
            ]
        },
        {
            title: '🗺 Minimap', fields: [
                ['showInMM', 'showInMm'],
                ['useMMColor', 'useMmColor'],
            ],
            color: {key: 'mmColor', label: 'Color'},
        },
        {
            title: '🌈 Vertex Colors', vertexColors: true,
        },
        {
            title: 'ℹ Meta', fields: [
                ['InBeta', 'inBeta'],
                ['version', 'version'],
            ]
        },
    ];

    function _colorBadge(r, g, b) {
        return '<span class="dd-color-badge" style="background:rgb(' + r + ',' + g + ',' + b + ')" title="rgb(' + r + ',' + g + ',' + b + ')"></span>';
    }

    function _categoryBadge(code, categoriesMap) {
        const label = categoriesMap[code] || code;
        return '<span class="ds-ts-badge">' + esc(code) + '</span> ' + esc(label);
    }

    function _tilesetBadges(val) {
        if (val === '*') {
            return '<span class="ds-ts-badge" style="background:rgba(78,154,241,0.25);color:var(--vscode-textLink-foreground,#3794ff);">*</span> All';
        }
        const chars = val.replace(/,/g, '').split('');
        return chars.map(function (ch) {
            const label = TILESET_NAMES[ch] || ch;
            return '<span class="ds-ts-badge" title="' + esc(label) + '">' + esc(ch) + '</span>';
        }).join(' ');
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
        if (!d) {
            // Doodad not found in SLK data — show feedback
            const win = document.getElementById('doodadDetailWindow');
            const body = document.getElementById('doodadDetailBody');
            if (win && body) {
                body.innerHTML = '<div style="padding:1rem;color:var(--vscode-errorForeground,#f44);">'
                    + '<b>' + esc(String(doodId)) + '</b> not found in Doodads.slk<br>'
                    + '<small style="opacity:0.7;">Loaded doodads: ' + Object.keys(_doodadDataMap).length + '</small>'
                    + '</div>';
                win.setAttribute('title-text', '\ud83c\udf33 ' + esc(String(doodId)));
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
                // Render vertex colours from the typed array
                if (d.vertColors && d.vertColors.length > 0) {
                    for (let i = 0; i < d.vertColors.length; i++) {
                        const c = d.vertColors[i];
                        const idx = String(i + 1).padStart(2, '0');
                        rows += '<tr><td class="key">Variation ' + idx + '</td><td>'
                            + c.r + ',' + c.g + ',' + c.b + ' '
                            + _colorBadge(c.r, c.g, c.b) + '</td></tr>';
                    }
                }
            } else if (group.modelFiles) {
                // Model files with variation links
                const filePath = d.file;
                const numVar = d.numVar || 1;
                if (filePath) {
                    const paths = _buildModelPaths(filePath, numVar);
                    const links = paths.map(p =>
                        '<a href="#" class="dd-model-link" data-path="' + esc(p) + '">' + esc(p) + '</a>'
                    ).join('');
                    rows += '<tr><td class="key">file</td><td>' + links + '</td></tr>';
                }
                rows += '<tr><td class="key">numVar</td><td>' + numVar + '</td></tr>';
                // Regular fields in this group
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        const val = d[key];
                        if (val === undefined || val === '' || val === null) continue;
                        rows += '<tr><td class="key">' + esc(label) + '</td><td>' + esc(String(val)) + '</td></tr>';
                    }
                }
            } else {
                // Regular fields
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        const val = d[key];
                        if (val === undefined || val === '' || val === null) continue;
                        let display;
                        if (key === 'name') {
                            display = _gsHtml(val);
                        } else if (key === 'pathTex') {
                            display = '<a href="#" class="dd-pathtex-link" data-pathtex="' + esc(String(val)) + '">' + esc(String(val)) + '</a>';
                        } else {
                            display = esc(String(val));
                        }
                        // Decode category code with badge
                        if (key === 'category' && val) {
                            display = _categoryBadge(val, DOODAD_CATEGORIES);
                        }
                        // Decode tileset codes with badges
                        if (key === 'tilesets' && val) {
                            display = _tilesetBadges(val);
                        }
                        rows += '<tr><td class="key">' + esc(label) + '</td><td>' + display + '</td></tr>';
                    }
                }
                // Single color (e.g. minimap)
                if (group.color) {
                    const c = d[group.color.key];
                    if (c) {
                        rows += '<tr><td class="key">' + esc(group.color.label) + '</td><td>'
                            + c.r + ',' + c.g + ',' + c.b + ' '
                            + _colorBadge(c.r, c.g, c.b) + '</td></tr>';
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

        body.innerHTML = html;
        win.setAttribute('title-text', '\ud83c\udf33 ' + (_gsValue(d.name) || d.doodId));
        win.show();

        // Listen for collapse-group toggle events and persist state
        body.addEventListener('collapse-toggle', function (e) {
            const state = _getDoodCollapseState();
            state[e.detail.title] = e.detail.open;
            _setDoodCollapseState(state);
        });

        // Bind model file & pathTex links via event delegation
        body.addEventListener('click', function (e) {
            var link = e.target.closest('.dd-model-link');
            if (link) {
                e.preventDefault();
                if (_vscode) _vscode.postMessage({command: 'openModel', path: link.getAttribute('data-path')});
                return;
            }
            var ptLink = e.target.closest('.dd-pathtex-link');
            if (ptLink) {
                e.preventDefault();
                showPathTex(ptLink.getAttribute('data-pathtex'));
            }
        });
    }

    // ── Units SLK rebuilder ──────────────────────────────────
    let _unitDataMap = {};

    // Sort state for units
    let _unitSort = {field: null, dir: 'asc'};

    function _saveUnitSort() {
        _patchWvState({_unitSort: {field: _unitSort.field, dir: _unitSort.dir}});
    }

    function _saveUnitFilters() {
        const uncheckedRaces = [];
        document.querySelectorAll('.us-race-cb').forEach(cb => {
            if (!cb.checked) uncheckedRaces.push(cb.getAttribute('data-race'));
        });
        _patchWvState({_unitUncheckedRaces: uncheckedRaces});
    }

    function _restoreUnitFilters() {
        const s = _getWvState();
        const uncheckedRaces = s._unitUncheckedRaces || [];
        if (uncheckedRaces.length) {
            document.querySelectorAll('.us-race-cb').forEach(cb => {
                if (uncheckedRaces.includes(cb.getAttribute('data-race'))) cb.checked = false;
            });
        }
    }

    function _restoreUnitSort() {
        const s = _getWvState();
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
        _updateUnitSortButtons();
        _filterAndRenderUnits();
    }

    function _updateUnitSortButtons() {
        document.querySelectorAll('.us-sort-col').forEach(btn => {
            const f = btn.getAttribute('data-sort');
            btn.classList.remove('ds-sort-active', 'ds-sort-asc', 'ds-sort-desc');
            if (f === _unitSort.field) {
                btn.classList.add('ds-sort-active', _unitSort.dir === 'asc' ? 'ds-sort-asc' : 'ds-sort-desc');
            }
        });
    }

    function _filterAndRenderUnits(saveState) {
        const enabledRaces = new Set();
        document.querySelectorAll('.us-race-cb').forEach(cb => {
            if (cb.checked) enabledRaces.add(cb.getAttribute('data-race'));
        });

        if (saveState !== false) _saveUnitFilters();

        const searchEl = document.getElementById('usSearchInput');
        const q = searchEl ? searchEl.value.toLowerCase().trim() : '';

        const filtered = _allUnits.filter(u => {
            if (q) {
                const name = (_gsValue(u.name) || '').toLowerCase();
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
                var va, vb;
                if (f === 'name') {
                    va = (_gsValue(a.name) || '').toLowerCase();
                    vb = (_gsValue(b.name) || '').toLowerCase();
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

    function _rebuildUnitSidebarCheckboxes() {
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
                cb.addEventListener('change', _filterAndRenderUnits);
                lbl.appendChild(cb);
                lbl.appendChild(document.createTextNode(' ' + label));
                raceChecks.appendChild(lbl);
            }
        }
        _restoreUnitFilters();
    }

    function rebuildUnits(slkData) {
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
                    return '<div class="ts-source" style="margin:1px 0;font-size:11px;">' + esc(s.name) + ': <span class="code">' + esc(s.source) + '</span> <span style="opacity:0.5;">(' + s.rows + ')</span></div>';
                }).join('');
            } else {
                srcEl.setAttribute('group-title', 'SLK Sources (0)');
                srcEl.innerHTML = '<div class="ts-source ts-no-slk">UnitData.slk not found \u2014 set Game Path</div>';
            }
        }

        const totalEl = document.getElementById('usUnitTotal');
        if (totalEl) totalEl.textContent = String(_allUnits.length);

        _rebuildUnitSidebarCheckboxes();
        _restoreUnitSort();
        _updateUnitSortButtons();
        _filterAndRenderUnits(false);

        const searchEl = document.getElementById('usSearchInput');
        if (searchEl && !searchEl._usBound) {
            searchEl._usBound = true;
            searchEl.addEventListener('input', _filterAndRenderUnits);
        }

        document.querySelectorAll('.us-sort-col').forEach(btn => {
            if (btn._usSortBound) return;
            btn._usSortBound = true;
            btn.addEventListener('click', () => _cycleUnitSort(btn.getAttribute('data-sort')));
        });
    }

    // ── Unit detail window populator ──────────────────────────

    const _UNIT_GROUPS = [
        {
            title: '\ud83c\udff7 Identity', fields: [
                ['unitID', 'unitId'],
                ['Name', 'name'],
                ['comment', 'comment'],
                ['sort', 'sort'],
                ['race', 'race'],
                ['tilesets', 'tilesets'],
                ['level', 'level'],
                ['type', 'unitType'],
                ['isBldg', 'isBldg'],
            ]
        },
        {
            title: '\ud83c\udfa8 Model', modelFiles: true, fields: [
                ['modelScale', 'modelScale'],
                ['scale', 'scale'],
                ['scaleBull', 'scaleBull'],
                ['unitShadow', 'unitShadow'],
                ['buildingShadow', 'buildingShadow'],
                ['shadowOnWater', 'shadowOnWater'],
                ['special', 'special'],
                ['unitSound', 'unitSound'],
                ['unitClass', 'unitClass'],
            ],
            color: {key: '_tint', label: 'Tint Color'},
        },
        {
            title: '\u2764 Health & Mana', fields: [
                ['HP', 'hp'],
                ['realHP', 'realHp'],
                ['regenHP', 'regenHp'],
                ['regenType', 'regenType'],
                ['mana0', 'mana0'],
                ['manaN', 'manaN'],
                ['realM', 'realM'],
                ['regenMana', 'regenMana'],
            ]
        },
        {
            title: '\ud83d\udee1 Defence', fields: [
                ['def', 'def'],
                ['defType', 'defType'],
                ['defUp', 'defUp'],
                ['realdef', 'realDef'],
                ['targType', 'targType'],
                ['collision', 'collision'],
            ]
        },
        {
            title: '\u2694 Weapon 1', fields: [
                ['weapTp1', 'weapTp1'],
                ['weapType1', 'weapType1'],
                ['atkType1', 'atkType1'],
                ['dmgplus1', 'dmgplus1'],
                ['dice1', 'dice1'],
                ['sides1', 'sides1'],
                ['cool1', 'cool1'],
                ['rangeN1', 'rangeN1'],
                ['dmgPt1', 'dmgPt1'],
                ['backSw1', 'backSw1'],
                ['targs1', 'targs1'],
                ['splashTargs1', 'splashTargs1'],
                ['showUI1', 'showUi1'],
                ['minRange', 'minRange'],
                ['acquire', 'acquire'],
            ]
        },
        {
            title: '\u2694 Weapon 2', fields: [
                ['weapTp2', 'weapTp2'],
                ['weapType2', 'weapType2'],
                ['atkType2', 'atkType2'],
                ['dmgplus2', 'dmgplus2'],
                ['dice2', 'dice2'],
                ['sides2', 'sides2'],
                ['cool2', 'cool2'],
                ['rangeN2', 'rangeN2'],
                ['dmgPt2', 'dmgPt2'],
                ['backSw2', 'backSw2'],
                ['targs2', 'targs2'],
                ['splashTargs2', 'splashTargs2'],
                ['showUI2', 'showUi2'],
            ]
        },
        {
            title: '\ud83d\udcaa Stats', fields: [
                ['Primary', 'primary'],
                ['STR', 'str'],
                ['STR+', 'strPlus'],
                ['AGI', 'agi'],
                ['AGI+', 'agiPlus'],
                ['INT', 'int'],
                ['INT+', 'intPlus'],
            ]
        },
        {
            title: '\ud83d\udeb6 Movement', fields: [
                ['moveTp', 'moveTp'],
                ['spd', 'spd'],
                ['minSpd', 'minSpd'],
                ['maxSpd', 'maxSpd'],
                ['moveHeight', 'moveHeight'],
                ['moveFloor', 'moveFloor'],
                ['turnRate', 'turnRate'],
                ['propWin', 'propWin'],
            ]
        },
        {
            title: '\ud83d\udc41 Vision & Placement', fields: [
                ['sight', 'sight'],
                ['nsight', 'nsight'],
                ['pathTex', 'pathTex'],
                ['occH', 'occH'],
                ['selZ', 'selZ'],
                ['fogRad', 'fogRad'],
                ['uberSplat', 'uberSplat'],
                ['selCircOnWater', 'selCircOnWater'],
                ['maxPitch', 'maxPitch'],
                ['maxRoll', 'maxRoll'],
                ['elevPts', 'elevPts'],
                ['elevRad', 'elevRad'],
                ['fatLOS', 'fatLos'],
                ['inEditor', 'inEditor'],
                ['hiddenInEditor', 'hiddenInEditor'],
            ]
        },
        {
            title: '\ud83d\udee0 Economy', fields: [
                ['goldcost', 'goldCost'],
                ['lumbercost', 'lumberCost'],
                ['bldtm', 'bldTm'],
                ['reptm', 'repTm'],
                ['goldRep', 'goldRep'],
                ['lumberRep', 'lumberRep'],
                ['fmade', 'fmade'],
                ['fused', 'fused'],
                ['bountyDice', 'bountyDice'],
                ['bountySides', 'bountySides'],
                ['bountyPlus', 'bountyPlus'],
                ['points', 'points'],
            ]
        },
        {
            title: '\ud83d\udcdd Strings', fields: [
                ['Tip', 'tip'],
                ['Ubertip', 'ubertip'],
                ['Hotkey', 'hotkey'],
                ['Propernames', 'propernames'],
                ['Revivetip', 'revivetip'],
                ['Awakentip', 'awakentip'],
                ['EditorSuffix', 'editorSuffix'],
                ['CasterUpgradeName', 'casterUpgradeName'],
                ['CasterUpgradeTip', 'casterUpgradeTip'],
            ]
        },
        {
            title: '\u2139 Meta', fields: [
                ['InBeta', 'inBeta'],
                ['version', 'version'],
            ]
        },
    ];

    function _getUnitCollapseState() {
        const s = _getWvState();
        return s._unitCollapse || {};
    }

    function _setUnitCollapseState(state) {
        _patchWvState({_unitCollapse: state});
    }

    function showUnitDetail(unitId) {
        const u = _unitDataMap[unitId];
        if (!u) {
            const win = document.getElementById('unitDetailWindow');
            const body = document.getElementById('unitDetailBody');
            if (win && body) {
                body.innerHTML = '<div style="padding:1rem;color:var(--vscode-errorForeground,#f44);">'
                    + '<b>' + esc(String(unitId)) + '</b> not found in UnitData.slk<br>'
                    + '<small style="opacity:0.7;">Loaded units: ' + Object.keys(_unitDataMap).length + '</small>'
                    + '</div>';
                win.setAttribute('title-text', '\ud83d\udde1 ' + esc(String(unitId)));
                win.show();
            }
            return;
        }

        const win = document.getElementById('unitDetailWindow');
        if (!win) return;

        const body = document.getElementById('unitDetailBody');
        if (!body) return;

        // Build tint color virtual property
        u._tint = {r: u.red || 255, g: u.green || 255, b: u.blue || 255};

        let html = '';
        const collapseState = _getUnitCollapseState();

        for (const group of _UNIT_GROUPS) {
            let rows = '';

            if (group.modelFiles) {
                const filePath = u.file;
                if (filePath) {
                    const link = '<a href="#" class="dd-model-link" data-path="' + esc(filePath) + '">' + esc(filePath) + '</a>';
                    rows += '<tr><td class="key">file</td><td>' + link + '</td></tr>';
                }
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        const val = u[key];
                        if (val === undefined || val === '' || val === null) continue;
                        rows += '<tr><td class="key">' + esc(label) + '</td><td>' + esc(String(val)) + '</td></tr>';
                    }
                }
                if (group.color) {
                    const c = u[group.color.key];
                    if (c) {
                        rows += '<tr><td class="key">' + esc(group.color.label) + '</td><td>'
                            + c.r + ',' + c.g + ',' + c.b + ' '
                            + _colorBadge(c.r, c.g, c.b) + '</td></tr>';
                    }
                }
            } else {
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        const val = u[key];
                        if (val === undefined || val === '' || val === null) continue;
                        let display;
                        if (key === 'name') {
                            display = _gsHtml(val);
                        } else if (key === 'pathTex') {
                            display = '<a href="#" class="dd-pathtex-link" data-pathtex="' + esc(String(val)) + '">' + esc(String(val)) + '</a>';
                        } else {
                            display = esc(String(val));
                        }
                        rows += '<tr><td class="key">' + esc(label) + '</td><td>' + display + '</td></tr>';
                    }
                }
            }

            if (!rows) continue;

            const isOpen = collapseState.hasOwnProperty(group.title) ? collapseState[group.title] : true;
            html += '<collapse-group group-title="' + esc(group.title) + '"' + (isOpen ? ' open' : '') + '>'
                + '<table class="info">' + rows + '</table>'
                + '</collapse-group>';
        }

        body.innerHTML = html;
        win.setAttribute('title-text', '\ud83d\udde1 ' + (_gsValue(u.name) || u.unitId));
        win.show();

        body.addEventListener('collapse-toggle', function (e) {
            const state = _getUnitCollapseState();
            state[e.detail.title] = e.detail.open;
            _setUnitCollapseState(state);
        });

        body.addEventListener('click', function (e) {
            var link = e.target.closest('.dd-model-link');
            if (link) {
                e.preventDefault();
                if (_vscode) _vscode.postMessage({command: 'openModel', path: link.getAttribute('data-path')});
                return;
            }
            var ptLink = e.target.closest('.dd-pathtex-link');
            if (ptLink) {
                e.preventDefault();
                showPathTex(ptLink.getAttribute('data-pathtex'));
            }
        });
    }

    // ── Destructables SLK rebuilder ────────────────────────────
    let _destructableDataMap = {};
    let _allDestructables = [];
    /** Whether destructablesSlk has been loaded at least once (even if empty). */
    var _destructablesSlkLoaded = false;

    // Sort state for destructables
    let _destSort = {field: null, dir: 'asc'};

    function _saveDestSort() {
        _patchWvState({_destSort: {field: _destSort.field, dir: _destSort.dir}});
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
        _patchWvState({_destUncheckedCats: uncheckedCats, _destUncheckedTs: uncheckedTs});
    }

    function _restoreDestFilters() {
        const s = _getWvState();
        const uncheckedCats = s._destUncheckedCats || [];
        const uncheckedTs = s._destUncheckedTs || [];
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
    }

    function _restoreDestSort() {
        const s = _getWvState();
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
        _updateDestSortButtons();
        _filterAndRenderDestructables();
    }

    function _updateDestSortButtons() {
        document.querySelectorAll('.dt-sort-col').forEach(btn => {
            const f = btn.getAttribute('data-sort');
            btn.classList.remove('ds-sort-active', 'ds-sort-asc', 'ds-sort-desc');
            if (f === _destSort.field) {
                btn.classList.add('ds-sort-active', _destSort.dir === 'asc' ? 'ds-sort-asc' : 'ds-sort-desc');
            }
        });
    }

    function _filterAndRenderDestructables(saveState) {
        const enabledCats = new Set();
        document.querySelectorAll('.dt-cat-cb').forEach(cb => {
            if (cb.checked) enabledCats.add(cb.getAttribute('data-cat'));
        });
        const enabledTs = new Set();
        document.querySelectorAll('.dt-ts-cb').forEach(cb => {
            if (cb.checked) enabledTs.add(cb.getAttribute('data-ts'));
        });

        if (saveState !== false) _saveDestFilters();

        const searchEl = document.getElementById('dtSearchInput');
        const q = searchEl ? searchEl.value.toLowerCase().trim() : '';

        const filtered = _allDestructables.filter(d => {
            if (q) {
                const rn = _gsValue(d.name);
                const rs = _gsValue(d.editorSuffix);
                const name = ((rn || '') + (rs ? ' ' + rs : '')).toLowerCase();
                const id = (d.destructableId || '').toLowerCase();
                const comment = (_gsValue(d.comment) || '').toLowerCase();
                if (!name.includes(q) && !id.includes(q) && !comment.includes(q)) return false;
            }
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
                const va = _gsValue(a[f]).toLowerCase();
                const vb = _gsValue(b[f]).toLowerCase();
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

    function _rebuildDestructableSidebarCheckboxes() {
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
                cb.addEventListener('change', _filterAndRenderDestructables);
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
                cb.addEventListener('change', _filterAndRenderDestructables);
                lbl.appendChild(cb);
                const badge = document.createElement('span');
                badge.className = 'ds-ts-badge';
                badge.textContent = code;
                lbl.appendChild(badge);
                lbl.appendChild(document.createTextNode(' ' + label));
                tsChecks.appendChild(lbl);
            }
        }
        _restoreDestFilters();
    }

    function rebuildDestructables(slkData) {
        _destructablesSlkLoaded = true;
        let source = '';
        _allDestructables = [];
        _destructableDataMap = {};
        if (slkData && slkData.destructables) {
            source = slkData.source || '';
            _destructableDataMap = slkData.destructables;
            _allDestructables = Object.entries(slkData.destructables).map(function (e) { e[1]._rawKey = e[0]; return e[1]; });
        }

        const srcEl = document.getElementById('dtSlkSource');
        if (srcEl) {
            if (source) {
                srcEl.className = 'ts-source';
                srcEl.innerHTML = 'DestructableData.slk: <span class="code">' + esc(source) + '</span>';
            } else {
                srcEl.className = 'ts-source ts-no-slk';
                srcEl.textContent = 'DestructableData.slk not found \u2014 set Game Path';
            }
        }

        const totalEl = document.getElementById('dtDestTotal');
        if (totalEl) totalEl.textContent = String(_allDestructables.length);

        _rebuildDestructableSidebarCheckboxes();
        _restoreDestSort();
        _updateDestSortButtons();
        _filterAndRenderDestructables(false);

        const searchEl = document.getElementById('dtSearchInput');
        if (searchEl && !searchEl._dtBound) {
            searchEl._dtBound = true;
            searchEl.addEventListener('input', _filterAndRenderDestructables);
        }

        document.querySelectorAll('.dt-sort-col').forEach(btn => {
            if (btn._dtSortBound) return;
            btn._dtSortBound = true;
            btn.addEventListener('click', () => _cycleDestSort(btn.getAttribute('data-sort')));
        });
    }

    // ── Destructable detail window populator ────────────────────

    const _DEST_GROUPS = [
        {
            title: '\ud83c\udff7 Identity', fields: [
                ['DestructableID', 'destructableId'],
                ['Name', 'name'],
                ['EditorSuffix', 'editorSuffix'],
                ['comment', 'comment'],
                ['category', 'category'],
                ['doodClass', 'doodClass'],
                ['tilesets', 'tilesets'],
                ['tilesetSpecific', 'tilesetSpecific'],
            ]
        },
        {
            title: '\ud83c\udfa8 Model', modelFiles: true, fields: [
                ['texID', 'texId'],
                ['texFile', 'texFile'],
            ]
        },
        {
            title: '\ud83d\udee1 Combat', fields: [
                ['HP', 'hp'],
                ['armor', 'armor'],
                ['targType', 'targType'],
            ]
        },
        {
            title: '\ud83d\udcd0 Scale', fields: [
                ['minScale', 'minScale'],
                ['maxScale', 'maxScale'],
                ['canPlaceRandScale', 'canPlaceRandScale'],
            ]
        },
        {
            title: '\ud83d\udccd Placement', fields: [
                ['onCliffs', 'onCliffs'],
                ['onWater', 'onWater'],
                ['walkable', 'walkable'],
                ['canPlaceDead', 'canPlaceDead'],
                ['cliffHeight', 'cliffHeight'],
                ['fixedRot', 'fixedRot'],
                ['maxPitch', 'maxPitch'],
                ['maxRoll', 'maxRoll'],
                ['pathTex', 'pathTex'],
                ['pathTexDeath', 'pathTexDeath'],
                ['occH', 'occH'],
                ['flyH', 'flyH'],
            ]
        },
        {
            title: '\ud83d\udc46 Interaction', fields: [
                ['selSize', 'selSize'],
                ['useClickHelper', 'useClickHelper'],
                ['selectable', 'selectable'],
                ['selcircsize', 'selcircsize'],
                ['radius', 'radius'],
                ['fogRadius', 'fogRadius'],
                ['fogVis', 'fogVis'],
                ['lightweight', 'lightweight'],
                ['fatLOS', 'fatLos'],
            ]
        },
        {
            title: '\ud83d\udc41 Rendering', fields: [
                ['shadow', 'shadow'],
                ['deathSnd', 'deathSnd'],
                ['portraitmodel', 'portraitmodel'],
            ],
            color: {key: 'color', label: 'Tint Color'},
        },
        {
            title: '\ud83d\uddfa Minimap', fields: [
                ['showInMM', 'showInMm'],
                ['useMMColor', 'useMmColor'],
            ],
            color: {key: 'mmColor', label: 'Color'},
        },
        {
            title: '\ud83d\udee0 Economy', fields: [
                ['buildTime', 'buildTime'],
                ['repairTime', 'repairTime'],
                ['goldRep', 'goldRep'],
                ['lumberRep', 'lumberRep'],
            ]
        },
        {
            title: '\u2139 Meta', fields: [
                ['InBeta', 'inBeta'],
                ['version', 'version'],
            ]
        },
    ];

    // Collapsed state persistence for destructable detail view
    function _getDestCollapseState() {
        const s = _getWvState();
        return s._destCollapse || {};
    }

    function _setDestCollapseState(state) {
        _patchWvState({_destCollapse: state});
    }

    function showDestructableDetail(destId) {
        const d = _destructableDataMap[destId];
        if (!d) {
            const win = document.getElementById('destructableDetailWindow');
            const body = document.getElementById('destructableDetailBody');
            if (win && body) {
                body.innerHTML = '<div style="padding:1rem;color:var(--vscode-errorForeground,#f44);">'
                    + '<b>' + esc(String(destId)) + '</b> not found in DestructableData.slk<br>'
                    + '<small style="opacity:0.7;">Loaded destructables: ' + Object.keys(_destructableDataMap).length + '</small>'
                    + '</div>';
                win.setAttribute('title-text', '\ud83c\udfda ' + esc(String(destId)));
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
                    const paths = _buildModelPaths(filePath, numVar);
                    const links = paths.map(p =>
                        '<a href="#" class="dd-model-link" data-path="' + esc(p) + '"'
                        + (dtTexId ? ' data-tex-id="' + dtTexId + '"' : '')
                        + (dtTexFile ? ' data-tex-file="' + esc(dtTexFile) + '"' : '')
                        + '>' + esc(p) + '</a>'
                    ).join('');
                    rows += '<tr><td class="key">file</td><td>' + links + '</td></tr>';
                }
                rows += '<tr><td class="key">numVar</td><td>' + numVar + '</td></tr>';
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        const val = d[key];
                        if (val === undefined || val === '' || val === null) continue;
                        rows += '<tr><td class="key">' + esc(label) + '</td><td>' + esc(String(val)) + '</td></tr>';
                    }
                }
            } else {
                if (group.fields) {
                    for (const [label, key] of group.fields) {
                        let val = d[key];
                        if (val === undefined || val === '' || val === null) continue;
                        let display;
                        if (key === 'name' || key === 'editorSuffix' || key === 'comment') {
                            display = _gsHtml(val);
                        } else if (key === 'pathTex' || key === 'pathTexDeath') {
                            display = '<a href="#" class="dd-pathtex-link" data-pathtex="' + esc(String(val)) + '">' + esc(String(val)) + '</a>';
                        } else {
                            display = esc(String(val));
                        }
                        if (key === 'category' && val) {
                            display = _categoryBadge(val, DESTRUCTABLE_CATEGORIES);
                        }
                        if (key === 'tilesets' && val) {
                            display = _tilesetBadges(val);
                        }
                        rows += '<tr><td class="key">' + esc(label) + '</td><td>' + display + '</td></tr>';
                    }
                }
                if (group.color) {
                    const c = d[group.color.key];
                    if (c) {
                        rows += '<tr><td class="key">' + esc(group.color.label) + '</td><td>'
                            + c.r + ',' + c.g + ',' + c.b + ' '
                            + _colorBadge(c.r, c.g, c.b) + '</td></tr>';
                    }
                }
            }

            if (!rows) continue;

            const isOpen = collapseState.hasOwnProperty(group.title) ? collapseState[group.title] : true;
            html += '<collapse-group group-title="' + esc(group.title) + '"' + (isOpen ? ' open' : '') + '>'
                + '<table class="info">' + rows + '</table>'
                + '</collapse-group>';
        }

        body.innerHTML = html;
        const titleName = _gsValue(d.name) || d.destructableId;
        const titleSuffix = _gsValue(d.editorSuffix);
        win.setAttribute('title-text', '\ud83c\udfda ' + titleName + (titleSuffix ? ' ' + titleSuffix : ''));
        win.show();

        body.addEventListener('collapse-toggle', function (e) {
            const state = _getDestCollapseState();
            state[e.detail.title] = e.detail.open;
            _setDestCollapseState(state);
        });

        body.addEventListener('click', function (e) {
            var link = e.target.closest('.dd-model-link');
            if (link) {
                e.preventDefault();
                var cmd = {command: 'openModel', path: link.getAttribute('data-path')};
                var tId = link.getAttribute('data-tex-id');
                var tFile = link.getAttribute('data-tex-file');
                if (tId) cmd.texId = parseInt(tId, 10);
                if (tFile) cmd.texFile = tFile;
                if (_vscode) _vscode.postMessage(cmd);
                return;
            }
            var ptLink = e.target.closest('.dd-pathtex-link');
            if (ptLink) {
                e.preventDefault();
                showPathTex(ptLink.getAttribute('data-pathtex'));
            }
        });
    }

    // ── Placed objects — name resolution from SLK data ──────────

    /** All DOO items from war3map.doo, set during init(). */
    var _doodadDooItems = [];

    function _fmtPlacedF(v) {
        return v != null ? Number(v).toFixed(1) : '—';
    }

    /** Categorize DOO items: resolve names and populate destructable canvas list. */
    function _categorizePlacedItems() {
        var destItems = [];

        for (var i = 0; i < _doodadDooItems.length; i++) {
            var it = _doodadDooItems[i];
            var rawKey = String(it.raw);
            if (_doodadDataMap[rawKey]) {
                it._name = _gsValue(_doodadDataMap[rawKey].name);
                it._error = false;
            } else if (_destructableDataMap[rawKey]) {
                var dObj = _destructableDataMap[rawKey];
                var rn = _gsValue(dObj.name);
                var rs = _gsValue(dObj.editorSuffix);
                it._name = rn + (rs ? ' ' + rs : '');
                it._error = false;
                destItems.push(it);
            } else {
                it._name = '';
                it._error = true;
                destItems.push(it);
            }
        }

        _destDooItems = destItems;

        // Update title
        var titleEl = document.getElementById('destDooTitle');
        if (titleEl) {
            titleEl.textContent = '\ud83c\udfda Placed Destructables (' + destItems.length + ')';
        }

        // Update canvas lists
        if (_doodadDooCanvasList) {
            _doodadDooCanvasList.setData(_doodadDooItems);
        }
        if (_destDooCanvasList) {
            _destDooCanvasList.setData(destItems);
        }
    }

    /** Categorize DOO items and populate placed-doodad / placed-destructable windows. */
    function _updatePlacedNames() {
        // Only categorize DOO items when both SLK catalogs have been loaded
        if (_doodadDooItems.length && _doodadsSlkLoaded && _destructablesSlkLoaded) {
            _categorizePlacedItems();
        }

        // Resolve unit names from SLK data
        for (var j = 0; j < _unitDooItems.length; j++) {
            var u = _unitDooItems[j];
            var rawKey = String(u.raw);
            var uObj = _unitDataMap[rawKey];
            u._name = uObj ? (_gsValue(uObj.name) || uObj.comment || '') : '';
        }
        if (_unitDooCanvasList) {
            _unitDooCanvasList.setData(_unitDooItems);
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
            const replaceableTextures = msg.replaceableTextures || null;

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
                    if (!tex) return;
                    // Determine actual texture path: use replaceable override if available
                    var actualPath = null;
                    if (tex.replaceable_id && replaceableTextures) {
                        if (replaceableTextures._cliffTex0 !== undefined) {
                            actualPath = (tex.replaceable_id % 2 === 0)
                                ? replaceableTextures._cliffTex0
                                : replaceableTextures._cliffTex1;
                        } else if (replaceableTextures[tex.replaceable_id]) {
                            actualPath = replaceableTextures[tex.replaceable_id];
                        }
                    } else if (tex.file_name && !tex.replaceable_id) {
                        actualPath = tex.file_name;
                    }
                    if (!actualPath) return;
                    var url = textureUrl(bs, archivePath, actualPath);
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
                        } else if (tex && tex.replaceable_id && replaceableTextures && replaceableTextures[tex.replaceable_id] && bs) {
                            var replPath = replaceableTextures[tex.replaceable_id];
                            var thumbUrl = textureUrl(bs, archivePath, replPath);
                            if (thumbUrl) {
                                var thumb = document.createElement('img');
                                thumb.className = 'mv-mat-thumb';
                                thumb.src = thumbUrl;
                                thumb.alt = replPath;
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
    var _groundTileCodes = [];
    var _cliffTileCodes = [];

    function init(config) {
        const vscode = config.vscode;
        _vscode = vscode;
        _groundTileCodes = config.groundTileCodes || [];
        _cliffTileCodes = config.cliffTileCodes || [];
        const isArchive = !!config.isArchive;

        // ── Populate initial doodad data map for detail window ──
        if (config.doodadDooItems) {
            _doodadDooItems = config.doodadDooItems;
        }
        if (config.unitDooItems) {
            _unitDooItems = config.unitDooItems;
        }
        if (config.initialDoodadsSlk && config.initialDoodadsSlk.doodads) {
            _doodadsSlkLoaded = true;
            _doodadDataMap = config.initialDoodadsSlk.doodads;
            _allDoodads = Object.entries(config.initialDoodadsSlk.doodads).map(function (e) { e[1]._rawKey = e[0]; return e[1]; });
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

        // ── Populate initial destructable data map for detail window ──
        if (config.initialDestructablesSlk && config.initialDestructablesSlk.destructables) {
            _destructablesSlkLoaded = true;
            _destructableDataMap = config.initialDestructablesSlk.destructables;
            _allDestructables = Object.entries(config.initialDestructablesSlk.destructables).map(function (e) { e[1]._rawKey = e[0]; return e[1]; });
            _restoreDestFilters();
            document.querySelectorAll('.dt-cat-cb').forEach(cb => {
                cb.addEventListener('change', _filterAndRenderDestructables);
            });
            document.querySelectorAll('.dt-ts-cb').forEach(cb => {
                cb.addEventListener('change', _filterAndRenderDestructables);
            });
            document.querySelectorAll('.dt-sort-col').forEach(btn => {
                if (btn._dtSortBound) return;
                btn._dtSortBound = true;
                btn.addEventListener('click', () => _cycleDestSort(btn.getAttribute('data-sort')));
            });
            const dtSearchEl = document.getElementById('dtSearchInput');
            if (dtSearchEl && !dtSearchEl._dtBound) {
                dtSearchEl._dtBound = true;
                dtSearchEl.addEventListener('input', _filterAndRenderDestructables);
            }
            _restoreDestSort();
            _updateDestSortButtons();
            _filterAndRenderDestructables(false);
        }

        // ── Populate initial unit data map for detail window ──
        if (config.initialUnitsSlk && config.initialUnitsSlk.units) {
            _unitDataMap = config.initialUnitsSlk.units;
            _allUnits = Object.entries(config.initialUnitsSlk.units).map(function (e) { e[1]._rawKey = e[0]; return e[1]; });
            _restoreUnitFilters();
            document.querySelectorAll('.us-race-cb').forEach(cb => {
                cb.addEventListener('change', _filterAndRenderUnits);
            });
            document.querySelectorAll('.us-sort-col').forEach(btn => {
                if (btn._usSortBound) return;
                btn._usSortBound = true;
                btn.addEventListener('click', () => _cycleUnitSort(btn.getAttribute('data-sort')));
            });
            const usSearchEl = document.getElementById('usSearchInput');
            if (usSearchEl && !usSearchEl._usBound) {
                usSearchEl._usBound = true;
                usSearchEl.addEventListener('input', _filterAndRenderUnits);
            }
            _restoreUnitSort();
            _updateUnitSortButtons();
            _filterAndRenderUnits(false);
        }

        // ── Resolve placed object names from initial SLK data ──
        _updatePlacedNames();

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

        // ── Status change handler (game path window update) ──────
        onStatusChanged(function (status) {
            if (!status) return;
            var gpBody = document.getElementById('gpBody');
            if (!gpBody) return;
            gpBody.innerHTML = renderGpBody(status);
            bindGpButtons();
        });


        // ── Canvas list lifecycle: create on show, destroy on hide ─
        document.addEventListener('float-toggled', function (evt) {
            var id = evt.detail && evt.detail.id;
            var win = id ? document.getElementById(id) : null;
            if (!win) return;
            if (id === 'doodadsSlkWindow') {
                if (win.open) { _ensureDoodadCanvasList(); _filterAndRenderDoodads(false); }
                else _disposeDoodadCanvasList();
            } else if (id === 'destructablesSlkWindow') {
                if (win.open) { _ensureDestCanvasList(); _filterAndRenderDestructables(false); }
                else _disposeDestCanvasList();
            } else if (id === 'unitsSlkWindow') {
                if (win.open) { _ensureUnitCanvasList(); _filterAndRenderUnits(false); }
                else _disposeUnitCanvasList();
            } else if (id === 'unitDooWindow') {
                if (win.open) { _ensureUnitDooCanvasList(); }
                else _disposeUnitDooCanvasList();
            } else if (id === 'doodadDooWindow') {
                if (win.open) { _ensureDoodadDooCanvasList(); }
                else _disposeDoodadDooCanvasList();
            } else if (id === 'destructableDooWindow') {
                if (win.open) { _ensureDestDooCanvasList(); }
                else _disposeDestDooCanvasList();
            }
        });

        // ── Create canvas lists for placed windows that are already open ────
        var _unitDooWin = document.getElementById('unitDooWindow');
        if (_unitDooWin && _unitDooWin.open) _ensureUnitDooCanvasList();
        var _doodadDooWin = document.getElementById('doodadDooWindow');
        if (_doodadDooWin && _doodadDooWin.open) _ensureDoodadDooCanvasList();
        var _destDooWin = document.getElementById('destructableDooWindow');
        if (_destDooWin && _destDooWin.open) _ensureDestDooCanvasList();

        // (Unit list click is handled by _unitCanvasList onClick callback)

        // ── Model viewer (embedded float-window) ─────────────
        const _modelViewer = _initModelViewer();

        // ── Message router ───────────────────────────────────
        window.addEventListener('message', e => {
            const msg = e.data;
            if (msg && msg.command === 'gamePathChanged') {
                try { _applyGamePathChanged(msg.status, msg.snapshot); } catch (_) {}
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

    // ── Path Texture viewer ────────────────────────────────────────

    /**
     * Fetch and display a pathing texture in the pathTexWindow.
     * @param {string} texPath  Game-internal path (e.g. "PathTextures\\4x4Default.tga")
     */
    function showPathTex(texPath) {
        var win = document.getElementById('pathTexWindow');
        var body = document.getElementById('pathTexBody');
        if (!win || !body) return;

        win.setAttribute('title-text', '\ud83d\udea7 ' + texPath.replace(/\\/g, '/').split('/').pop());
        win.show();
        body.innerHTML = '<div class="ptex-loading">\u231b Loading\u2026</div>';

        var data = window.__W3E_DATA__;
        if (!data || !data.binaryServer) {
            body.innerHTML = '<div class="ptex-error">\u26a0 Binary server not available</div>';
            return;
        }

        var bs = data.binaryServer;
        var params = new URLSearchParams({token: bs.token, path: texPath});
        if (data.isArchive && data.archivePath) params.set('archive', data.archivePath);

        fetch('http://127.0.0.1:' + bs.port + '/w3e/pathTex?' + params)
            .then(function (resp) {
                if (!resp.ok) throw new Error('HTTP ' + resp.status);
                return resp.json();
            })
            .then(function (result) {
                _renderPathTexGrid(body, result, texPath);
            })
            .catch(function (err) {
                body.innerHTML = '<div class="ptex-error">\u26a0 ' + esc(String(err)) + '</div>';
            });
    }

    function _renderPathTexGrid(container, result, texPath) {
        var w = result.width;
        var h = result.height;
        var px = result.pixels; // flat [R,G,B, R,G,B, ...]

        // Legend
        var html = '<div class="ptex-legend">'
            + '<div class="ptex-legend-row">'
            + '<div class="ptex-legend-cell">'
            + '<span style="background:#e53935"></span>'
            + '<span style="background:#43a047"></span>'
            + '<span style="background:#1e88e5"></span>'
            + '<span style="background:#666"></span>'
            + '</div>'
            + '<span>\u2190</span>'
            + '</div>'
            + '<div class="ptex-legend-row">'
            + '<span style="color:#e53935">\u25cf</span> 1 Walk'
            + '<span>\u2003</span>'
            + '<span style="color:#43a047">\u25cf</span> 2 Fly'
            + '</div>'
            + '<div class="ptex-legend-row">'
            + '<span style="color:#1e88e5">\u25cf</span> 3 Build'
            + '<span>\u2003</span>'
            + '<span style="background:#666;display:inline-block;width:10px;height:10px;border-radius:2px;vertical-align:middle;border:1px solid rgba(255,255,255,0.2);"></span> 4 Color'
            + '</div>'
            + '</div>';

        html += '<div class="ptex-source">' + esc(texPath) + ' \u2014 ' + w + '\u00d7' + h + ' \u2014 source: ' + esc(result.source) + '</div>';

        // Grid
        html += '<div class="ptex-grid" style="grid-template-columns:repeat(' + w + ', 24px);">';

        for (var y = 0; y < h; y++) {
            for (var x = 0; x < w; x++) {
                var idx = (y * w + x) * 3;
                var r = px[idx];
                var g = px[idx + 1];
                var b = px[idx + 2];

                // 0x00 = allowed, 0xFF = blocked
                var canWalk = (r === 0);
                var canFly = (g === 0);
                var canBuild = (b === 0);

                var walkColor = canWalk ? '#e53935' : 'rgba(229,57,53,0.12)';
                var flyColor = canFly ? '#43a047' : 'rgba(67,160,71,0.12)';
                var buildColor = canBuild ? '#1e88e5' : 'rgba(30,136,229,0.12)';
                var rgbColor = 'rgb(' + r + ',' + g + ',' + b + ')';

                var title = 'x=' + x + ' y=' + y
                    + '  R=' + r + ' G=' + g + ' B=' + b
                    + '\nWalk: ' + (canWalk ? 'YES' : 'no')
                    + '  Fly: ' + (canFly ? 'YES' : 'no')
                    + '  Build: ' + (canBuild ? 'YES' : 'no');

                html += '<div class="ptex-cell" title="' + esc(title) + '">'
                    + '<span style="background:' + walkColor + '"></span>'
                    + '<span style="background:' + flyColor + '"></span>'
                    + '<span style="background:' + buildColor + '"></span>'
                    + '<span style="background:' + rgbColor + '"></span>'
                    + '</div>';
            }
        }

        html += '</div>';
        container.innerHTML = html;
    }

    // ── Highlight a placed doodad/destructable by DOO index ──────────────
    function highlightPlacedDoodad(dooIndex) {
        // Find the item in doodadDooItems by its index
        var foundIdx = -1;
        for (var i = 0; i < _doodadDooItems.length; i++) {
            if (_doodadDooItems[i].index === dooIndex) { foundIdx = i; break; }
        }
        if (foundIdx < 0) return;

        // Show doodadDooWindow and scroll to the item in canvas
        var win = document.getElementById('doodadDooWindow');
        if (!win) return;
        win.show();
        // Ensure canvas list is created (show() triggers float-toggled → _ensureDoodadDooCanvasList)
        _ensureDoodadDooCanvasList();
        if (_doodadDooCanvasList) {
            _doodadDooCanvasList.scrollToIndex(foundIdx);
        }
    }

    return {init, onStatusChanged, onSnapshotChanged, indexToRgb, syncMenuActive, makeOrbitControls, highlightPlacedDoodad};
})();

