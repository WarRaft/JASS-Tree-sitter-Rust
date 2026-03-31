function esc(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

function fmtSize(bytes) {
    if (bytes == null) return '—'
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`
}

function errorHtml(msg) {
    return `<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"/></head>
<body style="background:var(--vscode-editor-background);color:var(--vscode-errorForeground);font-family:var(--vscode-font-family);padding:2rem;">
<h2>⚠ Error</h2><pre>${esc(msg)}</pre>
</body></html>`
}

// ── Colour palette ──────────────────────────────────────────────────
function indexToRgb(index) {
    const golden = 137.508
    const hue = (index * golden) % 360
    const sat = 0.55 + 0.15 * ((index % 3) / 2)
    const lum = 0.45 + 0.10 * ((index % 5) / 4)
    const c = (1 - Math.abs(2 * lum - 1)) * sat
    const x = c * (1 - Math.abs(((hue / 60) % 2) - 1))
    const m = lum - c / 2
    let r, g, b
    if (hue < 60) { r = c; g = x; b = 0 }
    else if (hue < 120) { r = x; g = c; b = 0 }
    else if (hue < 180) { r = 0; g = c; b = x }
    else if (hue < 240) { r = 0; g = x; b = c }
    else if (hue < 300) { r = x; g = 0; b = c }
    else { r = c; g = 0; b = x }
    return [
        Math.round((r + m) * 255),
        Math.round((g + m) * 255),
        Math.round((b + m) * 255),
    ]
}

const TILESET_NAMES = {
    A: 'Ashenvale', B: 'Barrens', K: 'Black Citadel', Y: 'Cityscape',
    X: 'Dalaran', J: 'Dalaran Ruins', D: 'Dungeon', C: 'Felwood',
    I: 'Icecrown Glacier', F: 'Lordaeron Fall', L: 'Lordaeron Summer',
    W: 'Lordaeron Winter', N: 'Northrend', O: 'Outland',
    Z: 'Sunken Ruins', G: 'Underground', V: 'Village', Q: 'Village Fall',
}

function fmtF(v) {
    if (v == null) return '—'
    return Number(v).toFixed(2)
}

module.exports = {esc, fmtSize, fmtF, errorHtml, indexToRgb, TILESET_NAMES}

