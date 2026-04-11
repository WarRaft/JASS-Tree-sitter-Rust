// noinspection JSUnusedGlobalSymbols

const {window, ViewColumn, Uri, workspace} = require('vscode')
const path = require('path')
const http = require('http')

/** @type {import('vscode').WebviewPanel | undefined} */
let panel

/**
 * @param {import('./serverClient.js').ServerClient} client
 * @param {import('vscode').Uri} extensionUri
 * @param {import('vscode').ExtensionContext} context
 * @param {string} [fileUri]
 */
async function showDiagnostics(client, extensionUri, context, fileUri) {
    if (!fileUri) {
        const editor = window.activeTextEditor
        if (!editor) {
            window.showWarningMessage('No active editor — open a .j or .as file first.')
            return
        }
        fileUri = editor.document.uri.toString()
    }

    if (panel) {
        panel.reveal(ViewColumn.Beside)
    } else {
        panel = window.createWebviewPanel(
            'diagnosticSummary',
            'Diagnostics',
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
                    await window.showTextDocument(doc, {preview: true})
                } catch (e) {
                    window.showErrorMessage(`Cannot open file: ${e.message}`)
                }
            } else if (msg.type === 'refresh') {
                await showDiagnostics(client, extensionUri, context, msg.uri)
            }
        })
    }

    const basename = path.basename(decodeURIComponent(new URL(fileUri).pathname))
    panel.title = `Diagnostics — ${basename}`

    // Set initial HTML with loading state
    panel.webview.html = buildHtml(fileUri)

    // Stream results via SSE
    const info = client.getServerInfo()
    if (!info) return

    const body = Buffer.from(JSON.stringify({uri: fileUri}), 'utf8')
    const qs = new (require('url').URLSearchParams)({token: info.token})

    const req = http.request({
        hostname: '127.0.0.1',
        port: info.port,
        path: `/graph/diagnostics?${qs.toString()}`,
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
            'Content-Length': body.length,
            'Accept': 'text/event-stream',
        },
    }, res => {
        let buffer = ''

        res.on('data', chunk => {
            if (!panel) {
                res.destroy()
                return
            }
            buffer += chunk.toString('utf8')
            const lines = buffer.split('\n')
            buffer = lines.pop() || ''

            for (const line of lines) {
                if (!line.startsWith('data:')) continue
                const dataStr = line.slice(5).trim()
                if (!dataStr) continue
                try {
                    const data = JSON.parse(dataStr)
                    if (panel) {
                        panel.webview.postMessage(data)
                    }
                } catch { /* ignore parse errors */ }
            }
        })

        res.on('end', () => {
            // flush remaining buffer
            if (buffer.startsWith('data:')) {
                const dataStr = buffer.slice(5).trim()
                if (dataStr) {
                    try {
                        const data = JSON.parse(dataStr)
                        if (panel) panel.webview.postMessage(data)
                    } catch { /* ignore */ }
                }
            }
        })
    })

    req.on('error', (e) => {
        if (panel) {
            panel.webview.postMessage({done: true, files: [], error: e.message})
        }
    })

    req.write(body)
    req.end()
}

/**
 * @param {string} rootUri
 * @returns {string}
 */
function buildHtml(rootUri) {
    return /*html*/`<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1.0"/>
<style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
        background: var(--vscode-editor-background, #1e1e1e);
        color: var(--vscode-editor-foreground, #d4d4d4);
        font-family: var(--vscode-font-family, 'Segoe UI', sans-serif);
        font-size: 13px;
        padding: 16px;
        overflow-y: auto;
    }

    .header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: 16px;
    }
    .header h2 {
        font-size: 16px;
        font-weight: 600;
        display: flex;
        align-items: center;
        gap: 8px;
    }
    .header button {
        background: var(--vscode-button-background, #0e639c);
        color: var(--vscode-button-foreground, #fff);
        border: none;
        border-radius: 4px;
        padding: 5px 14px;
        cursor: pointer;
        font-size: 12px;
    }
    .header button:hover {
        background: var(--vscode-button-hoverBackground, #1177bb);
    }
    .header button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .progress-container {
        margin-bottom: 20px;
    }
    .progress-bar-outer {
        width: 100%;
        height: 4px;
        background: var(--vscode-editorWidget-border, #454545);
        border-radius: 2px;
        overflow: hidden;
    }
    .progress-bar-inner {
        height: 100%;
        background: var(--vscode-progressBar-background, #0e70c0);
        border-radius: 2px;
        transition: width 0.15s ease-out;
        width: 0%;
    }
    .progress-text {
        margin-top: 6px;
        font-size: 12px;
        color: var(--vscode-descriptionForeground, #999);
        display: flex;
        align-items: center;
        gap: 8px;
    }
    .spinner {
        display: inline-block;
        width: 14px;
        height: 14px;
        border: 2px solid var(--vscode-descriptionForeground, #999);
        border-top-color: transparent;
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }
    @keyframes spin {
        to { transform: rotate(360deg); }
    }
    .hidden { display: none; }

    .summary {
        display: flex;
        gap: 20px;
        margin-bottom: 20px;
        padding: 12px 16px;
        background: var(--vscode-editorGroupHeader-tabsBackground, #252526);
        border-radius: 6px;
        border: 1px solid var(--vscode-editorWidget-border, #454545);
    }
    .summary-item {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 14px;
        font-weight: 600;
    }
    .summary-item .count { font-size: 20px; }
    .summary-item.errors .count { color: var(--vscode-errorForeground, #f48771); }
    .summary-item.warnings .count { color: var(--vscode-editorWarning-foreground, #cca700); }
    .summary-item.hints .count { color: var(--vscode-editorInfo-foreground, #75beff); }
    .summary-item.files .count { color: var(--vscode-editor-foreground, #d4d4d4); }

    table {
        width: 100%;
        border-collapse: collapse;
        table-layout: auto;
    }
    thead th {
        text-align: left;
        padding: 8px 12px;
        background: var(--vscode-editorGroupHeader-tabsBackground, #252526);
        border-bottom: 2px solid var(--vscode-editorWidget-border, #454545);
        font-weight: 600;
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--vscode-descriptionForeground, #999);
        cursor: pointer;
        user-select: none;
        white-space: nowrap;
    }
    thead th:hover {
        color: var(--vscode-editor-foreground, #d4d4d4);
    }
    thead th .sort-arrow {
        font-size: 10px;
        margin-left: 4px;
    }
    tbody tr {
        cursor: pointer;
        transition: background 0.1s;
    }
    tbody tr:hover {
        background: var(--vscode-list-hoverBackground, #2a2d2e);
    }
    tbody tr:nth-child(even) {
        background: rgba(255,255,255,0.02);
    }
    tbody tr:nth-child(even):hover {
        background: var(--vscode-list-hoverBackground, #2a2d2e);
    }
    tbody td {
        padding: 6px 12px;
        border-bottom: 1px solid rgba(255,255,255,0.04);
        white-space: nowrap;
    }
    .file-name {
        font-weight: 500;
    }
    .frozen-badge {
        display: inline-block;
        font-size: 10px;
        padding: 1px 5px;
        border-radius: 3px;
        margin-left: 6px;
        background: rgba(86, 156, 214, 0.2);
        color: #569cd6;
        vertical-align: middle;
    }
    .badge {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-width: 24px;
        height: 20px;
        padding: 0 6px;
        border-radius: 10px;
        font-size: 12px;
        font-weight: 600;
    }
    .badge.error {
        background: rgba(244, 135, 113, 0.15);
        color: var(--vscode-errorForeground, #f48771);
    }
    .badge.warning {
        background: rgba(204, 167, 0, 0.15);
        color: var(--vscode-editorWarning-foreground, #cca700);
    }
    .badge.hint {
        background: rgba(117, 190, 255, 0.1);
        color: var(--vscode-editorInfo-foreground, #75beff);
    }
    .badge.info {
        background: rgba(117, 190, 255, 0.1);
        color: var(--vscode-editorInfo-foreground, #75beff);
    }
    .badge.zero {
        opacity: 0.25;
    }
    .no-issues {
        text-align: center;
        padding: 40px;
        color: #73c991;
        font-size: 16px;
    }
    .no-issues .icon {
        font-size: 40px;
        margin-bottom: 12px;
    }
</style>
</head>
<body>

<div class="header">
    <h2>🔍 Diagnostic Summary</h2>
    <button id="btnRefresh" title="Rescan all files" disabled>↻ Refresh</button>
</div>

<div class="progress-container" id="progressArea">
    <div class="progress-bar-outer">
        <div class="progress-bar-inner" id="progressBar"></div>
    </div>
    <div class="progress-text">
        <span class="spinner"></span>
        <span id="progressLabel">Scanning files…</span>
    </div>
</div>

<div class="summary hidden" id="summaryBar"></div>
<div id="content"></div>

<script>
const vscode = acquireVsCodeApi();
const rootUri = '${rootUri}';

const allFiles = [];
let sortCol = 'errors';
let sortAsc = false;
let loading = true;

window.addEventListener('message', e => {
    const data = e.data;

    if (data.progress) {
        if (data.index === 0) {
            allFiles.length = 0;
            loading = true;
            document.getElementById('progressArea').classList.remove('hidden');
            document.getElementById('summaryBar').classList.add('hidden');
            document.getElementById('btnRefresh').disabled = true;
            document.getElementById('content').innerHTML = '';
        }
        const pct = data.total > 0 ? ((data.index + 1) / data.total * 100) : 0;
        document.getElementById('progressBar').style.width = pct + '%';
        document.getElementById('progressLabel').textContent =
            'Parsing ' + (data.index + 1) + '/' + data.total + ' — ' + data.file;
    }

    if (data.file_result && data.entry) {
        allFiles.push(data.entry);
        renderTable();
        updateSummary();
    }

    if (data.done) {
        loading = false;
        document.getElementById('progressArea').classList.add('hidden');
        document.getElementById('summaryBar').classList.remove('hidden');
        document.getElementById('btnRefresh').disabled = false;
        updateSummary();
        renderTable();
    }
});

function updateSummary() {
    let totalErrors = 0, totalWarnings = 0, totalHints = 0;
    for (const f of allFiles) {
        totalErrors += f.errors;
        totalWarnings += f.warnings;
        totalHints += f.hints;
    }
    const bar = document.getElementById('summaryBar');
    bar.innerHTML =
        '<div class="summary-item files"><span class="count">' + allFiles.length + '</span> files</div>' +
        '<div class="summary-item errors"><span class="count">' + totalErrors + '</span> errors</div>' +
        '<div class="summary-item warnings"><span class="count">' + totalWarnings + '</span> warnings</div>' +
        '<div class="summary-item hints"><span class="count">' + totalHints + '</span> hints</div>';
    if (!loading) bar.classList.remove('hidden');
}

function renderTable() {
    const sorted = [...allFiles].sort((a, b) => {
        let va = a[sortCol], vb = b[sortCol];
        if (typeof va === 'string') { va = va.toLowerCase(); vb = (vb || '').toLowerCase(); }
        if (typeof va === 'boolean') { va = va ? 1 : 0; vb = vb ? 1 : 0; }
        if (va < vb) return sortAsc ? -1 : 1;
        if (va > vb) return sortAsc ? 1 : -1;
        return 0;
    });

    let totalErrors = 0, totalWarnings = 0, totalHints = 0;
    for (const f of allFiles) {
        totalErrors += f.errors;
        totalWarnings += f.warnings;
        totalHints += f.hints;
    }

    let html = '';
    if (!loading && totalErrors === 0 && totalWarnings === 0 && totalHints === 0) {
        html += '<div class="no-issues">' +
            '<div class="icon">✅</div>' +
            'No issues found across ' + allFiles.length + ' files' +
            '</div>';
    }
    html += buildTable(sorted);
    document.getElementById('content').innerHTML = html;
}

function sortArrow(col) {
    if (sortCol !== col) return '';
    return '<span class="sort-arrow">' + (sortAsc ? '▲' : '▼') + '</span>';
}

function buildTable(files) {
    if (files.length === 0) return '';

    let html = '<table><thead><tr>';
    html += '<th data-col="file">File' + sortArrow('file') + '</th>';
    html += '<th data-col="errors">Errors' + sortArrow('errors') + '</th>';
    html += '<th data-col="warnings">Warnings' + sortArrow('warnings') + '</th>';
    html += '<th data-col="hints">Hints' + sortArrow('hints') + '</th>';
    html += '<th data-col="info">Info' + sortArrow('info') + '</th>';
    html += '</tr></thead><tbody>';

    for (const f of files) {
        html += '<tr data-uri="' + escapeAttr(f.uri) + '">';

        html += '<td class="file-name">' + escapeHtml(f.file);
        if (f.frozen) html += '<span class="frozen-badge">frozen</span>';
        html += '</td>';

        if (f.errors > 0) {
            html += '<td><span class="badge error">' + f.errors + '</span></td>';
        } else {
            html += '<td><span class="badge zero">0</span></td>';
        }

        if (f.warnings > 0) {
            html += '<td><span class="badge warning">' + f.warnings + '</span></td>';
        } else {
            html += '<td><span class="badge zero">0</span></td>';
        }

        if (f.hints > 0) {
            html += '<td><span class="badge hint">' + f.hints + '</span></td>';
        } else {
            html += '<td><span class="badge zero">0</span></td>';
        }

        if (f.info > 0) {
            html += '<td><span class="badge info">' + f.info + '</span></td>';
        } else {
            html += '<td><span class="badge zero">0</span></td>';
        }

        html += '</tr>';
    }

    html += '</tbody></table>';
    return html;
}

function escapeHtml(s) {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function escapeAttr(s) {
    return s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// Sort on header click
document.addEventListener('click', e => {
    const th = e.target.closest('th[data-col]');
    if (th) {
        const col = th.dataset.col;
        if (sortCol === col) {
            sortAsc = !sortAsc;
        } else {
            sortCol = col;
            sortAsc = col === 'file';
        }
        renderTable();
        return;
    }

    const tr = e.target.closest('tr[data-uri]');
    if (tr) {
        vscode.postMessage({type: 'openFile', uri: tr.dataset.uri});
    }
});

// Refresh button
document.getElementById('btnRefresh').addEventListener('click', () => {
    vscode.postMessage({type: 'refresh', uri: rootUri});
});
</script>
</body>
</html>`
}

module.exports = {showDiagnostics}

