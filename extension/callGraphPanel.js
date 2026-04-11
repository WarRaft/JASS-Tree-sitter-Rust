// noinspection JSUnusedGlobalSymbols

/**
 * @typedef {Object} CallGraphNode
 * @property {string}  name
 * @property {string}  uri
 * @property {boolean} is_frozen
 * @property {boolean} is_recursive
 * @property {boolean} in_cycle
 * @property {boolean} is_unused
 * @property {boolean} is_native
 */

/**
 * @typedef {Object} CallGraphResult
 * @property {CallGraphNode[]} nodes
 * @property {[number,number][]} edges
 * @property {number[]} topo_order
 * @property {boolean} is_orderable
 * @property {number[][]} cycles
 */

const {window, ViewColumn, Uri, workspace, Position, Selection} = require('vscode')
const path = require('path')

/** @type {import('vscode').WebviewPanel | undefined} */
let panel

/**
 * @param {import('./serverClient.js').ServerClient} client
 * @param {import('vscode').Uri} extensionUri
 * @param {import('vscode').ExtensionContext} context
 * @param {string} [fileUri]
 */
async function showCallGraph(client, extensionUri, context, fileUri) {
    if (!fileUri) {
        const editor = window.activeTextEditor
        if (!editor) {
            window.showWarningMessage('No active editor — open a .j or .as file first.')
            return
        }
        fileUri = editor.document.uri.toString()
    }

    /** @type {CallGraphResult} */
    const result = await client.sendRequest('graph/call', {uri: fileUri})

    if (!result || !result.nodes || result.nodes.length === 0) {
        window.showInformationMessage('Call graph is empty for this file.')
        return
    }

    if (panel) {
        panel.reveal(ViewColumn.Beside)
    } else {
        panel = window.createWebviewPanel(
            'callGraph',
            'Call Graph',
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

        panel.webview.onDidReceiveMessage(async (msg) => {
            if (msg.type === 'openFile') {
                try {
                    const uri = Uri.parse(msg.uri)
                    const doc = await workspace.openTextDocument(uri)
                    const text = doc.getText()
                    const name = msg.name || ''
                    const patterns = [
                        new RegExp(`\\bfunction\\s+${escapeRegex(name)}\\b`),
                        new RegExp(`\\bnative\\s+${escapeRegex(name)}\\b`),
                        new RegExp(`\\b${escapeRegex(name)}\\b`),
                    ]
                    let pos = new Position(0, 0)
                    for (const pat of patterns) {
                        const m = pat.exec(text)
                        if (m) {
                            pos = doc.positionAt(m.index)
                            break
                        }
                    }
                    const sel = new Selection(pos, pos)
                    await window.showTextDocument(doc, {selection: sel, preview: true})
                } catch (e) {
                    window.showErrorMessage(`Cannot open file: ${e.message}`)
                }
            } else if (msg.type === 'openCall') {
                try {
                    const uri = Uri.parse(msg.callerUri)
                    const doc = await workspace.openTextDocument(uri)
                    const text = doc.getText()
                    const callee = msg.calleeName || ''
                    const patterns = [
                        new RegExp(`\\bcall\\s+${escapeRegex(callee)}\\b`),
                        new RegExp(`\\b${escapeRegex(callee)}\\s*\\(`),
                        new RegExp(`\\b${escapeRegex(callee)}\\b`),
                    ]
                    let pos = new Position(0, 0)
                    for (const pat of patterns) {
                        const m = pat.exec(text)
                        if (m) {
                            pos = doc.positionAt(m.index)
                            break
                        }
                    }
                    const sel = new Selection(pos, pos)
                    await window.showTextDocument(doc, {selection: sel, preview: true})
                } catch (e) {
                    window.showErrorMessage(`Cannot open file: ${e.message}`)
                }
            } else if (msg.type === 'refresh') {
                await showCallGraph(client, extensionUri, context, msg.uri)
            } else if (msg.type === 'saveSettings') {
                if (context && context.globalState) {
                    await context.globalState.update('visCallPhysics', msg.settings)
                }
            }
        })
    }

    const visUri = panel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'vendor', 'vis-network.min.js')
    )
    const ppUri = panel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'vendor', 'physics-panel.js')
    )

    const basename = path.basename(decodeURIComponent(new URL(fileUri).pathname))
    const savedSettings = context.globalState.get('visCallPhysics', null)
    panel.title = `Call Graph — ${basename}`
    panel.webview.html = buildHtml(result, visUri.toString(), ppUri.toString(), fileUri, savedSettings)
}

/** @param {string} s */
function escapeRegex(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * @param {CallGraphResult} data
 * @param {string} visSrc
 * @param {string} ppSrc
 * @param {string} rootUri
 * @param {Object|null} savedSettings
 * @returns {string}
 */
function buildHtml(data, visSrc, ppSrc, rootUri, savedSettings) {
    const graphJSON = JSON.stringify({
        nodes: data.nodes.map((n, i) => ({
            id: i,
            name: n.name,
            uri: n.uri,
            isFrozen: n.is_frozen,
            isRecursive: n.is_recursive,
            inCycle: n.in_cycle,
            isUnused: n.is_unused,
            isNative: n.is_native,
        })),
        edges: data.edges.map(([s, t]) => ({from: s, to: t})),
        topoOrder: data.topo_order,
        isOrderable: data.is_orderable,
        cycles: data.cycles,
    })

    const settingsJSON = JSON.stringify(savedSettings || null)

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
    #graph { width: 100vw; height: 100vh; }

    .toolbar {
        position: fixed; top: 8px; right: 8px;
        display: flex; gap: 6px; z-index: 10;
    }
    .toolbar button {
        background: var(--vscode-button-background, #0e639c);
        color: var(--vscode-button-foreground, #fff);
        border: none; border-radius: 4px;
        padding: 4px 10px; cursor: pointer; font-size: 12px;
    }
    .toolbar button:hover {
        background: var(--vscode-button-hoverBackground, #1177bb);
    }

    .status-bar {
        position: fixed; top: 8px; left: 8px;
        font-size: 13px; font-weight: bold; z-index: 10;
        padding: 4px 10px; border-radius: 4px;
    }
    .status-bar.ok { color: #4ec9b0; background: rgba(78,201,176,0.1); }
    .status-bar.fail { color: #e06c40; background: rgba(224,108,64,0.1); }

    .legend {
        position: fixed; bottom: 8px; left: 8px;
        display: flex; flex-wrap: wrap; gap: 12px;
        align-items: center; font-size: 11px; opacity: 0.8; z-index: 10;
    }
    .legend-box {
        display: inline-block; width: 14px; height: 10px;
        border-radius: 2px; margin-right: 3px; vertical-align: middle;
    }
    .legend-box.normal {
        background: var(--vscode-editor-background, #1e1e1e);
        border: 1.5px solid var(--vscode-focusBorder, #007acc);
    }
    .legend-box.frozen { background: #1e2a1e; border: 1.5px solid #4ec9b0; }
    .legend-box.unused { background: #2d2d2d; border: 1.5px dashed #666; }
    .legend-box.cycle  { background: #2d1e1e; border: 2px solid #e06c40; }
    .legend-box.native-node { background: #1e1e2d; border: 1.5px dashed #569cd6; }
    .legend-box.recursive {
        background: #c586c0; border: none;
        width: 10px; height: 10px; border-radius: 50%;
    }
</style>
</head>
<body>

<div class="status-bar" id="statusBar"></div>

<div class="toolbar">
    <button id="btnFit" title="Fit to view">⊞ Fit</button>
    <button id="btnRefresh" title="Refresh graph">↻ Refresh</button>
    <button id="btnPhysics" title="Toggle physics config">⚙ Physics</button>
</div>

<div class="legend">
    <span><span class="legend-box normal"></span>Function</span>
    <span><span class="legend-box native-node"></span>Native</span>
    <span><span class="legend-box frozen"></span>Frozen</span>
    <span><span class="legend-box unused"></span>Unused</span>
    <span><span class="legend-box cycle"></span>Cycle</span>
    <span><span class="legend-box recursive"></span>Recursive</span>
    <span>→ calls</span>
</div>

<div id="graph"></div>
<physics-panel id="pp"></physics-panel>

<script src="${ppSrc}"></script>
<script src="${visSrc}"></script>
<script>
const vscode = acquireVsCodeApi();
const graphData = ${graphJSON};
const savedPhysics = ${settingsJSON};

const bgColor = getComputedStyle(document.documentElement).getPropertyValue('--vscode-editor-background').trim() || '#1e1e1e';
const fgColor = getComputedStyle(document.documentElement).getPropertyValue('--vscode-editor-foreground').trim() || '#d4d4d4';
const accentColor = getComputedStyle(document.documentElement).getPropertyValue('--vscode-focusBorder').trim() || '#007acc';
const borderColor = getComputedStyle(document.documentElement).getPropertyValue('--vscode-editorWidget-border').trim() || '#555';

// Status bar
const statusBar = document.getElementById('statusBar');
if (graphData.isOrderable) {
    statusBar.className = 'status-bar ok';
    statusBar.textContent = '✓ Orderable (' + graphData.nodes.length + ' functions)';
} else {
    statusBar.className = 'status-bar fail';
    const cycleCount = graphData.cycles.length;
    const cycleNames = graphData.cycles.map(c =>
        c.map(i => graphData.nodes[i].name).join(' ↔ ')
    ).join('; ');
    statusBar.textContent = '✗ ' + cycleCount + ' cycle(s): ' + cycleNames;
}

// Cycle node set
const cycleNodeSet = new Set();
graphData.cycles.forEach(c => c.forEach(i => cycleNodeSet.add(i)));

// Topo rank map for Y positioning
const rankMap = {};
graphData.topoOrder.forEach((idx, rank) => { rankMap[idx] = rank; });

function nodeColor(n) {
    if (n.inCycle) return { background: '#2d1e1e', border: '#e06c40' };
    if (n.isUnused) return { background: '#2d2d2d', border: '#666' };
    if (n.isFrozen) return { background: '#1e2a1e', border: '#4ec9b0' };
    if (n.isNative) return { background: '#1e1e2d', border: '#569cd6' };
    return { background: bgColor, border: accentColor };
}

function nodeFontColor(n) {
    if (n.inCycle) return '#e06c40';
    if (n.isUnused) return '#888';
    if (n.isFrozen) return '#4ec9b0';
    return fgColor;
}

function nodeBorderDashes(n) {
    if (n.isUnused) return [4, 2];
    if (n.isNative) return [2, 2];
    return false;
}

const nodes = new vis.DataSet(graphData.nodes.map(n => {
    const col = nodeColor(n);
    const label = n.isRecursive ? n.name + ' ↻' : n.name;
    return {
        id: n.id,
        label: label,
        title: buildTooltip(n),
        uri: n.uri,
        name: n.name,
        shape: 'box',
        color: {
            background: col.background,
            border: col.border,
            highlight: { background: col.background, border: '#dcdcaa' },
            hover: { background: col.background, border: '#dcdcaa' },
        },
        font: { color: nodeFontColor(n), size: 12, face: 'monospace' },
        borderWidth: n.inCycle ? 2 : 1.5,
        borderWidthSelected: 2.5,
        shapeProperties: { borderDashes: nodeBorderDashes(n) },
    };
}));

function buildTooltip(n) {
    let t = n.name;
    if (n.isNative) t += ' (native)';
    if (n.isFrozen) t += ' [frozen]';
    if (n.isRecursive) t += ' ↻ recursive';
    if (n.inCycle) t += ' ⚠ cyclic';
    if (n.isUnused) t += ' — unused';
    return t;
}

// Separate self-loop edges
const selfLoops = new Set();
graphData.edges.forEach((e, i) => { if (e.from === e.to) selfLoops.add(i); });

const edges = new vis.DataSet(graphData.edges.map((e, i) => {
    const isSelf = selfLoops.has(i);
    const isCycleEdge = !isSelf && cycleNodeSet.has(e.from) && cycleNodeSet.has(e.to);

    let edgeColor = borderColor;
    let dashes = false;
    let width = 1.2;
    if (isCycleEdge) { edgeColor = '#e06c40'; dashes = [6, 3]; width = 2; }
    if (isSelf) { edgeColor = '#c586c0'; width = 2; }

    return {
        id: i,
        from: e.from,
        to: e.to,
        arrows: 'to',
        color: { color: edgeColor, highlight: '#dcdcaa', hover: '#dcdcaa' },
        width: width,
        dashes: dashes,
        selfReference: isSelf ? { size: 25, angle: Math.PI / 4 } : undefined,
        smooth: isSelf ? { type: 'curvedCW', roundness: 0.6 }
                       : { type: 'cubicBezier', forceDirection: 'vertical', roundness: 0.4 },
    };
}));

const container = document.getElementById('graph');

const defaultPhysics = {
    enabled: true,
    solver: 'barnesHut',
    barnesHut: {
        gravitationalConstant: -3000,
        centralGravity: 0.05,
        springLength: 90,
        springConstant: 0.04,
        damping: 0.09,
        avoidOverlap: 0.3,
    },
    stabilization: { enabled: true, iterations: 200, updateInterval: 25 },
};

const network = new vis.Network(container, { nodes, edges }, {
    physics: savedPhysics || defaultPhysics,
    interaction: { hover: true, tooltipDelay: 200, keyboard: { enabled: true } },
    layout: { improvedLayout: true },
});

// Click to open file, double-click edge to open call site
network.on('click', params => {
    if (params.nodes.length > 0) {
        const n = nodes.get(params.nodes[0]);
        if (n && n.uri) vscode.postMessage({ type: 'openFile', uri: n.uri, name: n.name });
    }
});

network.on('doubleClick', params => {
    if (params.edges.length > 0 && params.nodes.length === 0) {
        const e = edges.get(params.edges[0]);
        if (e) {
            const src = nodes.get(e.from);
            const tgt = nodes.get(e.to);
            if (src && tgt) {
                vscode.postMessage({ type: 'openCall', callerUri: src.uri, calleeName: tgt.name });
            }
        }
    }
});

document.getElementById('btnFit').addEventListener('click', () => {
    network.fit({ animation: { duration: 500, easingFunction: 'easeInOutQuad' } });
});

document.getElementById('btnRefresh').addEventListener('click', () => {
    vscode.postMessage({ type: 'refresh', uri: '${rootUri}' });
});

const pp = document.getElementById('pp');
pp.physics = savedPhysics || defaultPhysics;
pp.addEventListener('change', e => {
    network.setOptions({ physics: e.detail });
    vscode.postMessage({ type: 'saveSettings', settings: e.detail });
});

document.getElementById('btnPhysics').addEventListener('click', () => pp.toggle());

network.once('stabilized', () => {
    network.fit({ animation: { duration: 500, easingFunction: 'easeInOutQuad' } });
});
</script>
</body>
</html>`
}

module.exports = {showCallGraph}

