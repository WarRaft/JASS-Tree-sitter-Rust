// noinspection JSUnusedGlobalSymbols

/**
 * @typedef {Object} ImportGraphResult
 * @property {string} uri - The URI of the requested file.
 * @property {string[]} nodes - List of file URIs (index 0 = requested file).
 * @property {[number, number][]} edges - List of [source_idx, target_idx] pairs.
 */

const {window, ViewColumn, Uri, commands} = require('vscode')
const path = require('path')

/** @type {import('vscode').WebviewPanel | undefined} */
let panel

/**
 * Show the import graph for the given file URI.
 *
 * @param {import('vscode-languageclient').LanguageClient} client
 * @param {import('vscode').Uri} extensionUri - Extension root URI (context.extensionUri).
 * @param {import('vscode').ExtensionContext} context
 * @param {string} [fileUri] - If not given, uses the active editor's URI.
 */
async function showImportGraph(client, extensionUri, context, fileUri) {
    if (!fileUri) {
        const editor = window.activeTextEditor
        if (!editor) {
            window.showWarningMessage('No active editor — open a .j or .as file first.')
            return
        }
        fileUri = editor.document.uri.toString()
    }

    /** @type {ImportGraphResult} */
    const result = await client.sendRequest('importGraph/subgraph', {
        uri: fileUri,
    })

    if (!result || !result.nodes || result.nodes.length === 0) {
        window.showInformationMessage('Import graph is empty for this file.')
        return
    }

    if (panel) {
        panel.reveal(ViewColumn.Beside)
    } else {
        panel = window.createWebviewPanel(
            'importGraph',
            'Import Graph',
            ViewColumn.Beside,
            {
                enableScripts: true,
                retainContextWhenHidden: true,
                localResourceRoots: [Uri.joinPath(extensionUri, 'extension', 'vendor')]
            }
        )
        panel.onDidDispose(() => {
            panel = undefined
        })

        // Handle messages from webview (file click)
        panel.webview.onDidReceiveMessage(async (msg) => {
            if (msg.type === 'openFile') {
                try {
                    const uri = Uri.parse(msg.uri)
                    await commands.executeCommand('vscode.open', uri)
                } catch (e) {
                    window.showErrorMessage(`Cannot open file: ${e.message}`)
                }
            } else if (msg.type === 'refresh') {
                await showImportGraph(client, extensionUri, context, msg.uri)
            } else if (msg.type === 'saveSettings') {
                if (context && context.globalState) {
                    await context.globalState.update('d3PhysicsSettings', msg.settings)
                }
            }
        })
    }

    const d3Uri = panel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'vendor', 'd3.v7.min.js')
    )

    panel.title = `Import Graph — ${path.basename(decodeURIComponent(new URL(result.uri).pathname))}`
    const savedSettings = context.globalState.get('d3PhysicsSettings', null)
    panel.webview.html = buildHtml(result, d3Uri.toString(), savedSettings)
}

/**
 * Find the longest common directory prefix among `paths` and strip it,
 * producing the shortest unique labels.
 *
 * Example:
 *   ["/a/b/c/d.j", "/a/b/e.j", "/a/b/e/f.j"]
 *   → common prefix "/a/b/" → ["c/d.j", "e.j", "e/f.j"]
 *
 * @param {string[]} paths - Filesystem paths (forward-slash separated).
 * @returns {string[]}
 */
function shortenPaths(paths) {
    if (paths.length === 0) return []
    if (paths.length === 1) {
        // Single path → just the filename
        const parts = paths[0].split('/')
        return [parts[parts.length - 1] || paths[0]]
    }

    // Split every path into directory segments (everything before the last /)
    const split = paths.map(p => p.split('/'))

    // Find the longest common prefix of *directory* segments.
    // We compare all segments except the last one (the filename) of the
    // shortest path, but we must stop at the shortest array minus one anyway.
    const minLen = Math.min(...split.map(s => s.length))
    let common = 0
    outer:
    for (let i = 0; i < minLen - 1; i++) {
        const seg = split[0][i]
        for (let j = 1; j < split.length; j++) {
            if (split[j][i] !== seg) break outer
        }
        common = i + 1
    }

    return split.map(parts => parts.slice(common).join('/'))
}

/**
 * @param {ImportGraphResult} data
 * @param {string} d3Src - Webview-safe URI to the local d3.v7.min.js.
 * @param {Object|null} savedSettings
 * @returns {string}
 */
function buildHtml(data, d3Src, savedSettings) {
    // Extract filesystem paths from URIs
    const paths = data.nodes.map(uri => {
        try {
            return decodeURIComponent(new URL(uri).pathname)
        } catch {
            return uri
        }
    })

    // Find the longest common directory prefix and strip it.
    // e.g. ["/a/b/c/d.j", "/a/b/e.j", "/a/b/e/f.j"] → common = "/a/b/"
    //      → labels: ["c/d.j", "e.j", "e/f.j"]
    const labels = shortenPaths(paths)

    const graphJSON = JSON.stringify({
        nodes: data.nodes.map((uri, i) => ({
            id: i,
            uri: uri,
            label: labels[i],
            isRoot: i === 0,
        })),
        links: data.edges.map(([s, t]) => ({source: s, target: t})),
    })

    const settingsJSON = JSON.stringify(savedSettings || {
        linkDistance: 120,
        chargeStrength: -400,
        collisionRadius: 30,
        centerStrength: 0.05,
    })

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
    svg { display: block; width: 100vw; height: 100vh; }

    .link {
        stroke: var(--vscode-editorWidget-border, #555);
        stroke-width: 1.5;
        fill: none;
        marker-end: url(#arrow);
    }

    .node-circle {
        stroke: var(--vscode-focusBorder, #007acc);
        stroke-width: 1.5;
        cursor: pointer;
        transition: r 0.15s;
    }
    .node-circle:hover { r: 9; }
    .node-circle.root {
        fill: var(--vscode-focusBorder, #007acc);
    }
    .node-circle.dep {
        fill: var(--vscode-editor-background, #1e1e1e);
    }

    .node-label {
        fill: var(--vscode-editor-foreground, #d4d4d4);
        font-size: 11px;
        pointer-events: none;
        text-shadow:
            -1px -1px 2px var(--vscode-editor-background, #1e1e1e),
             1px -1px 2px var(--vscode-editor-background, #1e1e1e),
            -1px  1px 2px var(--vscode-editor-background, #1e1e1e),
             1px  1px 2px var(--vscode-editor-background, #1e1e1e);
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

    .legend {
        position: fixed;
        bottom: 8px;
        left: 8px;
        display: flex;
        gap: 12px;
        align-items: center;
        font-size: 11px;
        opacity: 0.7;
    }
    .legend-dot {
        display: inline-block;
        width: 10px;
        height: 10px;
        border-radius: 50%;
        margin-right: 4px;
        vertical-align: middle;
    }
    .legend-dot.root {
        background: var(--vscode-focusBorder, #007acc);
    }
    .legend-dot.dep {
        background: var(--vscode-editor-background, #1e1e1e);
        border: 1.5px solid var(--vscode-focusBorder, #007acc);
    }

    #settingsBtn {
        position: fixed; bottom: 8px; right: 8px; z-index: 20;
        background: var(--vscode-button-background, #0e639c);
        color: var(--vscode-button-foreground, #fff);
        border: none; border-radius: 50%; width: 32px; height: 32px;
        font-size: 16px; cursor: pointer;
        display: flex; align-items: center; justify-content: center;
        box-shadow: 0 2px 6px rgba(0,0,0,0.3);
    }
    #settingsBtn:hover { background: var(--vscode-button-hoverBackground, #1177bb); }
    #settingsPanel {
        display: none; position: fixed; bottom: 48px; right: 8px; z-index: 20;
        background: var(--vscode-sideBar-background, #252526);
        border: 1px solid var(--vscode-editorWidget-border, #454545);
        border-radius: 6px; padding: 12px 14px; width: 240px;
        box-shadow: 0 4px 16px rgba(0,0,0,0.4);
    }
    #settingsPanel.open { display: block; }
    #settingsPanel h3 { margin: 0 0 10px 0; font-size: 12px; font-weight: 600; }
    .setting-row { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; font-size: 11px; }
    .setting-row label { flex: 1; }
    .setting-row input[type=range] { width: 100px; accent-color: var(--vscode-focusBorder, #007acc); }
    .setting-row .val { width: 36px; text-align: right; font-variant-numeric: tabular-nums; }
</style>
</head>
<body>
<div class="toolbar">
    <button id="btnFit" title="Fit to view">⊞ Fit</button>
    <button id="btnRefresh" title="Refresh graph">↻ Refresh</button>
</div>
<div class="legend">
    <span><span class="legend-dot root"></span>Current file</span>
    <span><span class="legend-dot dep"></span>Dependency</span>
    <span>→ imports</span>
</div>
<button id="settingsBtn" title="D3 Physics Settings">⚙</button>
<div id="settingsPanel">
    <h3>⚙ Physics Settings</h3>
    <div class="setting-row"><label>Link distance</label><input type="range" id="sLinkDist" min="20" max="300" step="5"/><span class="val" id="vLinkDist"></span></div>
    <div class="setting-row"><label>Charge</label><input type="range" id="sCharge" min="-1000" max="0" step="10"/><span class="val" id="vCharge"></span></div>
    <div class="setting-row"><label>Collision</label><input type="range" id="sCollision" min="5" max="80" step="1"/><span class="val" id="vCollision"></span></div>
    <div class="setting-row"><label>Center</label><input type="range" id="sCenter" min="0" max="1" step="0.01"/><span class="val" id="vCenter"></span></div>
</div>
<svg id="graph"></svg>

<script src="${d3Src}"></script>
<script>
const vscode = acquireVsCodeApi();
const graphData = ${graphJSON};
let settings = ${settingsJSON};

const svg = d3.select('#graph');
const width = window.innerWidth;
const height = window.innerHeight;

document.getElementById('settingsBtn').addEventListener('click', () => {
    document.getElementById('settingsPanel').classList.toggle('open');
});

function initSliders() {
    [['sLinkDist','vLinkDist','linkDistance'],['sCharge','vCharge','chargeStrength'],['sCollision','vCollision','collisionRadius'],['sCenter','vCenter','centerStrength']].forEach(([sid,vid,key]) => {
        const s = document.getElementById(sid), v = document.getElementById(vid);
        s.value = settings[key]; v.textContent = settings[key];
        s.addEventListener('input', () => { settings[key] = parseFloat(s.value); v.textContent = settings[key]; applySettings(); vscode.postMessage({type:'saveSettings',settings}); });
    });
}

function applySettings() {
    simulation.force('link').distance(settings.linkDistance);
    simulation.force('charge').strength(settings.chargeStrength);
    simulation.force('collision').radius(settings.collisionRadius);
    simulation.force('center').strength(settings.centerStrength);
    simulation.alpha(0.5).restart();
}

// Defs: arrow marker
svg.append('defs').append('marker')
    .attr('id', 'arrow')
    .attr('viewBox', '0 -5 10 10')
    .attr('refX', 18)
    .attr('refY', 0)
    .attr('markerWidth', 8)
    .attr('markerHeight', 8)
    .attr('orient', 'auto')
    .append('path')
    .attr('d', 'M0,-5L10,0L0,5')
    .attr('fill', getComputedStyle(document.documentElement)
        .getPropertyValue('--vscode-editorWidget-border').trim() || '#555');

const g = svg.append('g');

// Zoom
const zoom = d3.zoom()
    .scaleExtent([0.1, 5])
    .on('zoom', (e) => g.attr('transform', e.transform));
svg.call(zoom);

// Force simulation
const simulation = d3.forceSimulation(graphData.nodes)
    .force('link', d3.forceLink(graphData.links)
        .id(d => d.id)
        .distance(settings.linkDistance))
    .force('charge', d3.forceManyBody().strength(settings.chargeStrength))
    .force('center', d3.forceCenter(width / 2, height / 2).strength(settings.centerStrength))
    .force('collision', d3.forceCollide(settings.collisionRadius));

// Links
const link = g.append('g')
    .selectAll('line')
    .data(graphData.links)
    .join('line')
    .attr('class', 'link');

// Node groups
const node = g.append('g')
    .selectAll('g')
    .data(graphData.nodes)
    .join('g')
    .call(d3.drag()
        .on('start', dragStarted)
        .on('drag', dragged)
        .on('end', dragEnded));

node.append('circle')
    .attr('class', d => 'node-circle ' + (d.isRoot ? 'root' : 'dep'))
    .attr('r', d => d.isRoot ? 8 : 6)
    .on('dblclick', (e, d) => {
        e.stopPropagation();
        vscode.postMessage({type: 'openFile', uri: d.uri});
    });

node.append('title')
    .text(d => d.uri);

node.append('text')
    .attr('class', 'node-label')
    .attr('dx', 12)
    .attr('dy', 4)
    .text(d => d.label);

simulation.on('tick', () => {
    link
        .attr('x1', d => d.source.x)
        .attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x)
        .attr('y2', d => d.target.y);
    node.attr('transform', d => 'translate(' + d.x + ',' + d.y + ')');
});

function dragStarted(event, d) {
    if (!event.active) simulation.alphaTarget(0.3).restart();
    d.fx = d.x;
    d.fy = d.y;
}
function dragged(event, d) {
    d.fx = event.x;
    d.fy = event.y;
}
function dragEnded(event, d) {
    if (!event.active) simulation.alphaTarget(0);
    d.fx = null;
    d.fy = null;
}

// Fit button
document.getElementById('btnFit').addEventListener('click', () => {
    const bounds = g.node().getBBox();
    if (bounds.width === 0 || bounds.height === 0) return;
    const padX = 60, padY = 60;
    const scale = Math.min(
        width / (bounds.width + padX * 2),
        height / (bounds.height + padY * 2),
        2
    );
    const tx = width / 2 - scale * (bounds.x + bounds.width / 2);
    const ty = height / 2 - scale * (bounds.y + bounds.height / 2);
    svg.transition().duration(500)
        .call(zoom.transform, d3.zoomIdentity.translate(tx, ty).scale(scale));
});

// Refresh button
document.getElementById('btnRefresh').addEventListener('click', () => {
    const rootUri = graphData.nodes.length > 0 ? graphData.nodes[0].uri : null;
    if (rootUri) vscode.postMessage({type: 'refresh', uri: rootUri});
});

// Auto-fit after simulation settles
simulation.on('end', () => {
    document.getElementById('btnFit').click();
});

initSliders();
</script>
</body>
</html>`
}

module.exports = {showImportGraph}

