// noinspection NpmUsedModulesInstalled
const {Uri, commands, workspace, window} = require('vscode')
const crypto = require('crypto')
const fs = require('fs')
const path = require('path')
const {MpqFileSystemProvider} = require('../mpqFileSystemProvider.js')

const {SUPPORTED_BINARIES, findMapRoot, scanMapBinaries} = require('./mapRoot.js')
const {errorHtml} = require('./utils.js')
const {renderMapEditor} = require('./render.js')
const {validateGamePath, allMpqPresent} = require('./panels.js')

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

    let terrainData = null
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
    }

    // ── Three.js URI ────────────────────────────────────────────
    const threeUri = webviewPanel.webview.asWebviewUri(
        Uri.joinPath(extensionUri, 'extension', 'vendor', 'three.min.js')
    )

    // ── Nonce for CSP ────────────────────────────────────────────
    const nonce = crypto.randomBytes(16).toString('base64')
    const cspSource = webviewPanel.webview.cspSource

    // ── Textures ────────────────────────────────────────────────
    const texDir = path.join(extensionUri.fsPath, 'extension', 'assets', 'TexturePack')
    let texUris = []
    try {
        texUris = fs.readdirSync(texDir)
            .filter(f => f.toLowerCase().endsWith('.jpg'))
            .map(f => webviewPanel.webview.asWebviewUri(
                Uri.joinPath(extensionUri, 'extension', 'assets', 'TexturePack', f)
            ).toString())
    } catch (_) {
    }

    // ── Game path setting ───────────────────────────────────────
    const gamePath = workspace.getConfiguration('jassTerrainEditor').get('gamePath', '')
    const mpqStatus = gamePath ? validateGamePath(gamePath) : null

    // ── Archive files for the Files window ───────────────────────
    const archiveFiles = isArchive && archiveInfo ? (archiveInfo.files || []) : null
    const archiveHeader = isArchive && archiveInfo ? (archiveInfo.header || null) : null

    webviewPanel.webview.html = renderMapEditor(terrainData, fname, threeUri.toString(), texUris, {
        mapName,
        binaries,
        currentFile: fname,
        isArchive,
        isMap,
        archiveFiles,
        archiveHeader,
        gamePath,
        mpqStatus,
        nonce,
        cspSource,
    })

    // ── Message handling ────────────────────────────────────────
    webviewPanel.webview.onDidReceiveMessage(async (msg) => {
        if (msg.command === 'openFile' && isArchive) {
            const uri = MpqFileSystemProvider.makeUri(filePath, msg.name)
            await commands.executeCommand('vscode.open', uri)
        } else if (msg.command === 'browse' && isArchive) {
            await commands.executeCommand('mpq.browse', document.uri)
        } else if (msg.command === 'setGamePath') {
            await workspace.getConfiguration('jassTerrainEditor').update('gamePath', msg.value, true)
        } else if (msg.command === 'browseGamePath') {
            const uris = await window.showOpenDialog({
                canSelectFiles: false,
                canSelectFolders: true,
                canSelectMany: false,
                openLabel: 'Select Warcraft III Folder',
            })
            if (!uris || uris.length === 0) return
            const selectedPath = uris[0].fsPath
            const status = validateGamePath(selectedPath)
            if (!allMpqPresent(status)) {
                const missing = Object.entries(status).filter(([, ok]) => !ok).map(([f]) => f)
                await window.showWarningMessage(
                    `Missing MPQ files: ${missing.join(', ')}`,
                    {modal: false}
                )
                return
            }
            await workspace.getConfiguration('jassTerrainEditor').update('gamePath', selectedPath, true)
        }
    })
}

module.exports = {resolveW3eEditor}

