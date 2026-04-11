// noinspection JSUnusedGlobalSymbols

/**
 * <physics-panel> — Web Component for vis-network physics tuning.
 *
 * Usage:
 *   const pp = document.querySelector('physics-panel');
 *   pp.physics = savedPhysics || defaultPhysics;  // init
 *   pp.addEventListener('change', e => {
 *       network.setOptions({ physics: e.detail });
 *   });
 *   btnPhysics.onclick = () => pp.toggle();
 */
class PhysicsPanel extends HTMLElement {

    /** vis-network barnesHut physics ranges */
    static FIELDS = [
        // ── barnesHut ───────────────────────────────────────────────────
        {g: 'barnesHut', k: 'theta',                 l: 'Theta',           min: 0.1,    max: 1,     step: 0.01  },
        {g: 'barnesHut', k: 'gravitationalConstant',  l: 'Gravity',         min: -30000, max: 0,     step: 50    },
        {g: 'barnesHut', k: 'centralGravity',         l: 'Central gravity', min: 0,      max: 10,    step: 0.05  },
        {g: 'barnesHut', k: 'springLength',           l: 'Spring length',   min: 0,      max: 500,   step: 5     },
        {g: 'barnesHut', k: 'springConstant',         l: 'Spring constant', min: 0,      max: 1.2,   step: 0.005 },
        {g: 'barnesHut', k: 'damping',                l: 'Damping',         min: 0,      max: 1,     step: 0.01  },
        {g: 'barnesHut', k: 'avoidOverlap',           l: 'Avoid overlap',   min: 0,      max: 1,     step: 0.01  },
        // ── general ─────────────────────────────────────────────────────
        {g: null,        k: 'maxVelocity',            l: 'Max velocity',    min: 1,      max: 150,   step: 1     },
        {g: null,        k: 'minVelocity',            l: 'Min velocity',    min: 0.01,   max: 0.5,   step: 0.01  },
        {g: null,        k: 'timestep',               l: 'Timestep',        min: 0.01,   max: 1,     step: 0.01  },
        // ── wind ────────────────────────────────────────────────────────
        {g: 'wind',      k: 'x',                     l: 'Wind X',          min: -10,    max: 10,    step: 0.5   },
        {g: 'wind',      k: 'y',                     l: 'Wind Y',          min: -10,    max: 10,    step: 0.5   },
    ];

    constructor() {
        super();
        this.attachShadow({mode: 'open'});
        this._physics = null;
    }

    /** @param {Object} obj — full vis-network physics options */
    set physics(obj) {
        this._physics = this._deepClone(obj);
        // ensure sub-objects exist
        if (!this._physics.barnesHut) this._physics.barnesHut = {};
        if (!this._physics.wind) this._physics.wind = {x: 0, y: 0};
        if (this._physics.maxVelocity === undefined) this._physics.maxVelocity = 50;
        if (this._physics.minVelocity === undefined) this._physics.minVelocity = 0.1;
        if (this._physics.timestep === undefined) this._physics.timestep = 0.5;
        this._render();
    }

    /** @returns {Object} */
    get physics() {
        return this._physics;
    }

    toggle() {
        const p = this.shadowRoot.getElementById('panel');
        if (p) p.classList.toggle('open');
    }

    // ── private ────────────────────────────────────────────────────────

    _deepClone(o) {
        return JSON.parse(JSON.stringify(o));
    }

    _get(f) {
        if (f.g) return (this._physics[f.g] || {})[f.k] ?? 0;
        return this._physics[f.k] ?? 0;
    }

    _set(f, v) {
        if (f.g) {
            if (!this._physics[f.g]) this._physics[f.g] = {};
            this._physics[f.g][f.k] = v;
        } else {
            this._physics[f.k] = v;
        }
    }

    _rowsHtml(group) {
        return PhysicsPanel.FIELDS
            .map((f, i) => [f, i])
            .filter(([f]) => f.g === group)
            .map(([f, i]) => {
                const v = this._get(f);
                return `<div class="row">
                    <label title="${f.g ? f.g + '.' : ''}${f.k}">${f.l}</label>
                    <input type="range" data-i="${i}" min="${f.min}" max="${f.max}" step="${f.step}" value="${v}"/>
                    <span class="val">${v}</span>
                </div>`;
            }).join('');
    }

    _render() {
        if (!this._physics) return;

        this.shadowRoot.innerHTML = `
<style>
:host { display: block; }
#panel {
    display: none;
    position: fixed;
    bottom: 8px;
    right: 8px;
    z-index: 20;
    width: 280px;
    max-height: 80vh;
    overflow-y: auto;
    background: var(--vscode-sideBar-background, #252526);
    border: 1px solid var(--vscode-editorWidget-border, #454545);
    border-radius: 6px;
    padding: 12px 14px;
    box-shadow: 0 4px 16px rgba(0,0,0,0.4);
    color: var(--vscode-editor-foreground, #d4d4d4);
    font-family: var(--vscode-font-family, 'Segoe UI', sans-serif);
    font-size: 11px;
}
#panel.open { display: block; }
h3 { margin: 0 0 8px; font-size: 12px; font-weight: 600; }
h4 { margin: 10px 0 6px; font-size: 11px; font-weight: 600; color: #4ec9b0; }
.row {
    display: flex;
    align-items: center;
    margin-bottom: 5px;
}
.row label {
    flex: 0 0 105px;
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}
.row input[type=range] {
    flex: 1;
    min-width: 50px;
    accent-color: var(--vscode-focusBorder, #007acc);
}
.row .val {
    flex: 0 0 55px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-size: 10px;
}
</style>
<div id="panel">
    <h3>⚙ Physics</h3>
    <h4>Barnes-Hut</h4>
    ${this._rowsHtml('barnesHut')}
    <h4>General</h4>
    ${this._rowsHtml(null)}
    <h4>Wind</h4>
    ${this._rowsHtml('wind')}
</div>`;

        this.shadowRoot.querySelectorAll('input[type=range]').forEach(input => {
            input.addEventListener('input', () => {
                const f = PhysicsPanel.FIELDS[parseInt(input.dataset.i)];
                const v = parseFloat(input.value);
                this._set(f, v);
                input.nextElementSibling.textContent = v;
                this.dispatchEvent(new CustomEvent('change', {detail: this._physics}));
            });
        });
    }
}

customElements.define('physics-panel', PhysicsPanel);

