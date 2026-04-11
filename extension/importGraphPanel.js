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
 * @param {import('./serverClient.js').ServerClient} client
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
    const result = await client.sendRequest('graph/import', {
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
                    await context.globalState.update('visImportPhysics', msg.settings)
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

    panel.title = `Import Graph — ${path.basename(decodeURIComponent(new URL(result.uri).pathname))}`
    const savedSettings = context.globalState.get('visImportPhysics', null)
    panel.webview.html = buildHtml(result, visUri.toString(), ppUri.toString(), savedSettings)
}

/**
 * @param {string[]} paths
 * @returns {string[]}
 */
function shortenPaths(paths) {
    if (paths.length === 0) return []
    if (paths.length === 1) {
        const parts = paths[0].split('/')
        return [parts[parts.length - 1] || paths[0]]
    }
    const split = paths.map(p => p.split('/'))
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
 * @param {string} visSrc
 * @param {string} ppSrc
 * @param {Object|null} savedSettings
 * @returns {string}
 */
function buildHtml(data, visSrc, ppSrc, savedSettings) {
    const paths = data.nodes.map(uri => {
        try { return decodeURIComponent(new URL(uri).pathname) }
        catch { return uri }
    })
    const labels = shortenPaths(paths)

    const nodesJSON = JSON.stringify(data.nodes.map((uri, i) => ({
        id: i, label: labels[i], uri, isRoot: i === 0,
    })))
    const edgesJSON = JSON.stringify(data.edges.map(([s, t]) => ({from: s, to: t})))
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

    .legend {
        position: fixed; bottom: 8px; left: 8px;
        display: flex; gap: 12px; align-items: center;
        font-size: 11px; opacity: 0.7; z-index: 10;
    }
    .legend-dot {
        display: inline-block; width: 14px; height: 10px;
        border-radius: 2px; margin-right: 4px; vertical-align: middle;
    }
    .legend-dot.root { background: var(--vscode-focusBorder, #007acc); }
    .legend-dot.dep {
        background: var(--vscode-editor-background, #1e1e1e);
        border: 1.5px solid var(--vscode-focusBorder, #007acc);
    }
</style>
</head>
<body>
<div class="toolbar">
    <button id="btnFit" title="Fit to view">⊞ Fit</button>
    <button id="btnRefresh" title="Refresh graph">↻ Refresh</button>
    <button id="btnPhysics" title="Toggle physics config">⚙ Physics</button>
</div>
<div class="legend">
    <span><span class="legend-dot root"></span>Current file</span>
    <span><span class="legend-dot dep"></span>Dependency</span>
    <span>→ imports</span>
</div>
<div id="graph"></div>
<physics-panel id="pp"></physics-panel>

<script src="${ppSrc}"></script>
<script src="${visSrc}"></script>
<script>
const vscode = acquireVsCodeApi();
const rawNodes = ${nodesJSON};
const rawEdges = ${edgesJSON};
const savedPhysics = ${settingsJSON};

const bgColor = getComputedStyle(document.documentElement).getPropertyValue('--vscode-editor-background').trim() || '#1e1e1e';
const fgColor = getComputedStyle(document.documentElement).getPropertyValue('--vscode-editor-foreground').trim() || '#d4d4d4';
const accentColor = getComputedStyle(document.documentElement).getPropertyValue('--vscode-focusBorder').trim() || '#007acc';
const borderColor = getComputedStyle(document.documentElement).getPropertyValue('--vscode-editorWidget-border').trim() || '#555';

const nodes = new vis.DataSet(rawNodes.map(n => ({
    id: n.id,
    label: n.label,
    uri: n.uri,
    shape: 'box',
    color: {
        background: n.isRoot ? accentColor : bgColor,
        border: accentColor,
        highlight: { background: n.isRoot ? accentColor : bgColor, border: '#dcdcaa' },
        hover: { background: n.isRoot ? accentColor : bgColor, border: '#dcdcaa' },
    },
    font: { color: n.isRoot ? '#fff' : fgColor, size: 12, face: 'monospace' },
    borderWidth: 1.5,
    borderWidthSelected: 2.5,
})));

const edges = new vis.DataSet(rawEdges.map((e, i) => ({
    id: i, from: e.from, to: e.to,
    arrows: 'to',
    color: { color: borderColor, highlight: '#dcdcaa', hover: '#dcdcaa' },
    width: 1.5,
    smooth: { type: 'cubicBezier', forceDirection: 'vertical', roundness: 0.4 },
})));

const container = document.getElementById('graph');

const defaultPhysics = {
    enabled: true,
    solver: 'barnesHut',
    barnesHut: {
        gravitationalConstant: -4000,
        centralGravity: 0.1,
        springLength: 120,
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

network.on('doubleClick', params => {
    if (params.nodes.length > 0) {
        const n = nodes.get(params.nodes[0]);
        if (n && n.uri) vscode.postMessage({ type: 'openFile', uri: n.uri });
    } else if (params.edges.length > 0) {
        const e = edges.get(params.edges[0]);
        if (e) {
            const n = nodes.get(e.to);
            if (n && n.uri) vscode.postMessage({ type: 'openFile', uri: n.uri });
        }
    }
});

document.getElementById('btnFit').addEventListener('click', () => {
    network.fit({ animation: { duration: 500, easingFunction: 'easeInOutQuad' } });
});

document.getElementById('btnRefresh').addEventListener('click', () => {
    const rootUri = rawNodes.length > 0 ? rawNodes[0].uri : null;
    if (rootUri) vscode.postMessage({ type: 'refresh', uri: rootUri });
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

module.exports = {showImportGraph}

