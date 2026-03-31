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
async function resolveW3eEditor(document, webviewPanel, _token, client, extensionUri) {
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
        gamePathStatus = await client.sendRequest('w3e/gamePath/status', {})
    } catch (_) {
    }

    // ── Archive files for the Files window ───────────────────────
    const archiveFiles = isArchive && archiveInfo ? (archiveInfo.files || []) : null
    const archiveHeader = isArchive && archiveInfo ? (archiveInfo.header || null) : null

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

    // ── Listener: terrain SLK ───────────────────────────────────
    onGamePathChanged(async (payload) => {
        try {
            payload.terrainSlk = await client.sendRequest('w3e/terrainSlk', {
                archivePath: isArchive ? filePath : undefined,
            })
        } catch (_) {
            payload.terrainSlk = null
        }
    })

    // ── Listener: doodads SLK ───────────────────────────────────
    onGamePathChanged(async (payload) => {
        try {
            payload.doodadsSlk = await client.sendRequest('w3e/doodadsSlk', {
                archivePath: isArchive ? filePath : undefined,
            })
        } catch (_) {
            payload.doodadsSlk = null
        }
    })

    // ── Listener: units SLK ─────────────────────────────────────
    onGamePathChanged(async (payload) => {
        try {
            payload.unitsSlk = await client.sendRequest('w3e/unitsSlk', {
                archivePath: isArchive ? filePath : undefined,
            })
        } catch (_) {
            payload.unitsSlk = null
        }
    })

    // ── Message handling ────────────────────────────────────────
    webviewPanel.webview.onDidReceiveMessage(async (msg) => {
        if (msg.command === 'openFile' && isArchive) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            await commands.executeCommand('vscode.open', uri, {preview: false, viewColumn: ViewColumn.Beside})
        } else if (msg.command === 'browse' && isArchive) {
            await commands.executeCommand('mpq.browse', document.uri)
        } else if (msg.command === 'extractHere' && isArchive && msg.name) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            await commands.executeCommand('mpq.extractHere', uri)
        } else if (msg.command === 'extractTo' && isArchive && msg.name) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            await commands.executeCommand('mpq.extractTo', uri)
        } else if (msg.command === 'setGamePath') {
            try {
                const status = await client.sendRequest('w3e/gamePath/set', {gamePath: msg.value})
                await emitGamePathChanged(status)
            } catch (_) {
            }
        } else if (msg.command === 'browseGamePath') {
            const uris = await window.showOpenDialog({
                canSelectFiles: false,
                canSelectFolders: true,
                canSelectMany: false,
                openLabel: 'Select Warcraft III Folder',
            })
            if (!uris || uris.length === 0) return
            const selectedPath = uris[0].fsPath
            try {
                const status = await client.sendRequest('w3e/gamePath/set', {gamePath: selectedPath})
                if (!status.allPresent) {
                    const missing = Object.entries(status.mpqStatus || {}).filter(([, ok]) => !ok).map(([f]) => f)
                    await window.showWarningMessage(
                        `Missing MPQ files: ${missing.join(', ')}`,
                        {modal: false}
                    )
                }
                await emitGamePathChanged(status)
            } catch (_) {
            }
        }
    })
}

module.exports = {resolveW3eEditor}

