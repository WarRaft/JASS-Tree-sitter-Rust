// noinspection JSUnusedGlobalSymbols

/**
 * @typedef {Object} ExportEntry
 * @property {string} name
 * @property {string} ns       - "function" | "variable" | "type" | "method" | "property" | "class" | "interface" | "enum" | "mixin"
 * @property {string|null} class_name
 * @property {string} namespace
 * @property {string|null} type_name
 * @property {string|null} return_type
 * @property {string} params
 * @property {boolean} is_constant
 * @property {boolean} is_array
 * @property {string} uri
 * @property {string} file
 * @property {number} decl_line
 * @property {number} decl_char
 */

/**
 * @typedef {Object} ExportResult
 * @property {ExportEntry[]} entries
 */

const {window, ViewColumn, Uri, workspace, Position, Selection} = require('vscode')
const path = require('path')

/** @type {import('vscode').WebviewPanel | undefined} */
let panel

/** @type {string} */
let currentMode = 'file'

/**
 * @param {import('./serverClient.js').ServerClient} client
 * @param {import('vscode').Uri} extensionUri
 * @param {import('vscode').ExtensionContext} context
 * @param {string} [fileUri]
 * @param {string} [mode]
 */
async function showExports(client, extensionUri, context, fileUri, mode) {
    if (!fileUri) {
        const editor = window.activeTextEditor
        if (!editor) {
            window.showWarningMessage('No active editor — open a .j or .as file first.')
            return
        }
        fileUri = editor.document.uri.toString()
    }

    if (mode) currentMode = mode

    /** @type {ExportResult} */
    const result = await client.sendRequest('graph/exports', {uri: fileUri, mode: currentMode})

    if (!result || !result.entries) {
        if (!panel) {
            window.showInformationMessage('No exported symbols for this file.')
            return
        }
    }

    if (panel) {
        panel.reveal(ViewColumn.Beside)
    } else {
        panel = window.createWebviewPanel(
            'exportTable',
            'Exports',
            ViewColumn.Beside,
            {
                enableScripts: true,
                retainContextWhenHidden: true,
            }
        )
        panel.onDidDispose(() => {
            panel = undefined
        })

        panel.webview.onDidReceiveMessage(async (msg) => {
            if (msg.type === 'openFile') {
                try {
                    const uri = Uri.parse(msg.uri)
                    const doc = await workspace.openTextDocument(uri)
                    let pos
                    if (typeof msg.decl_line === 'number' && (msg.decl_line > 0 || msg.decl_char > 0)) {
                        pos = new Position(msg.decl_line, msg.decl_char)
                    } else {
                        const text = doc.getText()
                        const name = msg.name || ''
                        const patterns = [
                            new RegExp(`\\bfunction\\s+${escapeRegex(name)}\\b`),
                            new RegExp(`\\bnative\\s+${escapeRegex(name)}\\b`),
                            new RegExp(`\\btype\\s+${escapeRegex(name)}\\b`),
                            new RegExp(`\\bclass\\s+${escapeRegex(name)}\\b`),
                            new RegExp(`\\binterface\\s+${escapeRegex(name)}\\b`),
                            new RegExp(`\\benum\\s+${escapeRegex(name)}\\b`),
                            new RegExp(`\\bmixin\\s+class\\s+${escapeRegex(name)}\\b`),
                            new RegExp(`\\b${escapeRegex(name)}\\b`),
                        ]
                        pos = new Position(0, 0)
                        for (const pat of patterns) {
                            const m = pat.exec(text)
                            if (m) {
                                pos = doc.positionAt(m.index)
                                break
                            }
                        }
                    }
                    const sel = new Selection(pos, pos)
                    await window.showTextDocument(doc, {selection: sel, preview: true})
                } catch (e) {
                    window.showErrorMessage(`Cannot open file: ${e.message}`)
                }
            } else if (msg.type === 'refresh') {
                await showExports(client, extensionUri, context, msg.uri, msg.mode)
            } else if (msg.type === 'changeMode') {
                await showExports(client, extensionUri, context, msg.uri, msg.mode)
            }
        })
    }

    const basename = path.basename(decodeURIComponent(new URL(fileUri).pathname))
    const modeLabel = currentMode === 'file' ? 'File' : currentMode === 'tree' ? 'Tree' : 'All'
    panel.title = `Exports [${modeLabel}] — ${basename}`
    panel.webview.html = buildHtml(result || {entries: []}, fileUri, currentMode)
}

/** @param {string} s */
function escapeRegex(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * @param {ExportResult} data
 * @param {string} rootUri
 * @param {string} mode
 * @returns {string}
 */
function buildHtml(data, rootUri, mode) {
    const entriesJSON = JSON.stringify(data.entries)

    return /*html*/`<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1.0"/>
<style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
        overflow: hidden;
        background: var(--vscode-editor-background, #1e1e1e);
        color: var(--vscode-editor-foreground, #d4d4d4);
        font-family: var(--vscode-font-family, 'Segoe UI', sans-serif);
        font-size: 12px;
    }
    canvas {
        display: block;
        width: 100vw;
        height: 100vh;
    }
    .toolbar {
        position: fixed;
        top: 8px;
        right: 8px;
        display: flex;
        gap: 6px;
        z-index: 10;
    }
    .toolbar button {
        background: var(--vscode-button-background, #0e639c);
        color: var(--vscode-button-foreground, #fff);
        border: none;
        border-radius: 4px;
        padding: 4px 10px;
        cursor: pointer;
        font-size: 12px;
    }
    .toolbar button:hover {
        background: var(--vscode-button-hoverBackground, #1177bb);
    }
    .status-bar {
        position: fixed;
        top: 8px;
        left: 8px;
        font-size: 13px;
        font-weight: bold;
        z-index: 10;
        padding: 4px 10px;
        border-radius: 4px;
        color: var(--vscode-editor-foreground, #d4d4d4);
        background: rgba(128,128,128,0.1);
    }
    .mode-bar {
        position: fixed;
        top: 40px;
        left: 8px;
        z-index: 10;
        display: flex;
        gap: 0;
        align-items: center;
    }
    .mode-bar button {
        background: var(--vscode-dropdown-background, #3c3c3c);
        color: var(--vscode-dropdown-foreground, #ccc);
        border: 1px solid var(--vscode-dropdown-border, #555);
        padding: 4px 12px;
        cursor: pointer;
        font-size: 12px;
        transition: background 0.1s;
    }
    .mode-bar button:first-child {
        border-radius: 4px 0 0 4px;
    }
    .mode-bar button:last-child {
        border-radius: 0 4px 4px 0;
    }
    .mode-bar button:not(:first-child) {
        border-left: none;
    }
    .mode-bar button.active {
        background: var(--vscode-button-background, #0e639c);
        color: var(--vscode-button-foreground, #fff);
        border-color: var(--vscode-button-background, #0e639c);
    }
    .mode-bar button:hover:not(.active) {
        background: var(--vscode-list-hoverBackground, #2a2d2e);
    }
    .filter-bar {
        position: fixed;
        top: 70px;
        left: 8px;
        z-index: 10;
        display: flex;
        gap: 6px;
        align-items: center;
    }
    .filter-bar input {
        background: var(--vscode-input-background, #3c3c3c);
        color: var(--vscode-input-foreground, #ccc);
        border: 1px solid var(--vscode-input-border, #555);
        border-radius: 4px;
        padding: 3px 8px;
        font-size: 12px;
        width: 220px;
    }
    .filter-bar select {
        background: var(--vscode-dropdown-background, #3c3c3c);
        color: var(--vscode-dropdown-foreground, #ccc);
        border: 1px solid var(--vscode-dropdown-border, #555);
        border-radius: 4px;
        padding: 3px 6px;
        font-size: 12px;
    }
</style>
</head>
<body>

<div class="status-bar" id="statusBar"></div>

<div class="toolbar">
    <button id="btnRefresh" title="Refresh">↻ Refresh</button>
</div>

<div class="mode-bar">
    <button id="modeFile" data-mode="file" title="Current file only">📄 File</button>
    <button id="modeTree" data-mode="tree" title="Import tree of current file">🌲 Tree</button>
    <button id="modeAll" data-mode="all" title="All indexed symbols">🌍 All</button>
</div>

<div class="filter-bar">
    <input type="text" id="filterInput" placeholder="Filter by name…" />
    <select id="nsFilter">
        <option value="">All</option>
        <option value="function">Functions</option>
        <option value="variable">Variables</option>
        <option value="type">Types</option>
        <option value="class">Classes</option>
        <option value="interface">Interfaces</option>
        <option value="enum">Enums</option>
        <option value="mixin">Mixins</option>
        <option value="method">Methods</option>
        <option value="property">Properties</option>
    </select>
</div>

<canvas id="tableCanvas"></canvas>

<script>
const vscode = acquireVsCodeApi();
const allEntries = ${entriesJSON};
const rootUri = '${rootUri}';
let currentMode = '${mode}';

// ── Mode buttons ────────────────────────────────────────────────
const modeButtons = document.querySelectorAll('.mode-bar button');
function updateModeButtons() {
    modeButtons.forEach(btn => {
        btn.classList.toggle('active', btn.dataset.mode === currentMode);
    });
}
updateModeButtons();

modeButtons.forEach(btn => {
    btn.addEventListener('click', () => {
        const newMode = btn.dataset.mode;
        if (newMode === currentMode) return;
        currentMode = newMode;
        updateModeButtons();
        vscode.postMessage({type: 'changeMode', uri: rootUri, mode: newMode});
    });
});

// ── Column definitions ──────────────────────────────────────────
const COLUMNS = [
    { key: 'name',        label: 'Name',        width: 220 },
    { key: 'ns',          label: 'Kind',        width: 80  },
    { key: 'class_name',  label: 'Class',       width: 120 },
    { key: 'namespace',   label: 'Namespace',   width: 120 },
    { key: 'type_name',   label: 'Type',        width: 120 },
    { key: 'return_type', label: 'Return',      width: 100 },
    { key: 'params',      label: 'Params',      width: 260 },
    { key: 'file',        label: 'File',        width: 160 },
];

const ROW_HEIGHT = 22;
const HEADER_HEIGHT = 28;
const TABLE_TOP = 100; // below mode bar + filter bar

// ── State ───────────────────────────────────────────────────────
let sortCol = 'name';
let sortAsc = true;
let filterText = '';
let nsFilter = '';
let scrollY = 0;
let scrollX = 0;
let hoveredRow = -1;
let entries = [...allEntries];

// ── Canvas setup ────────────────────────────────────────────────
const canvas = document.getElementById('tableCanvas');
const ctx = canvas.getContext('2d');
let dpr = window.devicePixelRatio || 1;

function resize() {
    dpr = window.devicePixelRatio || 1;
    canvas.width = window.innerWidth * dpr;
    canvas.height = window.innerHeight * dpr;
    canvas.style.width = window.innerWidth + 'px';
    canvas.style.height = window.innerHeight + 'px';
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    draw();
}
window.addEventListener('resize', resize);

// ── Styling helpers ─────────────────────────────────────────────
const style = getComputedStyle(document.documentElement);
function cssVar(name, fallback) {
    return style.getPropertyValue(name).trim() || fallback;
}

const COLOR_BG        = cssVar('--vscode-editor-background', '#1e1e1e');
const COLOR_FG        = cssVar('--vscode-editor-foreground', '#d4d4d4');
const COLOR_HEADER_BG = cssVar('--vscode-editorGroupHeader-tabsBackground', '#252526');
const COLOR_BORDER    = cssVar('--vscode-editorWidget-border', '#454545');
const COLOR_HOVER     = cssVar('--vscode-list-hoverBackground', '#2a2d2e');
const COLOR_FUNC      = '#dcdcaa';
const COLOR_VAR       = '#9cdcfe';
const COLOR_TYPE      = '#4ec9b0';
const COLOR_KEYWORD   = '#569cd6';
const COLOR_SORT_MARK = '#007acc';

// ── Sort & filter ───────────────────────────────────────────────
function applyFilterAndSort() {
    const lc = filterText.toLowerCase();
    entries = allEntries.filter(e => {
        if (nsFilter && e.ns !== nsFilter) return false;
        if (lc && !e.name.toLowerCase().includes(lc) && !(e.class_name && e.class_name.toLowerCase().includes(lc))) return false;
        return true;
    });
    entries.sort((a, b) => {
        let va = a[sortCol] || '';
        let vb = b[sortCol] || '';
        if (typeof va === 'string') va = va.toLowerCase();
        if (typeof vb === 'string') vb = vb.toLowerCase();
        if (typeof va === 'boolean') { va = va ? 1 : 0; vb = vb ? 1 : 0; }
        if (va < vb) return sortAsc ? -1 : 1;
        if (va > vb) return sortAsc ? 1 : -1;
        return 0;
    });
    scrollY = 0;
    scrollX = 0;
    draw();
}

document.getElementById('filterInput').addEventListener('input', e => {
    filterText = e.target.value;
    applyFilterAndSort();
});
document.getElementById('nsFilter').addEventListener('change', e => {
    nsFilter = e.target.value;
    applyFilterAndSort();
});

// ── Column x positions ──────────────────────────────────────────
function colX(idx) {
    let x = 8 - scrollX;
    for (let i = 0; i < idx; i++) x += COLUMNS[i].width;
    return x;
}
function totalWidth() {
    return COLUMNS.reduce((s, c) => s + c.width, 0) + 16;
}

// ── Drawing ─────────────────────────────────────────────────────
function draw() {
    const W = window.innerWidth;
    const H = window.innerHeight;

    ctx.clearRect(0, 0, W, H);

    // Header background
    ctx.fillStyle = COLOR_HEADER_BG;
    ctx.fillRect(0, TABLE_TOP, W, HEADER_HEIGHT);

    // Header text + sort indicator
    ctx.font = 'bold 12px ' + cssVar('--vscode-font-family', 'monospace');
    for (let i = 0; i < COLUMNS.length; i++) {
        const col = COLUMNS[i];
        const x = colX(i);
        ctx.fillStyle = COLOR_FG;
        let label = col.label;
        if (col.key === sortCol) {
            label += sortAsc ? ' ▲' : ' ▼';
            ctx.fillStyle = COLOR_SORT_MARK;
        }
        ctx.fillText(label, x, TABLE_TOP + HEADER_HEIGHT / 2 + 4);
    }

    // Header bottom border
    ctx.strokeStyle = COLOR_BORDER;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, TABLE_TOP + HEADER_HEIGHT);
    ctx.lineTo(W, TABLE_TOP + HEADER_HEIGHT);
    ctx.stroke();

    // Rows
    const visibleStart = Math.floor(scrollY / ROW_HEIGHT);
    const bodyTop = TABLE_TOP + HEADER_HEIGHT;
    const maxVisible = Math.ceil((H - bodyTop) / ROW_HEIGHT) + 1;

    ctx.font = '12px ' + cssVar('--vscode-font-family', 'monospace');

    for (let vi = 0; vi < maxVisible; vi++) {
        const rowIdx = visibleStart + vi;
        if (rowIdx >= entries.length) break;
        const e = entries[rowIdx];
        const y = bodyTop + vi * ROW_HEIGHT - (scrollY % ROW_HEIGHT);

        if (y + ROW_HEIGHT < bodyTop || y > H) continue;

        // Hover highlight
        if (rowIdx === hoveredRow) {
            ctx.fillStyle = COLOR_HOVER;
            ctx.fillRect(0, y, W, ROW_HEIGHT);
        }

        // Alternating subtle stripe
        if (rowIdx % 2 === 1 && rowIdx !== hoveredRow) {
            ctx.fillStyle = 'rgba(255,255,255,0.02)';
            ctx.fillRect(0, y, W, ROW_HEIGHT);
        }

        const textY = y + ROW_HEIGHT / 2 + 4;

        // Name — color by kind
        const nameColor = (e.ns === 'function' || e.ns === 'method') ? COLOR_FUNC
            : (e.ns === 'class' || e.ns === 'interface' || e.ns === 'enum' || e.ns === 'mixin' || e.ns === 'type') ? COLOR_TYPE
            : COLOR_VAR;
        ctx.fillStyle = nameColor;
        ctx.fillText(clipText(ctx, e.name, COLUMNS[0].width - 8), colX(0), textY);

        // Kind
        ctx.fillStyle = COLOR_KEYWORD;
        ctx.fillText(e.ns, colX(1), textY);

        // Class
        ctx.fillStyle = e.class_name ? COLOR_TYPE : '#666';
        ctx.fillText(clipText(ctx, e.class_name || '—', COLUMNS[2].width - 8), colX(2), textY);

        // Namespace
        ctx.fillStyle = e.namespace ? COLOR_TYPE : '#666';
        ctx.fillText(clipText(ctx, e.namespace || '—', COLUMNS[3].width - 8), colX(3), textY);

        // Type
        ctx.fillStyle = COLOR_TYPE;
        ctx.fillText(clipText(ctx, e.type_name || '—', COLUMNS[4].width - 8), colX(4), textY);

        // Return type
        ctx.fillStyle = COLOR_TYPE;
        ctx.fillText(clipText(ctx, e.return_type || '—', COLUMNS[5].width - 8), colX(5), textY);

        // Params
        ctx.fillStyle = COLOR_FG;
        ctx.fillText(clipText(ctx, e.params || '—', COLUMNS[6].width - 8), colX(6), textY);

        // File
        ctx.fillStyle = '#888';
        ctx.fillText(clipText(ctx, e.file || '', COLUMNS[7].width - 8), colX(7), textY);

        // Row separator
        ctx.strokeStyle = 'rgba(255,255,255,0.04)';
        ctx.beginPath();
        ctx.moveTo(0, y + ROW_HEIGHT);
        ctx.lineTo(W, y + ROW_HEIGHT);
        ctx.stroke();
    }

    // Column separators
    ctx.strokeStyle = COLOR_BORDER;
    ctx.lineWidth = 0.5;
    for (let i = 1; i < COLUMNS.length; i++) {
        const x = colX(i) - 4;
        ctx.beginPath();
        ctx.moveTo(x, TABLE_TOP);
        ctx.lineTo(x, H);
        ctx.stroke();
    }

    // Status
    const modeLabel = currentMode === 'file' ? '📄 File' : currentMode === 'tree' ? '🌲 Tree' : '🌍 All';
    document.getElementById('statusBar').textContent =
        modeLabel + '  •  ' + entries.length + ' / ' + allEntries.length + ' symbols';
}

function clipText(ctx, text, maxWidth) {
    if (ctx.measureText(text).width <= maxWidth) return text;
    while (text.length > 1 && ctx.measureText(text + '…').width > maxWidth) {
        text = text.slice(0, -1);
    }
    return text + '…';
}

// ── Scroll ──────────────────────────────────────────────────────
const maxScroll = () => Math.max(0, entries.length * ROW_HEIGHT - (window.innerHeight - TABLE_TOP - HEADER_HEIGHT));
const maxScrollX = () => Math.max(0, totalWidth() - window.innerWidth);

canvas.addEventListener('wheel', e => {
    e.preventDefault();
    if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
        const dx = e.shiftKey ? e.deltaY : e.deltaX;
        scrollX = Math.max(0, Math.min(maxScrollX(), scrollX + dx));
    } else {
        scrollY = Math.max(0, Math.min(maxScroll(), scrollY + e.deltaY));
    }
    draw();
}, {passive: false});

// ── Mouse interaction ───────────────────────────────────────────
function rowFromY(clientY) {
    const bodyTop = TABLE_TOP + HEADER_HEIGHT;
    if (clientY < bodyTop) return -1;
    const relY = clientY - bodyTop + scrollY;
    return Math.floor(relY / ROW_HEIGHT);
}

canvas.addEventListener('mousemove', e => {
    const newHover = rowFromY(e.clientY);
    if (newHover !== hoveredRow) {
        hoveredRow = newHover;
        canvas.style.cursor = (hoveredRow >= 0 && hoveredRow < entries.length) ? 'pointer' : 'default';
        draw();
    }
});

canvas.addEventListener('mouseleave', () => {
    hoveredRow = -1;
    draw();
});

canvas.addEventListener('click', e => {
    // Header click → sort
    if (e.clientY >= TABLE_TOP && e.clientY < TABLE_TOP + HEADER_HEIGHT) {
        for (let i = 0; i < COLUMNS.length; i++) {
            const x0 = colX(i) - 4;
            const x1 = colX(i) + COLUMNS[i].width - 4;
            if (e.clientX >= x0 && e.clientX < x1) {
                if (sortCol === COLUMNS[i].key) {
                    sortAsc = !sortAsc;
                } else {
                    sortCol = COLUMNS[i].key;
                    sortAsc = true;
                }
                applyFilterAndSort();
                return;
            }
        }
        return;
    }

    // Row click → open file
    const idx = rowFromY(e.clientY);
    if (idx >= 0 && idx < entries.length) {
        const entry = entries[idx];
        vscode.postMessage({type: 'openFile', uri: entry.uri, name: entry.name, decl_line: entry.decl_line || 0, decl_char: entry.decl_char || 0});
    }
});

// ── Keyboard ────────────────────────────────────────────────────
document.addEventListener('keydown', e => {
    if (e.key === 'ArrowDown') {
        scrollY = Math.min(maxScroll(), scrollY + ROW_HEIGHT);
        draw();
    } else if (e.key === 'ArrowUp') {
        scrollY = Math.max(0, scrollY - ROW_HEIGHT);
        draw();
    } else if (e.key === 'ArrowRight') {
        scrollX = Math.min(maxScrollX(), scrollX + 40);
        draw();
    } else if (e.key === 'ArrowLeft') {
        scrollX = Math.max(0, scrollX - 40);
        draw();
    } else if (e.key === 'PageDown') {
        scrollY = Math.min(maxScroll(), scrollY + (window.innerHeight - TABLE_TOP - HEADER_HEIGHT));
        draw();
    } else if (e.key === 'PageUp') {
        scrollY = Math.max(0, scrollY - (window.innerHeight - TABLE_TOP - HEADER_HEIGHT));
        draw();
    } else if (e.key === 'Home') {
        scrollY = 0;
        draw();
    } else if (e.key === 'End') {
        scrollY = maxScroll();
        draw();
    }
});

// ── Refresh button ──────────────────────────────────────────────
document.getElementById('btnRefresh').addEventListener('click', () => {
    vscode.postMessage({type: 'refresh', uri: rootUri, mode: currentMode});
});

// ── Initial render ──────────────────────────────────────────────
applyFilterAndSort();
resize();
</script>
</body>
</html>`
}

module.exports = {showExports}
