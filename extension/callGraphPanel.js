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
                // Open the caller's file at the first call site of the callee
                try {
                    const uri = Uri.parse(msg.callerUri)
                    const doc = await workspace.openTextDocument(uri)
                    const text = doc.getText()
                    const callee = msg.calleeName || ''
                    // Search for "call <callee>" or "<callee>(" inside the caller
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
                    await context.globalState.update('d3PhysicsSettings', msg.settings)
                }
            }
        })
    }

    const d3Uri = panel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'vendor', 'd3.v7.min.js')
    )

    const basename = path.basename(decodeURIComponent(new URL(fileUri).pathname))
    const savedSettings = context.globalState.get('d3PhysicsSettings', null)
    panel.title = `Call Graph — ${basename}`
    panel.webview.html = buildHtml(result, d3Uri.toString(), fileUri, savedSettings)
}

/** @param {string} s */
function escapeRegex(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * @param {CallGraphResult} data
 * @param {string} d3Src
 * @param {string} rootUri
 * @param {Object|null} savedSettings
 * @returns {string}
 */
function buildHtml(data, d3Src, rootUri, savedSettings) {
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
        links: data.edges.map(([s, t]) => ({source: s, target: t})),
        topoOrder: data.topo_order,
        isOrderable: data.is_orderable,
        cycles: data.cycles,
    })

    const settingsJSON = JSON.stringify(savedSettings || {
        linkDistance: 90,
        chargeStrength: -250,
        collisionRadius: 8,
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

    .link-hit {
        stroke: transparent;
        stroke-width: 12;
        fill: none;
        cursor: pointer;
    }
    .link {
        stroke: var(--vscode-editorWidget-border, #555);
        stroke-width: 1.2;
        fill: none;
        pointer-events: none;
        marker-end: url(#arrow);
    }
    .link.cycle-edge {
        stroke: #e06c40;
        stroke-width: 2;
        stroke-dasharray: 6 3;
        marker-end: url(#arrow-cycle);
    }
    .link.self-loop {
        stroke: #c586c0;
        stroke-width: 2;
        fill: none;
        pointer-events: none;
        marker-end: url(#arrow-loop);
    }
    .self-loop-hit {
        stroke: transparent;
        stroke-width: 12;
        fill: none;
        cursor: pointer;
    }

    .node-rect {
        rx: 4; ry: 4;
        cursor: pointer;
        transition: opacity 0.15s;
    }
    .node-rect:hover { opacity: 0.85; }

    .node-rect.normal {
        fill: var(--vscode-editor-background, #1e1e1e);
        stroke: var(--vscode-focusBorder, #007acc);
        stroke-width: 1.5;
    }
    .node-rect.frozen {
        fill: #1e2a1e;
        stroke: #4ec9b0;
        stroke-width: 1.5;
    }
    .node-rect.unused {
        fill: #2d2d2d;
        stroke: #666;
        stroke-width: 1.5;
        stroke-dasharray: 4 2;
    }
    .node-rect.cycle {
        fill: #2d1e1e;
        stroke: #e06c40;
        stroke-width: 2;
    }
    .node-rect.native-node {
        fill: #1e1e2d;
        stroke: #569cd6;
        stroke-width: 1.5;
        stroke-dasharray: 2 2;
    }

    .node-label {
        fill: var(--vscode-editor-foreground, #d4d4d4);
        font-size: 11px;
        pointer-events: none;
        dominant-baseline: central;
        text-anchor: middle;
    }
    .node-label.unused { fill: #888; }
    .node-label.cycle  { fill: #e06c40; }
    .node-label.frozen { fill: #4ec9b0; }

    .recursion-badge {
        fill: #c586c0;
        font-size: 14px;
        pointer-events: none;
        dominant-baseline: central;
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

    .status-bar {
        position: fixed;
        top: 8px;
        left: 8px;
        font-size: 13px;
        font-weight: bold;
        z-index: 10;
        padding: 4px 10px;
        border-radius: 4px;
    }
    .status-bar.ok {
        color: #4ec9b0;
        background: rgba(78, 201, 176, 0.1);
    }
    .status-bar.fail {
        color: #e06c40;
        background: rgba(224, 108, 64, 0.1);
    }

    .legend {
        position: fixed;
        bottom: 8px;
        left: 8px;
        display: flex;
        flex-wrap: wrap;
        gap: 12px;
        align-items: center;
        font-size: 11px;
        opacity: 0.8;
    }
    .legend-box {
        display: inline-block;
        width: 14px; height: 10px;
        border-radius: 2px;
        margin-right: 3px;
        vertical-align: middle;
    }
    .legend-box.normal {
        background: var(--vscode-editor-background, #1e1e1e);
        border: 1.5px solid var(--vscode-focusBorder, #007acc);
    }
    .legend-box.frozen {
        background: #1e2a1e;
        border: 1.5px solid #4ec9b0;
    }
    .legend-box.unused {
        background: #2d2d2d;
        border: 1.5px dashed #666;
    }
    .legend-box.cycle {
        background: #2d1e1e;
        border: 2px solid #e06c40;
    }
    .legend-box.native-node {
        background: #1e1e2d;
        border: 1.5px dashed #569cd6;
    }
    .legend-box.recursive {
        background: #c586c0;
        border: none;
        width: 10px; height: 10px;
        border-radius: 50%;
    }
</style>
</head>
<body>

<div class="status-bar" id="statusBar"></div>

<div class="toolbar">
    <button id="btnFit" title="Fit to view">⊞ Fit</button>
    <button id="btnRefresh" title="Refresh graph">↻ Refresh</button>
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

<svg id="graph"></svg>

<script src="${d3Src}"></script>
<script>
const vscode = acquireVsCodeApi();
const graphData = ${graphJSON};

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

const svg = d3.select('#graph');
const width = window.innerWidth;
const height = window.innerHeight;

// Cycle node set for edge highlighting
const cycleNodeSet = new Set();
graphData.cycles.forEach(c => c.forEach(i => cycleNodeSet.add(i)));

// Separate self-loop edges from normal edges
const selfLoopIndices = new Set();
const normalLinks = [];
graphData.links.forEach((l, i) => {
    const s = typeof l.source === 'object' ? l.source.id : l.source;
    const t = typeof l.target === 'object' ? l.target.id : l.target;
    if (s === t) selfLoopIndices.add(i);
    else normalLinks.push(l);
});

// Measure text for node sizing
const tempSvg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
document.body.appendChild(tempSvg);
const tempText = document.createElementNS('http://www.w3.org/2000/svg', 'text');
tempText.style.fontSize = '11px';
tempText.style.fontFamily = getComputedStyle(document.body).fontFamily;
tempSvg.appendChild(tempText);
graphData.nodes.forEach(n => {
    tempText.textContent = n.name;
    n.boxWidth = Math.max(tempText.getComputedTextLength() + 16, 40);
    n.boxHeight = 22;
});
document.body.removeChild(tempSvg);

// Rank map from topo order
const rankMap = {};
graphData.topoOrder.forEach((idx, rank) => { rankMap[idx] = rank; });
const totalRanks = graphData.topoOrder.length || 1;

// Arrow markers
const defs = svg.append('defs');
function addMarker(id, color) {
    defs.append('marker')
        .attr('id', id)
        .attr('viewBox', '0 -5 10 10')
        .attr('refX', 10).attr('refY', 0)
        .attr('markerWidth', 7).attr('markerHeight', 7)
        .attr('orient', 'auto')
        .append('path')
        .attr('d', 'M0,-4L10,0L0,4')
        .attr('fill', color);
}
const edgeColor = getComputedStyle(document.documentElement)
    .getPropertyValue('--vscode-editorWidget-border').trim() || '#555';
addMarker('arrow', edgeColor);
addMarker('arrow-cycle', '#e06c40');
addMarker('arrow-loop', '#c586c0');

const g = svg.append('g');

const zoom = d3.zoom()
    .scaleExtent([0.1, 5])
    .on('zoom', e => g.attr('transform', e.transform));
svg.call(zoom);

// Force simulation: Y from topo rank so callees sit above callers
const simulation = d3.forceSimulation(graphData.nodes)
    .force('link', d3.forceLink(normalLinks)
        .id(d => d.id).distance(90).strength(0.3))
    .force('charge', d3.forceManyBody().strength(-250))
    .force('center', d3.forceCenter(width / 2, height / 2))
    .force('collision', d3.forceCollide().radius(d => d.boxWidth / 2 + 8))
    .force('y', d3.forceY(d => {
        const rank = rankMap[d.id] !== undefined ? rankMap[d.id] : 0;
        return 60 + (rank / totalRanks) * (height - 120);
    }).strength(0.6));

// Edge hit areas (wide invisible lines for clicking)
const linkHit = g.append('g')
    .selectAll('line')
    .data(normalLinks)
    .join('line')
    .attr('class', 'link-hit')
    .on('click', (e, d) => {
        e.stopPropagation();
        const src = typeof d.source === 'object' ? d.source : graphData.nodes[d.source];
        const tgt = typeof d.target === 'object' ? d.target : graphData.nodes[d.target];
        vscode.postMessage({
            type: 'openCall',
            callerUri: src.uri,
            calleeName: tgt.name
        });
    });

// Visible edges
const link = g.append('g')
    .selectAll('line')
    .data(normalLinks)
    .join('line')
    .attr('class', d => {
        const si = typeof d.source === 'object' ? d.source.id : d.source;
        const ti = typeof d.target === 'object' ? d.target.id : d.target;
        return 'link' + (cycleNodeSet.has(si) && cycleNodeSet.has(ti) ? ' cycle-edge' : '');
    });

// Self-loop hit areas
const selfLoopData = graphData.links.filter((_, i) => selfLoopIndices.has(i));
const selfLoopHit = g.append('g')
    .selectAll('path')
    .data(selfLoopData)
    .join('path')
    .attr('class', 'self-loop-hit')
    .on('click', (e, d) => {
        e.stopPropagation();
        const src = typeof d.source === 'object' ? d.source : graphData.nodes[d.source];
        vscode.postMessage({
            type: 'openCall',
            callerUri: src.uri,
            calleeName: src.name
        });
    });

// Visible self-loop paths
const selfLoop = g.append('g')
    .selectAll('path')
    .data(selfLoopData)
    .join('path')
    .attr('class', 'link self-loop');

// Node groups
const node = g.append('g')
    .selectAll('g')
    .data(graphData.nodes)
    .join('g')
    .call(d3.drag()
        .on('start', (e, d) => { if (!e.active) simulation.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
        .on('drag', (e, d) => { d.fx = e.x; d.fy = e.y; })
        .on('end', (e, d) => { if (!e.active) simulation.alphaTarget(0); d.fx = null; d.fy = null; })
    );

// Rectangle
node.append('rect')
    .attr('class', d => {
        if (d.inCycle) return 'node-rect cycle';
        if (d.isUnused) return 'node-rect unused';
        if (d.isFrozen) return 'node-rect frozen';
        if (d.isNative) return 'node-rect native-node';
        return 'node-rect normal';
    })
    .attr('width', d => d.boxWidth)
    .attr('height', d => d.boxHeight)
    .attr('x', d => -d.boxWidth / 2)
    .attr('y', d => -d.boxHeight / 2)
    .on('click', (e, d) => {
        e.stopPropagation();
        vscode.postMessage({ type: 'openFile', uri: d.uri, name: d.name });
    });

node.append('title')
    .text(d => {
        let t = d.name;
        if (d.isNative) t += ' (native)';
        if (d.isFrozen) t += ' [frozen]';
        if (d.isRecursive) t += ' ↻ recursive';
        if (d.inCycle) t += ' ⚠ cyclic';
        if (d.isUnused) t += ' — unused';
        return t;
    });

node.append('text')
    .attr('class', d => {
        if (d.inCycle) return 'node-label cycle';
        if (d.isUnused) return 'node-label unused';
        if (d.isFrozen) return 'node-label frozen';
        return 'node-label';
    })
    .text(d => d.name);

// Recursion badge
node.filter(d => d.isRecursive)
    .append('text')
    .attr('class', 'recursion-badge')
    .attr('dx', d => d.boxWidth / 2 + 3)
    .attr('dy', 0)
    .text('↻');

// Tick
simulation.on('tick', () => {
    // Update visible edges
    link
        .attr('x1', d => d.source.x)
        .attr('y1', d => d.source.y)
        .attr('x2', d => {
            const dx = d.target.x - d.source.x, dy = d.target.y - d.source.y;
            const len = Math.sqrt(dx*dx + dy*dy) || 1;
            return d.target.x - (dx/len) * (d.target.boxWidth/2 + 4);
        })
        .attr('y2', d => {
            const dx = d.target.x - d.source.x, dy = d.target.y - d.source.y;
            const len = Math.sqrt(dx*dx + dy*dy) || 1;
            return d.target.y - (dy/len) * (d.target.boxHeight/2 + 4);
        });

    // Update hit areas (same positions)
    linkHit
        .attr('x1', d => d.source.x)
        .attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x)
        .attr('y2', d => d.target.y);

    node.attr('transform', d => 'translate(' + d.x + ',' + d.y + ')');

    // Self-loop arcs
    const loopPath = d => {
        const n = graphData.nodes[typeof d.source === 'object' ? d.source.id : d.source];
        if (!n) return '';
        const x = n.x||0, y = n.y||0, r = 22, ox = n.boxWidth/2;
        return 'M'+(x+ox)+','+(y-5)+' C'+(x+ox+r)+','+(y-r-12)+' '+(x+ox+r)+','+(y+r+12)+' '+(x+ox)+','+(y+5);
    };
    selfLoop.attr('d', loopPath);
    selfLoopHit.attr('d', loopPath);
});

// Fit
document.getElementById('btnFit').addEventListener('click', () => {
    const b = g.node().getBBox();
    if (!b.width || !b.height) return;
    const pad = 60;
    const sc = Math.min(width/(b.width+pad*2), height/(b.height+pad*2), 2);
    const tx = width/2 - sc*(b.x+b.width/2);
    const ty = height/2 - sc*(b.y+b.height/2);
    svg.transition().duration(500)
        .call(zoom.transform, d3.zoomIdentity.translate(tx,ty).scale(sc));
});

document.getElementById('btnRefresh').addEventListener('click', () => {
    vscode.postMessage({type:'refresh', uri:'${rootUri}'});
});

simulation.on('end', () => document.getElementById('btnFit').click());
</script>
</body>
</html>`
}

module.exports = {showCallGraph}

