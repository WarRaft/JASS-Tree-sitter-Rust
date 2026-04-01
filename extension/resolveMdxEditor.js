// noinspection CssUnresolvedCustomProperty

/**
 * Ответ от LSP метода 'mdx/render'.
 *
 * @typedef {Object} MdxGeosetData
 * @property {number} material_id
 * @property {number} vertex_count
 * @property {number} face_count
 * @property {string} vertices  - base64(Float32Array) xyz interleaved
 * @property {string} normals   - base64(Float32Array) xyz interleaved
 * @property {string} faces     - base64(Uint16Array) triangle indices
 * @property {string} uvs       - base64(Float32Array) uv interleaved, V flipped
 * @property {string} normal_lines - base64(Float32Array) pre-computed normal line segments
 */

/**
 * @typedef {Object} MdxRenderResult
 * @property {string} uri
 * @property {number} version
 * @property {string} name
 * @property {number} size
 * @property {MdxGeosetData[]} geosets
 * @property {{name:string}[]} sequences
 * @property {{replaceable_id:number, file_name:string, flags:number}[]} textures
 * @property {{priority_plane:number, flags:number, layers:{filter_mode:number, shading_flags:number, texture_id:number, alpha:number}[]}[]} materials
 * @property {number} total_vertices
 * @property {number} total_faces
 * @property {{message:string}} [error]
 */

/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('vscode-languageclient').LanguageClient} client
 * @param {import('vscode').Uri} extensionUri
 * @param {() => {port: number, token: string} | null} getBinaryServer
 */
async function resolveMdxEditor(document, webviewPanel, _token, client, extensionUri, getBinaryServer) {
    const {Uri} = require('vscode')
    const path = require('path')

    const webview = webviewPanel.webview
    webview.options = {enableScripts: true}

    const vendorDir = Uri.file(path.join(extensionUri.fsPath, 'extension', 'vendor'))
    const extensionDir = Uri.file(path.join(extensionUri.fsPath, 'extension'))
    const threeJsUri = webview.asWebviewUri(Uri.joinPath(vendorDir, 'three.min.js'))
    const mdxViewerUri = webview.asWebviewUri(Uri.joinPath(extensionDir, 'mdxViewer.js'))

    /** @type {MdxRenderResult} */
    let result
    try {
        result = await client.sendRequest('mdx/render', {
            uri: document.uri.toString()
        })
    } catch (e) {
        webviewPanel.webview.html = errorHtml(`Failed to render MDX: ${e}`)
        return
    }

    if (result.error) {
        webviewPanel.webview.html = errorHtml(result.error.message || JSON.stringify(result.error))
        return
    }

    if (!result.geosets || result.geosets.length === 0) {
        webviewPanel.webview.html = errorHtml('No geosets found in MDX file.')
        return
    }

    const fname = document.uri.path.split('/').pop() || 'model.mdx'
    const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
    const cspSource = webview.cspSource

    // Build archive path for texture lookup (MPQ sources)
    const archivePath = document._mpqArchivePath || null

    webviewPanel.webview.html = renderMdxViewer(result, fname, threeJsUri.toString(), mdxViewerUri.toString(), bs, archivePath, cspSource)
}

function renderMdxViewer(result, fname, threeJsUrl, mdxViewerUrl, binaryServer, archivePath, cspSource) {
    const bsPort = binaryServer ? binaryServer.port : 0
    const connectSrc = binaryServer ? `connect-src http://127.0.0.1:${bsPort};` : ''
    const imgSrc = binaryServer
        ? `img-src ${cspSource} data: blob: http://127.0.0.1:${bsPort};`
        : `img-src ${cspSource} data: blob:;`
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src ${cspSource} 'unsafe-inline'; style-src 'unsafe-inline'; ${connectSrc} ${imgSrc}"/>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        html, body { width: 100%; height: 100%; overflow: hidden; }
        body {
            background: var(--vscode-editor-background, #1e1e1e);
            color: var(--vscode-editor-foreground, #ccc);
            font-family: var(--vscode-font-family, monospace), sans-serif;
            font-size: 13px;
            display: flex;
            flex-direction: column;
        }
        #toolbar {
            display: flex;
            align-items: center;
            gap: 0.75rem;
            padding: 0.4rem 0.75rem;
            background: var(--vscode-editorWidget-background, #252526);
            border-bottom: 1px solid var(--vscode-editorWidget-border, #454545);
            flex-shrink: 0;
            flex-wrap: wrap;
        }
        #toolbar label {
            display: inline-flex;
            align-items: center;
            gap: 0.3rem;
            cursor: pointer;
            user-select: none;
        }
        #toolbar .info {
            color: var(--vscode-descriptionForeground, #888);
            margin-left: auto;
        }
        #canvas-container {
            flex: 1;
            position: relative;
            overflow: hidden;
        }
        canvas { display: block; width: 100%; height: 100%; }
        #error-overlay {
            display: none;
            position: absolute;
            top: 50%; left: 50%;
            transform: translate(-50%, -50%);
            background: var(--vscode-editorWidget-background, #252526);
            border: 1px solid var(--vscode-editorWidget-border, #454545);
            border-radius: 6px;
            padding: 1.5rem 2rem;
            max-width: 80%;
            color: var(--vscode-errorForeground, #f44);
        }
        select, button {
            background: var(--vscode-dropdown-background, #3c3c3c);
            color: var(--vscode-dropdown-foreground, #ccc);
            border: 1px solid var(--vscode-dropdown-border, #454545);
            padding: 2px 6px;
            border-radius: 3px;
            font-size: 12px;
            cursor: pointer;
        }
        button:hover {
            background: var(--vscode-button-hoverBackground, #505050);
        }
        .tex-item {
            margin-bottom: 0.75rem;
            border: 1px solid var(--vscode-editorWidget-border, #454545);
            border-radius: 4px;
            overflow: hidden;
        }
        .tex-item .tex-info {
            padding: 0.35rem 0.5rem;
            font-size: 11px;
            background: var(--vscode-editor-background, #1e1e1e);
            color: var(--vscode-descriptionForeground, #888);
            word-break: break-all;
        }
        .tex-item .tex-info strong {
            color: var(--vscode-editor-foreground, #ccc);
        }
        .tex-item img {
            display: block;
            width: 100%;
            image-rendering: pixelated;
            background: repeating-conic-gradient(#333 0% 25%, #444 0% 50%) 50% / 16px 16px;
        }
        .tex-item .tex-placeholder {
            display: flex;
            align-items: center;
            justify-content: center;
            height: 80px;
            color: var(--vscode-descriptionForeground, #666);
            font-style: italic;
            font-size: 12px;
        }
        .tex-item .tex-loading {
            display: flex;
            align-items: center;
            justify-content: center;
            height: 80px;
            color: var(--vscode-descriptionForeground, #888);
            font-size: 12px;
        }
        /* ── Side panel (shared) ──────────────────────────── */
        .side-panel {
            position: absolute;
            top: 8px;
            width: 340px;
            max-height: calc(100% - 16px);
            background: var(--vscode-editorWidget-background, #252526);
            border: 1px solid var(--vscode-editorWidget-border, #454545);
            border-radius: 6px;
            box-shadow: 0 4px 16px rgba(0,0,0,0.4);
            z-index: 100;
            display: none;
            flex-direction: column;
            overflow: hidden;
        }
        .side-panel.open { display: flex; }
        .side-panel .sp-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 0.5rem 0.75rem;
            border-bottom: 1px solid var(--vscode-editorWidget-border, #454545);
            font-weight: bold;
            font-size: 13px;
            flex-shrink: 0;
        }
        .side-panel .sp-close {
            background: none;
            border: none;
            color: var(--vscode-editor-foreground, #ccc);
            font-size: 16px;
            cursor: pointer;
            padding: 0 4px;
            line-height: 1;
        }
        .side-panel .sp-close:hover { color: #fff; }
        .side-panel .sp-body {
            overflow-y: auto;
            padding: 0.5rem;
            flex: 1;
        }
        #textures-panel { right: 8px; }
        #materials-panel { right: 356px; }
        /* ── Materials panel ──────────────────────────────── */
        .mat-item {
            margin-bottom: 0.75rem;
            border: 1px solid var(--vscode-editorWidget-border, #454545);
            border-radius: 4px;
            overflow: hidden;
        }
        .mat-item .mat-header {
            padding: 0.35rem 0.5rem;
            font-size: 11px;
            background: var(--vscode-editor-background, #1e1e1e);
            color: var(--vscode-editor-foreground, #ccc);
            font-weight: bold;
        }
        .mat-layer {
            padding: 0.3rem 0.5rem;
            font-size: 11px;
            color: var(--vscode-descriptionForeground, #888);
            border-top: 1px solid var(--vscode-editorWidget-border, #353535);
        }
        .mat-layer .ml-row {
            display: flex;
            justify-content: space-between;
            margin-bottom: 1px;
        }
        .mat-layer .ml-label {
            color: var(--vscode-descriptionForeground, #888);
        }
        .mat-layer .ml-value {
            color: var(--vscode-editor-foreground, #ccc);
        }
        .mat-layer .ml-tex-thumb {
            width: 48px;
            height: 48px;
            image-rendering: pixelated;
            border: 1px solid var(--vscode-editorWidget-border, #454545);
            border-radius: 3px;
            background: repeating-conic-gradient(#333 0% 25%, #444 0% 50%) 50% / 8px 8px;
            margin-top: 4px;
        }
    </style>
</head>
<body>
    <div id="toolbar">
        <strong>🎮 ${escapeHtml(fname)}</strong>
        <label><input type="checkbox" id="wireframe-toggle" /> Wireframe</label>
        <label><input type="checkbox" id="normals-toggle" /> Normals</label>
        <label><input type="checkbox" id="axes-toggle" checked /> Axes</label>
        <label><input type="checkbox" id="grid-toggle" checked /> Grid</label>
        <label><input type="checkbox" id="textured-toggle" checked /> Textured</label>
        <button id="reset-camera">Reset Camera</button>
        <button id="textures-btn">🖼 Textures</button>
        <button id="materials-btn">🧱 Materials</button>
        <select id="geoset-select"><option value="all">All Geosets</option></select>
        <span class="info" id="model-info"></span>
    </div>
    <div id="canvas-container">
        <canvas id="viewport"></canvas>
        <div id="error-overlay"></div>
        <div id="textures-panel" class="side-panel">
            <div class="sp-header">
                <span>🖼 Textures</span>
                <button class="sp-close" id="textures-close">&times;</button>
            </div>
            <div class="sp-body" id="textures-body"></div>
        </div>
        <div id="materials-panel" class="side-panel">
            <div class="sp-header">
                <span>🧱 Materials</span>
                <button class="sp-close" id="materials-close">&times;</button>
            </div>
            <div class="sp-body" id="materials-body"></div>
        </div>
    </div>

    <script src="${threeJsUrl}"></script>
    <script>
    window.MDX_INIT = {
        MODEL: ${JSON.stringify({
            version: result.version,
            name: result.name,
            size: result.size,
            totalVertices: result.total_vertices,
            totalFaces: result.total_faces,
            geosetCount: result.geosets.length,
        })},
        GEOSETS_META: ${JSON.stringify(result.geosets.map(g => ({
            materialId: g.material_id,
            vertexCount: g.vertex_count,
            faceCount: g.face_count,
        })))},
        GEOSETS_B64: ${JSON.stringify(result.geosets.map(g => ({
            vertices: g.vertices,
            normals: g.normals,
            faces: g.faces,
            uvs: g.uvs,
            normalLines: g.normal_lines,
        })))},
        MATERIALS: ${JSON.stringify((result.materials || []).map(m => ({
            priorityPlane: m.priority_plane,
            flags: m.flags,
            layers: (m.layers || []).map(l => ({
                filterMode: l.filter_mode,
                shadingFlags: l.shading_flags,
                textureId: l.texture_id,
                alpha: l.alpha,
            })),
        })))},
        TEXTURES: ${JSON.stringify((result.textures || []).map(t => ({
            replaceableId: t.replaceable_id,
            fileName: t.file_name,
            flags: t.flags,
        })))},
        BINARY_SERVER: ${JSON.stringify(binaryServer)},
        ARCHIVE_PATH: ${JSON.stringify(archivePath)},
    };
    </script>
    <script src="${mdxViewerUrl}"></script>
</body>
</html>`
}

function escapeHtml(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

function errorHtml(msg) {
    const s = escapeHtml(msg)
    return `<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"/></head>
<body style="background:var(--vscode-editor-background);color:var(--vscode-errorForeground);font-family:var(--vscode-font-family),sans-serif;padding:2rem;">
<h2>⚠ Error</h2><pre>${s}</pre>
</body></html>`
}

module.exports = {
    resolveMdxEditor
}

