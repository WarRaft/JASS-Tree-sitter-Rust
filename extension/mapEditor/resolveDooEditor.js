/**
 * Response from the server method 'doo/render'.
 *
 * @typedef {Object} DooRenderResult
 * @property {string} magic
 * @property {number} format
 * @property {number} subformat
 * @property {Array<DooItem>} items
 * @property {Array<DooCliff>|null} cliffs
 */

/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('../serverClient.js').ServerClient} client
 */
async function resolveDooEditor(document, webviewPanel, _token, client) {
    /** @type {DooRenderResult} */
    const result = await client.sendRequest('doo/render', {
        uri: document.uri.toString()
    })

    if (result.error) {
        webviewPanel.webview.html = errorHtml(result.error.message)
        return
    }

    const fname = document.uri.path.split('/').pop() || 'doo'
    const isUnit = fname.toLowerCase().includes('units')

    webviewPanel.webview.html = renderDoo(result, fname, isUnit)
}

function errorHtml(msg) {
    return `<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"/></head>
<body style="background:var(--vscode-editor-background);color:var(--vscode-errorForeground);font-family:var(--vscode-font-family);padding:2rem;">
<h2>⚠ Error</h2><pre>${escapeHtml(msg)}</pre>
</body></html>`
}

function escapeHtml(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

function fmtFloat(v) {
    if (v == null) return '—'
    return Number(v).toFixed(2)
}

function renderDoo(data, fname, isUnit) {
    const headerRows = [
        ['Magic', escapeHtml(data.magic)],
        ['Format', data.format],
        ['Sub-format', data.subformat],
        ['Items', data.items ? data.items.length : 0],
    ]
    if (data.cliffs) {
        headerRows.push(['Cliffs', data.cliffs.length])
    }

    const headerHtml = headerRows.map(([k, v]) =>
        `<tr><td class="key">${k}</td><td>${v}</td></tr>`
    ).join('')

    const metaHtml = renderMeta(data._meta)

    // Items table
    let itemsHtml = ''
    if (data.items && data.items.length > 0) {
        const cols = isUnit
            ? ['#', 'Rawcode', 'Skin', 'Var', 'Position', 'Angle°', 'Scale', 'Flag', 'Player']
            : ['#', 'Rawcode', 'Skin', 'Var', 'Position', 'Angle°', 'Scale', 'Flag', 'HP', 'Num']

        const thead = cols.map(c => `<th>${c}</th>`).join('')

        const tbody = data.items.map((it, i) => {
            const pos = `${fmtFloat(it.position.x)}, ${fmtFloat(it.position.y)}, ${fmtFloat(it.position.z)}`
            const angle = fmtFloat(it.angle != null ? it.angle * 180 / Math.PI : null)
            const scale = `${fmtFloat(it.scale.x)}, ${fmtFloat(it.scale.y)}, ${fmtFloat(it.scale.z)}`
            const skin = it.skin != null ? escapeHtml(it.skin) : '—'
            const flag = it.flag != null ? escapeHtml(JSON.stringify(it.flag)) : '—'

            let extra = ''
            if (isUnit && it.unit) {
                extra = `<td>${it.unit.player}</td>`
            } else if (!isUnit && it.doodad) {
                extra = `<td>${it.doodad.health}</td><td>${it.doodad.num}</td>`
            } else {
                extra = isUnit ? '<td>—</td>' : '<td>—</td><td>—</td>'
            }

            return `<tr>
                <td class="num">${i + 1}</td>
                <td class="code">${escapeHtml(it.rawcode.text)}</td>
                <td class="code">${skin}</td>
                <td class="num">${it.variation}</td>
                <td class="mono">${pos}</td>
                <td class="num">${angle}</td>
                <td class="mono">${scale}</td>
                <td>${flag}</td>
                ${extra}
            </tr>`
        }).join('')

        itemsHtml = `
        <h2>${isUnit ? '🗡 Units' : '🌳 Doodads'} <span class="count">(${data.items.length})</span></h2>
        <div class="table-wrap">
            <table>
                <thead><tr>${thead}</tr></thead>
                <tbody>${tbody}</tbody>
            </table>
        </div>`
    }

    // Cliffs table
    let cliffsHtml = ''
    if (data.cliffs && data.cliffs.length > 0) {
        const chead = ['#', 'Rawcode', 'Variation', 'X', 'Y'].map(c => `<th>${c}</th>`).join('')
        const cbody = data.cliffs.map((c, i) => `<tr>
            <td class="num">${i + 1}</td>
            <td class="code">${escapeHtml(c.rawcode.text)}</td>
            <td class="num">${c.variation}</td>
            <td class="num">${c.x}</td>
            <td class="num">${c.y}</td>
        </tr>`).join('')

        cliffsHtml = `
        <h2>🏔 Cliffs <span class="count">(${data.cliffs.length})</span></h2>
        <div class="table-wrap">
            <table>
                <thead><tr>${chead}</tr></thead>
                <tbody>${cbody}</tbody>
            </table>
        </div>`
    }

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <style>${sharedStyles()}</style>
</head>
<body>
    <h1>📦 ${escapeHtml(fname)}</h1>
    ${metaHtml}
    <table class="info">${headerHtml}</table>
    ${itemsHtml}
    ${cliffsHtml}
</body>
</html>`
}

function sharedStyles() {
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
        h1 { font-size: 1.3em; margin: 0 0 0.75rem; }
        h2 {
            font-size: 1.1em;
            margin: 1.5rem 0 0.5rem;
            border-bottom: 1px solid var(--vscode-editorWidget-border);
            padding-bottom: 0.25rem;
        }
        .count { color: var(--vscode-descriptionForeground); font-weight: normal; }

        table.info {
            border-collapse: collapse;
            margin-bottom: 1rem;
        }
        table.info td {
            padding: 0.15rem 0.75rem 0.15rem 0;
        }
        table.info .key {
            color: var(--vscode-descriptionForeground);
            white-space: nowrap;
        }

        .table-wrap {
            overflow-x: auto;
            border: 1px solid var(--vscode-editorWidget-border);
            border-radius: 4px;
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
        .num { text-align: right; font-variant-numeric: tabular-nums; }
        .mono { font-family: var(--vscode-editor-font-family), monospace; font-size: 12px; }
        .code {
            font-family: var(--vscode-editor-font-family), monospace;
            font-size: 12px;
            color: var(--vscode-textLink-foreground);
        }
        .meta-banner {
            display: inline-flex;
            align-items: center;
            gap: 0.5rem;
            padding: 0.3rem 0.75rem;
            border-radius: 4px;
            font-size: 12px;
            margin-bottom: 0.75rem;
            font-variant-numeric: tabular-nums;
        }
        .meta-banner.ok {
            background: rgba(78, 201, 176, 0.12);
            color: #4ec9b0;
            border: 1px solid rgba(78, 201, 176, 0.3);
        }
        .meta-banner.warn {
            background: rgba(224, 108, 64, 0.12);
            color: #e06c40;
            border: 1px solid rgba(224, 108, 64, 0.3);
        }
    `
}

function renderMeta(meta) {
    if (!meta) return ''
    if (meta.remaining === 0) {
        return `<div class="meta-banner ok">✓ All ${meta.total} bytes read</div>`
    }
    return `<div class="meta-banner warn">⚠ ${meta.remaining} of ${meta.total} bytes not read (parser stopped at 0x${meta.read.toString(16).toUpperCase()})</div>`
}

module.exports = {
    resolveDooEditor
}

