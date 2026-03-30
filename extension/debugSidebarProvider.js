// noinspection JSUnusedGlobalSymbols

/**
 * @typedef {Object} DebugEntry
 * @property {string}  timestamp
 * @property {string}  method
 * @property {string}  status - "created" | "running" | "cancelled" | "completed"
 * @property {*}       [id]
 * @property {string}  [detail]
 * @property {string}  [uri]
 */

/** @type {import('vscode').WebviewView | undefined} */
let view

/** @type {Map<string, DebugEntry[]>} uri -> entries */
const hostUriMap = new Map()

/** Max entries per single URI on the host side. */
const MAX_PER_URI = 500

/** Whether debug logging is currently active. */
let enabled = false

/** @type {import('vscode-languageclient').LanguageClient | undefined} */
let _client

/** @type {Promise | undefined} */
let _clientReady

const NO_URI = '\u27E8no uri\u27E9'

/**
 * Post a debug entry to the webview (if open).
 * @param {DebugEntry} entry
 */
function pushEntry(entry) {
    const key = entry.uri || NO_URI
    if (!hostUriMap.has(key)) hostUriMap.set(key, [])
    const list = hostUriMap.get(key)
    list.push(entry)
    if (list.length > MAX_PER_URI) {
        const drop = Math.floor(MAX_PER_URI * 0.2)
        list.splice(0, drop)
    }
    if (view && view.webview) {
        view.webview.postMessage({type: 'entry', entry})
    }
}

/**
 * @returns {boolean}
 */
function isEnabled() {
    return enabled
}

/**
 * @implements {import('vscode').WebviewViewProvider}
 */
class DebugSidebarProvider {

    static viewType = 'jassDebugSidebar'

    /**
     * @param {import('vscode-languageclient').LanguageClient} client
     * @param {Promise} clientReady
     */
    constructor(client, clientReady) {
        _client = client
        _clientReady = clientReady
    }

    /**
     * @param {import('vscode').WebviewView} webviewView
     * @param {import('vscode').WebviewViewResolveContext} _context
     * @param {import('vscode').CancellationToken} _token
     */
    resolveWebviewView(webviewView, _context, _token) {
        view = webviewView

        webviewView.webview.options = {enableScripts: true}
        webviewView.webview.html = getHtml()

        // Send existing entries
        for (const list of hostUriMap.values()) {
            for (const entry of list) {
                webviewView.webview.postMessage({type: 'entry', entry})
            }
        }

        webviewView.webview.onDidReceiveMessage(async msg => {
            if (msg.type === 'clear') {
                hostUriMap.clear()
            } else if (msg.type === 'clearUri') {
                const list = hostUriMap.get(msg.uri)
                if (list) list.length = 0
            } else if (msg.type === 'toggle') {
                enabled = msg.enabled
                await _clientReady
                _client.sendNotification('custom/debugLogEnable', {enabled})
            } else if (msg.type === 'fetchInit') {
                try {
                    await _clientReady
                    const result = await _client.sendRequest('custom/debugInit', {})
                    webviewView.webview.postMessage({type: 'initData', data: result})
                } catch (e) {
                    webviewView.webview.postMessage({
                        type: 'initData',
                        data: {request: null, response: null, error: e.message}
                    })
                }
            }
        })

        webviewView.onDidDispose(() => { view = undefined })

        // Always re-enable logging when the view is resolved
        enabled = true
        webviewView.webview.postMessage({type: 'setEnabled', enabled: true})
        _clientReady.then(() => {
            _client.sendNotification('custom/debugLogEnable', {enabled: true})
        })
    }
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
        --list-hover: var(--vscode-list-hoverBackground, #2a2d2e);
        --list-active: var(--vscode-list-activeSelectionBackground, #094771);
        --list-active-fg: var(--vscode-list-activeSelectionForeground, #fff);
        --badge-bg: var(--vscode-badge-background, #4d4d4d);
        --badge-fg: var(--vscode-badge-foreground, #fff);
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
        font-family: var(--vscode-font-family, monospace);
        font-size: var(--vscode-font-size, 13px);
        background: var(--bg); color: var(--fg);
        overflow: hidden; display: flex; flex-direction: column; height: 100vh;
    }

    /* ── Tabs ──────────────────────────────────────── */
    .tabs {
        display: flex; background: var(--header-bg);
        border-bottom: 1px solid var(--border); flex-shrink: 0;
    }
    .tab {
        padding: 4px 14px; cursor: pointer; font-size: 0.9em;
        border-bottom: 2px solid transparent; opacity: 0.6; user-select: none;
    }
    .tab:hover { opacity: 0.85; }
    .tab.active { opacity: 1; border-bottom-color: var(--vscode-focusBorder, #007fd4); }
    .tab-content { display: none; flex: 1; flex-direction: column; overflow: hidden; }
    .tab-content.active { display: flex; }

    /* ── Toolbar ───────────────────────────────────── */
    .toolbar {
        display: flex; align-items: center; gap: 6px; padding: 3px 8px;
        background: var(--header-bg); border-bottom: 1px solid var(--border); flex-shrink: 0;
    }
    .toolbar button, .btn {
        background: var(--vscode-button-background, #0e639c);
        color: var(--vscode-button-foreground, #fff);
        border: none; padding: 2px 6px; cursor: pointer;
        border-radius: 2px; font-size: inherit; line-height: 1.4;
    }
    .toolbar button:hover, .btn:hover { background: var(--vscode-button-hoverBackground, #1177bb); }
    .btn-danger { background: var(--vscode-inputValidation-errorBorder, #be1100); }
    .btn-danger:hover { background: var(--vscode-charts-red, #f14c4c); }
    .toolbar label {
        display: flex; align-items: center; gap: 3px;
        cursor: pointer; user-select: none; font-size: 0.9em;
    }
    .toolbar input[type="checkbox"] { outline: none; }
    .toolbar input[type="checkbox"]:focus-visible {
        outline: 1.5px solid var(--vscode-focusBorder, #007acc);
        outline-offset: 1px;
        border-radius: 2px;
    }
    .toolbar .spacer { flex: 1; }
    .toolbar .count { opacity: 0.6; font-size: 0.85em; }

    /* ── Split ─────────────────────────────────────── */
    .split { flex: 1; display: flex; overflow: hidden; }

    /* ── URI pane ──────────────────────────────────── */
    .uri-pane {
        width: 40%; min-width: 100px; border-right: 1px solid var(--border);
        display: flex; flex-direction: column; overflow: hidden;
    }
    .pane-header {
        padding: 3px 8px; font-weight: bold; font-size: 0.85em;
        text-transform: uppercase; letter-spacing: 0.5px;
        background: var(--header-bg); border-bottom: 1px solid var(--border);
        flex-shrink: 0; display: flex; align-items: center; gap: 4px;
    }
    .pane-header .spacer { flex: 1; }
    .pane-header input[type="text"] {
        background: var(--vscode-input-background, #3c3c3c);
        color: var(--vscode-input-foreground, #ccc);
        border: 1px solid var(--vscode-input-border, #555);
        padding: 1px 4px; font-size: inherit; font-family: inherit;
        border-radius: 2px; width: 0; flex: 1; min-width: 40px;
    }
    .uri-list { flex: 1; overflow-y: auto; list-style: none; }
    .uri-list li {
        padding: 2px 8px; cursor: pointer; display: flex; align-items: center;
        gap: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    }
    .uri-list li:hover { background: var(--list-hover); }
    .uri-list li.active { background: var(--list-active); color: var(--list-active-fg); }
    .uri-list .badge {
        background: var(--badge-bg); color: var(--badge-fg);
        font-size: 0.75em; padding: 0 4px; border-radius: 8px;
        flex-shrink: 0; line-height: 1.5;
    }
    .uri-list li.active .badge { background: rgba(255,255,255,0.2); }
    .uri-list .uri-name { overflow: hidden; text-overflow: ellipsis; direction: rtl; text-align: left; flex: 1; }

    /* ── Request table ─────────────────────────────── */
    .request-pane { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
    .request-header-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    #requestLog { flex: 1; overflow-y: auto; }
    table { width: 100%; border-collapse: collapse; table-layout: fixed; }
    th, td {
        text-align: left; padding: 2px 6px; border-bottom: 1px solid var(--border);
        white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    }
    th { position: sticky; top: 0; background: var(--header-bg); z-index: 1; }
    col.c-time   { width: 120px; }
    col.c-id     { width: 50px; }
    col.c-status { width: 20px; }
    col.c-dur    { width: 100px; }
    col.c-method { width: auto; }
    td.status-cell { text-align: center; padding: 0; vertical-align: middle; }
    td.status-cell .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; }
    td.status-cell .dot.s-created   { background: var(--vscode-charts-blue, #3794ff); }
    td.status-cell .dot.s-running   { background: var(--vscode-charts-yellow, #cca700); }
    td.status-cell .dot.s-cancelled { background: var(--vscode-charts-red, #f14c4c); }
    td.status-cell .dot.s-completed { background: var(--vscode-charts-green, #89d185); }
    td.dur-cell { font-size: 0.9em; opacity: 0.85; }
    .empty-state {
        display: flex; align-items: center; justify-content: center;
        height: 100%; opacity: 0.5; font-style: italic; padding: 20px; text-align: center;
    }

    /* ── Splitter ──────────────────────────────────── */
    .splitter { width: 4px; cursor: col-resize; background: transparent; flex-shrink: 0; }
    .splitter:hover, .splitter.dragging { background: var(--vscode-focusBorder, #007fd4); }

    /* ── Init tab ──────────────────────────────────── */
    .init-split { flex: 1; display: flex; overflow: hidden; }
    .init-pane { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
    .init-pane + .init-pane { border-left: 1px solid var(--border); }
    .init-pane .pane-header { justify-content: center; }
    .json-view {
        flex: 1; overflow: auto; padding: 8px 10px;
        font-family: var(--vscode-editor-font-family, 'Courier New', monospace);
        font-size: var(--vscode-editor-font-size, 12px);
        white-space: pre-wrap; word-break: break-all; line-height: 1.5;
    }
    .json-key   { color: var(--vscode-symbolIcon-propertyForeground, #9cdcfe); }
    .json-str   { color: var(--vscode-symbolIcon-stringForeground, #ce9178); }
    .json-num   { color: var(--vscode-symbolIcon-numberForeground, #b5cea8); }
    .json-bool  { color: var(--vscode-symbolIcon-booleanForeground, #569cd6); }
    .json-null  { color: var(--vscode-symbolIcon-nullForeground, #569cd6); }
</style>
</head>
<body>
    <div class="tabs">
        <div class="tab active" data-tab="log">Log</div>
        <div class="tab" data-tab="init">Init</div>
    </div>

    <!-- Log tab -->
    <div class="tab-content active" id="tabLog">
        <div class="toolbar">
            <label><input type="checkbox" id="toggle" checked> ON</label>
            <div class="spacer"></div>
            <span class="count" id="count">0</span>
            <button id="clearBtn" title="Clear all">\u2715</button>
            <label><input type="checkbox" id="autoScroll" checked> \u2193</label>
        </div>
        <div class="split">
            <div class="uri-pane">
                <div class="pane-header">
                    <span>URI</span>
                    <input type="text" id="uriFilter" placeholder="Filter\u2026">
                </div>
                <ul class="uri-list" id="uriList"></ul>
            </div>
            <div class="splitter" id="splitter"></div>
            <div class="request-pane">
                <div class="pane-header">
                    <span class="request-header-text" id="requestHeaderText">Requests</span>
                    <span class="spacer"></span>
                    <button class="btn btn-danger" id="clearUriBtn" style="display:none" title="Clear this URI">\u2715</button>
                </div>
                <div id="requestLog">
                    <div class="empty-state" id="emptyState">Select a URI</div>
                    <table style="display:none" id="requestTable">
                        <colgroup>
                            <col class="c-time"><col class="c-id"><col class="c-status">
                            <col class="c-dur"><col class="c-method">
                        </colgroup>
                        <thead><tr>
                            <th>Time</th><th>ID</th><th></th><th>Duration</th><th>Method</th>
                        </tr></thead>
                        <tbody id="tbody"></tbody>
                    </table>
                </div>
            </div>
        </div>
    </div>

    <!-- Init tab -->
    <div class="tab-content" id="tabInit">
        <div class="toolbar">
            <button id="initRefreshBtn">\u21BB Refresh</button>
        </div>
        <div class="init-split">
            <div class="init-pane">
                <div class="pane-header">Request</div>
                <div class="json-view" id="initRequest"><div class="empty-state">Click Refresh</div></div>
            </div>
            <div class="init-pane">
                <div class="pane-header">Response</div>
                <div class="json-view" id="initResponse"><div class="empty-state">Click Refresh</div></div>
            </div>
        </div>
    </div>

<script>
    const vscode = acquireVsCodeApi()
    const MAX_PER_URI = 500
    const DROP_RATIO  = 0.2

    // ── Tabs ─────────────────────────────────────────
    document.querySelectorAll('.tab').forEach(tab => {
        tab.addEventListener('click', () => {
            document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'))
            document.querySelectorAll('.tab-content').forEach(tc => tc.classList.remove('active'))
            tab.classList.add('active')
            const id = 'tab' + tab.dataset.tab.charAt(0).toUpperCase() + tab.dataset.tab.slice(1)
            document.getElementById(id).classList.add('active')
        })
    })

    // ── DOM refs ─────────────────────────────────────
    const uriListEl       = document.getElementById('uriList')
    const uriFilterEl     = document.getElementById('uriFilter')
    const tbody           = document.getElementById('tbody')
    const requestLog      = document.getElementById('requestLog')
    const requestTable    = document.getElementById('requestTable')
    const emptyState      = document.getElementById('emptyState')
    const requestHeaderText = document.getElementById('requestHeaderText')
    const clearUriBtn     = document.getElementById('clearUriBtn')
    const countEl         = document.getElementById('count')
    const toggleCb        = document.getElementById('toggle')
    const autoScrollCb    = document.getElementById('autoScroll')
    const splitterEl      = document.getElementById('splitter')
    const uriPane         = document.querySelector('.uri-pane')
    const initRequestEl   = document.getElementById('initRequest')
    const initResponseEl  = document.getElementById('initResponse')

    // ── State ────────────────────────────────────────
    const uriMap = new Map()
    const NO_URI = '\u27E8no uri\u27E9'
    let selectedUri = null
    let totalCount = 0
    let uriFilterText = ''
    const STATUS_LABEL = {created:'Created', running:'Running', cancelled:'Cancelled', completed:'Completed'}
    const timingMap = new Map()

    // ── Toggle ───────────────────────────────────────
    toggleCb.addEventListener('change', () => {
        vscode.postMessage({type: 'toggle', enabled: toggleCb.checked})
    })

    // ── Clear all ────────────────────────────────────
    document.getElementById('clearBtn').addEventListener('click', () => {
        uriMap.clear(); timingMap.clear()
        totalCount = 0; countEl.textContent = '0'; selectedUri = null
        renderUriList(); renderRequests()
        vscode.postMessage({type: 'clear'})
    })

    // ── Clear selected URI ───────────────────────────
    clearUriBtn.addEventListener('click', () => {
        if (!selectedUri) return
        const list = uriMap.get(selectedUri)
        if (list) { totalCount -= list.length; list.length = 0; countEl.textContent = String(totalCount) }
        vscode.postMessage({type: 'clearUri', uri: selectedUri})
        updateUriItem(selectedUri); renderRequests()
    })

    // ── URI filter ───────────────────────────────────
    uriFilterEl.addEventListener('input', () => {
        uriFilterText = uriFilterEl.value.toLowerCase(); renderUriList()
    })

    // ── Splitter ─────────────────────────────────────
    ;(function() {
        let startX, startW
        splitterEl.addEventListener('mousedown', e => {
            e.preventDefault(); startX = e.clientX
            startW = uriPane.getBoundingClientRect().width
            splitterEl.classList.add('dragging')
            document.addEventListener('mousemove', onMove)
            document.addEventListener('mouseup', onUp)
        })
        function onMove(e) {
            const w = Math.max(80, Math.min(startW + e.clientX - startX, window.innerWidth - 120))
            uriPane.style.width = w + 'px'
        }
        function onUp() {
            splitterEl.classList.remove('dragging')
            document.removeEventListener('mousemove', onMove)
            document.removeEventListener('mouseup', onUp)
        }
    })()

    // ── Init tab ─────────────────────────────────────
    document.getElementById('initRefreshBtn').addEventListener('click', () => {
        initRequestEl.innerHTML = '<div class="empty-state">Loading\u2026</div>'
        initResponseEl.innerHTML = '<div class="empty-state">Loading\u2026</div>'
        vscode.postMessage({type: 'fetchInit'})
    })

    // ── Entries ──────────────────────────────────────
    function addEntry(e) {
        const key = e.uri || NO_URI
        if (!uriMap.has(key)) uriMap.set(key, [])
        const list = uriMap.get(key)
        list.push(e); totalCount++

        const rid = e.id != null ? String(e.id) : null
        if (rid) {
            const ts = new Date(e.timestamp).getTime()
            if (e.status === 'created') timingMap.set(rid, {created: ts, running: 0})
            else if (e.status === 'running') { const t = timingMap.get(rid); if (t) t.running = ts }
        }

        if (list.length > MAX_PER_URI) {
            const drop = Math.floor(MAX_PER_URI * DROP_RATIO)
            list.splice(0, drop); totalCount -= drop
            if (key === selectedUri) { renderRequests(); countEl.textContent = String(totalCount); updateUriItem(key); return }
        }
        countEl.textContent = String(totalCount); updateUriItem(key)
        if (key === selectedUri) {
            appendRequestRow(e)
            if (autoScrollCb.checked) requestLog.scrollTop = requestLog.scrollHeight
        }
    }

    // ── URI list ─────────────────────────────────────
    function renderUriList() {
        uriListEl.innerHTML = ''
        const keys = [...uriMap.keys()].sort((a,b) => a === NO_URI ? 1 : b === NO_URI ? -1 : a.localeCompare(b))
        for (const k of keys) {
            if (uriFilterText && !k.toLowerCase().includes(uriFilterText)) continue
            uriListEl.appendChild(createUriLi(k))
        }
    }
    function createUriLi(key) {
        const li = document.createElement('li'); li.dataset.uri = key
        if (key === selectedUri) li.classList.add('active')
        const n = document.createElement('span'); n.className = 'uri-name'; n.textContent = shortUri(key); n.title = key
        const b = document.createElement('span'); b.className = 'badge'; b.textContent = String((uriMap.get(key)||[]).length)
        li.appendChild(n); li.appendChild(b)
        li.addEventListener('click', () => selectUri(key))
        return li
    }
    function updateUriItem(key) {
        let li = null
        for (const c of uriListEl.children) { if (c.dataset.uri === key) { li = c; break } }
        if (!li) {
            if (uriFilterText && !key.toLowerCase().includes(uriFilterText)) return
            li = createUriLi(key)
            let inserted = false
            for (const item of [...uriListEl.children]) {
                if (key !== NO_URI && (item.dataset.uri === NO_URI || key.localeCompare(item.dataset.uri) < 0)) {
                    uriListEl.insertBefore(li, item); inserted = true; break
                }
            }
            if (!inserted) uriListEl.appendChild(li)
        } else {
            const b = li.querySelector('.badge'); if (b) b.textContent = String((uriMap.get(key)||[]).length)
        }
    }
    function selectUri(key) {
        selectedUri = key
        for (const li of uriListEl.children) li.classList.toggle('active', li.dataset.uri === key)
        renderRequests()
    }
    function shortUri(uri) {
        if (uri === NO_URI) return NO_URI
        try { const p = uri.split('/'); return p[p.length-1] || uri } catch { return uri }
    }

    // ── Request table ────────────────────────────────
    function renderRequests() {
        tbody.innerHTML = ''
        if (!selectedUri || !uriMap.has(selectedUri)) {
            requestTable.style.display = 'none'; emptyState.style.display = 'flex'
            requestHeaderText.textContent = 'Requests'; requestHeaderText.title = ''
            clearUriBtn.style.display = 'none'; return
        }
        emptyState.style.display = 'none'; requestTable.style.display = ''
        requestHeaderText.textContent = shortUri(selectedUri); requestHeaderText.title = selectedUri
        clearUriBtn.style.display = ''
        for (const e of (uriMap.get(selectedUri)||[])) appendRequestRow(e)
        if (autoScrollCb.checked) requestLog.scrollTop = requestLog.scrollHeight
    }
    function appendRequestRow(e) {
        const tr = document.createElement('tr')
        const time = (e.timestamp||'').replace(/^.*T/,'').replace('Z','')
        const id = e.id != null ? String(e.id) : ''
        const status = e.status || '', method = e.method || ''
        const label = STATUS_LABEL[status] || status
        let dur = ''
        if ((status==='completed'||status==='cancelled') && id) {
            const t = timingMap.get(id)
            if (t && t.created) {
                const now = new Date(e.timestamp).getTime(), total = now - t.created
                const server = t.running ? now - t.running : 0
                const overhead = t.running ? t.running - t.created : total
                dur = t.running ? overhead+'+'+server+'ms' : total+'ms'
            }
        }
        tr.innerHTML =
            '<td>'+esc(time)+'</td><td>'+esc(id)+'</td>' +
            '<td class="status-cell" title="'+esc(label)+'"><span class="dot s-'+esc(status)+'"></span></td>' +
            '<td class="dur-cell">'+esc(dur)+'</td>' +
            '<td title="'+esc(method)+'">'+esc(method)+'</td>'
        tbody.appendChild(tr)
    }

    // ── Helpers ──────────────────────────────────────
    function esc(s) { return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;') }
    function hj(val, ind) {
        const p = '  '.repeat(ind), p1 = '  '.repeat(ind+1)
        if (val === null || val === undefined) return '<span class="json-null">null</span>'
        if (typeof val === 'boolean') return '<span class="json-bool">'+val+'</span>'
        if (typeof val === 'number') return '<span class="json-num">'+val+'</span>'
        if (typeof val === 'string') return '<span class="json-str">"'+esc(val)+'"</span>'
        if (Array.isArray(val)) {
            if (!val.length) return '[]'
            return '[\\n'+val.map(v=>p1+hj(v,ind+1)).join(',\\n')+'\\n'+p+']'
        }
        if (typeof val === 'object') {
            const keys = Object.keys(val)
            if (!keys.length) return '{}'
            return '{\\n'+keys.map(k=>p1+'<span class="json-key">"'+esc(k)+'"</span>: '+hj(val[k],ind+1)).join(',\\n')+'\\n'+p+'}'
        }
        return esc(String(val))
    }

    // ── Messages ─────────────────────────────────────
    window.addEventListener('message', ev => {
        const msg = ev.data
        if (msg.type === 'entry') addEntry(msg.entry)
        else if (msg.type === 'setEnabled') toggleCb.checked = msg.enabled
        else if (msg.type === 'initData') {
            const d = msg.data || {}
            initRequestEl.innerHTML = d.request ? hj(d.request, 0) : '<div class="empty-state">No data</div>'
            initResponseEl.innerHTML = d.response ? hj(d.response, 0) : '<div class="empty-state">No data</div>'
        }
    })
</script>
</body>
</html>`
}

module.exports = {DebugSidebarProvider, pushEntry, isEnabled}

