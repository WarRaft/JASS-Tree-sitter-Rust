// noinspection JSUnusedGlobalSymbols

/**
 * @typedef {Object} TypeGraphNode
 * @property {string}  name
 * @property {string}  uri
 * @property {boolean} is_root
 * @property {boolean} is_frozen
 */

/**
 * @typedef {Object} TypeGraphResult
 * @property {TypeGraphNode[]} nodes
 * @property {[number, number][]} edges
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
async function showTypeGraph(client, extensionUri, context, fileUri) {
    if (!fileUri) {
        const editor = window.activeTextEditor
        if (!editor) {
            window.showWarningMessage('No active editor — open a .j or .as file first.')
            return
        }
        fileUri = editor.document.uri.toString()
    }

    /** @type {TypeGraphResult} */
    const result = await client.sendRequest('graph/type', {uri: fileUri})

    if (!result || !result.nodes || result.nodes.length === 0) {
        window.showInformationMessage('Type graph is empty for this file.')
        return
    }

    if (panel) {
        panel.reveal(ViewColumn.Beside)
    } else {
        panel = window.createWebviewPanel(
            'typeGraph',
            'Type Graph',
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
                        new RegExp(`\\btype\\s+${escapeRegex(name)}\\b`),
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
            } else if (msg.type === 'openEdge') {
                try {
                    const uri = Uri.parse(msg.childUri)
                    const doc = await workspace.openTextDocument(uri)
                    const text = doc.getText()
                    const childName = msg.childName || ''
                    const pat = new RegExp(`\\btype\\s+${escapeRegex(childName)}\\s+extends\\b`)
                    const m = pat.exec(text)
                    let pos = new Position(0, 0)
                    if (m) pos = doc.positionAt(m.index)
                    const sel = new Selection(pos, pos)
                    await window.showTextDocument(doc, {selection: sel, preview: true})
                } catch (e) {
                    window.showErrorMessage(`Cannot open file: ${e.message}`)
                }
            } else if (msg.type === 'refresh') {
                await showTypeGraph(client, extensionUri, context, msg.uri)
            } else if (msg.type === 'saveSettings') {
                if (context && context.globalState) {
                    await context.globalState.update('visTypePhysics', msg.settings)
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

    const savedSettings = context.globalState.get('visTypePhysics', null)

    const basename = path.basename(decodeURIComponent(new URL(fileUri).pathname))
    panel.title = `Type Graph — ${basename}`
    panel.webview.html = buildHtml(result, visUri.toString(), ppUri.toString(), fileUri, savedSettings)
}

/** @param {string} s */
function escapeRegex(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * @param {TypeGraphResult} data
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
            isRoot: n.is_root,
            isFrozen: n.is_frozen,
        })),
        edges: data.edges.map(([s, t]) => ({from: s, to: t})),
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
        color: #4ec9b0; background: rgba(78,201,176,0.1);
    }

    .legend {
        position: fixed; bottom: 8px; left: 8px;
        display: flex; flex-wrap: wrap; gap: 12px;
        align-items: center; font-size: 11px; opacity: 0.8; z-index: 10;
    }
    .legend-box {
        display: inline-block; width: 14px; height: 10px;
        border-radius: 2px; margin-right: 3px; vertical-align: middle;
    }
    .legend-box.root { background: #1e2a1e; border: 2px solid #4ec9b0; }
    .legend-box.normal {
        background: var(--vscode-editor-background, #1e1e1e);
        border: 1.5px solid var(--vscode-focusBorder, #007acc);
    }
    .legend-box.frozen { background: #1e2a1e; border: 1.5px solid #4ec9b0; }
    .legend-box.synthetic { background: #2d2d2d; border: 1.5px dashed #666; }
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
    <span><span class="legend-box root"></span>handle (root)</span>
    <span><span class="legend-box normal"></span>Type</span>
    <span><span class="legend-box frozen"></span>Frozen</span>
    <span><span class="legend-box synthetic"></span>Synthetic</span>
    <span>→ extends</span>
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

// ── Build parent/children maps ──
const childrenOf = {};
const parentOf = {};
graphData.edges.forEach(e => {
    const s = e.from, t = e.to;
    if (!childrenOf[s]) childrenOf[s] = [];
    childrenOf[s].push(t);
    parentOf[t] = s;
});

// ── Compute depth from root (BFS) ──
const depthMap = {};
let maxDepth = 0;
const rootNode = graphData.nodes.find(n => n.isRoot);
if (rootNode) {
    const queue = [{ id: rootNode.id, depth: 0 }];
    depthMap[rootNode.id] = 0;
    while (queue.length > 0) {
        const { id, depth } = queue.shift();
        (childrenOf[id] || []).forEach(cid => {
            if (depthMap[cid] === undefined) {
                depthMap[cid] = depth + 1;
                if (depth + 1 > maxDepth) maxDepth = depth + 1;
                queue.push({ id: cid, depth: depth + 1 });
            }
        });
    }
}
graphData.nodes.forEach(n => {
    if (depthMap[n.id] === undefined) depthMap[n.id] = 0;
    n.depth = depthMap[n.id];
});

// ── Status bar ──
const statusBar = document.getElementById('statusBar');
const leafCount = graphData.nodes.filter(n =>
    !childrenOf[n.id] || childrenOf[n.id].length === 0
).length;
statusBar.textContent = graphData.nodes.length + ' types · depth ' + maxDepth + ' · ' + leafCount + ' leaf types';

// ── Ancestor / subtree helpers for highlighting ──
function getAncestorPath(nodeId) {
    const p = new Set();
    let cur = nodeId;
    while (cur !== undefined) { p.add(cur); cur = parentOf[cur]; }
    return p;
}
function getSubtree(nodeId) {
    const t = new Set();
    const stack = [nodeId];
    while (stack.length > 0) {
        const cur = stack.pop();
        t.add(cur);
        (childrenOf[cur] || []).forEach(c => { if (!t.has(c)) stack.push(c); });
    }
    return t;
}

function nodeStyle(n) {
    if (n.isRoot) return { bg: '#1e2a1e', border: '#4ec9b0', fontColor: '#4ec9b0', dashes: false, bw: 2 };
    if (n.isFrozen) return { bg: '#1e2a1e', border: '#4ec9b0', fontColor: '#4ec9b0', dashes: false, bw: 1.5 };
    if (!n.uri) return { bg: '#2d2d2d', border: '#666', fontColor: '#888', dashes: [4, 2], bw: 1.5 };
    return { bg: bgColor, border: accentColor, fontColor: fgColor, dashes: false, bw: 1.5 };
}

const nodes = new vis.DataSet(graphData.nodes.map(n => {
    const s = nodeStyle(n);
    return {
        id: n.id,
        label: n.name + (n.depth > 0 ? '\\nL' + n.depth : ''),
        title: buildTooltip(n),
        uri: n.uri,
        name: n.name,
        depth: n.depth,
        shape: 'box',
        color: {
            background: s.bg, border: s.border,
            highlight: { background: s.bg, border: '#dcdcaa' },
            hover: { background: s.bg, border: '#dcdcaa' },
        },
        font: { color: s.fontColor, size: 12, face: 'monospace', bold: n.isRoot ? { color: s.fontColor } : undefined },
        borderWidth: s.bw,
        borderWidthSelected: 2.5,
        shapeProperties: { borderDashes: s.dashes },
        level: n.depth,
    };
}));

function buildTooltip(n) {
    let t = n.name;
    if (n.isRoot) t += ' (root)';
    if (n.isFrozen) t += ' [frozen]';
    if (!n.uri && !n.isRoot) t += ' (synthetic)';
    t += '\\ndepth: ' + n.depth;
    const ch = childrenOf[n.id] || [];
    if (ch.length > 0) t += '\\nchildren: ' + ch.length;
    return t;
}

const edges = new vis.DataSet(graphData.edges.map((e, i) => ({
    id: i,
    from: e.from,
    to: e.to,
    arrows: 'to',
    color: { color: borderColor, highlight: '#dcdcaa', hover: '#dcdcaa' },
    width: 1.2,
    smooth: { type: 'cubicBezier', forceDirection: 'vertical', roundness: 0.4 },
})));

const container = document.getElementById('graph');

const defaultPhysics = {
    enabled: true,
    solver: 'barnesHut',
    barnesHut: {
        gravitationalConstant: -3000,
        centralGravity: 0.05,
        springLength: 80,
        springConstant: 0.04,
        damping: 0.09,
        avoidOverlap: 0.3,
    },
    stabilization: { enabled: true, iterations: 200, updateInterval: 25 },
};

const network = new vis.Network(container, { nodes, edges }, {
    physics: savedPhysics || defaultPhysics,
    interaction: { hover: true, tooltipDelay: 200, keyboard: { enabled: true } },
    layout: {
        hierarchical: {
            enabled: true,
            direction: 'UD',
            sortMethod: 'directed',
            levelSeparation: 100,
            nodeSpacing: 150,
            treeSpacing: 200,
        }
    },
});

// ── Highlight subtree/ancestor on hover ──
let highlightActive = false;
network.on('hoverNode', params => {
    const nodeId = params.node;
    const ancestors = getAncestorPath(nodeId);
    const subtree = getSubtree(nodeId);
    const hl = new Set([...ancestors, ...subtree]);

    const updatedNodes = [];
    nodes.forEach(n => {
        const isHighlighted = hl.has(n.id);
        updatedNodes.push({
            id: n.id,
            opacity: isHighlighted ? 1.0 : 0.15,
        });
    });
    nodes.update(updatedNodes);

    const updatedEdges = [];
    edges.forEach(e => {
        const isHL = hl.has(e.from) && hl.has(e.to);
        updatedEdges.push({
            id: e.id,
            color: isHL
                ? { color: '#dcdcaa', highlight: '#dcdcaa', hover: '#dcdcaa' }
                : { color: borderColor, highlight: '#dcdcaa', hover: '#dcdcaa' },
            width: isHL ? 2.5 : 1.2,
        });
    });
    edges.update(updatedEdges);
    highlightActive = true;
});

network.on('blurNode', () => {
    if (!highlightActive) return;
    const updatedNodes = [];
    nodes.forEach(n => { updatedNodes.push({ id: n.id, opacity: 1.0 }); });
    nodes.update(updatedNodes);

    const updatedEdges = [];
    edges.forEach(e => {
        updatedEdges.push({
            id: e.id,
            color: { color: borderColor, highlight: '#dcdcaa', hover: '#dcdcaa' },
            width: 1.2,
        });
    });
    edges.update(updatedEdges);
    highlightActive = false;
});

// Click to open file
network.on('click', params => {
    if (params.nodes.length > 0) {
        const n = nodes.get(params.nodes[0]);
        if (n && n.uri) vscode.postMessage({ type: 'openFile', uri: n.uri, name: n.name });
    }
});

// Double-click edge to open extends declaration
network.on('doubleClick', params => {
    if (params.edges.length > 0 && params.nodes.length === 0) {
        const e = edges.get(params.edges[0]);
        if (e) {
            const tgt = nodes.get(e.to);
            if (tgt && tgt.uri) {
                vscode.postMessage({ type: 'openEdge', childUri: tgt.uri, childName: tgt.name });
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

module.exports = {showTypeGraph}

