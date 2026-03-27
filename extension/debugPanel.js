// noinspection JSUnusedGlobalSymbols

/**
 * @typedef {Object} DebugEntry
 * @property {string}  timestamp
 * @property {string}  method
 * @property {string}  status - "created" | "running" | "cancelled" | "completed"
 * @property {*}       [id]
 * @property {string}  [detail]
 */

const {window, ViewColumn} = require('vscode')

/** @type {import('vscode').WebviewPanel | undefined} */
let panel

/** @type {DebugEntry[]} */
const entries = []

/** Maximum entries kept in memory. */
const MAX_ENTRIES = 5000

/** Whether debug logging is currently active. */
let enabled = false

/**
 * Post a debug entry to the webview (if open).
 * @param {DebugEntry} entry
 */
function pushEntry(entry) {
    entries.push(entry)
    if (entries.length > MAX_ENTRIES) {
        entries.splice(0, entries.length - MAX_ENTRIES)
    }
    if (panel) {
        panel.webview.postMessage({type: 'entry', entry})
    }
}

/**
 * Open (or reveal) the debug panel.
 * @param {import('vscode-languageclient').LanguageClient} client
 */
function showDebugPanel(client) {
    if (panel) {
        panel.reveal(ViewColumn.Beside)
        return
    }

    panel = window.createWebviewPanel(
        'jassDebugLog',
        'JASS Debug Log',
        ViewColumn.Beside,
        {
            enableScripts: true,
            retainContextWhenHidden: true,
        }
    )

    panel.webview.html = getHtml()

    // Send existing entries to the newly created panel
    for (const entry of entries) {
        panel.webview.postMessage({type: 'entry', entry})
    }

    panel.webview.onDidReceiveMessage(msg => {
        if (msg.type === 'clear') {
            entries.length = 0
        } else if (msg.type === 'toggle') {
            enabled = msg.enabled
            client.sendNotification('custom/debugLogEnable', {enabled})
        }
    })

    panel.onDidDispose(() => {
        panel = undefined
    })

    // Enable logging on the server when the panel opens
    if (!enabled) {
        enabled = true
        client.sendNotification('custom/debugLogEnable', {enabled: true})
        // The webview will show the toggle as ON — sync it
        panel.webview.postMessage({type: 'setEnabled', enabled: true})
    }
}

/**
 * @returns {boolean}
 */
function isEnabled() {
    return enabled
}

function getHtml() {
    return /*html*/`<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>
    :root {
        --bg: var(--vscode-editor-background);
        --fg: var(--vscode-editor-foreground);
        --border: var(--vscode-panel-border, #444);
        --header-bg: var(--vscode-sideBarSectionHeader-background, #252526);
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
        font-family: var(--vscode-font-family, monospace);
        font-size: var(--vscode-font-size, 13px);
        background: var(--bg);
        color: var(--fg);
        overflow: hidden;
        display: flex;
        flex-direction: column;
        height: 100vh;
    }
    .toolbar {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 6px 10px;
        background: var(--header-bg);
        border-bottom: 1px solid var(--border);
        flex-shrink: 0;
    }
    .toolbar button {
        background: var(--vscode-button-background, #0e639c);
        color: var(--vscode-button-foreground, #fff);
        border: none;
        padding: 4px 10px;
        cursor: pointer;
        border-radius: 2px;
        font-size: inherit;
    }
    .toolbar button:hover {
        background: var(--vscode-button-hoverBackground, #1177bb);
    }
    .toolbar label {
        display: flex;
        align-items: center;
        gap: 4px;
        cursor: pointer;
        user-select: none;
    }
    .toolbar .spacer { flex: 1; }
    .toolbar .count {
        opacity: 0.6;
        font-size: 0.9em;
    }

    #log {
        flex: 1;
        overflow-y: auto;
        padding: 0;
    }
    table {
        width: 100%;
        border-collapse: collapse;
        table-layout: fixed;
    }
    th, td {
        text-align: left;
        padding: 2px 6px;
        border-bottom: 1px solid var(--border);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }
    th {
        position: sticky;
        top: 0;
        background: var(--header-bg);
        z-index: 1;
    }
    col.c-time   { width: 100px; }
    col.c-id     { width: 60px; }
    col.c-status { width: 80px; }
    col.c-method { width: 280px; }
    col.c-detail { width: auto; }

    .s-created   { color: var(--vscode-charts-blue, #3794ff); }
    .s-running   { color: var(--vscode-charts-yellow, #cca700); }
    .s-cancelled { color: var(--vscode-charts-red, #f14c4c); }
    .s-completed { color: var(--vscode-charts-green, #89d185); }

    .filter-bar {
        display: flex;
        gap: 6px;
        padding: 4px 10px;
        background: var(--header-bg);
        border-bottom: 1px solid var(--border);
        flex-shrink: 0;
    }
    .filter-bar input {
        flex: 1;
        background: var(--vscode-input-background, #3c3c3c);
        color: var(--vscode-input-foreground, #ccc);
        border: 1px solid var(--vscode-input-border, #555);
        padding: 3px 6px;
        font-size: inherit;
        font-family: inherit;
        border-radius: 2px;
    }
</style>
</head>
<body>
    <div class="toolbar">
        <label>
            <input type="checkbox" id="toggle" checked>
            Enabled
        </label>
        <div class="spacer"></div>
        <span class="count" id="count">0</span>
        <button id="clearBtn">Clear</button>
        <label>
            <input type="checkbox" id="autoScroll" checked>
            Auto-scroll
        </label>
    </div>
    <div class="filter-bar">
        <input type="text" id="filter" placeholder="Filter by method name…">
    </div>
    <div id="log">
        <table>
            <colgroup>
                <col class="c-time">
                <col class="c-id">
                <col class="c-status">
                <col class="c-method">
                <col class="c-detail">
            </colgroup>
            <thead>
                <tr>
                    <th>Time</th>
                    <th>ID</th>
                    <th>Status</th>
                    <th>Method</th>
                    <th>Detail</th>
                </tr>
            </thead>
            <tbody id="tbody"></tbody>
        </table>
    </div>

<script>
    const vscode = acquireVsCodeApi()
    const tbody = document.getElementById('tbody')
    const logDiv = document.getElementById('log')
    const countEl = document.getElementById('count')
    const filterInput = document.getElementById('filter')
    const autoScrollCb = document.getElementById('autoScroll')
    const toggleCb = document.getElementById('toggle')
    let totalCount = 0
    let filterText = ''

    toggleCb.addEventListener('change', () => {
        vscode.postMessage({type: 'toggle', enabled: toggleCb.checked})
    })

    document.getElementById('clearBtn').addEventListener('click', () => {
        tbody.innerHTML = ''
        totalCount = 0
        countEl.textContent = '0'
        vscode.postMessage({type: 'clear'})
    })

    filterInput.addEventListener('input', () => {
        filterText = filterInput.value.toLowerCase()
        for (const row of tbody.rows) {
            const method = row.dataset.method || ''
            row.style.display = method.includes(filterText) ? '' : 'none'
        }
    })

    function addEntry(e) {
        const tr = document.createElement('tr')
        const time = (e.timestamp || '').replace(/^.*T/, '').replace('Z', '')
        const id = e.id != null ? String(e.id) : ''
        const status = e.status || ''
        const method = e.method || ''
        const detail = e.detail || ''

        tr.dataset.method = method.toLowerCase()
        tr.innerHTML =
            '<td>' + esc(time) + '</td>' +
            '<td>' + esc(id) + '</td>' +
            '<td class="s-' + esc(status) + '">' + esc(status) + '</td>' +
            '<td>' + esc(method) + '</td>' +
            '<td title="' + esc(detail) + '">' + esc(detail) + '</td>'

        if (filterText && !tr.dataset.method.includes(filterText)) {
            tr.style.display = 'none'
        }

        tbody.appendChild(tr)
        totalCount++
        countEl.textContent = String(totalCount)

        if (autoScrollCb.checked) {
            logDiv.scrollTop = logDiv.scrollHeight
        }
    }

    function esc(s) {
        return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;')
    }

    window.addEventListener('message', ev => {
        const msg = ev.data
        if (msg.type === 'entry') {
            addEntry(msg.entry)
        } else if (msg.type === 'setEnabled') {
            toggleCb.checked = msg.enabled
        }
    })
</script>
</body>
</html>`
}

module.exports = {showDebugPanel, pushEntry, isEnabled}

