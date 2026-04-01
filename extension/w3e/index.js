// noinspection NpmUsedModulesInstalled
const {Uri, commands, window, ViewColumn} = require('vscode')
const crypto = require('crypto')
const path = require('path')
const {MpqFileSystemProvider} = require('../mpqFileSystemProvider.js')

const {SUPPORTED_BINARIES, findMapRoot, scanMapBinaries} = require('./mapRoot.js')
const {errorHtml} = require('./utils.js')
const {renderMapEditor} = require('./render.js')

/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('vscode-languageclient').LanguageClient} client
 * @param {import('vscode').Uri} extensionUri
 */
async function resolveW3eEditor(document, webviewPanel, _token, client, extensionUri, getBinaryServer) {
    const filePath = document.uri.fsPath
    const fname = document.uri.path.split('/').pop() || 'w3e'
    const ext = (fname.split('.').pop() || '').toLowerCase()
    const isArchive = ext === 'w3x' || ext === 'w3m' || ext === 'w3n' || ext === 'mpq'
    const isMap = ext === 'w3x' || ext === 'w3m'
    const isW3i = ext === 'w3i'
    const isW3e = ext === 'w3e'
    const isDoo = ext === 'doo'
    const isDooUnit = isDoo && fname.toLowerCase().includes('units')

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
    let unitDooData = null
    let doodadDooData = null
    let archiveInfo = null
    let mapName = null
    let binaries = []

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
            const result = await client.sendRequest('w3e/render', {
                uri: document.uri.toString(),
                archivePath: filePath,
            })
            if (!result.error) terrainData = result
        } catch (_) {
        }

        // ── 3. Load w3i from archive ────────────────────────────
        try {
            const result = await client.sendRequest('w3i/render', {
                uri: document.uri.toString(),
                archivePath: filePath,
            })
            if (!result.error) w3iData = result
            else if (result.format != null) w3iData = result // partial data with _error
        } catch (_) {
        }

        // ── 4. Load unit DOO from archive ─────────────────────────
        try {
            const result = await client.sendRequest('doo/render', {
                uri: document.uri.toString(),
                isUnit: true,
                archivePath: filePath,
            })
            if (!result.error) unitDooData = result
        } catch (_) {
        }

        // ── 5. Load doodad DOO from archive ───────────────────────
        try {
            const result = await client.sendRequest('doo/render', {
                uri: document.uri.toString(),
                isUnit: false,
                archivePath: filePath,
            })
            if (!result.error) doodadDooData = result
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

        const result = await client.sendRequest('w3i/render', params)
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
                    const tResult = await client.sendRequest('w3e/render', {
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

        const result = await client.sendRequest('doo/render', params)
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
                    const tResult = await client.sendRequest('w3e/render', {
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
                    const iResult = await client.sendRequest('w3i/render', {
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
    } else {
        // ── .w3e file ───────────────────────────────────────────
        const params = {uri: document.uri.toString()}
        if (document._mpqArchivePath) params.archivePath = document._mpqArchivePath

        const result = await client.sendRequest('w3e/render', params)
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
                    const iResult = await client.sendRequest('w3i/render', {
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
                    const r = await client.sendRequest('doo/render', {
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
                    const r = await client.sendRequest('doo/render', {
                        uri: Uri.file(doodadPath).toString(),
                        isUnit: false,
                    })
                    if (!r.error) doodadDooData = r
                } catch (_) {
                }
            }
        }
    }

    // ── Three.js URI ────────────────────────────────────────────
    const threeUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'vendor', 'three.min.js')
    )

    // ── Components URI ───────────────────────────────────────────
    const componentsUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'w3e', 'webview-components.js')
    )

    // ── Terrain script URI ──────────────────────────────────────
    const terrainUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'w3e', 'terrain.js')
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
            gamePathStatus = await client.sendRequest('w3e/gamePath/status', {})
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
        archiveFiles,
        archiveHeader,
        w3iData,
        unitDooData,
        doodadDooData,
        gamePath: gamePathStatus.gamePath,
        mpqStatus: gamePathStatus.mpqStatus,
        nonce,
        cspSource,
        componentsSrc: componentsUri.toString(),
        terrainSrc: terrainUri.toString(),
        binaryServer,
        terrainUri: document.uri.toString(),
        archivePath: isArchive ? filePath : undefined,
    })

    // ── Game-path event dispatcher ──────────────────────────────
    // All logic that depends on the game directory subscribes here.
    // When the path changes we collect the data once and broadcast
    // a single `gamePathChanged` message to the webview.

    const gamePathListeners = []

    function onGamePathChanged(fn) { gamePathListeners.push(fn) }

    async function emitGamePathChanged(status) {
        /** @type {Record<string,any>} */
        const payload = {status}

        // Let every listener enrich the payload with its data.
        for (const fn of gamePathListeners) {
            try { await fn(payload) } catch (_) {}
        }

        webviewPanel.webview.postMessage({command: 'gamePathChanged', ...payload})
    }

    // ── Helper: fetch JSON from the binary HTTP server ─────────
    async function fetchSlk(endpoint) {
        const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
        if (!bs) return null
        const params = new URLSearchParams({token: bs.token})
        if (isArchive) params.set('archive', filePath)
        const resp = await fetch(`http://127.0.0.1:${bs.port}/w3e/${endpoint}?${params}`)
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

    // ── Listener: terrain SLK ───────────────────────────────────
    onGamePathChanged(async (payload) => {
        try {
            payload.terrainSlk = await fetchSlk('terrainSlk')
            if (payload.terrainSlk == null) {
                payload.terrainSlk = await client.sendRequest('w3e/terrainSlk', {
                    archivePath: isArchive ? filePath : undefined,
                })
            }
        } catch (_) {
            payload.terrainSlk = null
        }
    })

    // ── Listener: doodads SLK ───────────────────────────────────
    onGamePathChanged(async (payload) => {
        try {
            payload.doodadsSlk = await fetchSlk('doodadsSlk')
            if (payload.doodadsSlk == null) {
                payload.doodadsSlk = await client.sendRequest('w3e/doodadsSlk', {
                    archivePath: isArchive ? filePath : undefined,
                })
            }
        } catch (_) {
            payload.doodadsSlk = null
        }
    })

    // ── Listener: units SLK ─────────────────────────────────────
    onGamePathChanged(async (payload) => {
        try {
            payload.unitsSlk = await fetchSlk('unitsSlk')
            if (payload.unitsSlk == null) {
                payload.unitsSlk = await client.sendRequest('w3e/unitsSlk', {
                    archivePath: isArchive ? filePath : undefined,
                })
            }
        } catch (_) {
            payload.unitsSlk = null
        }
    })


    // ── Message handling ────────────────────────────────────────
    webviewPanel.webview.onDidReceiveMessage(async (msg) => {
        if (msg.command === 'openFile' && isArchive) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            const ext = (msg.name.split('.').pop() || '').toLowerCase()
            const viewTypeMap = {
                mdx: 'mdx.preview',
                blp: 'blp.preview',
                doo: 'doo.preview',
                w3i: 'w3i.preview',
                slk: 'slk.preview',
            }
            const viewType = viewTypeMap[ext]
            if (viewType) {
                await commands.executeCommand('vscode.openWith', uri, viewType, {viewColumn: ViewColumn.Beside})
            } else {
                await commands.executeCommand('vscode.open', uri, {preview: false, viewColumn: ViewColumn.Beside})
            }
        } else if (msg.command === 'openModel' && msg.path) {
            // Resolve a game-internal model path via cascading file lookup,
            // render MDX data, and send the result back to the webview for
            // the embedded model viewer float-window.
            try {
                const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
                const hasExt = /\.\w+$/.test(msg.path)

                // Build a list of paths to try
                const pathsToTry = hasExt
                    ? [msg.path]
                    : [msg.path + '.mdx', msg.path + '.mdl']

                let buf = null
                let resolvedPath = msg.path

                for (const tryPath of pathsToTry) {
                    // Try HTTP server first
                    if (bs && !buf) {
                        const params = new URLSearchParams({
                            token: bs.token,
                            path: tryPath,
                        })
                        if (isArchive) params.set('archive', filePath)
                        try {
                            const resp = await fetch(`http://127.0.0.1:${bs.port}/w3e/file?${params}`)
                            if (resp.ok) {
                                buf = Buffer.from(await resp.arrayBuffer())
                                const rp = resp.headers.get('x-resolved-path')
                                resolvedPath = rp || tryPath
                            }
                        } catch (_) {}
                    }

                    // Fallback to LSP
                    if (!buf) {
                        try {
                            const result = await client.sendRequest('w3e/lookupFile', {
                                path: tryPath,
                                archivePath: isArchive ? filePath : undefined,
                            })
                            if (result && result.content) {
                                buf = Buffer.from(result.content, 'base64')
                                resolvedPath = result.resolvedPath || tryPath
                            }
                        } catch (_) {}
                    }

                    if (buf) break
                }

                if (!buf) {
                    window.showWarningMessage(`Model not found: ${msg.path}`)
                    return
                }

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
                    // Render MDX via LSP and send result to webview
                    try {
                        const renderResult = await client.sendRequest('mdx/render', {
                            uri: tmpUri.toString()
                        })

                        if (renderResult && !renderResult.error && renderResult.geosets && renderResult.geosets.length > 0) {
                            const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
                            webviewPanel.webview.postMessage({
                                command: 'modelData',
                                name: fname,
                                geosets: renderResult.geosets,
                                textures: renderResult.textures || [],
                                materials: renderResult.materials || [],
                                sequences: renderResult.sequences || [],
                                total_vertices: renderResult.total_vertices,
                                total_faces: renderResult.total_faces,
                                binaryServer: bs ? {port: bs.port, token: bs.token} : null,
                                archivePath: isArchive ? filePath : null,
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
        } else if (msg.command === 'extractHere' && isArchive && msg.name) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            await commands.executeCommand('mpq.extractHere', uri)
        } else if (msg.command === 'extractTo' && isArchive && msg.name) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            await commands.executeCommand('mpq.extractTo', uri)
        } else if (msg.command === 'loadMapObjects' && msg.paths) {
            // Bulk-load MDX models for placing doodads/units on the terrain.
            const bs = typeof getBinaryServer === 'function' ? getBinaryServer() : null
            const fs = require('fs')
            const os = require('os')

            for (const modelPath of msg.paths) {
                try {
                    const hasExt = /\.\w+$/.test(modelPath)
                    const pathsToTry = hasExt
                        ? [modelPath]
                        : [modelPath + '.mdx', modelPath + '.mdl']

                    let buf = null
                    let resolvedPath = modelPath

                    for (const tryPath of pathsToTry) {
                        if (bs && !buf) {
                            const params = new URLSearchParams({token: bs.token, path: tryPath})
                            if (isArchive) params.set('archive', filePath)
                            try {
                                const resp = await fetch(`http://127.0.0.1:${bs.port}/w3e/file?${params}`)
                                if (resp.ok) {
                                    buf = Buffer.from(await resp.arrayBuffer())
                                    resolvedPath = resp.headers.get('x-resolved-path') || tryPath
                                }
                            } catch (_) {}
                        }
                        if (!buf) {
                            try {
                                const result = await client.sendRequest('w3e/lookupFile', {
                                    path: tryPath,
                                    archivePath: isArchive ? filePath : undefined,
                                })
                                if (result && result.content) {
                                    buf = Buffer.from(result.content, 'base64')
                                    resolvedPath = result.resolvedPath || tryPath
                                }
                            } catch (_) {}
                        }
                        if (buf) break
                    }

                    if (!buf) {
                        webviewPanel.webview.postMessage({
                            command: 'mapObjectModelNotFound',
                            path: modelPath,
                        })
                        continue
                    }
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
                        const renderResult = await client.sendRequest('mdx/render', {
                            uri: Uri.file(tmpPath).toString()
                        })
                        if (renderResult && !renderResult.error && renderResult.geosets) {
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
                    status = await client.sendRequest('w3e/gamePath/status', {})
                }
                await emitGamePathChanged(status)
            } catch (_) {
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
                if (!status.allPresent) {
                    const missing = Object.entries(status.mpqStatus || {}).filter(([, ok]) => !ok).map(([f]) => f)
                    await window.showWarningMessage(
                        `Missing MPQ files: ${missing.join(', ')}`,
                        {modal: false}
                    )
                }
                await emitGamePathChanged(status)
            } catch (_) {
                webviewPanel.webview.postMessage({command: 'loadingDone'})
            }
        }
    })
}

module.exports = {resolveW3eEditor}

