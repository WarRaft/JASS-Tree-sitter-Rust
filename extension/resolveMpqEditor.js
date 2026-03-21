// noinspection NpmUsedModulesInstalled
const vscode = require('vscode')
const {MpqFileSystemProvider} = require('./mpqFileSystemProvider.js')

/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('vscode-languageclient').LanguageClient} client
 */
async function resolveMpqEditor(document, webviewPanel, _token, client) {
    const archivePath = document.uri.fsPath
    const fname = document.uri.path.split('/').pop() || 'archive'
    const ext = fname.split('.').pop().toLowerCase()

    /** @type {Object} */
    let result
    try {
        result = await client.sendRequest('mpq/info', {archivePath})
    } catch (e) {
        webviewPanel.webview.html = errorHtml(`Failed to read archive: ${e}`)
        return
    }

    if (result.error) {
        webviewPanel.webview.html = errorHtml(result.error)
        return
    }

    webviewPanel.webview.options = {enableScripts: true}
    webviewPanel.webview.html = renderMpqPage(result, fname, ext)

    // Handle messages from the webview
    webviewPanel.webview.onDidReceiveMessage(async (msg) => {
        if (msg.command === 'browse') {
            await vscode.commands.executeCommand('mpq.browse', document.uri)
        } else if (msg.command === 'openFile') {
            const uri = MpqFileSystemProvider.makeUri(archivePath, msg.name)
            await vscode.commands.executeCommand('vscode.open', uri)
        }
    })
}

function errorHtml(msg) {
    return `<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"/></head>
<body style="background:var(--vscode-editor-background);color:var(--vscode-errorForeground);font-family:var(--vscode-font-family);padding:2rem;">
<h2>⚠ Error</h2><pre>${esc(msg)}</pre>
</body></html>`
}

function esc(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

function fmtSize(bytes) {
    if (bytes == null) return '—'
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`
}

/**
 * Decode W3X/W3M map flags bitmask into human-readable tags.
 * Reference: https://www.hiveworkshop.com/threads/322007/
 */
function decodeMapFlags(flags) {
    if (flags == null) return []
    const names = []
    if (flags & 0x0001) names.push('Hide Minimap in Preview')
    if (flags & 0x0002) names.push('Modify Ally Priorities')
    if (flags & 0x0004) names.push('Melee Map')
    if (flags & 0x0008) names.push('Large (non-default size)')
    if (flags & 0x0010) names.push('Masked Areas Partially Visible')
    if (flags & 0x0020) names.push('Fixed Player Settings')
    if (flags & 0x0040) names.push('Use Custom Forces')
    if (flags & 0x0080) names.push('Use Custom Techtree')
    if (flags & 0x0100) names.push('Use Custom Abilities')
    if (flags & 0x0200) names.push('Use Custom Upgrades')
    if (flags & 0x0400) names.push('Has Properties Menu Opened Before')
    if (flags & 0x0800) names.push('Show Water Waves on Cliff Shores')
    if (flags & 0x1000) names.push('Show Water Waves on Rolling Shores')
    if (flags & 0x2000) names.push('Unknown (0x2000)')
    if (flags & 0x4000) names.push('Unknown (0x4000)')
    if (flags & 0x8000) names.push('Use Item Classification System')
    return names
}

function renderMpqPage(data, fname, ext) {
    const header = data.header || {}
    const isMap = ext === 'w3x' || ext === 'w3m'
    const isCampaign = ext === 'w3n'
    const signature = header.signature || null

    // ── Icon / color per extension ──────────────────────
    const extColors = {
        w3x: '#e53935',
        w3m: '#43a047',
        w3n: '#7e57c2',
        mpq: '#ffab00',
    }
    const color = extColors[ext] || '#ffab00'
    const emoji = isMap ? '🗺' : isCampaign ? '📚' : '📦'

    // ── Header section ──────────────────────────────────
    let headerHtml = ''
    if (signature === 'HM3W' && (isMap || isCampaign)) {
        const rows = []
        rows.push(['Signature', `<code>HM3W</code>`])
        if (header.mapName) rows.push(['Map Name', esc(header.mapName)])
        if (header.maxPlayers != null) rows.push(['Max Players', header.maxPlayers])
        if (header.mapFlags != null) {
            rows.push(['Map Flags', `<code>0x${header.mapFlags.toString(16).toUpperCase().padStart(4, '0')}</code>`])
        }
        headerHtml = `
        <h2>📋 W3X/W3M Header</h2>
        <p class="hint">Format: <a href="https://www.hiveworkshop.com/threads/322007/" title="hiveworkshop.com">hiveworkshop.com/threads/322007</a></p>
        <table class="info">${rows.map(([k, v]) =>
            `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`

        // Map flags tags
        const flagNames = decodeMapFlags(header.mapFlags)
        if (flagNames.length > 0) {
            headerHtml += `<div class="tags">${flagNames.map(f => `<span class="tag">${esc(f)}</span>`).join(' ')}</div>`
        }
    } else if (signature === 'HM3C') {
        const rows = [['Signature', `<code>HM3C</code> (Campaign)`]]
        if (header.campaignVersion != null) rows.push(['Campaign Version', header.campaignVersion])
        if (header.editorVersion != null) rows.push(['Editor Version', header.editorVersion])
        if (header.campaignName) rows.push(['Campaign Name', esc(header.campaignName)])
        if (header.campaignDifficulty) rows.push(['Difficulty', esc(header.campaignDifficulty)])
        headerHtml = `
        <h2>📋 W3N Campaign Header</h2>
        <p class="hint">Format: <a href="https://www.hiveworkshop.com/threads/322007/" title="hiveworkshop.com">hiveworkshop.com/threads/322007</a></p>
        <table class="info">${rows.map(([k, v]) =>
            `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`
    } else if (signature) {
        headerHtml = `<p class="hint">Signature: <code>${esc(signature)}</code> (plain MPQ archive)</p>`
    }

    // ── Minimap ─────────────────────────────────────────
    let minimapHtml = ''
    if (data.minimap && data.minimap.dataUrl) {
        const m = data.minimap
        minimapHtml = `
        <h2>🗺 Minimap</h2>
        <div class="minimap-wrap">
            <img src="${m.dataUrl}" alt="minimap" class="minimap-img" />
            <span class="minimap-size">${m.width} × ${m.height}</span>
        </div>`
    }

    // ── Preview (war3mapPreview.tga / .blp) ─────────────
    let previewHtml = ''
    if (data.preview && data.preview.dataUrl) {
        const p = data.preview
        previewHtml = `
        <h2>🖼 Preview</h2>
        <div class="minimap-wrap">
            <img src="${p.dataUrl}" alt="preview" class="preview-img" />
            <span class="minimap-size">${p.width} × ${p.height} · ${p.format.toUpperCase()}</span>
        </div>`
    }

    // ── W3I summary (from archive) ──────────────────────
    let w3iHtml = ''
    if (data.w3i && data.w3i !== null && typeof data.w3i === 'object') {
        const w = data.w3i
        const rows = []
        if (w.map_name) rows.push(['Map Name (W3I)', esc(w.map_name)])
        if (w.author) rows.push(['Author', esc(w.author)])
        if (w.description) rows.push(['Description', `<span class="desc-short">${esc(w.description.slice(0, 200))}${w.description.length > 200 ? '…' : ''}</span>`])
        if (w.map_width != null && w.map_height != null) rows.push(['Map Size', `${w.map_width} × ${w.map_height}`])
        if (w.is_lua != null) rows.push(['Scripting', w.is_lua === 1 ? 'Lua' : w.is_lua === 0 ? 'JASS' : '—'])
        if (w.save_count != null) rows.push(['Save Count', w.save_count])
        if (w.editor_version != null) rows.push(['Editor Version', w.editor_version])
        if (w.format != null) rows.push(['Format', w.format])

        if (rows.length > 0) {
            w3iHtml = `
            <h2>📄 war3map.w3i</h2>
            <table class="info">${rows.map(([k, v]) =>
                `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`
        }
    }

    // ── File list ───────────────────────────────────────
    const files = data.files || []
    const fileCount = data.fileCount || files.length
    const totalSize = data.totalSize || 0

    let filesHtml = ''
    if (files.length > 0) {
        const fRows = files.map((f, i) => {
            const name = f.name || ''
            return `<tr class="file-row" data-name="${esc(name)}">
                <td class="num">${i + 1}</td>
                <td class="code file-link">${esc(name)}</td>
                <td class="num">${fmtSize(f.size)}</td>
            </tr>`
        }).join('')

        filesHtml = `
        <h2>📂 Files <span class="count">(${fileCount}, ${fmtSize(totalSize)} total)</span></h2>
        <div class="table-wrap">
            <table>
                <thead><tr><th>#</th><th>Name</th><th>Size</th></tr></thead>
                <tbody>${fRows}</tbody>
            </table>
        </div>`
    }

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <style>${sharedStyles(color)}</style>
</head>
<body>
    <div class="header-bar">
        <div class="title-row">
            <span class="archive-icon">${emoji}</span>
            <div>
                <h1>${esc(fname)}</h1>
                <span class="subtitle">${esc(ext.toUpperCase())} Archive · ${fmtSize(totalSize)} · ${fileCount} files</span>
            </div>
        </div>
        <button class="browse-btn" id="browseBtn">
            <span class="codicon">📁</span> Browse MPQ Archive
        </button>
    </div>

    ${headerHtml}
    ${minimapHtml}
    ${previewHtml}
    ${w3iHtml}
    ${filesHtml}

    <script>
        const vscode = acquireVsCodeApi();
        document.getElementById('browseBtn').addEventListener('click', () => {
            vscode.postMessage({ command: 'browse' });
        });
        document.querySelectorAll('.file-row').forEach(row => {
            row.addEventListener('click', () => {
                const name = row.dataset.name;
                if (name) {
                    vscode.postMessage({ command: 'openFile', name });
                }
            });
        });
    </script>
</body>
</html>`
}

function sharedStyles(accentColor) {
    return `
        * { box-sizing: border-box; }
        body {
            background: var(--vscode-editor-background);
            color: var(--vscode-editor-foreground);
            font-family: var(--vscode-font-family), sans-serif;
            font-size: 13px;
            margin: 0;
            padding: 1rem 1.5rem;
        }
        a { color: var(--vscode-textLink-foreground); }
        a:hover { color: var(--vscode-textLink-activeForeground); }
        h1 { font-size: 1.4em; margin: 0; }
        h2 {
            font-size: 1.1em;
            margin: 1.5rem 0 0.5rem;
            border-bottom: 1px solid var(--vscode-editorWidget-border);
            padding-bottom: 0.25rem;
        }
        .count { color: var(--vscode-descriptionForeground); font-weight: normal; }
        .hint {
            color: var(--vscode-descriptionForeground);
            font-size: 12px;
            margin: 0 0 0.5rem;
        }

        .header-bar {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 1rem;
            flex-wrap: wrap;
            margin-bottom: 1rem;
            padding: 1rem;
            background: var(--vscode-editorWidget-background);
            border: 1px solid var(--vscode-editorWidget-border);
            border-radius: 6px;
            border-left: 4px solid ${accentColor};
        }
        .title-row {
            display: flex;
            align-items: center;
            gap: 0.75rem;
        }
        .archive-icon {
            font-size: 2rem;
        }
        .subtitle {
            color: var(--vscode-descriptionForeground);
            font-size: 12px;
        }

        .browse-btn {
            display: inline-flex;
            align-items: center;
            gap: 0.4rem;
            padding: 0.5rem 1rem;
            border: none;
            border-radius: 4px;
            background: var(--vscode-button-background);
            color: var(--vscode-button-foreground);
            font-family: inherit;
            font-size: 13px;
            cursor: pointer;
            white-space: nowrap;
        }
        .browse-btn:hover {
            background: var(--vscode-button-hoverBackground);
        }

        table.info {
            border-collapse: collapse;
            margin-bottom: 1rem;
            width: 100%;
            table-layout: fixed;
        }
        table.info td {
            padding: 0.15rem 0.75rem 0.15rem 0;
            word-break: break-word;
            white-space: pre-wrap;
        }
        table.info .key {
            color: var(--vscode-descriptionForeground);
            white-space: nowrap;
            width: 10rem;
            vertical-align: top;
        }

        .table-wrap {
            overflow-x: auto;
            border: 1px solid var(--vscode-editorWidget-border);
            border-radius: 4px;
            margin-bottom: 0.5rem;
            max-height: 60vh;
            overflow-y: auto;
        }
        table {
            width: 100%;
            border-collapse: collapse;
            white-space: nowrap;
        }
        thead {
            position: sticky;
            top: 0;
            z-index: 1;
        }
        th {
            background: var(--vscode-editorWidget-background);
            color: var(--vscode-descriptionForeground);
            text-align: left;
            padding: 0.35rem 0.6rem;
            border-bottom: 2px solid var(--vscode-editorWidget-border);
            font-weight: 600;
        }
        td {
            padding: 0.25rem 0.6rem;
            border-bottom: 1px solid var(--vscode-editorWidget-border);
        }
        tr:hover td {
            background: var(--vscode-list-hoverBackground);
        }
        .file-row {
            cursor: pointer;
        }
        .file-row:active td {
            background: var(--vscode-list-activeSelectionBackground);
        }
        .file-link {
            color: var(--vscode-textLink-foreground);
        }
        .file-row:hover .file-link {
            color: var(--vscode-textLink-activeForeground);
            text-decoration: underline;
        }
        .num { text-align: right; font-variant-numeric: tabular-nums; }
        .code {
            font-family: var(--vscode-editor-font-family), monospace;
            font-size: 12px;
        }
        code {
            font-family: var(--vscode-editor-font-family), monospace;
            font-size: 12px;
            background: var(--vscode-textBlockQuote-background);
            padding: 0.1rem 0.35rem;
            border-radius: 3px;
        }

        .tags { display: flex; flex-wrap: wrap; gap: 0.4rem; margin: 0.5rem 0 1rem; }
        .tag {
            background: var(--vscode-badge-background);
            color: var(--vscode-badge-foreground);
            padding: 0.15rem 0.5rem;
            border-radius: 3px;
            font-size: 12px;
        }

        .desc-short {
            color: var(--vscode-descriptionForeground);
            font-style: italic;
        }

        .minimap-wrap {
            display: inline-flex;
            flex-direction: column;
            align-items: center;
            gap: 0.3rem;
            border: 1px solid var(--vscode-editorWidget-border);
            border-radius: 4px;
            padding: 0.5rem;
            background: var(--vscode-editorWidget-background);
        }
        .minimap-img {
            max-width: 256px;
            max-height: 256px;
            image-rendering: pixelated;
            display: block;
        }
        .preview-img {
            max-width: 512px;
            max-height: 512px;
            display: block;
        }
        .minimap-size {
            font-size: 11px;
            color: var(--vscode-descriptionForeground);
        }
    `
}

module.exports = {
    resolveMpqEditor
}





