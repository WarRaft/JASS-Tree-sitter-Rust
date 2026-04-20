const {esc, fmtSize, fmtF} = require('./utils.js')

// ── Map Info panel content ──────────────────────────────────────────
function renderMapInfoContent(mapInfo) {
    if (!mapInfo.mapName) {
        return `
            <div class="mi-no-map">
                <div class="mi-warn">⚠ Map not found</div>
                <div class="mi-file">Current file: <span class="mi-code">${esc(mapInfo.currentFile)}</span></div>
                <div class="mi-hint">No <span class="mi-code">.w3x</span> or <span class="mi-code">.w3m</span> directory found in parent path.</div>
            </div>`
    }

    let binaryRows = ''
    if (mapInfo.binaries && mapInfo.binaries.length > 0) {
        binaryRows = mapInfo.binaries.map(b => {
            const icon = b.exists ? '✅' : '❌'
            const cls = b.exists ? 'mi-exists' : 'mi-missing'
            const current = mapInfo.currentFile === b.file ? ' mi-current' : ''
            return `<div class="mi-row ${cls}${current}">
                <span class="mi-icon">${icon}</span>
                <span class="mi-file-name">${esc(b.file)}</span>
                <span class="mi-label">${esc(b.label)}</span>
            </div>`
        }).join('')
    }

    const emoji = mapInfo.isMap ? '🗺' : '📦'
    return `
        <div class="mi-header">${emoji} ${esc(mapInfo.mapName)}</div>
        <div class="mi-divider"></div>
        ${binaryRows ? `<div class="mi-binaries">${binaryRows}</div>` : ''}`
}

// ── Map flags decoder ───────────────────────────────────────────────
function decodeMapFlags(flags) {
    if (flags == null) return []
    const names = []
    if (flags & 0x0001) names.push('Hide Minimap in Preview')
    if (flags & 0x0002) names.push('Modify Ally Priorities')
    if (flags & 0x0004) names.push('Melee Map')
    if (flags & 0x0008) names.push('Custom Terrain Type')
    if (flags & 0x0010) names.push('Masked Areas Partially Visible')
    if (flags & 0x0020) names.push('Fixed Player Settings')
    if (flags & 0x0040) names.push('Use Custom Forces')
    if (flags & 0x0080) names.push('Use Custom Techtree')
    if (flags & 0x0100) names.push('Use Custom Abilities')
    if (flags & 0x0200) names.push('Use Custom Upgrades')
    if (flags & 0x0400) names.push('Has Properties Menu Opened Before')
    if (flags & 0x0800) names.push('Show Water Waves on Cliff Shores')
    if (flags & 0x1000) names.push('Show Water Waves on Rolling Shores')
    if (flags & 0x2000) names.push('Terrain Fog Enabled')
    if (flags & 0x4000) names.push('Expansion Required')
    if (flags & 0x8000) names.push('Use Item Classification System')
    if (flags & 0x10000) names.push('Water Tint Color Override')
    return names
}

// ── Header panel content (archive only) ─────────────────────────────
function renderHeaderContent(header) {
    if (!header) return '<div class="fi-empty">No header data</div>'

    const rows = []
    if (header.signature) rows.push(['Signature', `<code>${esc(header.signature)}</code>`])
    if (header.mapName) rows.push(['Map Name', esc(header.mapName)])
    if (header.maxPlayers != null) rows.push(['Max Players', header.maxPlayers])
    if (header.mapFlags != null) rows.push(['Map Flags', `<code>0x${header.mapFlags.toString(16).toUpperCase().padStart(4, '0')}</code>`])
    if (header.campaignVersion != null) rows.push(['Campaign Version', header.campaignVersion])
    if (header.editorVersion != null) rows.push(['Editor Version', header.editorVersion])
    if (header.campaignName) rows.push(['Campaign Name', esc(header.campaignName)])

    if (rows.length === 0) return '<div class="fi-empty">Empty header</div>'

    let html = `<table class="info">${rows.map(([k, v]) =>
        `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`

    // Flag tags
    if (header.mapFlags != null) {
        const flagNames = decodeMapFlags(header.mapFlags)
        if (flagNames.length > 0) {
            html += `<div class="flag-tags">${flagNames.map(f => `<span class="flag-tag">${esc(f)}</span>`).join('')}</div>`
        }
    }

    return html
}

// ── Game Path panel content ─────────────────────────────────────────

const REQUIRED_MPQ_FILES = ['War3.mpq', 'War3x.mpq', 'War3xLocal.mpq', 'War3Patch.mpq']

function renderGamePathContent(gamePath, mpqStatus) {
    const hasPath = !!gamePath
    const pathDisplay = hasPath
        ? `<div class="gp-path">${esc(gamePath)}</div>`
        : `<div class="gp-no-path">Not selected</div>`

    let statusHtml = ''
    if (hasPath && mpqStatus) {
        const rows = REQUIRED_MPQ_FILES.map(f => {
            const ok = mpqStatus[f]
            const icon = ok ? '✅' : '❌'
            const cls = ok ? 'gp-ok' : 'gp-missing'
            return `<div class="gp-mpq-row ${cls}"><span>${icon}</span> <span>${esc(f)}</span></div>`
        }).join('')
        statusHtml = `<div class="gp-mpq-list">${rows}</div>`
    }

    return `
        <div class="gp-hint">Path to Warcraft III installation folder.</div>
        ${pathDisplay}
        ${statusHtml}
        <div class="gp-actions">
            <button class="gp-browse" id="gamePathBrowse">📂 Browse…</button>
            ${hasPath ? '<button class="gp-clear" id="gamePathClear">✕ Clear</button>' : ''}
        </div>`
}

function validateGamePath(dirPath) {
    const fs = require('fs')
    const path = require('path')
    const status = {}
    for (const f of REQUIRED_MPQ_FILES) {
        status[f] = fs.existsSync(path.join(dirPath, f))
    }
    return status
}

function allMpqPresent(mpqStatus) {
    return REQUIRED_MPQ_FILES.every(f => mpqStatus[f])
}

// ── Files panel content (folder tree) ───────────────────────────────

/** Build a tree structure from a flat file list */
function buildFileTree(files) {
    const root = {children: {}, files: []}
    for (const f of files) {
        const name = typeof f === 'string' ? f : f.name || ''
        const parts = name.replace(/\\/g, '/').split('/').filter(Boolean)
        if (parts.length === 0) continue
        let node = root
        for (let i = 0; i < parts.length - 1; i++) {
            const dir = parts[i]
            if (!node.children[dir]) node.children[dir] = {children: {}, files: []}
            node = node.children[dir]
        }
        node.files.push({name, basename: parts[parts.length - 1], size: f.size, discovered: !!f.discovered, found: !!f.found})
    }
    return root
}

/** Count all files in a subtree */
function countFiles(node) {
    let n = node.files.length
    for (const ch of Object.values(node.children)) n += countFiles(ch)
    return n
}

/** Render a tree node recursively */
function renderTreeNode(node, prefix = '') {
    let html = ''
    // Sort folders alphabetically
    const dirs = Object.keys(node.children).sort((a, b) => a.localeCompare(b, undefined, {sensitivity: 'base'}))
    for (const dir of dirs) {
        const child = node.children[dir]
        const cnt = countFiles(child)
        const fullPath = prefix ? prefix + '/' + dir : dir
        html += `<div class="folder-row" data-folder data-path="${esc(fullPath)}">
            <span class="folder-chevron">▼</span>
            <span class="folder-icon">📁</span>
            <span class="folder-name">${esc(dir)}</span>
            <span class="folder-count">${cnt}</span>
        </div>
        <div class="folder-children">${renderTreeNode(child, fullPath)}</div>`
    }
    // Sort files alphabetically
    const sorted = [...node.files].sort((a, b) => a.basename.localeCompare(b.basename, undefined, {sensitivity: 'base'}))
    for (const f of sorted) {
        const size = f.size != null ? fmtSize(f.size) : ''
        const source = f.found ? 'found' : f.discovered ? 'discovered' : 'listfile'
        const cls = f.found ? 'file-row file-found' : f.discovered ? 'file-row file-discovered' : 'file-row'
        const badge = f.found ? '<span class="file-badge file-badge-found" title="Found by probing map data">found</span>'
            : f.discovered ? '<span class="file-badge file-badge-discovered" title="Discovered by probing known names">discovered</span>'
            : ''
        html += `<div class="${cls}" data-name="${esc(f.name)}" data-source="${source}">
            <span class="file-name">${esc(f.basename)}</span>
            ${badge}
            <span class="file-size">${size}</span>
        </div>`
    }
    return html
}

function renderFilesRows(archiveFiles) {
    if (!archiveFiles || archiveFiles.length === 0) {
        return '<div class="fi-empty">No files in archive</div>'
    }
    const tree = buildFileTree(archiveFiles)
    return renderTreeNode(tree)
}

// ── W3i Meta banner ─────────────────────────────────────────────────
function renderW3iMeta(meta, tailCount) {
    if (!meta) return ''
    const unread = (meta.remaining || 0) + (tailCount || 0)
    if (unread === 0) {
        return `<div class="meta-banner ok">✓ All ${meta.total} bytes read</div>`
    }
    const read = meta.total - unread
    return `<div class="meta-banner warn">⚠ ${read} of ${meta.total} bytes read, ${unread} unrecognised</div>`
}

// ── W3i content for float-window ────────────────────────────────────
function renderW3iContent(d) {
    if (!d) return '<div class="fi-empty">No map info data</div>'

    // ── Parse error banner
    const errorHtml = d._error
        ? `<div class="meta-banner error">✕ Parse error: ${esc(d._error)}</div>`
        : ''

    // ── General info
    const general = [
        ['Format', d.format],
        ['Save count', d.save_count],
        ['Editor version', d.editor_version],
        ['Map name', esc(d.map_name)],
        ['Author', esc(d.author)],
        ['Map size', `${d.playable_width} × ${d.playable_height}`],
        ['Scripting', d.script_language === 1 ? 'Lua' : d.script_language === 0 ? 'JASS' : '—'],
    ]
    if (d.editor_version_full) {
        general.splice(3, 0, ['Editor version full', d.editor_version_full.join('.')])
    }
    const generalHtml = general.map(([k, v]) =>
        `<tr><td class="key">${k}</td><td>${v}</td></tr>`
    ).join('')

    const tailCount = d.tail_bytes ? d.tail_bytes.length : 0
    const metaHtml = renderW3iMeta(d._meta, tailCount)

    // ── Description
    const descHtml = d.description
        ? `<collapse-group group-title="Description"><pre class="w3i-desc">${esc(d.description)}</pre></collapse-group>`
        : ''
    const pDescHtml = d.recommended_players
        ? `<collapse-group group-title="Players description"><pre class="w3i-desc">${esc(d.recommended_players)}</pre></collapse-group>`
        : ''

    // ── Map flags (same decoder as header)
    let flagsHtml = ''
    if (d.map_flags != null) {
        const raw = typeof d.map_flags === 'object' ? d.map_flags.raw : d.map_flags
        const flagNames = decodeMapFlags(raw)
        if (flagNames.length > 0) {
            flagsHtml = `<div class="tw-section-title">🚩 Map Flags</div><div class="flag-tags">${flagNames.map(f => `<span class="flag-tag">${esc(f)}</span>`).join('')}</div>`
        }
    }

    // ── Camera bounds
    const cam = d.camera_bounds
    let camHtml = ''
    if (cam) {
        camHtml = `<div class="tw-section-title">📐 Camera Bounds</div>
        <table class="info">
            <tr><td class="key">LB</td><td>${fmtF(cam.lb.x)}, ${fmtF(cam.lb.y)}</td></tr>
            <tr><td class="key">RT</td><td>${fmtF(cam.rt.x)}, ${fmtF(cam.rt.y)}</td></tr>
            <tr><td class="key">LT</td><td>${fmtF(cam.lt.x)}, ${fmtF(cam.lt.y)}</td></tr>
            <tr><td class="key">RB</td><td>${fmtF(cam.rb.x)}, ${fmtF(cam.rb.y)}</td></tr>
        </table>`
    }

    // ── Fog / weather
    let fogHtml = ''
    if (d.fog_type != null) {
        const fogRows = [
            ['Fog type', d.fog_type],
            ['Start', fmtF(d.fog_z_start)],
            ['End', fmtF(d.fog_z_end)],
            ['Density', fmtF(d.fog_density)],
            ['Fog color', d.fog_color != null ? `0x${d.fog_color.toString(16).padStart(8, '0')}` : '—'],
            ['Weather', d.global_weather ? d.global_weather.text : '—'],
            ['Sound', d.ambient_sound || '—'],
            ['Water color', d.water_tint_color != null ? `0x${d.water_tint_color.toString(16).padStart(8, '0')}` : '—'],
        ]
        fogHtml = `<div class="tw-section-title">🌫 Fog & Weather</div><table class="info">${fogRows.map(([k, v]) =>
            `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`
    }

    // ── Loading screen
    const loadRows = [
        ['Number', d.loading_screen_preset],
        ['Path', esc(d.loading_screen_model || '—')],
        ['Title', esc(d.loading_screen_title || '—')],
        ['Subtitle', esc(d.loading_screen_subtitle || '—')],
        ['Text', esc(d.loading_screen_text || '—')],
    ]
    const loadHtml = `<div class="tw-section-title">🖼 Loading Screen</div><table class="info">${loadRows.map(([k, v]) =>
        `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`

    // ── Prologue
    let prologueHtml = ''
    if (d.prologue_title || d.prologue_text) {
        const pRows = [
            ['Path', esc(d.prologue_screen_model || '—')],
            ['Title', esc(d.prologue_title || '—')],
            ['Subtitle', esc(d.prologue_subtitle || '—')],
            ['Text', esc(d.prologue_text || '—')],
        ]
        prologueHtml = `<div class="tw-section-title">📜 Prologue</div><table class="info">${pRows.map(([k, v]) =>
            `<tr><td class="key">${k}</td><td>${v}</td></tr>`).join('')}</table>`
    }

    // ── Players
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
                <td class="mono">${fmtF(p.start_position.x)}, ${fmtF(p.start_position.y)}</td>
                <td class="num">${p.fixed_start_position}</td>
            </tr>`
        }).join('')
        playersHtml = `
        <div class="tw-section-title">👤 Players (${d.players.length})</div>
        <div class="table-wrap"><table><thead><tr>${pHead}</tr></thead><tbody>${pBody}</tbody></table></div>`
    }

    // ── Forces
    let clansHtml = ''
    if (d.forces && d.forces.length > 0) {
        const cHead = ['#', 'Name', 'Players', 'Flags'].map(c => `<th>${c}</th>`).join('')
        const cBody = d.forces.map((c, i) => {
            const flags = c.flags && typeof c.flags === 'object'
                ? Object.entries(c.flags).filter(([, v]) => v).map(([k]) => k).join(', ') || '—'
                : '—'
            return `<tr>
                <td class="num">${i + 1}</td>
                <td>${esc(c.name)}</td>
                <td class="num">${c.player_mask}</td>
                <td>${esc(flags)}</td>
            </tr>`
        }).join('')
        clansHtml = `
        <div class="tw-section-title">⚔ Forces (${d.forces.length})</div>
        <div class="table-wrap"><table><thead><tr>${cHead}</tr></thead><tbody>${cBody}</tbody></table></div>`
    }

    // ── Upgrades
    let upgradesHtml = ''
    if (d.custom_upgrades_missing) {
        upgradesHtml = '<div class="tw-section-title">⬆ Upgrades</div><div class="meta-banner warn">Section not present in file</div>'
    } else if (d.custom_upgrades && d.custom_upgrades.length > 0) {
        const uHead = ['#', 'ID', 'Level', 'Status'].map(c => `<th>${c}</th>`).join('')
        const uBody = d.custom_upgrades.map((u, i) => {
            const status = u.status ? (typeof u.status === 'string' ? u.status : JSON.stringify(u.status)) : '—'
            return `<tr>
                <td class="num">${i + 1}</td>
                <td class="code">${esc(u.id.text)}</td>
                <td class="num">${u.level}</td>
                <td>${esc(status)}</td>
            </tr>`
        }).join('')
        upgradesHtml = `
        <div class="tw-section-title">⬆ Upgrades (${d.custom_upgrades.length})</div>
        <div class="table-wrap"><table><thead><tr>${uHead}</tr></thead><tbody>${uBody}</tbody></table></div>`
    }

    // ── Techs
    let techsHtml = ''
    if (d.disabled_techs_missing) {
        techsHtml = '<div class="tw-section-title">🔬 Disabled Techs</div><div class="meta-banner warn">Section not present in file</div>'
    } else if (d.disabled_techs && d.disabled_techs.length > 0) {
        const tHead = ['#', 'ID'].map(c => `<th>${c}</th>`).join('')
        const tBody = d.disabled_techs.map((t, i) => `<tr>
            <td class="num">${i + 1}</td>
            <td class="code">${esc(t.id.text)}</td>
        </tr>`).join('')
        techsHtml = `
        <div class="tw-section-title">🔬 Disabled Techs (${d.disabled_techs.length})</div>
        <div class="table-wrap"><table><thead><tr>${tHead}</tr></thead><tbody>${tBody}</tbody></table></div>`
    }

    // ── Random groups
    let groupsHtml = ''
    if (d.random_groups_missing) {
        groupsHtml = '<div class="tw-section-title">🎲 Random Groups</div><div class="meta-banner warn">Section not present in file</div>'
    } else if (d.random_groups && d.random_groups.length > 0) {
        const gItems = d.random_groups.map((g) => {
            const chancesHead = ['Chance %', ...g.column_types.map((_, ci) => `Col ${ci + 1}`)].map(c => `<th>${c}</th>`).join('')
            const chancesBody = g.chances.map(ch => {
                const ids = ch.ids.map(id => `<td class="code">${esc(id.text)}</td>`).join('')
                return `<tr><td class="num">${ch.chance}%</td>${ids}</tr>`
            }).join('')
            return `<collapse-group group-title="${esc(g.name)} (#${g.num}, ${g.chances.length} rows)">
            <div class="table-wrap"><table><thead><tr>${chancesHead}</tr></thead><tbody>${chancesBody}</tbody></table></div></collapse-group>`
        }).join('')
        groupsHtml = `<div class="tw-section-title">🎲 Random Groups (${d.random_groups.length})</div>${gItems}`
    }

    // ── Random item tables
    let itemsHtml = ''
    if (d.random_item_tables_missing) {
        itemsHtml = '<div class="tw-section-title">🎁 Random Item Tables</div><div class="meta-banner warn">Section not present in file</div>'
    } else if (d.random_item_tables && d.random_item_tables.length > 0) {
        const iItems = d.random_item_tables.map(item => {
            const gParts = item.groups.map((g, gi) => {
                const rows = g.chances.map(ch => `<tr>
                    <td class="num">${ch.chance}%</td>
                    <td class="code">${esc(ch.id.text)}</td>
                </tr>`).join('')
                return rows ? `<div class="w3i-sub-group"><em>Set ${gi + 1}</em>
                    <table><thead><tr><th>Chance</th><th>ID</th></tr></thead><tbody>${rows}</tbody></table>
                </div>` : ''
            }).join('')
            return `<collapse-group group-title="${esc(item.name)} (#${item.num})">${gParts}</collapse-group>`
        }).join('')
        itemsHtml = `<div class="tw-section-title">🎁 Random Item Tables (${d.random_item_tables.length})</div>${iItems}`
    }

    return `
    ${metaHtml}
    ${errorHtml}
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
    ${itemsHtml}`
}

// ── DOO Meta banner ─────────────────────────────────────────────────
function renderDooMeta(meta) {
    if (!meta) return ''
    if (meta.remaining === 0) {
        return `<div class="meta-banner ok">✓ All ${meta.total} bytes read</div>`
    }
    return `<div class="meta-banner warn">⚠ ${meta.remaining} of ${meta.total} bytes not read</div>`
}

// ── DOO content for float-window ────────────────────────────────────
function renderDooContent(data, isUnit) {
    if (!data) return '<div class="fi-empty">No data</div>'

    const metaHtml = renderDooMeta(data._meta)

    const errorHtml = data._error
        ? `<div class="meta-banner error">✕ Parse error: ${esc(data._error)}</div>`
        : ''

    // ── Header info (only show binary format fields if present)
    const headerRows = []
    if (data.magic != null) headerRows.push(['Magic', `<code>${esc(data.magic)}</code>`])
    if (data.format != null) headerRows.push(['Format', data.format])
    if (data.subformat != null) headerRows.push(['Sub-format', data.subformat])
    headerRows.push(['Items', data.items ? data.items.length : 0])
    if (data.cliffs) headerRows.push(['Cliffs', data.cliffs.length])

    const headerHtml = headerRows.map(([k, v]) =>
        `<tr><td class="key">${k}</td><td>${v}</td></tr>`
    ).join('')

    // ── Canvas container for items (replaces heavy DOM table)
    const itemCount = data.items ? data.items.length : 0
    const listId = isUnit ? 'unitDooList' : 'doodadDooList'
    const itemsHtml = `
        <div class="tw-section-title">${isUnit ? '🗡 Units' : '🌳 Items'} (${itemCount})</div>
        <div class="legend" id="${listId}" style="flex:1;min-height:0;overflow:hidden;"></div>`

    // ── Cliffs (collapse-group, doodad DOO only, usually small)
    let cliffsHtml = ''
    if (data.cliffs && data.cliffs.length > 0) {
        const chead = ['#', 'Rawcode', 'Variation', 'X', 'Y'].map(c => `<th>${c}</th>`).join('')
        const cbody = data.cliffs.map((c, i) => `<tr>
            <td class="num">${i + 1}</td>
            <td class="code">${esc(c.rawcode.text)}</td>
            <td class="num">${c.variation}</td>
            <td class="num">${c.x}</td>
            <td class="num">${c.y}</td>
        </tr>`).join('')

        cliffsHtml = `
        <collapse-group group-title="🏔 Cliffs (${data.cliffs.length})" style="flex-shrink:0;">
            <div class="table-wrap" style="max-height:200px;overflow:auto;"><table><thead><tr>${chead}</tr></thead><tbody>${cbody}</tbody></table></div>
        </collapse-group>`
    }

    return `<div class="doo-content">
    ${metaHtml}
    ${errorHtml}
    <table class="info">${headerHtml}</table>
    ${cliffsHtml}
    ${itemsHtml}
    </div>`
}

// ── W3R (regions) content for float-window ──────────────────────────
function renderW3rContent(data) {
    if (!data) return '<div class="fi-empty">No data</div>'

    const metaHtml = renderDooMeta(data._meta)

    const headerRows = [
        ['Format', data.format],
        ['Regions', data.regions ? data.regions.length : 0],
    ]
    const headerHtml = headerRows.map(([k, v]) =>
        `<tr><td class="key">${k}</td><td>${v}</td></tr>`
    ).join('')

    const regionCount = data.regions ? data.regions.length : 0

    return `<div class="doo-content">
    ${metaHtml}
    <table class="info">${headerHtml}</table>
    <div style="padding:6px 10px 2px;flex-shrink:0;display:flex;align-items:center;gap:6px;">
        <label id="rgMasterToggle" style="display:inline-flex;align-items:center;gap:3px;cursor:pointer;font-size:11px;user-select:none;" title="Enable/disable region overlay on terrain">
            <input type="checkbox" id="rgMasterCheckbox" style="cursor:pointer;margin:0;" />
        </label>
        <button id="rgShowAllBtn" style="font-size:11px;padding:2px 8px;cursor:pointer;background:var(--vscode-button-background,#0e639c);color:var(--vscode-button-foreground,#fff);border:none;border-radius:3px;" title="Show all regions on terrain">Show All</button>
        <button id="rgHideAllBtn" style="font-size:11px;padding:2px 8px;cursor:pointer;background:var(--vscode-button-background,#0e639c);color:var(--vscode-button-foreground,#fff);border:none;border-radius:3px;" title="Hide all regions on terrain">Hide All</button>
    </div>
    <div class="ds-sort-bar">
        <span class="ds-sort-col rg-sort-col" data-sort="num">#</span>
        <span class="ds-sort-col rg-sort-col ds-sort-name" data-sort="name">Name</span>
        <span class="ds-sort-col rg-sort-col ds-sort-cat" data-sort="weather">Weather</span>
        <span class="ds-sort-info">(<span id="rgRegionCount">${regionCount}</span>)</span>
    </div>
    <div class="legend" id="regionsList" style="flex:1;min-height:0;overflow:hidden;"></div>
    </div>`
}

module.exports = {renderMapInfoContent, renderHeaderContent, renderGamePathContent, renderFilesRows, renderW3iContent, renderDooContent, renderW3rContent, REQUIRED_MPQ_FILES}
