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
                    await context.globalState.update('d3TypeGraphSettings', msg.settings)
                }
            }
        })
    }

    const d3Uri = panel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'vendor', 'd3.v7.min.js')
    )

    const savedSettings = context.globalState.get('d3TypeGraphSettings', null)

    const basename = path.basename(decodeURIComponent(new URL(fileUri).pathname))
    panel.title = `Type Graph — ${basename}`
    panel.webview.html = buildHtml(result, d3Uri.toString(), fileUri, savedSettings)
}

/** @param {string} s */
function escapeRegex(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * @param {TypeGraphResult} data
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
            isRoot: n.is_root,
            isFrozen: n.is_frozen,
        })),
        links: data.edges.map(([s, t]) => ({source: s, target: t})),
    })

    const settingsJSON = JSON.stringify(savedSettings || {
        linkDistance: 80,
        chargeStrength: -300,
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
    .link.highlighted {
        stroke: #dcdcaa;
        stroke-width: 2.5;
        marker-end: url(#arrow-highlight);
    }

    .node-rect {
        rx: 4; ry: 4;
        cursor: pointer;
        transition: opacity 0.15s;
    }
    .node-rect:hover { opacity: 0.85; }
    .node-rect.root {
        fill: #1e2a1e;
        stroke: #4ec9b0;
        stroke-width: 2;
    }
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
    .node-rect.synthetic {
        fill: #2d2d2d;
        stroke: #666;
        stroke-width: 1.5;
        stroke-dasharray: 4 2;
    }
    .node-rect.highlighted {
        stroke: #dcdcaa;
        stroke-width: 2.5;
    }
    .node-rect.dimmed {
        opacity: 0.25;
    }

    .node-label {
        fill: var(--vscode-editor-foreground, #d4d4d4);
        font-size: 11px;
        pointer-events: none;
        dominant-baseline: central;
        text-anchor: middle;
    }
    .node-label.root { fill: #4ec9b0; font-weight: bold; }
    .node-label.frozen { fill: #4ec9b0; }
    .node-label.dimmed { opacity: 0.25; }

    .depth-badge {
        fill: #888;
        font-size: 8px;
        pointer-events: none;
        dominant-baseline: central;
        text-anchor: middle;
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
        color: #4ec9b0;
        background: rgba(78, 201, 176, 0.1);
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
    .legend-box.root {
        background: #1e2a1e;
        border: 2px solid #4ec9b0;
    }
    .legend-box.normal {
        background: var(--vscode-editor-background, #1e1e1e);
        border: 1.5px solid var(--vscode-focusBorder, #007acc);
    }
    .legend-box.frozen {
        background: #1e2a1e;
        border: 1.5px solid #4ec9b0;
    }
    .legend-box.synthetic {
        background: #2d2d2d;
        border: 1.5px dashed #666;
    }

    /* ── Settings panel ── */
    #settingsBtn {
        position: fixed;
        bottom: 8px;
        right: 8px;
        z-index: 20;
        background: var(--vscode-button-background, #0e639c);
        color: var(--vscode-button-foreground, #fff);
        border: none;
        border-radius: 50%;
        width: 32px;
        height: 32px;
        font-size: 16px;
        cursor: pointer;
        display: flex;
        align-items: center;
        justify-content: center;
        box-shadow: 0 2px 6px rgba(0,0,0,0.3);
    }
    #settingsBtn:hover {
        background: var(--vscode-button-hoverBackground, #1177bb);
    }
    #settingsPanel {
        display: none;
        position: fixed;
        bottom: 48px;
        right: 8px;
        z-index: 20;
        background: var(--vscode-sideBar-background, #252526);
        border: 1px solid var(--vscode-editorWidget-border, #454545);
        border-radius: 6px;
        padding: 12px 14px;
        width: 240px;
        box-shadow: 0 4px 16px rgba(0,0,0,0.4);
    }
    #settingsPanel.open { display: block; }
    #settingsPanel h3 {
        margin: 0 0 10px 0;
        font-size: 12px;
        font-weight: 600;
        color: var(--vscode-editor-foreground, #d4d4d4);
    }
    .setting-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: 8px;
        font-size: 11px;
    }
    .setting-row label { flex: 1; }
    .setting-row input[type=range] {
        width: 100px;
        accent-color: var(--vscode-focusBorder, #007acc);
    }
    .setting-row .val {
        width: 36px;
        text-align: right;
        font-variant-numeric: tabular-nums;
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
    <span><span class="legend-box root"></span>handle (root)</span>
    <span><span class="legend-box normal"></span>Type</span>
    <span><span class="legend-box frozen"></span>Frozen</span>
    <span><span class="legend-box synthetic"></span>Synthetic</span>
    <span>→ extends</span>
</div>

<button id="settingsBtn" title="D3 Physics Settings">⚙</button>
<div id="settingsPanel">
    <h3>⚙ Physics Settings</h3>
    <div class="setting-row">
        <label>Link distance</label>
        <input type="range" id="sLinkDist" min="20" max="300" step="5"/>
        <span class="val" id="vLinkDist"></span>
    </div>
    <div class="setting-row">
        <label>Charge</label>
        <input type="range" id="sCharge" min="-1000" max="0" step="10"/>
        <span class="val" id="vCharge"></span>
    </div>
    <div class="setting-row">
        <label>Collision</label>
        <input type="range" id="sCollision" min="5" max="80" step="1"/>
        <span class="val" id="vCollision"></span>
    </div>
    <div class="setting-row">
        <label>Center</label>
        <input type="range" id="sCenter" min="0" max="1" step="0.01"/>
        <span class="val" id="vCenter"></span>
    </div>
</div>

<svg id="graph"></svg>

<script src="${d3Src}"></script>
<script>
const vscode = acquireVsCodeApi();
const graphData = ${graphJSON};
let settings = ${settingsJSON};

// ── Compute depth from root (BFS) ──────────────────────────────────────────
const childrenOf = {};
const parentOf = {};
graphData.links.forEach(l => {
    const s = typeof l.source === 'object' ? l.source.id : l.source;
    const t = typeof l.target === 'object' ? l.target.id : l.target;
    if (!childrenOf[s]) childrenOf[s] = [];
    childrenOf[s].push(t);
    parentOf[t] = s;
});

const depthMap = {};
let maxDepth = 0;
const rootNode = graphData.nodes.find(n => n.isRoot);
if (rootNode) {
    const queue = [{id: rootNode.id, depth: 0}];
    depthMap[rootNode.id] = 0;
    while (queue.length > 0) {
        const {id, depth} = queue.shift();
        (childrenOf[id] || []).forEach(cid => {
            if (depthMap[cid] === undefined) {
                depthMap[cid] = depth + 1;
                if (depth + 1 > maxDepth) maxDepth = depth + 1;
                queue.push({id: cid, depth: depth + 1});
            }
        });
    }
}
graphData.nodes.forEach(n => {
    if (depthMap[n.id] === undefined) depthMap[n.id] = 0;
    n.depth = depthMap[n.id];
});

// ── Ancestor path + subtree (for highlighting) ────────────────────────────
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

// ── Status bar ─────────────────────────────────────────────────────────────
const statusBar = document.getElementById('statusBar');
const leafCount = graphData.nodes.filter(n =>
    !childrenOf[n.id] || childrenOf[n.id].length === 0
).length;
statusBar.textContent = graphData.nodes.length + ' types · depth ' + maxDepth + ' · ' + leafCount + ' leaf types';

const svg = d3.select('#graph');
const width = window.innerWidth;
const height = window.innerHeight;

// ── Settings panel ─────────────────────────────────────────────────────────
document.getElementById('settingsBtn').addEventListener('click', () => {
    document.getElementById('settingsPanel').classList.toggle('open');
});

function initSliders() {
    const ids = [
        ['sLinkDist', 'vLinkDist', 'linkDistance'],
        ['sCharge',   'vCharge',   'chargeStrength'],
        ['sCollision','vCollision','collisionRadius'],
        ['sCenter',   'vCenter',  'centerStrength'],
    ];
    ids.forEach(([sid, vid, key]) => {
        const slider = document.getElementById(sid);
        const valEl = document.getElementById(vid);
        slider.value = settings[key];
        valEl.textContent = settings[key];
        slider.addEventListener('input', () => {
            const v = parseFloat(slider.value);
            settings[key] = v;
            valEl.textContent = v;
            applySettings();
            vscode.postMessage({type: 'saveSettings', settings});
        });
    });
}

function applySettings() {
    simulation.force('link').distance(settings.linkDistance);
    simulation.force('charge').strength(settings.chargeStrength);
    simulation.force('collision').radius(d => d.boxWidth / 2 + settings.collisionRadius);
    simulation.force('center').strength(settings.centerStrength);
    simulation.alpha(0.5).restart();
}

// ── Arrow markers ──────────────────────────────────────────────────────────
const defs = svg.append('defs');
const edgeColor = getComputedStyle(document.documentElement)
    .getPropertyValue('--vscode-editorWidget-border').trim() || '#555';

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
addMarker('arrow', edgeColor);
addMarker('arrow-highlight', '#dcdcaa');

const g = svg.append('g');
const zoom = d3.zoom()
    .scaleExtent([0.1, 5])
    .on('zoom', e => g.attr('transform', e.transform));
svg.call(zoom);

// ── Measure text ───────────────────────────────────────────────────────────
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

// ── Force simulation with depth-based Y ────────────────────────────────────
const levelSpacing = Math.min(100, (height - 120) / (maxDepth + 1 || 1));

const simulation = d3.forceSimulation(graphData.nodes)
    .force('link', d3.forceLink(graphData.links)
        .id(d => d.id)
        .distance(settings.linkDistance)
        .strength(0.4))
    .force('charge', d3.forceManyBody().strength(settings.chargeStrength))
    .force('center', d3.forceCenter(width / 2, height / 2).strength(settings.centerStrength))
    .force('collision', d3.forceCollide().radius(d => d.boxWidth / 2 + settings.collisionRadius))
    .force('y', d3.forceY(d => 60 + d.depth * levelSpacing).strength(0.8));

// ── Edge hit areas ─────────────────────────────────────────────────────────
const linkHit = g.append('g')
    .selectAll('line')
    .data(graphData.links)
    .join('line')
    .attr('class', 'link-hit')
    .on('click', (e, d) => {
        e.stopPropagation();
        const tgt = typeof d.target === 'object' ? d.target : graphData.nodes[d.target];
        if (tgt && tgt.uri) {
            vscode.postMessage({ type: 'openEdge', childUri: tgt.uri, childName: tgt.name });
        }
    });

// ── Visible edges ──────────────────────────────────────────────────────────
const link = g.append('g')
    .selectAll('line')
    .data(graphData.links)
    .join('line')
    .attr('class', 'link');

// ── Nodes ──────────────────────────────────────────────────────────────────
const node = g.append('g')
    .selectAll('g')
    .data(graphData.nodes)
    .join('g')
    .call(d3.drag()
        .on('start', (e, d) => { if (!e.active) simulation.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
        .on('drag', (e, d) => { d.fx = e.x; d.fy = e.y; })
        .on('end', (e, d) => { if (!e.active) simulation.alphaTarget(0); d.fx = null; d.fy = null; })
    );

node.append('rect')
    .attr('class', d => {
        if (d.isRoot) return 'node-rect root';
        if (d.isFrozen) return 'node-rect frozen';
        if (!d.uri) return 'node-rect synthetic';
        return 'node-rect normal';
    })
    .attr('width', d => d.boxWidth)
    .attr('height', d => d.boxHeight)
    .attr('x', d => -d.boxWidth / 2)
    .attr('y', d => -d.boxHeight / 2)
    .on('click', (e, d) => {
        e.stopPropagation();
        if (d.uri) vscode.postMessage({type: 'openFile', uri: d.uri, name: d.name});
    });

node.append('title')
    .text(d => {
        let t = d.name;
        if (d.isRoot) t += ' (root)';
        if (d.isFrozen) t += ' [frozen]';
        if (!d.uri && !d.isRoot) t += ' (synthetic)';
        t += '\\ndepth: ' + d.depth;
        const ch = childrenOf[d.id] || [];
        if (ch.length > 0) t += '\\nchildren: ' + ch.length;
        return t;
    });

node.append('text')
    .attr('class', d => {
        if (d.isRoot) return 'node-label root';
        if (d.isFrozen) return 'node-label frozen';
        return 'node-label';
    })
    .text(d => d.name);

// Depth badge
node.filter(d => d.depth > 0)
    .append('text')
    .attr('class', 'depth-badge')
    .attr('dy', d => d.boxHeight / 2 + 10)
    .text(d => 'L' + d.depth);

// ── Highlight on hover ─────────────────────────────────────────────────────
node.on('mouseenter', (e, d) => {
    const ancestors = getAncestorPath(d.id);
    const subtree = getSubtree(d.id);
    const hl = new Set([...ancestors, ...subtree]);

    node.select('rect')
        .classed('dimmed', n => !hl.has(n.id))
        .classed('highlighted', n => hl.has(n.id) && n.id !== d.id);
    node.select('text.node-label')
        .classed('dimmed', n => !hl.has(n.id));
    link.classed('highlighted', l => {
        const si = typeof l.source === 'object' ? l.source.id : l.source;
        const ti = typeof l.target === 'object' ? l.target.id : l.target;
        return hl.has(si) && hl.has(ti);
    });
}).on('mouseleave', () => {
    node.select('rect').classed('dimmed', false).classed('highlighted', false);
    node.select('text.node-label').classed('dimmed', false);
    link.classed('highlighted', false);
});

svg.on('click', () => {
    node.select('rect').classed('dimmed', false).classed('highlighted', false);
    node.select('text.node-label').classed('dimmed', false);
    link.classed('highlighted', false);
});

// ── Tick ───────────────────────────────────────────────────────────────────
simulation.on('tick', () => {
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
    linkHit
        .attr('x1', d => d.source.x)
        .attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x)
        .attr('y2', d => d.target.y);
    node.attr('transform', d => 'translate(' + d.x + ',' + d.y + ')');
});

// ── Fit ────────────────────────────────────────────────────────────────────
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

initSliders();
</script>
</body>
</html>`
}

module.exports = {showTypeGraph}

