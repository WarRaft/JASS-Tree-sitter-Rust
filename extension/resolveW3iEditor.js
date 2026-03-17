/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('vscode-languageclient').LanguageClient} client
 */
async function resolveW3iEditor(document, webviewPanel, _token, client) {
    /** @type {Object} */
    const result = await client.sendRequest('w3i/render', {
        uri: document.uri.toString()
    })

    if (result.error) {
        webviewPanel.webview.html = errorHtml(result.error.message)
        return
    }

    const fname = document.uri.path.split('/').pop() || 'w3i'
    webviewPanel.webview.html = renderW3i(result, fname)
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

function fmtF(v) {
    if (v == null) return '—'
    return Number(v).toFixed(2)
}

function renderW3i(d, fname) {
    // ── General info ────────────────────────────────────
    const general = [
        ['Format', d.format],
        ['Save count', d.save_count],
        ['Editor version', d.editor_version],
        ['Map name', esc(d.map_name)],
        ['Author', esc(d.author)],
        ['Map size', `${d.map_width} × ${d.map_height}`],
        ['Scripting', d.is_lua === 1 ? 'Lua' : d.is_lua === 0 ? 'JASS' : '—'],
    ]

    if (d.editor_version_full) {
        general.splice(3, 0, ['Editor version full', d.editor_version_full.join('.')])
    }

    const generalHtml = general.map(([k, v]) =>
        `<tr><td class="key">${k}</td><td>${v}</td></tr>`
    ).join('')

    const metaHtml = renderMeta(d._meta)

    // ── Description ─────────────────────────────────────
    const descHtml = d.description
        ? `<details><summary>Description</summary><pre class="desc">${esc(d.description)}</pre></details>`
        : ''

    const pDescHtml = d.players_description
        ? `<details><summary>Players description</summary><pre class="desc">${esc(d.players_description)}</pre></details>`
        : ''

    // ── Map flags ───────────────────────────────────────
    let flagsHtml = ''
    if (d.map_flags && typeof d.map_flags === 'object') {
        const entries = Object.entries(d.map_flags)
            .filter(([, v]) => v)
            .map(([k]) => `<span class="tag">${esc(k)}</span>`)
        if (entries.length > 0) {
            flagsHtml = `<h2>🚩 Map Flags</h2><div class="tags">${entries.join(' ')}</div>`
        }
    }

    // ── Camera bounds ───────────────────────────────────
    const cam = d.cam_bounds
    let camHtml = ''
    if (cam) {
        camHtml = `<h2>📐 Camera Bounds</h2>
        <table class="info">
            <tr><td class="key">LB</td><td>${fmtF(cam.lb.x)}, ${fmtF(cam.lb.y)}</td></tr>
            <tr><td class="key">RT</td><td>${fmtF(cam.rt.x)}, ${fmtF(cam.rt.y)}</td></tr>
            <tr><td class="key">LT</td><td>${fmtF(cam.lt.x)}, ${fmtF(cam.lt.y)}</td></tr>
            <tr><td class="key">RB</td><td>${fmtF(cam.rb.x)}, ${fmtF(cam.rb.y)}</td></tr>
        </table>`
    }

    // ── Fog / weather ───────────────────────────────────
    let fogHtml = ''
    if (d.fog != null) {
        const fogRows = [
            ['Fog type', d.fog],
            ['Start', fmtF(d.fog_start)],
            ['End', fmtF(d.fog_end)],
            ['Density', fmtF(d.fog_density)],
            ['Fog color', d.fog_color != null ? `0x${d.fog_color.toString(16).padStart(8, '0')}` : '—'],
            ['Weather', d.weather || '—'],
            ['Sound', d.sound || '—'],
            ['Water color', d.water_color != null ? `0x${d.water_color.toString(16).padStart(8, '0')}` : '—'],
        ]
        fogHtml = `<h2>🌫 Fog & Weather</h2><table class="info">${fogRows.map(([k, v]) =>
            `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`
    }

    // ── Loading screen ──────────────────────────────────
    const loadRows = [
        ['Number', d.loadscreen_num],
        ['Path', esc(d.loadscreen_path || '—')],
        ['Title', esc(d.loadscreen_title || '—')],
        ['Subtitle', esc(d.loadscreen_subtitle || '—')],
        ['Text', esc(d.loadscreen_text || '—')],
    ]
    const loadHtml = `<h2>🖼 Loading Screen</h2><table class="info">${loadRows.map(([k, v]) =>
        `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`

    // ── Prologue ────────────────────────────────────────
    let prologueHtml = ''
    if (d.prologue_title || d.prologue_text) {
        const pRows = [
            ['Path', esc(d.prologue_path || '—')],
            ['Title', esc(d.prologue_title || '—')],
            ['Subtitle', esc(d.prologue_subtitle || '—')],
            ['Text', esc(d.prologue_text || '—')],
        ]
        prologueHtml = `<h2>📜 Prologue</h2><table class="info">${pRows.map(([k, v]) =>
            `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`
    }

    // ── Players ─────────────────────────────────────────
    let playersHtml = ''
    if (d.players && d.players.length > 0) {
        const pHead = ['#', 'Name', 'Type', 'Race', 'Position', 'Fix'].map(c => `<th>${c}</th>`).join('')
        const pBody = d.players.map((p, i) => {
            const ptype = p.player_type ? (typeof p.player_type === 'string' ? p.player_type : JSON.stringify(p.player_type)) : '—'
            const race = p.race ? (typeof p.race === 'string' ? p.race : JSON.stringify(p.race)) : '—'
            return `<tr>
                <td class="num">${p.num != null ? p.num : i}</td>
                <td>${esc(p.name)}</td>
                <td>${esc(ptype)}</td>
                <td>${esc(race)}</td>
                <td class="mono">${fmtF(p.pos.x)}, ${fmtF(p.pos.y)}</td>
                <td class="num">${p.fix}</td>
            </tr>`
        }).join('')

        playersHtml = `
        <h2>👤 Players <span class="count">(${d.players.length})</span></h2>
        <div class="table-wrap">
            <table><thead><tr>${pHead}</tr></thead><tbody>${pBody}</tbody></table>
        </div>`
    }

    // ── Clans / Forces ──────────────────────────────────
    let clansHtml = ''
    if (d.clans && d.clans.length > 0) {
        const cHead = ['#', 'Name', 'Players', 'Flags'].map(c => `<th>${c}</th>`).join('')
        const cBody = d.clans.map((c, i) => {
            const flags = c.flags && typeof c.flags === 'object'
                ? Object.entries(c.flags).filter(([, v]) => v).map(([k]) => k).join(', ') || '—'
                : '—'
            return `<tr>
                <td class="num">${i + 1}</td>
                <td>${esc(c.name)}</td>
                <td class="num">${c.players}</td>
                <td>${esc(flags)}</td>
            </tr>`
        }).join('')

        clansHtml = `
        <h2>⚔ Forces <span class="count">(${d.clans.length})</span></h2>
        <div class="table-wrap">
            <table><thead><tr>${cHead}</tr></thead><tbody>${cBody}</tbody></table>
        </div>`
    }

    // ── Upgrades ────────────────────────────────────────
    let upgradesHtml = ''
    if (d.upgrades && d.upgrades.length > 0) {
        const uHead = ['#', 'ID', 'Level', 'Status'].map(c => `<th>${c}</th>`).join('')
        const uBody = d.upgrades.map((u, i) => {
            const status = u.status ? (typeof u.status === 'string' ? u.status : JSON.stringify(u.status)) : '—'
            return `<tr>
                <td class="num">${i + 1}</td>
                <td class="code">${esc(u.id)}</td>
                <td class="num">${u.level}</td>
                <td>${esc(status)}</td>
            </tr>`
        }).join('')

        upgradesHtml = `
        <h2>⬆ Upgrades <span class="count">(${d.upgrades.length})</span></h2>
        <div class="table-wrap">
            <table><thead><tr>${uHead}</tr></thead><tbody>${uBody}</tbody></table>
        </div>`
    }

    // ── Techs ───────────────────────────────────────────
    let techsHtml = ''
    if (d.techs && d.techs.length > 0) {
        const tHead = ['#', 'ID'].map(c => `<th>${c}</th>`).join('')
        const tBody = d.techs.map((t, i) => `<tr>
            <td class="num">${i + 1}</td>
            <td class="code">${esc(t.id)}</td>
        </tr>`).join('')

        techsHtml = `
        <h2>🔬 Disabled Techs <span class="count">(${d.techs.length})</span></h2>
        <div class="table-wrap">
            <table><thead><tr>${tHead}</tr></thead><tbody>${tBody}</tbody></table>
        </div>`
    }

    // ── Random groups ───────────────────────────────────
    let groupsHtml = ''
    if (d.groups && d.groups.length > 0) {
        const gItems = d.groups.map((g) => {
            const chancesHead = ['Chance %', ...g.column_types.map((_, ci) => `Col ${ci + 1}`)].map(c => `<th>${c}</th>`).join('')
            const chancesBody = g.chances.map(ch => {
                const ids = ch.ids.map(id => `<td class="code">${esc(id)}</td>`).join('')
                return `<tr><td class="num">${ch.chance}%</td>${ids}</tr>`
            }).join('')

            return `<details><summary>${esc(g.name)} <span class="count">(#${g.num}, ${g.chances.length} rows)</span></summary>
            <div class="table-wrap">
                <table><thead><tr>${chancesHead}</tr></thead><tbody>${chancesBody}</tbody></table>
            </div></details>`
        }).join('')

        groupsHtml = `<h2>🎲 Random Groups <span class="count">(${d.groups.length})</span></h2>${gItems}`
    }

    // ── Random item tables ──────────────────────────────
    let itemsHtml = ''
    if (d.items && d.items.length > 0) {
        const iItems = d.items.map(item => {
            const gParts = item.groups.map((g, gi) => {
                const rows = g.chances.map(ch => `<tr>
                    <td class="num">${ch.chance}%</td>
                    <td class="code">${esc(ch.id)}</td>
                </tr>`).join('')
                return rows ? `<div class="sub-group"><em>Set ${gi + 1}</em>
                    <table><thead><tr><th>Chance</th><th>ID</th></tr></thead><tbody>${rows}</tbody></table>
                </div>` : ''
            }).join('')

            return `<details><summary>${esc(item.name)} <span class="count">(#${item.num})</span></summary>${gParts}</details>`
        }).join('')

        itemsHtml = `<h2>🎁 Random Item Tables <span class="count">(${d.items.length})</span></h2>${iItems}`
    }

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <style>${sharedStyles()}</style>
</head>
<body>
    <h1>🗺 ${esc(fname)}</h1>
    ${metaHtml}
    <table class="info">${generalHtml}</table>
    ${descHtml}
    ${pDescHtml}
    ${flagsHtml}
    ${camHtml}
    ${fogHtml}
    ${loadHtml}
    ${prologueHtml}
    ${playersHtml}
    ${clansHtml}
    ${upgradesHtml}
    ${techsHtml}
    ${groupsHtml}
    ${itemsHtml}
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
            margin-bottom: 0.5rem;
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

        details {
            margin: 0.5rem 0;
            border: 1px solid var(--vscode-editorWidget-border);
            border-radius: 4px;
            padding: 0.25rem 0.5rem;
        }
        details[open] {
            padding-bottom: 0.5rem;
        }
        summary {
            cursor: pointer;
            padding: 0.25rem 0;
            font-weight: 600;
        }
        pre.desc {
            background: var(--vscode-textBlockQuote-background);
            border: 1px solid var(--vscode-editorWidget-border);
            border-radius: 4px;
            padding: 0.5rem;
            white-space: pre-wrap;
            word-break: break-word;
            margin: 0.5rem 0;
        }

        .tags { display: flex; flex-wrap: wrap; gap: 0.4rem; }
        .tag {
            background: var(--vscode-badge-background);
            color: var(--vscode-badge-foreground);
            padding: 0.15rem 0.5rem;
            border-radius: 3px;
            font-size: 12px;
        }

        .sub-group {
            margin: 0.5rem 0;
        }
        .sub-group em {
            display: block;
            margin-bottom: 0.25rem;
            color: var(--vscode-descriptionForeground);
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
    resolveW3iEditor
}

