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

        // Click-to-focus: any click inside the window brings it to front
        this.addEventListener('pointerdown', () => this._bringToFront());

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
        // Cascade: hide all child windows linked via parent-window attribute
        if (this.id) {
            document.querySelectorAll('float-window[parent-window="' + this.id + '"]').forEach(w => w.hide());
        }
        this._notifyToggle();
    }

    _bringToFront() {
        document.querySelectorAll('float-window').forEach(w => { w.style.zIndex = '10'; });
        this.style.zIndex = '11';
        // If this window has children, bring them above
        if (this.id) {
            document.querySelectorAll('float-window[parent-window="' + this.id + '"]').forEach(w => { w.style.zIndex = '12'; });
        }
        // If this is a child window, bring parent and all siblings to front too
        const parentId = this.getAttribute('parent-window');
        if (parentId) {
            const parent = document.getElementById(parentId);
            if (parent) parent.style.zIndex = '11';
            document.querySelectorAll('float-window[parent-window="' + parentId + '"]').forEach(w => { w.style.zIndex = '12'; });
        }
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


// ── <slk-source-list> Custom Element ─────────────────────────────────
// Usage:
//   <slk-source-list id="dsSlkSource"></slk-source-list>
// API:
//   .clear()                – remove all items
//   .addPath(text, onClick) – add a clickable SLK path item
//   .addError(text, onClick)– add an error item (red)
//   .setNotFound(text)      – show a single "not found" message

class SlkSourceList extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({mode: 'open'});
        this.shadowRoot.innerHTML = `
<style>
:host { display: block; font-size: 11px; }
.item {
    background: rgba(255, 255, 255, 0.05);
    padding: 4px 6px;
    word-break: break-all;
}
.item:not(:first-child) { margin-top: 1px; }
.item:first-child { border-radius: 3px 3px 0 0; }
.item:last-child { border-radius: 0 0 3px 3px; }
.item:only-child { border-radius: 3px; }
a {
    color: var(--vscode-textLink-foreground, #3794ff);
    text-decoration: none;
    cursor: pointer;
    display: block;
}
a:hover { text-decoration: underline; }
.error { color: var(--vscode-errorForeground, #f44); }
.error a { color: inherit; }
.not-found {
    color: var(--vscode-errorForeground, #f48771);
    font-style: italic;
    border-radius: 3px;
}
</style>
<div id="c"></div>`;
        this._c = this.shadowRoot.getElementById('c');
    }

    clear() { this._c.innerHTML = ''; }

    addPath(text, onClick) {
        const item = document.createElement('div');
        item.className = 'item';
        const link = document.createElement('a');
        link.href = '#';
        link.textContent = text;
        link.title = 'Open in side tab';
        link.addEventListener('click', (e) => { e.preventDefault(); if (onClick) onClick(); });
        item.appendChild(link);
        this._c.appendChild(item);
    }

    addError(text, onClick) {
        const item = document.createElement('div');
        item.className = 'item error';
        if (onClick) {
            const link = document.createElement('a');
            link.href = '#';
            link.textContent = text;
            link.addEventListener('click', (e) => { e.preventDefault(); onClick(); });
            item.appendChild(link);
        } else {
            item.textContent = text;
        }
        this._c.appendChild(item);
    }

    setNotFound(text) {
        this._c.innerHTML = '';
        const item = document.createElement('div');
        item.className = 'item not-found';
        item.textContent = text;
        this._c.appendChild(item);
    }
}

customElements.define('slk-source-list', SlkSourceList);


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
        return ['index', 'code', 'tile-name', 'tile-path', 'tile-source', 'swatch-color', 'tile-preview'];
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
    color: var(--vscode-textLink-foreground, #3794ff);
    word-break: break-all;
    cursor: pointer;
    opacity: 0.7;
}
.path:empty { display: none; }
.path:hover { opacity: 1; text-decoration: underline; }
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
        this._pathEl = shadow.getElementById('path');

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

        this._pathEl.addEventListener('click', (e) => {
            e.stopPropagation();
            const p = this.getAttribute('tile-path');
            if (p) {
                this.dispatchEvent(new CustomEvent('open-blp', {
                    bubbles: true, composed: true,
                    detail: {path: p}
                }));
            }
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
        s.getElementById('path').textContent = this.getAttribute('tile-source') || this.getAttribute('tile-path') || '';
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

