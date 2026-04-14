// noinspection NpmUsedModulesInstalled
const {Uri, commands, window, ViewColumn, Range} = require('vscode')
const crypto = require('crypto')
const path = require('path')
const {MpqFileSystemProvider} = require('../mpqFileSystemProvider.js')

const {SUPPORTED_BINARIES, findMapRoot, scanMapBinaries} = require('./mapRoot.js')
const {errorHtml} = require('./utils.js')
const {renderMapEditor} = require('./render.js')
const {parseMdxBinary} = require('./parseMdxBinary.js')

/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('../serverClient.js').ServerClient} client
 * @param {import('vscode').Uri} extensionUri
 */
async function resolveMapEditor(document, webviewPanel, _token, client, extensionUri, getBinaryServer) {
    const filePath = document.uri.fsPath
    const fname = document.uri.path.split('/').pop() || 'w3e'
    const ext = (fname.split('.').pop() || '').toLowerCase()
    const isArchive = ext === 'w3x' || ext === 'w3m' || ext === 'w3n' || ext === 'mpq'
    const isMap = ext === 'w3x' || ext === 'w3m'
    const isW3i = ext === 'w3i'
    const isW3e = ext === 'w3e'
    const isDoo = ext === 'doo'
    const isDooUnit = isDoo && fname.toLowerCase().includes('units')
    const isMdx = ext === 'mdx'
    const isBlp = ext === 'blp'

    // Detect whether the archive path points to a real file or an extracted folder.
    let isArchiveFile = false
    if (isArchive) {
        try {
            const fs = require('fs')
            isArchiveFile = fs.statSync(filePath).isFile()
        } catch (_) {}
    }

    let terrainData = null
    let w3iData = null
    let w3rData = null
    let unitDooData = null
    let doodadDooData = null
    let archiveInfo = null
    let mapName = null
    let binaries = []
    let _pendingMdxData = null
    let _pendingBlpData = null

    if (isArchive) {
        // ── 1. Get archive file list & header ───────────────────
        try {
            archiveInfo = await client.sendRequest('mpq/info', {archivePath: filePath})
            if (archiveInfo.error) archiveInfo = null
        } catch (_) {
            archiveInfo = null
        }

        // ── 2. Load terrain from archive ────────────────────────
        try {
            const result = await client.sendRequest('render/w3e', {
                uri: document.uri.toString(),
                archivePath: filePath,
            })
            if (!result.error) terrainData = result
        } catch (_) {
        }

        // ── 3. Load w3i from archive ────────────────────────────
        try {
            const result = await client.sendRequest('render/w3i', {
                uri: document.uri.toString(),
                archivePath: filePath,
            })
            if (!result.error) w3iData = result
            else if (result.format != null) w3iData = result // partial data with _error
        } catch (_) {
        }

        // ── 4. Load unit DOO from archive ─────────────────────────
        try {
            const result = await client.sendRequest('render/doo', {
                uri: document.uri.toString(),
                isUnit: true,
                archivePath: filePath,
            })
            if (!result.error) unitDooData = result
        } catch (_) {
        }

        // ── 5. Load doodad DOO from archive ───────────────────────
        try {
            const result = await client.sendRequest('render/doo', {
                uri: document.uri.toString(),
                isUnit: false,
                archivePath: filePath,
            })
            if (!result.error) doodadDooData = result
        } catch (_) {
        }

        // ── 6. Load w3r (regions) from archive ──────────────────────
        try {
            const result = await client.sendRequest('render/w3r', {
                uri: document.uri.toString(),
                archivePath: filePath,
            })
            if (!result.error) w3rData = result
        } catch (_) {
        }

        mapName = fname
        if (archiveInfo && archiveInfo.files) {
            const archiveFiles = new Set(
                archiveInfo.files.map(f => (typeof f === 'string' ? f : f.name || '').replace(/\\/g, '/'))
            )
            binaries = SUPPORTED_BINARIES.map(entry => ({
                ...entry,
                exists: archiveFiles.has(entry.file) || archiveFiles.has(entry.file.replace(/\//g, '\\'))
            }))
        }
    } else if (isW3i) {
        // ── .w3i file ───────────────────────────────────────────
        const params = {uri: document.uri.toString()}
        if (document._mpqArchivePath) params.archivePath = document._mpqArchivePath

        const result = await client.sendRequest('render/w3i', params)
        if (result.error && result.format == null) {
            webviewPanel.webview.html = errorHtml(result.error.message)
            return
        }
        w3iData = result

        const mapRoot = findMapRoot(filePath)
        mapName = mapRoot ? path.basename(mapRoot) : null
        binaries = mapRoot ? scanMapBinaries(mapRoot) : []

        // Try to load terrain from the same map directory
        if (mapRoot) {
            const fs = require('fs')
            const w3ePath = path.join(mapRoot, 'war3map.w3e')
            if (fs.existsSync(w3ePath)) {
                try {
                    const tResult = await client.sendRequest('render/w3e', {
                        uri: Uri.file(w3ePath).toString(),
                    })
                    if (!tResult.error) terrainData = tResult
                } catch (_) {
                }
            }
        }

        // Try to load DOO from the same map directory
        await _loadDooFromMapRoot(mapRoot, client)
    } else if (isDoo) {
        // ── .doo file ────────────────────────────────────────────
        const params = {uri: document.uri.toString(), isUnit: isDooUnit}
        if (document._mpqArchivePath) params.archivePath = document._mpqArchivePath

        const result = await client.sendRequest('render/doo', params)
        if (result.error) {
            webviewPanel.webview.html = errorHtml(result.error.message)
            return
        }
        if (isDooUnit) unitDooData = result
        else doodadDooData = result

        const mapRoot = findMapRoot(filePath)
        mapName = mapRoot ? path.basename(mapRoot) : null
        binaries = mapRoot ? scanMapBinaries(mapRoot) : []

        // Try to load terrain from the same map directory
        if (mapRoot) {
            const fs = require('fs')
            const w3ePath = path.join(mapRoot, 'war3map.w3e')
            if (fs.existsSync(w3ePath)) {
                try {
                    const tResult = await client.sendRequest('render/w3e', {
                        uri: Uri.file(w3ePath).toString(),
                    })
                    if (!tResult.error) terrainData = tResult
                } catch (_) {
                }
            }

            // Try to load w3i from the same map directory
            const w3iPath = path.join(mapRoot, 'war3map.w3i')
            if (fs.existsSync(w3iPath)) {
                try {
                    const iResult = await client.sendRequest('render/w3i', {
                        uri: Uri.file(w3iPath).toString(),
                    })
                    if (!iResult.error) w3iData = iResult
                    else if (iResult.format != null) w3iData = iResult
                } catch (_) {
                }
            }

            // Load the other DOO file
            await _loadDooFromMapRoot(mapRoot, client)
        }
    } else if (isMdx) {
        // ── .mdx file ───────────────────────────────────────────
        // Render immediately; the model data will be sent to the
        // webview after the HTML is built so the viewer opens it.
        try {
            const buf = await client.sendBinaryRequest('render/mdx', {
                uri: document.uri.toString()
            })
            const renderResult = buf ? parseMdxBinary(buf) : null
            if (renderResult && renderResult.geosets && renderResult.geosets.length > 0) {
                const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
                _pendingMdxData = {
                    command: 'modelData',
                    name: fname,
                    geosets: renderResult.geosets,
                    textures: renderResult.textures || [],
                    materials: renderResult.materials || [],
                    sequences: renderResult.sequences || [],
                    bones: renderResult.bones || [],
                    helpers: renderResult.helpers || [],
                    pivot_points: renderResult.pivot_points || [],
                    total_vertices: renderResult.total_vertices,
                    total_faces: renderResult.total_faces,
                    binaryServer: bs ? {port: bs.port, token: bs.token} : null,
                    archivePath: null,
                    replaceableTextures: null,
                }
            } else {
                webviewPanel.webview.html = errorHtml('No geosets found in model.')
                return
            }
        } catch (e) {
            webviewPanel.webview.html = errorHtml(`Failed to render MDX: ${e.message || e}`)
            return
        }
    } else if (isBlp) {
        // ── .blp file ───────────────────────────────────────────
        // Render immediately; the BLP data will be sent to the
        // webview after the HTML is built so the viewer opens it.
        try {
            const result = await client.sendRequest('render/blp', {
                uri: document.uri.toString()
            })
            if (result.error) {
                webviewPanel.webview.html = errorHtml(result.error.message || JSON.stringify(result.error))
                return
            }
            if (!result.mipmaps || result.mipmaps.length === 0) {
                webviewPanel.webview.html = errorHtml('No mipmaps returned by server.')
                return
            }
            _pendingBlpData = {
                command: 'blpData',
                name: fname,
                mipmaps: result.mipmaps,
            }
        } catch (e) {
            webviewPanel.webview.html = errorHtml(`Failed to render BLP: ${e.message || e}`)
            return
        }
    } else {
        // ── .w3e file ───────────────────────────────────────────
        const params = {uri: document.uri.toString()}
        if (document._mpqArchivePath) params.archivePath = document._mpqArchivePath

        const result = await client.sendRequest('render/w3e', params)
        if (result.error) {
            webviewPanel.webview.html = errorHtml(result.error.message)
            return
        }
        terrainData = result

        const mapRoot = findMapRoot(filePath)
        mapName = mapRoot ? path.basename(mapRoot) : null
        binaries = mapRoot ? scanMapBinaries(mapRoot) : []

        // Try to load w3i from the same map directory
        if (mapRoot) {
            const fs = require('fs')
            const w3iPath = path.join(mapRoot, 'war3map.w3i')
            if (fs.existsSync(w3iPath)) {
                try {
                    const iResult = await client.sendRequest('render/w3i', {
                        uri: Uri.file(w3iPath).toString(),
                    })
                    if (!iResult.error) w3iData = iResult
                    else if (iResult.format != null) w3iData = iResult
                } catch (_) {
                }
            }
        }

        // Try to load DOO from the same map directory
        await _loadDooFromMapRoot(mapRoot, client)
    }

    // Helper: load DOO files from a map root directory
    async function _loadDooFromMapRoot(mapRoot, client) {
        if (!mapRoot) return
        const fs = require('fs')

        if (!unitDooData) {
            const unitPath = path.join(mapRoot, 'war3mapUnits.doo')
            if (fs.existsSync(unitPath)) {
                try {
                    const r = await client.sendRequest('render/doo', {
                        uri: Uri.file(unitPath).toString(),
                        isUnit: true,
                    })
                    if (!r.error) unitDooData = r
                } catch (_) {
                }
            }
        }

        if (!doodadDooData) {
            const doodadPath = path.join(mapRoot, 'war3map.doo')
            if (fs.existsSync(doodadPath)) {
                try {
                    const r = await client.sendRequest('render/doo', {
                        uri: Uri.file(doodadPath).toString(),
                        isUnit: false,
                    })
                    if (!r.error) doodadDooData = r
                } catch (_) {
                }
            }
        }

        if (!w3rData) {
            const w3rPath = path.join(mapRoot, 'war3map.w3r')
            if (fs.existsSync(w3rPath)) {
                try {
                    const r = await client.sendRequest('render/w3r', {
                        uri: Uri.file(w3rPath).toString(),
                    })
                    if (!r.error) w3rData = r
                } catch (_) {
                }
            }
        }
    }

    // ── Three.js URI ────────────────────────────────────────────
    const threeUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'vendor', 'three.min.js')
    )

    // ── Components URIs ─────────────────────────────────────────
    const wvDir = Uri.joinPath(extensionUri, 'extension', 'mapEditor', 'webview')
    const elementsUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'elements.js')
    )
    const canvasListUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'canvas-list.js')
    )
    const utilsUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'utils.js')
    )
    const stateUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'state.js')
    )
    const tilesetUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'tileset.js')
    )
    const doodadsUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'doodads.js')
    )
    const destructablesUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'destructables.js')
    )
    const unitsUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'units.js')
    )
    const placedUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'placed.js')
    )
    const gamePathUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'game-path.js')
    )
    const pathTexUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'path-tex.js')
    )
    const modelViewerUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'model-viewer.js')
    )
    const orbitUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'orbit.js')
    )
    const fpsUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'fps.js')
    )
    const appUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(wvDir, 'app.js')
    )

    // ── Terrain script URI ──────────────────────────────────────
    const terrainUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'mapEditor', 'terrain.js')
    )

    // ── Nonce for CSP ────────────────────────────────────────────
    const nonce = crypto.randomBytes(16).toString('base64')
    const cspSource = webviewPanel.webview.cspSource


    // ── Game path status from server ────────────────────────────
    let gamePathStatus = {gamePath: '', hasPath: false, mpqStatus: null, allPresent: false}
    try {
        const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
        if (bs) {
            const resp = await fetch(`http://127.0.0.1:${bs.port}/w3e/gamePath/status?token=${encodeURIComponent(bs.token)}`)
            if (resp.ok) gamePathStatus = await resp.json()
        }
        if (!gamePathStatus.hasPath) {
                    const buf = await client.http.getBinary('/w3e/gamePath/status')
                    gamePathStatus = JSON.parse(buf.toString('utf8'))
                }
    } catch (_) {
    }

    // ── Archive files for the Files window ───────────────────────
    const archiveFiles = isArchive && archiveInfo ? (archiveInfo.files || []) : null
    const archiveHeader = isArchive && archiveInfo ? (archiveInfo.header || null) : null

    // ── Binary HTTP server info ────────────────────────────────
    const binaryServer = typeof getBinaryServer === 'function' ? getBinaryServer() : null

    webviewPanel.webview.html = renderMapEditor(terrainData, fname, threeUri.toString(), {
        mapName,
        binaries,
        currentFile: fname,
        isArchive,
        isArchiveFile,
        isMap,
        isW3i,
        isW3e,
        isDoo,
        isDooUnit,
        isMdx,
        isBlp,
        archiveFiles,
        archiveHeader,
        w3iData,
        w3rData,
        unitDooData,
        doodadDooData,
        gamePath: gamePathStatus.gamePath,
        mpqStatus: gamePathStatus.mpqStatus,
        nonce,
        cspSource,
        elementsSrc: elementsUri.toString(),
        canvasListSrc: canvasListUri.toString(),
        utilsSrc: utilsUri.toString(),
        stateSrc: stateUri.toString(),
        tilesetSrc: tilesetUri.toString(),
        doodadsSrc: doodadsUri.toString(),
        destructablesSrc: destructablesUri.toString(),
        unitsSrc: unitsUri.toString(),
        placedSrc: placedUri.toString(),
        gamePathSrc: gamePathUri.toString(),
        pathTexSrc: pathTexUri.toString(),
        modelViewerSrc: modelViewerUri.toString(),
        orbitSrc: orbitUri.toString(),
        fpsSrc: fpsUri.toString(),
        appSrc: appUri.toString(),
        terrainSrc: terrainUri.toString(),
        binaryServer,
        terrainUri: document.uri.toString(),
        archivePath: isArchive ? filePath : undefined,
    })


    // If an MDX was opened directly, send the model data to the webview
    // so the model viewer auto-opens with the rendered model.
    if (_pendingMdxData) {
        // Small delay to let the webview scripts initialize
        setTimeout(() => {
            webviewPanel.webview.postMessage(_pendingMdxData)
        }, 300)
    }

    // If a BLP was opened directly, send the image data to the webview
    // so the BLP viewer auto-opens with the rendered image.
    if (_pendingBlpData) {
        setTimeout(() => {
            webviewPanel.webview.postMessage(_pendingBlpData)
        }, 300)
    }

    // ── Snapshot-based data flow ───────────────────────────────────
    // When the game path changes:
    //   1. POST /w3e/gamePath/set → Rust builds snapshot
    //   2. GET  /w3e/snapshot     → full GameSnapshot JSON
    //   3. postMessage('gamePathChanged', {status, snapshot})
    //
    // The webview receives the complete snapshot in one message.

    /**
     * Fetch the snapshot and send it to the webview along with status.
     */
    async function emitGamePathChanged(status) {
        const snapshot = await fetchSnapshot()
        webviewPanel.webview.postMessage({
            command: 'gamePathChanged',
            status,
            snapshot,
        })
    }

    // ── Helper: fetch the full snapshot from the binary HTTP server ──
    async function fetchSnapshot() {
        const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
        if (!bs) return null
        const params = new URLSearchParams({token: bs.token})
        if (isArchive) params.set('archive', filePath)
        const resp = await fetch(`http://127.0.0.1:${bs.port}/w3e/snapshot?${params}`)
        if (!resp.ok) return null
        return await resp.json()
    }

    // ── Helper: set game path via HTTP (POST) ────────────────────
    async function setGamePathViaHttp(gamePath) {
        const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
        if (!bs) return null
        try {
            const resp = await fetch(
                `http://127.0.0.1:${bs.port}/w3e/gamePath/set?token=${encodeURIComponent(bs.token)}`,
                {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify({gamePath}),
                }
            )
            if (resp.ok) return await resp.json()
        } catch (_) {}
        return null
    }

    // ── Push initial snapshot if game path is already set ─────────
    if (gamePathStatus.allPresent) {
        emitGamePathChanged(gamePathStatus).catch(() => {})
    }

    // ── Helper: lookup a game file via HTTP server or WebSocket fallback ──
    // The server handles .mdx/.mdl and .tga/.blp extension fallback internally,
    // so callers just send the path as-is — no extension juggling needed.
    async function lookupGameFile(searchPath, opts) {
        const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
        let buf = null
        let resolvedPath = searchPath

        if (bs) {
            const params = new URLSearchParams({token: bs.token, path: searchPath})
            if (isArchive) params.set('archive', filePath)
            if (opts && opts.tileset) params.set('tileset', opts.tileset)
            try {
                const resp = await fetch(`http://127.0.0.1:${bs.port}/w3e/file?${params}`)
                if (resp.ok) {
                    buf = Buffer.from(await resp.arrayBuffer())
                    resolvedPath = resp.headers.get('x-resolved-path') || searchPath
                }
            } catch (_) {}
        }

        if (!buf) {
            try {
                const result = await client.sendRequest('w3e/lookupFile', {
                    path: searchPath,
                    archivePath: isArchive ? filePath : undefined,
                })
                if (result && result.content) {
                    buf = Buffer.from(result.content, 'base64')
                    resolvedPath = result.resolvedPath || searchPath
                }
            } catch (_) {}
        }

        return buf ? {buf, resolvedPath} : null
    }



    // ── Message handling ────────────────────────────────────────
    webviewPanel.webview.onDidReceiveMessage(async (msg) => {
        if (msg.command === 'openFile' && isArchive) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            const ext = (msg.name.split('.').pop() || '').toLowerCase()
            const viewTypeMap = {
                w3i: 'w3i.preview',
                slk: 'slk.preview',
            }
            const viewType = viewTypeMap[ext]
            if (viewType) {
                await commands.executeCommand('vscode.openWith', uri, viewType, {viewColumn: ViewColumn.Beside})
            } else {
                const opts = {preview: false, viewColumn: ViewColumn.Beside}
                if (typeof msg.line === 'number') {
                    opts.selection = new Range(msg.line, 0, msg.line, 0)
                }
                await commands.executeCommand('vscode.open', uri, opts)
            }
        } else if (msg.command === 'openBlp' && msg.path) {
            // Render a BLP image via the binary HTTP server and send
            // the result to the webview for the embedded BLP viewer.
            try {
                const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
                if (!bs) {
                    window.showWarningMessage('Binary server not ready')
                    return
                }

                const params = new URLSearchParams({token: bs.token, path: msg.path})
                if (isArchive) params.set('archive', filePath)

                const resp = await fetch(`http://127.0.0.1:${bs.port}/blp/render?${params}`)
                if (!resp.ok) {
                    window.showWarningMessage(`BLP not found: ${msg.path}`)
                    return
                }

                const data = await resp.json()
                webviewPanel.webview.postMessage({
                    command: 'blpData',
                    name: data.name || msg.path.replace(/\\/g, '/').split('/').pop() || 'image.blp',
                    mipmaps: data.mipmaps,
                })
            } catch (e) {
                window.showErrorMessage(`Failed to open BLP: ${e.message || e}`)
            }
        } else if (msg.command === 'openModel' && msg.path) {
            // Resolve a game-internal model path via cascading file lookup,
            // render MDX data, and send the result back to the webview for
            // the embedded model viewer float-window.
            try {
                const found = await lookupGameFile(msg.path)

                if (!found) {
                    const missingName = msg.path.replace(/\\/g, '/').split('/').pop() || msg.path
                    webviewPanel.webview.postMessage({
                        command: 'modelUnsupported',
                        name: missingName,
                        reason: 'Model not found: ' + msg.path,
                    })
                    return
                }

                const {buf, resolvedPath} = found
                const resolvedExt = (resolvedPath.split('.').pop() || '').toLowerCase()

                // .mdl format — not supported yet, show notice in viewer
                if (resolvedExt === 'mdl') {
                    const fname = resolvedPath.replace(/\\/g, '/').split('/').pop() || 'model.mdl'
                    webviewPanel.webview.postMessage({
                        command: 'modelUnsupported',
                        name: fname,
                        reason: '.mdl format is temporarily not supported',
                    })
                    return
                }

                const fs = require('fs')
                const os = require('os')
                const fname = resolvedPath.replace(/\\/g, '/').split('/').pop() || 'model.mdx'
                const tmpDir = path.join(os.tmpdir(), `vscode-mdx-${Date.now()}`)
                fs.mkdirSync(tmpDir, {recursive: true})
                const tmpPath = path.join(tmpDir, fname)
                fs.writeFileSync(tmpPath, buf)

                const tmpUri = Uri.file(tmpPath)
                const ext = (fname.split('.').pop() || '').toLowerCase()

                if (ext === 'mdx') {
                    // Render MDX via server and send result to webview
                    try {
                        const mdxBuf = await client.sendBinaryRequest('render/mdx', {
                            uri: tmpUri.toString()
                        })
                        const renderResult = mdxBuf ? parseMdxBinary(mdxBuf) : null

                        if (renderResult && renderResult.geosets && renderResult.geosets.length > 0) {
                            const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
                            let replTex = null
                            if (msg.cliffTex) {
                                replTex = {_cliffTex: msg.cliffTex}
                            } else if (msg.texId && msg.texFile) {
                                replTex = {[msg.texId]: msg.texFile}
                            }
                            webviewPanel.webview.postMessage({
                                command: 'modelData',
                                name: fname,
                                geosets: renderResult.geosets,
                                textures: renderResult.textures || [],
                                materials: renderResult.materials || [],
                                sequences: renderResult.sequences || [],
                                bones: renderResult.bones || [],
                                helpers: renderResult.helpers || [],
                                pivot_points: renderResult.pivot_points || [],
                                total_vertices: renderResult.total_vertices,
                                total_faces: renderResult.total_faces,
                                binaryServer: bs ? {port: bs.port, token: bs.token} : null,
                                archivePath: isArchive ? filePath : null,
                                replaceableTextures: replTex,
                            })
                        } else {
                            window.showWarningMessage(`Failed to render model: ${fname}`)
                        }
                    } catch (renderErr) {
                        window.showWarningMessage(`Failed to render model: ${renderErr.message || renderErr}`)
                    }
                } else {
                    // Non-MDX files: open normally in a separate tab
                    await commands.executeCommand('vscode.open', tmpUri, {preview: false, viewColumn: ViewColumn.Beside})
                }

                // Clean up temp file after a delay
                setTimeout(() => {
                    try { fs.unlinkSync(tmpPath) } catch (_) {}
                    try { fs.rmdirSync(tmpDir) } catch (_) {}
                }, 5000)
            } catch (e) {
                window.showErrorMessage(`Failed to open model: ${e.message || e}`)
            }
        } else if (msg.command === 'browse' && isArchive) {
            await commands.executeCommand('mpq.browse', document.uri)
        } else if (msg.command === 'openSlk' && msg.path) {
            // Resolve an SLK file via cascading file lookup, write to temp,
            // and open in a side tab with the SLK preview.
            try {
                const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
                if (!bs) {
                    window.showWarningMessage('Binary server not ready')
                    return
                }
                const params = new URLSearchParams({token: bs.token, path: msg.path})
                if (isArchive) params.set('archive', filePath)
                const resp = await fetch(`http://127.0.0.1:${bs.port}/w3e/file?${params}`)
                if (!resp.ok) {
                    window.showWarningMessage(`SLK not found: ${msg.path}`)
                    return
                }
                const buf = Buffer.from(await resp.arrayBuffer())
                const fs = require('fs')
                const os = require('os')
                const fname = msg.path.replace(/\\/g, '/').split('/').pop() || 'file.slk'
                const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'jass-slk-'))
                const tmpPath = path.join(tmpDir, fname)
                fs.writeFileSync(tmpPath, buf)
                const tmpUri = Uri.file(tmpPath)
                await commands.executeCommand('vscode.openWith', tmpUri, 'slk.preview', {viewColumn: ViewColumn.Beside})
                setTimeout(() => {
                    try { fs.unlinkSync(tmpPath) } catch (_) {}
                    try { fs.rmdirSync(tmpDir) } catch (_) {}
                }, 5000)
            } catch (e) {
                window.showErrorMessage(`Failed to open SLK: ${e.message || e}`)
            }
        } else if (msg.command === 'extractHere' && isArchive && msg.name) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            await commands.executeCommand('mpq.extractHere', uri)
        } else if (msg.command === 'extractTo' && isArchive && msg.name) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            await commands.executeCommand('mpq.extractTo', uri)
        } else if (msg.command === 'loadMapObjects' && msg.paths) {
            // Bulk-load MDX models for placing doodads/units on the terrain.
            const fs = require('fs')
            const os = require('os')
            const mapTileset = terrainData && terrainData.tileset ? terrainData.tileset : null

            for (const modelPath of msg.paths) {
                try {
                    const found = await lookupGameFile(modelPath, {tileset: mapTileset})

                    if (!found) {
                        webviewPanel.webview.postMessage({
                            command: 'mapObjectModelNotFound',
                            path: modelPath,
                        })
                        continue
                    }

                    const {buf, resolvedPath} = found
                    const resolvedExt = (resolvedPath.split('.').pop() || '').toLowerCase()
                    if (resolvedExt !== 'mdx') {
                        webviewPanel.webview.postMessage({
                            command: 'mapObjectModelNotFound',
                            path: modelPath,
                        })
                        continue
                    }

                    const fname = resolvedPath.replace(/\\/g, '/').split('/').pop() || 'model.mdx'
                    const tmpDir = path.join(os.tmpdir(), `vscode-mdx-map-${Date.now()}`)
                    fs.mkdirSync(tmpDir, {recursive: true})
                    const tmpPath = path.join(tmpDir, fname)
                    fs.writeFileSync(tmpPath, buf)

                    try {
                        const mdxBuf = await client.sendBinaryRequest('render/mdx', {
                            uri: Uri.file(tmpPath).toString()
                        })
                        const renderResult = mdxBuf ? parseMdxBinary(mdxBuf) : null
                        if (renderResult && renderResult.geosets) {
                            webviewPanel.webview.postMessage({
                                command: 'mapObjectModel',
                                path: modelPath,
                                geosets: renderResult.geosets,
                                textures: renderResult.textures || [],
                                materials: renderResult.materials || [],
                            })
                        } else {
                            webviewPanel.webview.postMessage({
                                command: 'mapObjectModelNotFound',
                                path: modelPath,
                            })
                        }
                    } catch (_) {
                        webviewPanel.webview.postMessage({
                            command: 'mapObjectModelNotFound',
                            path: modelPath,
                        })
                    }

                    try { fs.unlinkSync(tmpPath) } catch (_) {}
                    try { fs.rmdirSync(tmpDir) } catch (_) {}
                } catch (_) {
                    webviewPanel.webview.postMessage({
                        command: 'mapObjectModelNotFound',
                        path: modelPath,
                    })
                }
            }

            webviewPanel.webview.postMessage({command: 'mapObjectsLoaded'})
        } else if (msg.command === 'setGamePath') {
            webviewPanel.webview.postMessage({command: 'loadingStart'})
            try {
                const status = await setGamePathViaHttp(msg.value) ||
                    await client.sendRequest('w3e/gamePath/set', {gamePath: msg.value})
                await emitGamePathChanged(status)
            } catch (_) {
            } finally {
                webviewPanel.webview.postMessage({command: 'loadingDone'})
            }
        } else if (msg.command === 'reloadGamePath') {
            webviewPanel.webview.postMessage({command: 'loadingStart'})
            try {
                let status = null
                const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
                if (bs) {
                    const resp = await fetch(`http://127.0.0.1:${bs.port}/w3e/gamePath/status?token=${encodeURIComponent(bs.token)}`)
                    if (resp.ok) status = await resp.json()
                }
                if (!status) {
                    const buf = await client.http.getBinary('/w3e/gamePath/status')
                    status = JSON.parse(buf.toString('utf8'))
                }
                await emitGamePathChanged(status)
            } catch (_) {
            } finally {
                webviewPanel.webview.postMessage({command: 'loadingDone'})
            }
        } else if (msg.command === 'browseGamePath') {
            const uris = await window.showOpenDialog({
                canSelectFiles: false,
                canSelectFolders: true,
                canSelectMany: false,
                openLabel: 'Select Warcraft III Folder',
            })
            if (!uris || uris.length === 0) {
                return
            }
            const selectedPath = uris[0].fsPath
            webviewPanel.webview.postMessage({command: 'loadingStart'})
            try {
                const status = await setGamePathViaHttp(selectedPath) ||
                    await client.sendRequest('w3e/gamePath/set', {gamePath: selectedPath})
                if (status && !status.allPresent) {
                    const missing = Object.entries(status.mpqStatus || {}).filter(([, ok]) => !ok).map(([f]) => f)
                    if (missing.length > 0) {
                        window.showWarningMessage(`Missing MPQ files: ${missing.join(', ')}`)
                    }
                }
                await emitGamePathChanged(status)
            } catch (_) {
            } finally {
                webviewPanel.webview.postMessage({command: 'loadingDone'})
            }
        }
    })
}

module.exports = {resolveMapEditor}

