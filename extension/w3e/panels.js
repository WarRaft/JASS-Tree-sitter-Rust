const {esc, fmtSize} = require('./utils.js')

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
    if (flags & 0x8000) names.push('Use Item Classification System')
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
        node.files.push({name, basename: parts[parts.length - 1], size: f.size})
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
function renderTreeNode(node) {
    let html = ''
    // Sort folders alphabetically
    const dirs = Object.keys(node.children).sort((a, b) => a.localeCompare(b, undefined, {sensitivity: 'base'}))
    for (const dir of dirs) {
        const child = node.children[dir]
        const cnt = countFiles(child)
        html += `<div class="folder-row" data-folder>
            <span class="folder-chevron">▼</span>
            <span class="folder-icon">📁</span>
            <span class="folder-name">${esc(dir)}</span>
            <span class="folder-count">${cnt}</span>
        </div>
        <div class="folder-children">${renderTreeNode(child)}</div>`
    }
    // Sort files alphabetically
    const sorted = [...node.files].sort((a, b) => a.basename.localeCompare(b.basename, undefined, {sensitivity: 'base'}))
    for (const f of sorted) {
        const size = f.size != null ? fmtSize(f.size) : ''
        html += `<div class="file-row" data-name="${esc(f.name)}">
            <span class="file-name">${esc(f.basename)}</span>
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

module.exports = {renderMapInfoContent, renderHeaderContent, renderGamePathContent, renderFilesRows, validateGamePath, allMpqPresent, REQUIRED_MPQ_FILES}
