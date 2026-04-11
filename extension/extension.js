// noinspection JSUnusedGlobalSymbols
// noinspection NpmUsedModulesInstalled
const {
    window,
    Uri, commands, ProgressLocation, workspace,
    languages, InlayHint, InlayHintKind, Position, Range, EventEmitter,
    ShellExecution, Task, TaskScope, tasks,
    Diagnostic: VscDiagnostic,
    DocumentSymbol: VscDocumentSymbol,
    FoldingRange, FoldingRangeKind,
    SemanticTokensLegend,
    DocumentLink: VscDocumentLink,
    ColorInformation, Color, ColorPresentation: VscColorPresentation, TextEdit,
    CodeLens: VscCodeLens,
    WorkspaceEdit: VscWorkspaceEdit,
    CompletionItem: VscCompletionItem,
    CompletionList: VscCompletionList,
    Hover: VscHover,
    MarkdownString,
    Location: VscLocation,
    DocumentHighlight: VscDocumentHighlight,
    DocumentHighlightKind: VscDocumentHighlightKind,
    SignatureHelp: VscSignatureHelp,
    SignatureInformation: VscSignatureInformation,
    ParameterInformation: VscParameterInformation,
    CodeAction: VscCodeAction,
    CodeActionKind: VscCodeActionKind,
    CallHierarchyItem: VscCallHierarchyItem,
    CallHierarchyIncomingCall: VscCallHierarchyIncomingCall,
    CallHierarchyOutgoingCall: VscCallHierarchyOutgoingCall,
    TypeHierarchyItem: VscTypeHierarchyItem,
    SymbolKind: VscSymbolKind,
    FileDecoration: VscFileDecoration,
    ThemeColor,
    DiagnosticSeverity: VscDiagnosticSeverity,
} = require('vscode')

const {ServerClient} = require('./serverClient.js')
const {resolveBlpEditor} = require('./mapEditor/resolveBlpEditor.js')
const {resolveMapEditor} = require('./mapEditor/index.js')
const {showImportGraph} = require('./importGraphPanel.js')
const {showCallGraph} = require('./callGraphPanel.js')
const {showTypeGraph} = require('./typeGraphPanel.js')
const {showExports} = require('./exportPanel.js')
const {showDiagnostics} = require('./diagnosticPanel.js')

const {MpqFileSystemProvider} = require('./mpqFileSystemProvider.js')
const {resolveSlkEditor} = require('./resolveSlkEditor.js')

const path = require('path')
const fs = require('fs')

/**
 * Extract a file or directory from an MPQ archive to the local filesystem,
 * preserving internal directory structure.
 *
 * @param {import('vscode').Uri} resourceUri  the mpq:// URI of the item
 * @param {string} archivePath               absolute fs path to the archive
 * @param {string} internalPath              path inside the archive (e.g. "A/B/C.jpg")
 * @param {string} baseDir                   local directory to extract into
 * @param {MpqFileSystemProvider} mpqProvider the filesystem provider instance
 */
async function _doExtract(resourceUri, archivePath, internalPath, baseDir, mpqProvider) {
    // Determine whether this is a file or a directory.
    let stat
    try {
        stat = await mpqProvider.stat(resourceUri)
    } catch {
        window.showErrorMessage(`Cannot stat: ${internalPath}`)
        return
    }

    /** @type {{internal: string, uri: import('vscode').Uri}[]} */
    const filesToExtract = []

    // Check: FileType.File = 1, FileType.Directory = 2
    if (stat.type === 1) {
        filesToExtract.push({internal: internalPath, uri: resourceUri})
    } else {
        await _collectFiles(mpqProvider, archivePath, internalPath, filesToExtract)
    }

    if (filesToExtract.length === 0) {
        window.showWarningMessage('No files to extract.')
        return
    }

    await window.withProgress(
        {
            location: ProgressLocation.Notification,
            title: `Extracting ${filesToExtract.length} file(s)…`,
            cancellable: true,
        },
        async (progress, token) => {
            let extracted = 0
            for (const item of filesToExtract) {
                if (token.isCancellationRequested) break

                const destPath = path.join(baseDir, item.internal.replace(/\//g, path.sep))
                const destDir = path.dirname(destPath)

                // Create parent directories
                fs.mkdirSync(destDir, {recursive: true})

                try {
                    const content = await mpqProvider.readFile(item.uri)
                    fs.writeFileSync(destPath, Buffer.from(content))
                } catch (e) {
                    window.showWarningMessage(`Failed to extract ${item.internal}: ${e.message || e}`)
                }

                extracted++
                progress.report({
                    increment: 100 / filesToExtract.length,
                    message: `${extracted}/${filesToExtract.length}`,
                })
            }
            window.showInformationMessage(`Extracted ${extracted} file(s) to ${baseDir}`)
        }
    )
}

/**
 * Recursively collect all files under a given MPQ directory.
 */
async function _collectFiles(mpqProvider, archivePath, dirPath, result) {
    const uri = MpqFileSystemProvider.makeUri(archivePath, dirPath)
    const children = await mpqProvider.readDirectory(uri)

    for (const [name, fileType] of children) {
        const childInternal = dirPath ? dirPath + '/' + name : name
        const childUri = MpqFileSystemProvider.makeUri(archivePath, childInternal)

        if (fileType === 1) {
            // FileType.File
            result.push({internal: childInternal, uri: childUri})
        } else if (fileType === 2) {
            // FileType.Directory
            await _collectFiles(mpqProvider, archivePath, childInternal, result)
        }
    }
}

/**
 * @typedef {import('vscode').Uri} Uri
 */

/**
 * Connect a raw WebSocket (using built-in `http` module) and return
 * helpers to send/receive JSON text frames.
 *
 * @param {number} port
 * @param {string} path  e.g. `/ws/rescan?token=xxx`
 * @returns {Promise<{send: (obj: any) => void, onMessage: (cb: (data: any) => void) => void, onClose: (cb: () => void) => void, close: () => void}>}
 */
function _connectWS(port, path) {
    const http = require('http')
    const crypto = require('crypto')

    return new Promise((resolve, reject) => {
        const key = crypto.randomBytes(16).toString('base64')
        const req = http.request({
            hostname: '127.0.0.1',
            port,
            path,
            headers: {
                'Connection': 'Upgrade',
                'Upgrade': 'websocket',
                'Sec-WebSocket-Version': '13',
                'Sec-WebSocket-Key': key,
            },
        })

        req.on('upgrade', (_res, socket) => {
            let buf = Buffer.alloc(0)
            const messageCbs = []
            const closeCbs = []
            let closed = false

            function fireClose() {
                if (closed) return
                closed = true
                for (const cb of closeCbs) cb()
            }

            socket.on('data', chunk => {
                buf = Buffer.concat([buf, chunk])

                while (buf.length >= 2) {
                    const opcode = buf[0] & 0x0f
                    const len0 = buf[1] & 0x7f
                    let payloadLen, headerLen

                    if (len0 <= 125) {
                        payloadLen = len0
                        headerLen = 2
                    } else if (len0 === 126) {
                        if (buf.length < 4) return
                        payloadLen = buf.readUInt16BE(2)
                        headerLen = 4
                    } else {
                        if (buf.length < 10) return
                        payloadLen = Number(buf.readBigUInt64BE(2))
                        headerLen = 10
                    }

                    if (buf.length < headerLen + payloadLen) return

                    const payload = buf.slice(headerLen, headerLen + payloadLen)
                    buf = buf.slice(headerLen + payloadLen)

                    if (opcode === 0x1) { // text
                        const text = payload.toString('utf8')
                        try {
                            const data = JSON.parse(text)
                            for (const cb of messageCbs) cb(data)
                        } catch { /* not JSON — ignore */ }
                    } else if (opcode === 0x8) { // close
                        socket.end()
                        fireClose()
                        return
                    } else if (opcode === 0x9) { // ping → pong
                        const pong = Buffer.alloc(2 + payload.length)
                        pong[0] = 0x8a
                        pong[1] = payload.length
                        payload.copy(pong, 2)
                        socket.write(pong)
                    }
                }
            })

            socket.on('close', fireClose)
            socket.on('error', e => reject(e))

            /** Send a masked WS text frame */
            function send(obj) {
                const data = Buffer.from(JSON.stringify(obj), 'utf8')
                const mask = crypto.randomBytes(4)
                let header
                if (data.length <= 125) {
                    header = Buffer.alloc(6)
                    header[0] = 0x81 // fin + text
                    header[1] = 0x80 | data.length
                    mask.copy(header, 2)
                } else if (data.length <= 65535) {
                    header = Buffer.alloc(8)
                    header[0] = 0x81
                    header[1] = 0x80 | 126
                    header.writeUInt16BE(data.length, 2)
                    mask.copy(header, 4)
                } else {
                    header = Buffer.alloc(14)
                    header[0] = 0x81
                    header[1] = 0x80 | 127
                    header.writeBigUInt64BE(BigInt(data.length), 2)
                    mask.copy(header, 10)
                }
                const masked = Buffer.alloc(data.length)
                for (let i = 0; i < data.length; i++) masked[i] = data[i] ^ mask[i & 3]
                socket.write(Buffer.concat([header, masked]))
            }

            resolve({
                send,
                onMessage: cb => messageCbs.push(cb),
                onClose: cb => closeCbs.push(cb),
                close: () => { try { socket.end() } catch (_) {} },
            })
        })

        req.on('error', reject)
        req.end()
    })
}

/**
 * Run a rescan via WebSocket and report progress.
 *
 * The server upgrades `/ws/rescan?token=...` to WS.
 * Client sends `{"uri":"..."}` as the first frame, then receives
 * progress and result frames.
 *
 * @param {ServerClient} client
 * @param {string} uri
 * @param {import('vscode').Progress<{increment?: number, message?: string}>} progress
 * @returns {Promise<{ok?: boolean, busy?: boolean, message?: string, errors?: string[]}>}
 */
function _consumeRescanWS(client, uri, progress) {
    const info = client.getServerInfo()
    if (!info) return Promise.reject(new Error('Server not started'))

    const qs = new (require('url').URLSearchParams)({token: info.token})

    return new Promise(async (resolve, reject) => {
        try {
            const ws = await _connectWS(info.port, `/ws/rescan?${qs.toString()}`)

            let lastResult = null

            ws.onMessage(data => {
                if (data.busy || data.done || (data.ok === false && data.message)) {
                    lastResult = data
                    return
                }
                if (data.total) {
                    const step = data.step || 1
                    const stepBase = (step - 1) * 50
                    const stepPct = ((data.index + 1) / data.total) * 50
                    const totalPct = stepBase + stepPct
                    const prevPct = stepBase + (data.index / data.total) * 50
                    progress.report({
                        increment: totalPct - prevPct,
                        message: `Step ${step}/2 (${data.index + 1}/${data.total}) ${data.file}`,
                    })
                }
            })

            ws.onClose(() => {
                resolve(lastResult || {ok: true, message: 'Rescan completed'})
            })

            // Send the URI payload
            ws.send({uri})

            // Safety timeout (5 min for large projects)
            const timeout = setTimeout(() => {
                ws.close()
                reject(new Error('Rescan timeout (5 min)'))
            }, 300_000)

            ws.onClose(() => clearTimeout(timeout))
        } catch (e) {
            reject(e)
        }
    })
}

/**
 * Run a shell command as a VS Code Task and wait for it to finish.
 * The output is shown in a terminal panel so the user can see it.
 *
 * @param {string} label  Task label (e.g. "build-before")
 * @param {string} cmd    Shell command to execute
 * @param {string} cwd    Working directory
 * @returns {Promise<number>} exit code (0 = success)
 */
function _runHookTask(label, cmd, cwd) {
    return new Promise((resolve) => {
        const execution = new ShellExecution(cmd, {cwd})
        const task = new Task(
            {type: 'jass-hook', label},
            TaskScope.Workspace,
            label,
            'JASS',
            execution
        )
        task.presentationOptions = {
            reveal: 2 /* TaskRevealKind.Always */,
            panel: 2 /* TaskPanelKind.Dedicated */,
            clear: true,
        }

        tasks.executeTask(task).then(taskExecution => {
            const disposable = tasks.onDidEndTaskProcess(e => {
                if (e.execution === taskExecution) {
                    disposable.dispose()
                    resolve(e.exitCode ?? -1)
                }
            })
        }, err => {
            window.showErrorMessage(`Failed to start ${label}: ${err.message}`)
            resolve(-1)
        })
    })
}

/** @type {ServerClient} */ let client

module.exports = {

    /** @param {ExtensionContext} context */
    async activate(context) {
        let binName = 'JASS-Tree-sitter-Rust-'

        switch (process.platform) {
            case 'win32':
                binName += 'windows.exe'
                break
            case 'darwin':
                binName += 'macos'
                break
            case 'linux':
                binName += 'linux'
                break
            default:
                window.showErrorMessage(`Unsupported platform: ${process.platform}`)
                return
        }

        const binPath = path.join(context.extensionPath, 'bin', binName)

        client = new ServerClient(binPath)

        // ── Trace output channel ───────────────────────────────────
        const traceChannel = window.createOutputChannel('JASS Server Trace')
        client._traceOutputChannel = traceChannel
        context.subscriptions.push(traceChannel)

        // ── Debug log output channel ─────────────────────────────────
        const debugLogChannel = window.createOutputChannel('JASS Server Log')
        context.subscriptions.push(debugLogChannel)
        function debugLog(msg) {
            const ts = new Date().toISOString()
            console.log(`[JASS] ${msg}`)
            debugLogChannel.appendLine(`[${ts}] ${msg}`)
        }
        debugLog('extension activated')

        context.subscriptions.push(commands.registerCommand('jass.toggleTrace', () => {
            client.traceMessages = !client.traceMessages
            const state = client.traceMessages ? 'ON' : 'OFF'
            traceChannel.appendLine(`[trace] Message tracing ${state}`)
            if (client.traceMessages) traceChannel.show(true)
            window.showInformationMessage(`JASS Server Trace: ${state}`)
        }))

        // ── Supported languages for document sync ─────────────────────
        const SUPPORTED_LANGUAGES = new Set(['bni', 'jass', 'angelscript', 'wts', 'slk'])

        // ── Diagnostics collection ────────────────────────────────────
        const diagnosticCollection = languages.createDiagnosticCollection('jass')


        // ── Tree membership set (URIs belonging to any open import tree) ──
        /** @type {Set<string>} */
        const treeUris = new Set()

        // ── Per-document tracking (for correct cleanup on close) ──
        /** @type {Map<string, Set<string>>} doc URI → tree URIs contributed by that doc */
        const perDocTreeUris = new Map()

        // ── File decoration provider (error/warning badges in Explorer) ──
        const fileDecoChanged = new EventEmitter()
        const fileDecorationProvider = window.registerFileDecorationProvider({
            onDidChangeFileDecorations: fileDecoChanged.event,
            provideFileDecoration(uri) {
                // Tree membership indicator
                if (treeUris.has(uri.toString())) {
                    return new VscFileDecoration(
                        '🔗',
                        'Import tree member',
                        new ThemeColor('textLink.foreground')
                    )
                }
                return undefined
            }
        })

        // ── Caches for pushed data ────────────────────────────────────
        /** @type {Map<string, import('vscode').InlayHint[]>} */
        const inlayHintsCache = new Map()
        const inlayHintsChanged = new EventEmitter()

        /** @type {Map<string, number[]>} uri → raw semantic token data */
        const semanticCache = new Map()
        const semanticChanged = new EventEmitter()

        /** @type {Map<string, import('vscode').FoldingRange[]>} */
        const foldingCache = new Map()

        /** @type {Map<string, import('vscode').DocumentSymbol[]>} */
        const symbolsCache = new Map()

        /** @type {Map<string, import('vscode').DocumentLink[]>} */
        const linksCache = new Map()
        /** @type {Map<string, (links: import('vscode').DocumentLink[]) => void>} */
        const linksResolvers = new Map()

        /** @type {Map<string, import('vscode').ColorInformation[]>} */
        const colorsCache = new Map()

        /** @type {Map<string, import('vscode').CodeLens[]>} */
        const codeLensCache = new Map()
        const codeLensChanged = new EventEmitter()

        // ── Semantic token legend (must match Rust Kind/Mod enums) ─────
        const tokenTypes = [
            'namespace', 'class', 'enum', 'interface', 'struct',
            'typeParameter', 'type', 'parameter', 'variable', 'property',
            'enumMember', 'decorator', 'event', 'function', 'method',
            'macro', 'label', 'comment', 'string', 'keyword',
            'number', 'regexp', 'operator',
        ]
        const tokenModifiers = [
            'declaration', 'definition', 'readonly', 'static',
            'deprecated', 'abstract', 'async', 'modification',
            'documentation', 'defaultLibrary',
        ]
        const legend = new SemanticTokensLegend(tokenTypes, tokenModifiers)

        // ── Binary TLV section types (must match Rust `section::*`) ───
        // Response sections (server → client)
        const SECTION_SEMANTIC = 0x01
        const SECTION_INLAY_HINTS = 0x02
        const SECTION_SEMANTIC_EDIT = 0x03
        const SECTION_DIAGNOSTICS = 0x04
        const SECTION_FOLDING = 0x05
        const SECTION_SYMBOLS = 0x06
        const SECTION_LINKS = 0x07
        const SECTION_COLORS = 0x08
        const SECTION_CODE_LENSES = 0x09
        const SECTION_PEER_DIAG_SUMMARY = 0x0A
        const SECTION_TREE_URIS = 0x0B
        // Request sections (client → server)
        const SECTION_FULL_TEXT = 0x10
        const SECTION_CONTENT_CHANGE = 0x11
        const SECTION_OPEN_URI = 0x12



        // ── Binary HTTP server (parallel data channel) ───────────────
        // Server info (port + token) is available immediately after start().
        /** @returns {{port: number, token: string} | null} */
        function getBinaryServer() { return client.getServerInfo() }

        // ── Semantic delta state ──────────────────────────────────────
        // `semanticBase` holds the last SERVER-sent token array (un-adjusted
        // by _adjustSemanticTokens).  Deltas are applied to this base, not
        // to `semanticCache` which may have been locally shifted.
        /** @type {Map<string, Uint32Array>} uri → last server semantic data */
        const semanticBase = new Map()
        /** @type {Map<string, number>} uri → last received resultId */
        const semanticResultId = new Map()

        // ── Document selectors ────────────────────────────────────────
        const allSelector = [
            {scheme: 'file', language: 'bni'},
            {scheme: 'file', language: 'jass'},
            {scheme: 'file', language: 'angelscript'},
            {scheme: 'file', language: 'wts'},
            {scheme: 'file', language: 'slk'},
            {scheme: 'mpq', language: 'jass'},
            {scheme: 'mpq', language: 'angelscript'},
            {scheme: 'mpq', language: 'wts'},
            {scheme: 'mpq', language: 'slk'},
        ]

        const inlaySelector = [
            {scheme: 'file', language: 'jass'},
            {scheme: 'file', language: 'angelscript'},
            {scheme: 'mpq', language: 'jass'},
            {scheme: 'mpq', language: 'angelscript'},
        ]

        // ── Register cache-based providers ────────────────────────────
        const inlayHintsProvider = languages.registerInlayHintsProvider(inlaySelector, {
            onDidChangeInlayHints: inlayHintsChanged.event,
            provideInlayHints(document) {
                return inlayHintsCache.get(document.uri.toString()) || []
            }
        })

        const semanticTokensProvider = languages.registerDocumentSemanticTokensProvider(
            allSelector,
            {
                onDidChangeSemanticTokens: semanticChanged.event,
                provideDocumentSemanticTokens(document) {
                    // Lazy open: first time VS Code asks for tokens for this
                    // document — send FULL_TEXT to the server. The response
                    // will fire semanticChanged → re-query with real data.
                    const uri = document.uri.toString()
                    if (!openedDocs.has(uri) && SUPPORTED_LANGUAGES.has(document.languageId)
                        && (document.uri.scheme === 'file' || document.uri.scheme === 'mpq')) {
                        clientReady.then(() => _sendDidOpen(document))
                    }
                    const data = semanticCache.get(uri)
                    if (!data || data.length === 0) return undefined
                    const src = data instanceof Uint32Array ? data : new Uint32Array(data)

                    // Clamp stale tokens so they never exceed current line
                    // lengths (avoids "Invalid Semantic Tokens Data" error).
                    // In the common case (tokens match document) this is a
                    // single fast scan with no copy.
                    const lineCount = document.lineCount
                    let line = 0, startChar = 0, needsClamp = false
                    for (let i = 0; i + 4 <= src.length; i += 5) {
                        line += src[i]
                        startChar = src[i] > 0 ? src[i + 1] : startChar + src[i + 1]
                        if (line >= lineCount || startChar + src[i + 2] > document.lineAt(line).text.length) {
                            needsClamp = true
                            break
                        }
                    }
                    if (!needsClamp) return {data: src}

                    // Slow path: copy and clamp positions to fit the current document.
                    const arr = new Uint32Array(src)
                    line = 0; startChar = 0
                    let validLen = arr.length
                    for (let i = 0; i + 4 <= arr.length; i += 5) {
                        line += arr[i]
                        startChar = arr[i] > 0 ? arr[i + 1] : startChar + arr[i + 1]
                        if (line >= lineCount) { validLen = i; break }
                        const lineLen = document.lineAt(line).text.length
                        if (startChar + arr[i + 2] > lineLen) {
                            arr[i + 2] = Math.max(0, lineLen - startChar)
                        }
                    }
                    const result = validLen < arr.length ? arr.slice(0, validLen) : arr
                    return result.length > 0 ? {data: result} : undefined
                },
            },
            legend
        )

        const foldingProvider = languages.registerFoldingRangeProvider(allSelector, {
            provideFoldingRanges(document) {
                return foldingCache.get(document.uri.toString()) || []
            }
        })

        const symbolProvider = languages.registerDocumentSymbolProvider(allSelector, {
            provideDocumentSymbols(document) {
                return symbolsCache.get(document.uri.toString()) || []
            }
        })

        const linkProvider = languages.registerDocumentLinkProvider(allSelector, {
            provideDocumentLinks(document, token) {
                const uri = document.uri.toString()
                const cached = linksCache.get(uri)
                if (cached) return cached
                return new Promise(resolve => {
                    linksResolvers.set(uri, resolve)
                    token.onCancellationRequested(() => {
                        linksResolvers.delete(uri)
                        resolve([])
                    })
                    setTimeout(() => {
                        if (linksResolvers.delete(uri)) resolve(linksCache.get(uri) || [])
                    }, 15000)
                })
            }
        })

        const colorProvider = languages.registerColorProvider(allSelector, {
            provideDocumentColors(document) {
                return colorsCache.get(document.uri.toString()) || []
            },
            provideColorPresentations(color, ctx) {
                const uri = ctx.document.uri.toString()
                return client.sendRequest('lsp/colorPresentation', {
                    uri,
                    color: {red: color.red, green: color.green, blue: color.blue, alpha: color.alpha},
                    range: {
                        start: {line: ctx.range.start.line, character: ctx.range.start.character},
                        end: {line: ctx.range.end.line, character: ctx.range.end.character},
                    },
                }, uri).then(presentations => {
                    return (presentations || []).map(p => {
                        const cp = new VscColorPresentation(p.label)
                        if (p.textEdit) {
                            cp.textEdit = new TextEdit(
                                new Range(
                                    p.textEdit.range.start.line, p.textEdit.range.start.character,
                                    p.textEdit.range.end.line, p.textEdit.range.end.character,
                                ),
                                p.textEdit.newText
                            )
                        }
                        return cp
                    })
                }).catch(() => [])
            }
        })

        const codeLensProvider = languages.registerCodeLensProvider(allSelector, {
            onDidChangeCodeLenses: codeLensChanged.event,
            provideCodeLenses(document) {
                return codeLensCache.get(document.uri.toString()) || []
            }
        })

        // ── Rename provider ─────────────────────────────────────────
        const renameSelector = [
            {scheme: 'file', language: 'jass'},
            {scheme: 'file', language: 'angelscript'},
        ]
        const renameProvider = languages.registerRenameProvider(renameSelector, {
            async prepareRename(document, position) {
                const uri = document.uri.toString()
                const result = await client.sendRequest('lsp/prepareRename', {
                    uri,
                    position: {line: position.line, character: position.character},
                })
                if (!result) return undefined
                return {
                    range: new Range(
                        result.range.start.line, result.range.start.character,
                        result.range.end.line, result.range.end.character,
                    ),
                    placeholder: result.placeholder,
                }
            },
            async provideRenameEdits(document, position, newName) {
                const uri = document.uri.toString()
                const result = await client.sendRequest('lsp/rename', {
                    uri,
                    position: {line: position.line, character: position.character},
                    newName,
                })
                if (!result || !result.changes) return undefined
                const edit = new VscWorkspaceEdit()
                for (const [docUri, edits] of Object.entries(result.changes)) {
                    const fileUri = Uri.parse(docUri)
                    for (const e of edits) {
                        edit.replace(fileUri,
                            new Range(
                                e.range.start.line, e.range.start.character,
                                e.range.end.line, e.range.end.character,
                            ),
                            e.newText,
                        )
                    }
                }
                return edit
            }
        })

        // ── Helpers for LSP → VS Code type conversions ────────────
        function _pos(p) { return new Position(p.line, p.character) }
        function _range(r) { return new Range(_pos(r.start), _pos(r.end)) }
        function _location(loc) { return new VscLocation(Uri.parse(loc.uri), _range(loc.range)) }
        function _posParam(position) { return {line: position.line, character: position.character} }

        // ── Completion provider ───────────────────────────────────
        const completionProvider = languages.registerCompletionItemProvider(renameSelector, {
            async provideCompletionItems(document, position) {
                const result = await client.sendRequest('lsp/completion', {
                    uri: document.uri.toString(),
                    position: _posParam(position),
                })
                if (!result || !result.items) return undefined
                const items = result.items.map(item => {
                    const ci = new VscCompletionItem(item.label, item.kind)
                    if (item.detail) ci.detail = item.detail
                    if (item.insertText) ci.insertText = item.insertText
                    if (item.insertTextFormat === 2) {
                        const {SnippetString} = require('vscode')
                        ci.insertText = new SnippetString(item.insertText || item.label)
                    }
                    if (item.sortText) ci.sortText = item.sortText
                    return ci
                })
                return new VscCompletionList(items, result.isIncomplete)
            }
        }, '.', '/', '\\')

        // ── Combined cursor context (hover + highlight + codeAction) ──
        // All three fire on every mouse move. A shared cache ensures only
        // one HTTP request per cursor position.
        let _cursorCtx = { uri: '', line: -1, char: -1, hasRange: false, promise: null }
        function _getCursorContext(uri, position, range, context) {
            const line = position.line, char = position.character
            const hasRange = !!(range && context)
            // Cache hit only when params match (zero-width requests share cache)
            if (_cursorCtx.uri === uri && _cursorCtx.line === line
                && _cursorCtx.char === char && _cursorCtx.hasRange === hasRange
                && _cursorCtx.promise) {
                return _cursorCtx.promise
            }
            const body = { uri, position: _posParam(position) }
            if (range) {
                body.range = { start: _posParam(range.start), end: _posParam(range.end) }
            }
            if (context) {
                body.context = {
                    diagnostics: context.diagnostics.map(d => ({
                        range: {start: _posParam(d.range.start), end: _posParam(d.range.end)},
                        message: d.message,
                        severity: d.severity != null ? d.severity + 1 : undefined,
                        code: d.code != null ? (typeof d.code === 'object' ? d.code.value : d.code) : undefined,
                    })),
                }
            }
            _cursorCtx = {
                uri, line, char, hasRange,
                promise: client.sendRequest('lsp/cursor', body).catch(() => null),
            }
            return _cursorCtx.promise
        }

        // ── Hover provider ────────────────────────────────────────
        const hoverProvider = languages.registerHoverProvider(renameSelector, {
            async provideHover(document, position) {
                const ctx = await _getCursorContext(document.uri.toString(), position)
                const result = ctx?.hover
                if (!result || !result.contents) return undefined
                const md = new MarkdownString(result.contents.value)
                md.isTrusted = true
                const hover = new VscHover([md])
                if (result.range) hover.range = _range(result.range)
                return hover
            }
        })

        // ── Definition provider ───────────────────────────────────
        const definitionProvider = languages.registerDefinitionProvider(renameSelector, {
            async provideDefinition(document, position) {
                const result = await client.sendRequest('lsp/definition', {
                    uri: document.uri.toString(),
                    position: _posParam(position),
                })
                if (!result || !Array.isArray(result) || result.length === 0) return undefined
                return result.map(_location)
            }
        })

        // ── References provider ───────────────────────────────────
        const referencesProvider = languages.registerReferenceProvider(renameSelector, {
            async provideReferences(document, position, context) {
                const result = await client.sendRequest('lsp/references', {
                    uri: document.uri.toString(),
                    position: _posParam(position),
                    context: {includeDeclaration: context.includeDeclaration},
                })
                if (!result || !Array.isArray(result)) return undefined
                return result.map(_location)
            }
        })

        // ── Document Highlight provider ───────────────────────────
        const highlightProvider = languages.registerDocumentHighlightProvider(renameSelector, {
            async provideDocumentHighlights(document, position) {
                const ctx = await _getCursorContext(document.uri.toString(), position)
                const result = ctx?.highlights
                if (!result || !Array.isArray(result)) return undefined
                return result.map(h => {
                    const kind = h.kind === 3 ? VscDocumentHighlightKind.Write
                        : h.kind === 2 ? VscDocumentHighlightKind.Read
                            : VscDocumentHighlightKind.Text
                    return new VscDocumentHighlight(_range(h.range), kind)
                })
            }
        })

        // ── Formatting provider ───────────────────────────────────
        const formattingProvider = languages.registerDocumentFormattingEditProvider(renameSelector, {
            async provideDocumentFormattingEdits(document, options) {
                const result = await client.sendRequest('lsp/formatting', {
                    uri: document.uri.toString(),
                    options: {
                        tabSize: options.tabSize,
                        insertSpaces: options.insertSpaces,
                    },
                })
                if (!result || !Array.isArray(result)) return undefined
                return result.map(e => new TextEdit(_range(e.range), e.newText))
            }
        })

        // ── Signature Help provider ───────────────────────────────
        const signatureHelpProvider = languages.registerSignatureHelpProvider(renameSelector, {
            async provideSignatureHelp(document, position) {
                const result = await client.sendRequest('lsp/signatureHelp', {
                    uri: document.uri.toString(),
                    position: _posParam(position),
                })
                if (!result || !result.signatures) return undefined
                const help = new VscSignatureHelp()
                help.signatures = result.signatures.map(sig => {
                    const si = new VscSignatureInformation(sig.label,
                        sig.documentation ? new MarkdownString(sig.documentation.value) : undefined)
                    if (sig.parameters) {
                        si.parameters = sig.parameters.map(p => {
                            const label = Array.isArray(p.label) ? [p.label[0], p.label[1]] : p.label
                            return new VscParameterInformation(label,
                                p.documentation ? new MarkdownString(p.documentation.value) : undefined)
                        })
                    }
                    if (sig.activeParameter !== undefined && sig.activeParameter !== null) {
                        si.activeParameter = sig.activeParameter
                    }
                    return si
                })
                help.activeSignature = result.activeSignature ?? 0
                help.activeParameter = result.activeParameter ?? 0
                return help
            }
        }, '(', ',')

        // ── Code Action provider ──────────────────────────────────
        const codeActionProvider = languages.registerCodeActionsProvider(renameSelector, {
            async provideCodeActions(document, range, context) {
                const isZeroWidth = range.start.line === range.end.line
                    && range.start.character === range.end.character
                const ctx = await _getCursorContext(
                    document.uri.toString(),
                    range.start,
                    isZeroWidth ? undefined : range,
                    isZeroWidth ? undefined : context,
                )
                const result = ctx?.codeActions
                if (!result || !Array.isArray(result) || result.length === 0) return undefined
                return result.map(action => {
                    const ca = new VscCodeAction(action.title,
                        action.kind === 'quickfix' ? VscCodeActionKind.QuickFix : VscCodeActionKind.Refactor)
                    if (action.edit && action.edit.changes) {
                        const we = new VscWorkspaceEdit()
                        for (const [docUri, edits] of Object.entries(action.edit.changes)) {
                            const fileUri = Uri.parse(docUri)
                            for (const e of edits) {
                                we.replace(fileUri, _range(e.range), e.newText)
                            }
                        }
                        ca.edit = we
                    }
                    if (action.command) {
                        ca.command = {
                            title: action.command.title,
                            command: action.command.command,
                            arguments: action.command.arguments || [],
                        }
                    }
                    return ca
                })
            }
        })

        // ── Call Hierarchy provider ───────────────────────────────
        const callHierarchyProvider = languages.registerCallHierarchyProvider(renameSelector, {
            async prepareCallHierarchy(document, position) {
                const result = await client.sendRequest('lsp/callHierarchy/prepare', {
                    uri: document.uri.toString(),
                    position: _posParam(position),
                })
                if (!result || !Array.isArray(result) || result.length === 0) return undefined
                return result.map(item => _makeCallHierarchyItem(item))
            },
            async provideCallHierarchyIncomingCalls(item) {
                const result = await client.sendRequest('lsp/callHierarchy/incoming', {
                    item: _serializeHierarchyItem(item),
                })
                if (!result || !Array.isArray(result)) return []
                return result.map(call => new VscCallHierarchyIncomingCall(
                    _makeCallHierarchyItem(call.from),
                    call.fromRanges.map(_range),
                ))
            },
            async provideCallHierarchyOutgoingCalls(item) {
                const result = await client.sendRequest('lsp/callHierarchy/outgoing', {
                    item: _serializeHierarchyItem(item),
                })
                if (!result || !Array.isArray(result)) return []
                return result.map(call => new VscCallHierarchyOutgoingCall(
                    _makeCallHierarchyItem(call.to),
                    call.fromRanges.map(_range),
                ))
            },
        })

        // ── Type Hierarchy provider ───────────────────────────────
        const typeHierarchyProvider = languages.registerTypeHierarchyProvider(renameSelector, {
            async prepareTypeHierarchy(document, position) {
                const result = await client.sendRequest('lsp/typeHierarchy/prepare', {
                    uri: document.uri.toString(),
                    position: _posParam(position),
                })
                if (!result || !Array.isArray(result) || result.length === 0) return undefined
                return result.map(item => _makeTypeHierarchyItem(item))
            },
            async provideTypeHierarchySupertypes(item) {
                const result = await client.sendRequest('lsp/typeHierarchy/supertypes', {
                    item: _serializeHierarchyItem(item),
                })
                if (!result || !Array.isArray(result)) return []
                return result.map(i => _makeTypeHierarchyItem(i))
            },
            async provideTypeHierarchySubtypes(item) {
                const result = await client.sendRequest('lsp/typeHierarchy/subtypes', {
                    item: _serializeHierarchyItem(item),
                })
                if (!result || !Array.isArray(result)) return []
                return result.map(i => _makeTypeHierarchyItem(i))
            },
        })

        // ── Hierarchy helpers ─────────────────────────────────────
        /** Map LSP SymbolKind (1-based int) to VS Code SymbolKind */
        function _symbolKind(k) {
            // LSP and VS Code use the same numbering for SymbolKind
            return k ?? VscSymbolKind.Function
        }

        function _makeCallHierarchyItem(item) {
            const chi = new VscCallHierarchyItem(
                _symbolKind(item.kind),
                item.name,
                item.detail || '',
                Uri.parse(item.uri),
                _range(item.range),
                _range(item.selectionRange),
            )
            if (item.data) chi._serverData = item.data
            return chi
        }

        function _makeTypeHierarchyItem(item) {
            const thi = new VscTypeHierarchyItem(
                _symbolKind(item.kind),
                item.name,
                item.detail || '',
                Uri.parse(item.uri),
                _range(item.range),
                _range(item.selectionRange),
            )
            if (item.data) thi._serverData = item.data
            return thi
        }

        function _serializeHierarchyItem(item) {
            return {
                name: item.name,
                kind: item.kind,
                uri: item.uri.toString(),
                range: {start: _posParam(item.range.start), end: _posParam(item.range.end)},
                selectionRange: {start: _posParam(item.selectionRange.start), end: _posParam(item.selectionRange.end)},
                detail: item.detail || undefined,
                data: item._serverData || undefined,
            }
        }

        // ── Will / Did rename files ─────────────────────────────────
        const willRenameDisposable = workspace.onWillRenameFiles(event => {
            const files = event.files
                .filter(f => /\.(j|ai)$/i.test(f.oldUri.fsPath))
                .map(f => ({oldUri: f.oldUri.toString(), newUri: f.newUri.toString()}))
            if (files.length === 0) return
            const editPromise = client.sendRequest('lsp/willRenameFiles', {files}).then(result => {
                if (!result || !result.changes) return undefined
                const edit = new VscWorkspaceEdit()
                for (const [docUri, edits] of Object.entries(result.changes)) {
                    const fileUri = Uri.parse(docUri)
                    for (const e of edits) {
                        edit.replace(fileUri,
                            new Range(
                                e.range.start.line, e.range.start.character,
                                e.range.end.line, e.range.end.character,
                            ),
                            e.newText,
                        )
                    }
                }
                return edit
            }).catch(() => undefined)
            event.waitUntil(editPromise)
        })

        const didRenameDisposable = workspace.onDidRenameFiles(event => {
            const files = event.files
                .filter(f => /\.(j|ai)$/i.test(f.oldUri.fsPath) || /\.(j|ai)$/i.test(f.newUri.fsPath))
                .map(f => ({oldUri: f.oldUri.toString(), newUri: f.newUri.toString()}))
            if (files.length === 0) return

            // Swap URI in local caches
            for (const f of files) {
                const oldKey = f.oldUri
                const newKey = f.newUri

                if (openedDocs.has(oldKey)) {
                    openedDocs.delete(oldKey)
                    openedDocs.add(newKey)
                }

                if (_docVersion.has(oldKey)) {
                    _docVersion.set(newKey, _docVersion.get(oldKey))
                    _docVersion.delete(oldKey)
                }

                if (_queue.has(oldKey)) {
                    _queue.set(newKey, _queue.get(oldKey))
                    _queue.delete(oldKey)
                }
                if (_locked.has(oldKey)) {
                    _locked.set(newKey, _locked.get(oldKey))
                    _locked.delete(oldKey)
                }

                if (semanticBase.has(oldKey)) {
                    semanticBase.set(newKey, semanticBase.get(oldKey))
                    semanticBase.delete(oldKey)
                }
                if (semanticResultId.has(oldKey)) {
                    semanticResultId.set(newKey, semanticResultId.get(oldKey))
                    semanticResultId.delete(oldKey)
                }
                if (semanticCache.has(oldKey)) {
                    semanticCache.set(newKey, semanticCache.get(oldKey))
                    semanticCache.delete(oldKey)
                }

                if (inlayHintsCache.has(oldKey)) {
                    inlayHintsCache.set(newKey, inlayHintsCache.get(oldKey))
                    inlayHintsCache.delete(oldKey)
                }
                if (foldingCache.has(oldKey)) {
                    foldingCache.set(newKey, foldingCache.get(oldKey))
                    foldingCache.delete(oldKey)
                }
                if (symbolsCache.has(oldKey)) {
                    symbolsCache.set(newKey, symbolsCache.get(oldKey))
                    symbolsCache.delete(oldKey)
                }
                if (linksCache.has(oldKey)) {
                    linksCache.set(newKey, linksCache.get(oldKey))
                    linksCache.delete(oldKey)
                }
                if (colorsCache.has(oldKey)) {
                    colorsCache.set(newKey, colorsCache.get(oldKey))
                    colorsCache.delete(oldKey)
                }
                if (codeLensCache.has(oldKey)) {
                    codeLensCache.set(newKey, codeLensCache.get(oldKey))
                    codeLensCache.delete(oldKey)
                }

                // Swap diagnostics
                const oldDiagUri = Uri.parse(oldKey)
                const existingDiags = diagnosticCollection.get(oldDiagUri)
                if (existingDiags && existingDiags.length > 0) {
                    diagnosticCollection.delete(oldDiagUri)
                    const newDiagUri = Uri.parse(newKey)
                    diagnosticCollection.set(newDiagUri, existingDiags)
                }
            }

            // Notify server to swap URIs (no rescan)
            client.http.post('/document/didRenameFiles', {files})
                .catch(e => console.error('document/didRenameFiles error:', e))
        })

        // ── Start the client and perform document sync ────────────────
        const clientReady = client.start().then(() => {
            const info = getBinaryServer()
            debugLog(`server started on port ${info ? info.port : '?'}`)
        }).catch(err => {
            debugLog(`server start FAILED: ${err.message}`)
            window.showErrorMessage(`❌ Failed to start server:\n\n${err.message}`)
        })

        // ── Manual document sync ──────────────────────────────────────
        /** Track documents we've sent didOpen for */
        const openedDocs = new Set()

        // Send didOpen for all already-open documents
        clientReady.then(() => {
            for (const doc of workspace.textDocuments) {
                if (SUPPORTED_LANGUAGES.has(doc.languageId) && (doc.uri.scheme === 'file' || doc.uri.scheme === 'mpq')) {
                    _sendDidOpen(doc)
                }
            }
        })

        // ── WebSocket debug log ────────────────────────────────────
        clientReady.then(() => {
            const info = getBinaryServer()
            if (!info) {
                debugLog('no server info, WS skipped')
                return
            }
            debugLog(`connecting WS to port ${info.port}`)
            const http = require('http')
            const crypto = require('crypto')

            const key = crypto.randomBytes(16).toString('base64')
            const req = http.request({
                hostname: '127.0.0.1',
                port: info.port,
                path: `/ws/log?token=${info.token}`,
                headers: {
                    'Connection': 'Upgrade',
                    'Upgrade': 'websocket',
                    'Sec-WebSocket-Version': '13',
                    'Sec-WebSocket-Key': key,
                },
            })

            req.on('upgrade', (res, socket) => {
                debugLog('WS connected')
                let buf = Buffer.alloc(0)

                socket.on('data', chunk => {
                    buf = Buffer.concat([buf, chunk])

                    while (buf.length >= 2) {
                        const opcode = buf[0] & 0x0f
                        const len0 = buf[1] & 0x7f
                        let payloadLen, headerLen

                        if (len0 <= 125) {
                            payloadLen = len0
                            headerLen = 2
                        } else if (len0 === 126) {
                            if (buf.length < 4) return
                            payloadLen = buf.readUInt16BE(2)
                            headerLen = 4
                        } else {
                            if (buf.length < 10) return
                            payloadLen = Number(buf.readBigUInt64BE(2))
                            headerLen = 10
                        }

                        if (buf.length < headerLen + payloadLen) return

                        const payload = buf.slice(headerLen, headerLen + payloadLen)
                        buf = buf.slice(headerLen + payloadLen)

                        if (opcode === 0x1) { // text frame
                            debugLog(payload.toString('utf8'))
                        } else if (opcode === 0x8) { // close
                            socket.end()
                            return
                        } else if (opcode === 0x9) { // ping → pong
                            const pong = Buffer.alloc(2 + payload.length)
                            pong[0] = 0x8a // fin + pong
                            pong[1] = payload.length
                            payload.copy(pong, 2)
                            socket.write(pong)
                        }
                    }
                })

                socket.on('close', () => debugLog('WS closed'))
                socket.on('error', e => debugLog(`WS socket error: ${e.message}`))

                context.subscriptions.push({dispose: () => {
                    try { socket.end() } catch (_) {}
                }})
            })

            req.on('error', e => debugLog(`WS error: ${e.message}`))
            req.end()
            context.subscriptions.push({dispose: () => req.destroy()})
        })


        // ── Per-URI serial update queue ─────────────────────────────
        // Only ONE /document/update request is in flight per URI at any
        // time.  While a request is running, edits accumulate in
        // `_pending`.  When the response arrives the accumulated edits
        // are sent as a single batch.  If no request is in flight the
        // edit is sent immediately.
        //
        // Guarantees:
        //  • All edits reach the server in order (no lost edits)
        //  • Only one parse per batch (efficient)
        //  • Version echo discards stale responses

        /** @type {Map<string, number>} uri → monotonic document version */
        const _docVersion = new Map()

        /** @type {Map<string, {version: number, languageId: string, sections: Buffer[]}>} */
        const _queue = new Map()

        /** @type {Map<string, boolean>} */
        const _locked = new Map()



        /**
         * Parse a binary TLV response from /document/update and populate caches.
         *
         * When `fresh` is true the response matches the current document
         * version — display caches (`semanticCache`, `inlayHintsCache`) are
         * updated and change events fired.
         *
         * When `fresh` is false the response is stale (version mismatch) —
         * only the delta-tracking state (`semanticBase`, `semanticResultId`)
         * is updated so the next request can still send `lastResultId` and
         * receive a compact delta instead of a full token array.
         *
         * @param {string} uri
         * @param {Buffer} buf
         * @param {boolean} fresh
         */
        function _applyBinaryResponse(uri, buf, fresh) {
            let offset = 0
            while (offset + 5 <= buf.length) {
                const type = buf[offset]; offset += 1
                const len = buf.readUInt32LE(offset); offset += 4
                if (offset + len > buf.length) break
                const data = buf.slice(offset, offset + len); offset += len

                switch (type) {
                    case SECTION_SEMANTIC: {
                        // Full semantic tokens: [u32 resultId][u32... tokens]
                        const resultId = data.readUInt32LE(0)
                        const tokenData = data.slice(4)
                        const aligned = new Uint8Array(tokenData).buffer
                        const u32 = new Uint32Array(aligned)
                        semanticBase.set(uri, u32)
                        semanticResultId.set(uri, resultId)
                        if (fresh) {
                            semanticCache.set(uri, u32)
                            semanticChanged.fire()
                        }
                        break
                    }
                    case SECTION_SEMANTIC_EDIT: {
                        // Token-aware delta: [u32 resultId][...stream of 5×u32 tuples]
                        // Each tuple is either:
                        //   regular token: [deltaLine, deltaChar, len, type, mods]
                        //   COPY command:  [0xFFFFFFFF, 0, count, 0, 0]
                        //   SKIP command:  [0xFFFFFFFF, 1, count, 0, 0]
                        const SENTINEL = 0xFFFFFFFF
                        const OP_COPY = 0
                        const OP_SKIP = 1

                        const resultId = data.readUInt32LE(0)
                        const stream = new Uint32Array(new Uint8Array(data.slice(4)).buffer)
                        const base = semanticBase.get(uri) || new Uint32Array(0)

                        const result = []
                        let oldCursor = 0 // index into base (u32 units)

                        for (let si = 0; si + 4 < stream.length; si += 5) {
                            if (stream[si] === SENTINEL) {
                                const op = stream[si + 1]
                                const count = stream[si + 2]
                                const len = count * 5
                                if (op === OP_COPY) {
                                    for (let j = 0; j < len && oldCursor + j < base.length; j++) {
                                        result.push(base[oldCursor + j])
                                    }
                                    oldCursor += len
                                } else if (op === OP_SKIP) {
                                    oldCursor += len
                                }
                            } else {
                                // Regular token — insert
                                result.push(stream[si], stream[si + 1], stream[si + 2], stream[si + 3], stream[si + 4])
                            }
                        }

                        const tokens = new Uint32Array(result)
                        semanticBase.set(uri, tokens)
                        semanticResultId.set(uri, resultId)
                        if (fresh) {
                            semanticCache.set(uri, tokens)
                            semanticChanged.fire()
                        }
                        break
                    }
                    case SECTION_INLAY_HINTS: {
                        if (!fresh) break
                        const hints = []
                        let p = 0
                        while (p + 11 <= data.length) {
                            const line = data.readUInt32LE(p); p += 4
                            const character = data.readUInt32LE(p); p += 4
                            const kind = data[p]; p += 1
                            const labelLen = data.readUInt16LE(p); p += 2
                            if (p + labelLen > data.length) break
                            const label = data.toString('utf8', p, p + labelLen); p += labelLen

                            const hint = new InlayHint(
                                new Position(line, character),
                                label,
                                kind === 1 ? InlayHintKind.Type
                                    : kind === 2 ? InlayHintKind.Parameter
                                        : undefined
                            )
                            hint.paddingLeft = true
                            hint.paddingRight = false
                            hints.push(hint)
                        }
                        inlayHintsCache.set(uri, hints)
                        inlayHintsChanged.fire()
                        break
                    }
                    case SECTION_DIAGNOSTICS: {
                        if (!fresh) break
                        const diags = []
                        let p = 0
                        while (p + 17 <= data.length) {
                            const startLine = data.readUInt32LE(p); p += 4
                            const startChar = data.readUInt32LE(p); p += 4
                            const endLine = data.readUInt32LE(p); p += 4
                            const endChar = data.readUInt32LE(p); p += 4
                            const severity = data[p]; p += 1
                            const msgLen = data.readUInt16LE(p); p += 2
                            if (p + msgLen > data.length) break
                            const message = data.toString('utf8', p, p + msgLen); p += msgLen
                            // Tags
                            if (p >= data.length) break
                            const tagCount = data[p]; p += 1
                            const tags = []
                            for (let t = 0; t < tagCount && p < data.length; t++) {
                                tags.push(data[p]); p += 1
                            }
                            // Code
                            if (p + 2 > data.length) break
                            const codeLen = data.readUInt16LE(p); p += 2
                            const code = codeLen > 0 ? data.toString('utf8', p, p + codeLen) : undefined; p += codeLen
                            // Code href
                            if (p + 2 > data.length) break
                            const codeHrefLen = data.readUInt16LE(p); p += 2
                            const codeHref = codeHrefLen > 0 ? data.toString('utf8', p, p + codeHrefLen) : undefined; p += codeHrefLen
                            // Source
                            if (p + 2 > data.length) break
                            const sourceLen = data.readUInt16LE(p); p += 2
                            const source = sourceLen > 0 ? data.toString('utf8', p, p + sourceLen) : undefined; p += sourceLen

                            const diag = new VscDiagnostic(
                                new Range(startLine, startChar, endLine, endChar),
                                message,
                                severity === 1 ? 0 : severity === 2 ? 1 : severity === 3 ? 2 : severity === 4 ? 3 : 2
                            )
                            if (source) diag.source = source
                            if (code && codeHref) {
                                diag.code = {value: code, target: Uri.parse(codeHref)}
                            } else if (code) {
                                diag.code = code
                            }
                            if (tags.length > 0) diag.tags = tags
                            diags.push(diag)
                        }
                        try {
                            const parsedUri = Uri.parse(uri)
                            diagnosticCollection.set(parsedUri, diags)
                        } catch {}
                        break
                    }
                    case SECTION_FOLDING: {
                        if (!fresh) break
                        const ranges = []
                        let p = 0
                        while (p + 9 <= data.length) {
                            const startLine = data.readUInt32LE(p); p += 4
                            const endLine = data.readUInt32LE(p); p += 4
                            const kind = data[p]; p += 1
                            const fr = new FoldingRange(startLine, endLine,
                                kind === 1 ? FoldingRangeKind.Comment
                                    : kind === 2 ? FoldingRangeKind.Imports
                                        : kind === 3 ? FoldingRangeKind.Region
                                            : undefined
                            )
                            ranges.push(fr)
                        }
                        foldingCache.set(uri, ranges)
                        break
                    }
                    case SECTION_SYMBOLS: {
                        if (!fresh) break
                        try {
                            const raw = JSON.parse(data.toString('utf8'))
                            const convert = (items) => (items || []).map(s => {
                                const range = new Range(
                                    s.range.start.line, s.range.start.character,
                                    s.range.end.line, s.range.end.character
                                )
                                const selRange = new Range(
                                    s.selectionRange.start.line, s.selectionRange.start.character,
                                    s.selectionRange.end.line, s.selectionRange.end.character
                                )
                                const sym = new VscDocumentSymbol(
                                    s.name, s.detail || '', s.kind, range, selRange
                                )
                                if (s.children) sym.children = convert(s.children)
                                if (s.tags) sym.tags = s.tags
                                if (s.deprecated) sym.deprecated = true
                                return sym
                            })
                            symbolsCache.set(uri, convert(raw))
                        } catch {}
                        break
                    }
                    case SECTION_LINKS: {
                        if (!fresh) break
                        const links = []
                        let p = 0
                        while (p + 16 <= data.length) {
                            const startLine = data.readUInt32LE(p); p += 4
                            const startChar = data.readUInt32LE(p); p += 4
                            const endLine = data.readUInt32LE(p); p += 4
                            const endChar = data.readUInt32LE(p); p += 4
                            if (p + 2 > data.length) break
                            const targetLen = data.readUInt16LE(p); p += 2
                            const target = targetLen > 0 ? data.toString('utf8', p, p + targetLen) : undefined; p += targetLen
                            if (p + 2 > data.length) break
                            const tooltipLen = data.readUInt16LE(p); p += 2
                            const tooltip = tooltipLen > 0 ? data.toString('utf8', p, p + tooltipLen) : undefined; p += tooltipLen
                            const range = new Range(startLine, startChar, endLine, endChar)
                            const link = new VscDocumentLink(range, target ? Uri.parse(target) : undefined)
                            if (tooltip) link.tooltip = tooltip
                            links.push(link)
                        }
                        linksCache.set(uri, links)
                        const resolver = linksResolvers.get(uri)
                        if (resolver) {
                            linksResolvers.delete(uri)
                            resolver(links)
                        }
                        break
                    }
                    case SECTION_COLORS: {
                        if (!fresh) break
                        const colors = []
                        let p = 0
                        while (p + 32 <= data.length) {
                            const startLine = data.readUInt32LE(p); p += 4
                            const startChar = data.readUInt32LE(p); p += 4
                            const endLine = data.readUInt32LE(p); p += 4
                            const endChar = data.readUInt32LE(p); p += 4
                            const r = data.readFloatLE(p); p += 4
                            const g = data.readFloatLE(p); p += 4
                            const b = data.readFloatLE(p); p += 4
                            const a = data.readFloatLE(p); p += 4
                            colors.push(new ColorInformation(
                                new Range(startLine, startChar, endLine, endChar),
                                new Color(r, g, b, a)
                            ))
                        }
                        colorsCache.set(uri, colors)
                        break
                    }
                    case SECTION_CODE_LENSES: {
                        if (!fresh) break
                        const lenses = []
                        let p = 0
                        while (p + 12 <= data.length) {
                            const declLine = data.readUInt32LE(p); p += 4
                            const declChar = data.readUInt32LE(p); p += 4
                            const refCount = data.readUInt32LE(p); p += 4
                            const refs = []
                            for (let i = 0; i < refCount && p + 16 <= data.length; i++) {
                                const sl = data.readUInt32LE(p); p += 4
                                const sc = data.readUInt32LE(p); p += 4
                                const el = data.readUInt32LE(p); p += 4
                                const ec = data.readUInt32LE(p); p += 4
                                refs.push({uri, range: {start: {line: sl, character: sc}, end: {line: el, character: ec}}})
                            }
                            const title = refCount === 1 ? '1 reference' : `${refCount} references`
                            const pos = new Position(declLine, declChar)
                            lenses.push(new VscCodeLens(
                                new Range(pos, pos),
                                {title, command: 'editor.action.showReferences', arguments: [Uri.parse(uri), pos, refs.map(r => ({
                                    uri: Uri.parse(r.uri),
                                    range: new Range(r.range.start.line, r.range.start.character, r.range.end.line, r.range.end.character)
                                }))]}
                            ))
                        }
                        codeLensCache.set(uri, lenses)
                        codeLensChanged.fire()
                        break
                    }
                    case SECTION_TREE_URIS: {
                        let p = 0
                        const changedUris = []
                        const docTree = new Set()
                        while (p + 2 <= data.length) {
                            const uriLen = data.readUInt16LE(p); p += 2
                            if (p + uriLen > data.length) break
                            const peerUri = data.toString('utf8', p, p + uriLen); p += uriLen
                            docTree.add(peerUri)
                            if (!treeUris.has(peerUri)) {
                                treeUris.add(peerUri)
                                changedUris.push(Uri.parse(peerUri))
                            }
                        }
                        perDocTreeUris.set(uri, docTree)
                        if (changedUris.length > 0) {
                            fileDecoChanged.fire(changedUris)
                        }
                        break
                    }
                    // Future section types: just add cases here
                }
            }
        }

        /**
         * Build a TLV section: [u8 type][u32 LE length][...data]
         * @param {number} type
         * @param {Buffer} data
         * @returns {Buffer}
         */
        function _tlvSection(type, data) {
            const header = Buffer.alloc(5)
            header[0] = type
            header.writeUInt32LE(data.length, 1)
            return Buffer.concat([header, data])
        }

        /**
         * Enqueue a /document/update.
         *
         * - Правка всегда идёт в очередь.
         * - Если залочено → return, finally заберёт.
         * - Если нет → _flush() лочит, забирает, шлёт.
         * - В finally: есть очередь → не снимая лок шлём дальше.
         *              нет очереди → снимаем лок.
         */
        function _enqueueUpdate(uri, languageId, version, body) {
            const q = _queue.get(uri)
            if (q) {
                q.sections.push(body)
                q.version = version
            } else {
                _queue.set(uri, {version, languageId, sections: [body]})
            }
            if (_locked.get(uri)) return
            _flush(uri)
        }

        function _flush(uri) {
            const q = _queue.get(uri)
            if (!q) {
                _locked.set(uri, false)
                return
            }
            _locked.set(uri, true)
            _queue.delete(uri)

            const params = {uri, languageId: q.languageId, version: String(q.version), hints: 'ref,type'}
            const lastId = semanticResultId.get(uri)
            if (lastId !== undefined) params.lastResultId = String(lastId)

            client.http.postBinary('/document/update', params, Buffer.concat(q.sections)).then(buf => {
                if (buf.length >= 4) {
                    const echoedVersion = buf.readUInt32LE(0)
                    const fresh = echoedVersion === _docVersion.get(uri)
                    _applyBinaryResponse(uri, buf.slice(4), fresh)
                }
            }).catch(e => {
                console.error('document/update error:', e)
            }).finally(() => {
                _flush(uri)
            })
        }

        function _sendDidOpen(doc) {
            const key = doc.uri.toString()
            if (openedDocs.has(key)) return
            openedDocs.add(key)
            const ver = (_docVersion.get(key) || 0) + 1
            _docVersion.set(key, ver)

            if (doc.uri.scheme === 'file') {
                // Server reads the file from disk — no text over the wire.
                _enqueueUpdate(key, doc.languageId, ver, _tlvSection(SECTION_OPEN_URI, Buffer.alloc(0)))
            } else {
                // mpq:// or other schemes — server can't access the file,
                // send full text.
                const textBuf = Buffer.from(doc.getText(), 'utf8')
                _enqueueUpdate(key, doc.languageId, ver, _tlvSection(SECTION_FULL_TEXT, textBuf))
            }
        }

        /**
         * Adjust delta-encoded semantic tokens for a single content change.
         * Keeps the cached tokens in sync with the document so that
         * VS Code's delayed re-query returns correctly-positioned tokens
         * instead of stale pre-edit data.
         * @param {Uint32Array} data  delta-encoded [dLine, dChar, len, type, mod, …]
         * @param {import('vscode').TextDocumentContentChangeEvent} change
         * @returns {Uint32Array} adjusted copy
         */
        function _adjustSemanticTokens(data, change) {
            const sL = change.range.start.line, sC = change.range.start.character
            const eL = change.range.end.line, eC = change.range.end.character
            const parts = change.text.split('\n')
            const addedLines = parts.length - 1
            const lineDelta = addedLines - (eL - sL)
            const lastPartLen = parts[parts.length - 1].length

            const arr = new Uint32Array(data) // copy
            let prevLine = 0, prevChar = 0
            let prevNewLine = 0, prevNewChar = 0

            for (let i = 0; i + 4 <= arr.length; i += 5) {
                const dLine = arr[i], dChar = arr[i + 1]
                const absLine = prevLine + dLine
                const absChar = dLine > 0 ? dChar : prevChar + dChar

                let newLine = absLine, newChar = absChar

                if (absLine > eL || (absLine === eL && absChar >= eC)) {
                    // ── Token is AFTER the edit ─────────────────────
                    if (absLine === eL) {
                        newLine = sL + addedLines
                        newChar = (addedLines > 0 ? lastPartLen : sC + lastPartLen) + (absChar - eC)
                    } else {
                        newLine = absLine + lineDelta
                    }
                } else if (absLine > sL || (absLine === sL && absChar >= sC)) {
                    // ── Token is INSIDE the deleted range ───────────
                    // Collapse to edit start; server will fix it.
                    newLine = sL
                    newChar = sC
                }

                // Re-encode delta
                arr[i] = Math.max(0, newLine - prevNewLine)
                arr[i + 1] = (newLine > prevNewLine) ? newChar : Math.max(0, newChar - prevNewChar)

                prevLine = absLine; prevChar = absChar
                prevNewLine = newLine; prevNewChar = newChar
            }
            return arr
        }

        /**
         * Adjust cached InlayHint positions for a single content change.
         * VS Code does NOT auto-adjust InlayHint positions on edit,
         * so we must do it ourselves for instant visual feedback.
         * @param {import('vscode').InlayHint[]} hints
         * @param {import('vscode').TextDocumentContentChangeEvent} change
         * @returns {import('vscode').InlayHint[]}
         */
        function _adjustInlayHints(hints, change) {
            const sL = change.range.start.line, sC = change.range.start.character
            const eL = change.range.end.line, eC = change.range.end.character
            const parts = change.text.split('\n')
            const addedLines = parts.length - 1
            const deletedLines = eL - sL
            const lastPartLen = parts[parts.length - 1].length

            const result = []
            for (const hint of hints) {
                const L = hint.position.line, C = hint.position.character

                // Before change — keep as-is
                if (L < sL || (L === sL && C <= sC)) {
                    result.push(hint)
                    continue
                }

                // Inside deleted range — drop
                if (L < eL || (L === eL && C < eC)) continue

                // After change — adjust
                let newL, newC
                if (L === eL) {
                    newL = sL + addedLines
                    newC = (addedLines > 0 ? lastPartLen : sC + lastPartLen) + (C - eC)
                } else {
                    newL = L + addedLines - deletedLines
                    newC = C
                }

                const h = new InlayHint(new Position(newL, newC), hint.label, hint.kind)
                h.paddingLeft = hint.paddingLeft
                h.paddingRight = hint.paddingRight
                result.push(h)
            }
            return result
        }

        const docOpenDisposable = workspace.onDidOpenTextDocument(doc => {
            if (!SUPPORTED_LANGUAGES.has(doc.languageId)) return
            if (doc.uri.scheme !== 'file' && doc.uri.scheme !== 'mpq') return
            clientReady.then(() => _sendDidOpen(doc))
        })

        const docChangeDisposable = workspace.onDidChangeTextDocument(e => {
            const doc = e.document
            if (!SUPPORTED_LANGUAGES.has(doc.languageId)) return
            if (doc.uri.scheme !== 'file' && doc.uri.scheme !== 'mpq') return
            if (!openedDocs.has(doc.uri.toString())) {
                _sendDidOpen(doc)
                return
            }
            if (e.contentChanges.length === 0) return

            const uri = doc.uri.toString()

            // ── Bump document version ───────────────────────────────────
            const ver = (_docVersion.get(uri) || 0) + 1
            _docVersion.set(uri, ver)

            // ── Adjust caches so VS Code shows correct positions ────────
            // Sort changes bottom-to-top so each adjustment doesn't shift
            // positions of subsequent (higher) changes.  VS Code provides
            // ranges relative to the original document — processing from
            // bottom guarantees correctness for delta-encoded tokens.
            const changes = e.contentChanges.length === 1
                ? e.contentChanges
                : [...e.contentChanges].sort((a, b) =>
                    b.range.start.line - a.range.start.line
                    || b.range.start.character - a.range.start.character)

            let cached = semanticCache.get(uri)
            if (cached && cached.length > 0) {
                for (const change of changes) {
                    cached = _adjustSemanticTokens(cached, change)
                }
                semanticCache.set(uri, cached)
                semanticChanged.fire()
            }
            let hints = inlayHintsCache.get(uri)
            if (hints && hints.length > 0) {
                for (const change of changes) {
                    hints = _adjustInlayHints(hints, change)
                }
                inlayHintsCache.set(uri, hints)
                inlayHintsChanged.fire()
            }

            // ── Send edits through the serial queue ──────────────────────
            // Never abort — the queue guarantees all edits reach the server
            // in order.  If a request is in flight, edits accumulate and are
            // sent as a single batch when it completes.
            const sections = e.contentChanges.map(c => {
                const textBytes = Buffer.from(c.text, 'utf8')
                const data = Buffer.alloc(20 + textBytes.length)
                data.writeUInt32LE(c.range.start.line, 0)
                data.writeUInt32LE(c.range.start.character, 4)
                data.writeUInt32LE(c.range.end.line, 8)
                data.writeUInt32LE(c.range.end.character, 12)
                data.writeUInt32LE(textBytes.length, 16)
                textBytes.copy(data, 20)
                return _tlvSection(SECTION_CONTENT_CHANGE, data)
            })
            _enqueueUpdate(uri, doc.languageId, ver, Buffer.concat(sections))
        })

        /**
         * Shared cleanup when a document is no longer open.
         * Called from onDidCloseTextDocument AND onDidChangeTabs fallback.
         * @param {string} key  document URI string
         */
        function _handleDocClose(key) {
            if (!openedDocs.has(key)) return
            openedDocs.delete(key)
            _docVersion.delete(key)
            _queue.delete(key)
            _locked.delete(key)
            semanticBase.delete(key)
            semanticResultId.delete(key)
            inlayHintsCache.delete(key)
            foldingCache.delete(key)
            symbolsCache.delete(key)
            linksCache.delete(key)
            linksResolvers.delete(key)
            colorsCache.delete(key)
            codeLensCache.delete(key)
            const closedUri = Uri.parse(key)
            diagnosticCollection.delete(closedUri)

            // ── Rebuild tree URIs from remaining open docs ──
            perDocTreeUris.delete(key)

            // Rebuild treeUris from remaining per-doc sets
            const newTreeUris = new Set()
            for (const [, uris] of perDocTreeUris) {
                for (const u of uris) newTreeUris.add(u)
            }
            const removedTreeUris = []
            for (const u of treeUris) {
                if (!newTreeUris.has(u)) removedTreeUris.push(Uri.parse(u))
            }
            treeUris.clear()
            for (const u of newTreeUris) treeUris.add(u)

            // Fire decoration change for all affected URIs
            const staleUris = [...removedTreeUris, closedUri]
            if (staleUris.length > 0) fileDecoChanged.fire(staleUris)

            client.http.post('/document/close', {
                textDocument: {uri: key}
            }).catch(e => console.error('document/close error:', e))
        }

        const docCloseDisposable = workspace.onDidCloseTextDocument(doc => {
            _handleDocClose(doc.uri.toString())
        })

        // ── Fallback: detect tab closes that VS Code doesn't surface
        //    as onDidCloseTextDocument (preview tabs, side-panel closes, etc.) ──
        const tabCloseDisposable = window.tabGroups.onDidChangeTabs(event => {
            for (const tab of event.closed) {
                const inputUri = tab.input?.uri
                if (!inputUri) continue
                const key = inputUri.toString()
                if (!openedDocs.has(key)) continue

                // Check if any remaining tab still shows this URI
                const stillOpen = window.tabGroups.activeTabGroup.tabs.some(t => t.input?.uri?.toString() === key)
                if (stillOpen) continue

                _handleDocClose(key)
            }
        })


        /** Helper: open-custom-document boilerplate */
        const openCustomDocument = uri => ({uri, dispose: () => {}})

        // ── MPQ virtual filesystem ───────────────────────────────
        const mpqProvider = new MpqFileSystemProvider(() => client, clientReady)

        context.subscriptions.push(
            diagnosticCollection,
            fileDecorationProvider,
            fileDecoChanged,
            inlayHintsProvider,
            inlayHintsChanged,
            semanticTokensProvider,
            semanticChanged,
            foldingProvider,
            symbolProvider,
            linkProvider,
            colorProvider,
            codeLensProvider,
            codeLensChanged,
            renameProvider,
            completionProvider,
            hoverProvider,
            definitionProvider,
            referencesProvider,
            highlightProvider,
            formattingProvider,
            signatureHelpProvider,
            codeActionProvider,
            callHierarchyProvider,
            typeHierarchyProvider,
            willRenameDisposable,
            didRenameDisposable,
            docOpenDisposable,
            docChangeDisposable,
            docCloseDisposable,
            tabCloseDisposable,

            // TaskProvider for jass-hook tasks (hooks are created programmatically)
            tasks.registerTaskProvider('jass-hook', {
                provideTasks() { return [] },
                resolveTask(_task) { return undefined }
            }),

            workspace.registerFileSystemProvider('mpq', mpqProvider, {
                isCaseSensitive: false,
                isReadonly: true,
            }),

            commands.registerCommand('mpq.browse', async (resourceUri) => {
                // resourceUri may come from explorer context menu or be undefined
                let archivePath
                if (resourceUri && resourceUri.fsPath) {
                    archivePath = resourceUri.fsPath
                } else {
                    const editor = window.activeTextEditor
                    if (editor && editor.document.uri.fsPath.match(/\.(?:w3[xmn]|mpq)$/i)) {
                        archivePath = editor.document.uri.fsPath
                    }
                }
                if (!archivePath) {
                    window.showWarningMessage('No MPQ archive selected.')
                    return
                }

                // Wait for server to be ready before mounting.
                await clientReady

                const rootUri = MpqFileSystemProvider.makeUri(archivePath)
                const name = archivePath.split(/[\\/]/).pop() || 'MPQ Archive'

                // Add as a workspace folder so it appears in the explorer
                const existing = (workspace.workspaceFolders || [])
                    .findIndex(f => f.uri.toString() === rootUri.toString())
                if (existing === -1) {
                    workspace.updateWorkspaceFolders(
                        (workspace.workspaceFolders || []).length,
                        0,
                        {uri: rootUri, name: `📦 ${name}`}
                    )
                }
            }),

            commands.registerCommand('mpq.openFile', async (archivePath, internalPath) => {
                if (!archivePath || !internalPath) {
                    window.showWarningMessage('Missing archive path or internal path.')
                    return
                }
                await clientReady
                const uri = MpqFileSystemProvider.makeUri(archivePath, internalPath)
                await commands.executeCommand('vscode.open', uri)
            }),

            commands.registerCommand('mpq.extractHere', async (resourceUri) => {
                if (!resourceUri || resourceUri.scheme !== 'mpq') return

                const archivePath = MpqFileSystemProvider.decodeAuthority(resourceUri.authority)
                const internalPath = resourceUri.path.replace(/^\//, '')
                if (!internalPath) {
                    window.showWarningMessage('Cannot extract the archive root.')
                    return
                }

                // Determine the extraction base directory:
                // If .w3x/.w3m/.w3n/.mpq is a file → extract next to it
                // If it's a directory → extract into that directory
                let baseDir
                try {
                    const stat = fs.statSync(archivePath)
                    if (stat.isDirectory()) {
                        baseDir = archivePath
                    } else {
                        baseDir = path.dirname(archivePath)
                    }
                } catch {
                    baseDir = path.dirname(archivePath)
                }

                await _doExtract(resourceUri, archivePath, internalPath, baseDir, mpqProvider)
            }),

            commands.registerCommand('mpq.extractTo', async (resourceUri) => {
                if (!resourceUri || resourceUri.scheme !== 'mpq') return

                const archivePath = MpqFileSystemProvider.decodeAuthority(resourceUri.authority)
                const internalPath = resourceUri.path.replace(/^\//, '')
                if (!internalPath) {
                    window.showWarningMessage('Cannot extract the archive root.')
                    return
                }

                const selected = await window.showOpenDialog({
                    canSelectFolders: true,
                    canSelectFiles: false,
                    canSelectMany: false,
                    openLabel: 'Extract To',
                })
                if (!selected || selected.length === 0) return

                const baseDir = selected[0].fsPath
                await _doExtract(resourceUri, archivePath, internalPath, baseDir, mpqProvider)
            }),
        )

        /** Helper: register a binary-file custom editor that talks to the server */
        function binaryEditor(viewType, resolver) {
            return window.registerCustomEditorProvider(
                viewType,
                {
                    openCustomDocument,
                    async resolveCustomEditor(document, webviewPanel, _token) {
                        webviewPanel.webview.options = {
                            enableScripts: true,
                            localResourceRoots: [
                                Uri.joinPath(context.extensionUri, 'extension'),
                            ]
                        }
                        await clientReady

                        // For mpq:// URIs the server cannot read the file
                        // directly (it only handles file:// paths). Extract the
                        // file content via the virtual filesystem, write it to a
                        // temp file, and pass the temp-file document to the resolver.
                        if (document.uri.scheme === 'mpq') {
                            const fs = require('fs')
                            const os = require('os')
                            const content = await workspace.fs.readFile(document.uri)
                            const fname = document.uri.path.split('/').pop() || 'temp'
                            const tmpDir = path.join(os.tmpdir(), `vscode-mpq-${Date.now()}`)
                            fs.mkdirSync(tmpDir, {recursive: true})
                            const tmpPath = path.join(tmpDir, fname)
                            fs.writeFileSync(tmpPath, Buffer.from(content))

                            const tmpDoc = {
                                uri: Uri.file(tmpPath),
                                _mpqArchivePath: MpqFileSystemProvider.decodeAuthority(document.uri.authority),
                                dispose() {}
                            }
                            try {
                                return await resolver(tmpDoc, webviewPanel, _token, client, context.extensionUri, getBinaryServer)
                            } finally {
                                try { fs.unlinkSync(tmpPath) } catch {}
                                try { fs.rmdirSync(tmpDir) } catch {}
                            }
                        }

                        return resolver(document, webviewPanel, _token, client, context.extensionUri, getBinaryServer)
                    }
                },
                {
                    webviewOptions: {retainContextWhenHidden: true},
                    supportsMultipleEditorsPerDocument: false
                }
            )
        }

        context.subscriptions.push(
            binaryEditor('blp.preview', resolveBlpEditor),
            binaryEditor('mdx.preview', resolveMapEditor),
            binaryEditor('doo.preview', resolveMapEditor),
            binaryEditor('w3i.preview', resolveMapEditor),
            binaryEditor('mapEditor', resolveMapEditor),

            // SLK table editor (text-based — uses CustomTextEditorProvider for undo/redo)
            window.registerCustomEditorProvider(
                'slk.preview',
                {
                    async resolveCustomTextEditor(document, webviewPanel, token) {
                        webviewPanel.webview.options = {enableScripts: true}
                        await clientReady
                        return resolveSlkEditor(document, webviewPanel, token, client, context)
                    }
                },
                {
                    webviewOptions: {retainContextWhenHidden: true},
                    supportsMultipleEditorsPerDocument: false
                }
            ),

            // SLK: Open as Table (from text editor)
            commands.registerCommand('slk.openTable', () => {
                const editor = window.activeTextEditor
                if (editor && editor.document.languageId === 'slk') {
                    commands.executeCommand('vscode.openWith', editor.document.uri, 'slk.preview')
                }
            }),

            // SLK: Open as Text (from table editor)
            commands.registerCommand('slk.openText', () => {
                // The active custom editor resource URI is available via activeTextEditor
                // but when a custom editor is active, we need to use the document URI.
                // VS Code exposes the resource via the tab API.
                const tab = window.tabGroups.activeTabGroup.activeTab
                if (tab && tab.input && tab.input.uri) {
                    commands.executeCommand('vscode.openWith', tab.input.uri, 'default')
                }
            }),


            // Import Graph panel
            commands.registerCommand('importGraph.show', () => {
                showImportGraph(client, context.extensionUri, context)
            }),

            // Call Graph panel
            commands.registerCommand('callGraph.show', () => {
                showCallGraph(client, context.extensionUri, context)
            }),

            // Type Graph panel
            commands.registerCommand('typeGraph.show', () => {
                showTypeGraph(client, context.extensionUri, context)
            }),

            // Export Table panel
            commands.registerCommand('exportTable.show', () => {
                showExports(client, context.extensionUri, context)
            }),

            // Diagnostic Summary panel
            commands.registerCommand('diagnosticSummary.show', () => {
                showDiagnostics(client, context.extensionUri, context)
            }),


            // Rescan all files (SSE with real progress)
            commands.registerCommand('rescan.execute', async () => {
                const editor = window.activeTextEditor
                if (!editor) {
                    window.showWarningMessage('No active editor.')
                    return
                }
                const uri = editor.document.uri.toString()

                let result
                try {
                    result = await window.withProgress(
                        {
                            location: ProgressLocation.Notification,
                            title: 'Rescanning…',
                            cancellable: false
                        },
                        async (progress) => {
                            const res = await _consumeRescanWS(client, uri, progress)
                            if (res && res.busy) return res

                            // ── Re-request document/update only for open files ──
                            if (res && res.entries && res.entries.length > 0) {
                                progress.report({message: 'Refreshing diagnostics…'})
                                for (const entry of res.entries) {
                                    if (!openedDocs.has(entry.uri)) continue
                                    const ver = (_docVersion.get(entry.uri) || 0) + 1
                                    _docVersion.set(entry.uri, ver)
                                    _enqueueUpdate(entry.uri, entry.languageId || 'jass', ver,
                                        _tlvSection(SECTION_OPEN_URI, Buffer.alloc(0)))
                                }

                                // Refresh file decorations for all rescanned URIs
                                fileDecoChanged.fire(res.entries.map(e => Uri.parse(e.uri)))
                            }
                            return res
                        }
                    )
                } catch (e) {
                    window.showErrorMessage(`Rescan error: ${e.message}`)
                    return
                }

                if (!result) {
                    window.showErrorMessage('✗ Rescan failed')
                    return
                }
                if (result.busy) {
                    window.showWarningMessage('Rescan is already in progress.')
                    return
                }

                if (result.ok) {
                    const action = await window.showInformationMessage(
                        `✓ ${result.message}`, 'Show Files'
                    )
                    if (action === 'Show Files') {
                        const ch = window.createOutputChannel('JASS Rescan')
                        ch.clear()
                        if (result.root) ch.appendLine(`Root: ${result.root}`)
                        if (result.files && result.files.length > 0) {
                            for (const f of result.files) ch.appendLine(`  ${f}`)
                        }
                        ch.show()
                    }
                } else if (result.errors && result.errors.length > 0) {
                    const action = await window.showErrorMessage(
                        `✗ ${result.message.split('\n')[0]}`, 'Show Details'
                    )
                    if (action === 'Show Details') {
                        const ch = window.createOutputChannel('JASS Rescan')
                        ch.clear()
                        if (result.root) ch.appendLine(`Root: ${result.root}`)
                        if (result.files && result.files.length > 0) {
                            for (const f of result.files) ch.appendLine(`  ${f}`)
                        }
                        if (result.errors.length > 0) {
                            ch.appendLine('')
                            ch.appendLine('Errors:')
                            for (const e of result.errors) ch.appendLine(`  • ${e}`)
                        }
                        ch.show()
                    }
                } else {
                    window.showErrorMessage(`✗ ${result.message}`)
                }
            }),

            // Build
            commands.registerCommand('build.execute', async () => {
                const editor = window.activeTextEditor
                if (!editor) {
                    window.showWarningMessage('No active editor.')
                    return
                }
                const uri = editor.document.uri.toString()
                try {
                    // 1. Resolve hook commands (expanded, with cwd).
                    const hooks = await client.sendRequest('build/hooks', {uri})

                    // 2. Run build-before hook in VS Code terminal (if present).
                    if (hooks && hooks.before_cmd) {
                        const exitCode = await _runHookTask('build-before', hooks.before_cmd, hooks.cwd)
                        if (exitCode !== 0) {
                            window.showErrorMessage(`✗ build-before exited with code ${exitCode}`)
                            return
                        }
                    }

                    // 3. Execute the actual build.
                    const result = await client.sendRequest('build/execute', {uri})
                    if (!result) {
                        window.showErrorMessage('✗ Build failed')
                        return
                    }

                    if (result.ok) {
                        window.showInformationMessage(`✓ ${result.message}`)
                        // 4. Run build-after hook in VS Code terminal (if present).
                        if (hooks && hooks.after_cmd) {
                            const exitCode = await _runHookTask('build-after', hooks.after_cmd, hooks.cwd)
                            if (exitCode !== 0) {
                                window.showWarningMessage(`⚠ build-after exited with code ${exitCode}`)
                            }
                        }
                    } else {
                        window.showErrorMessage(`✗ ${result.message}`)
                    }
                } catch (e) {
                    window.showErrorMessage(`Build error: ${e.message}`)
                }
            }),

            // Restart server
            commands.registerCommand('jass.restartServer', async () => {
                if (!client) {
                    window.showWarningMessage('Server is not running.')
                    return
                }
                await window.withProgress(
                    {
                        location: ProgressLocation.Notification,
                        title: 'Restarting JASS server…',
                        cancellable: false
                    },
                    async () => {
                        try {
                            await client.restart()
                            // Re-open all tracked documents
                            openedDocs.clear()
                            for (const doc of workspace.textDocuments) {
                                if (SUPPORTED_LANGUAGES.has(doc.languageId) && (doc.uri.scheme === 'file' || doc.uri.scheme === 'mpq')) {
                                    _sendDidOpen(doc)
                                }
                            }
                            window.showInformationMessage('✓ JASS server restarted.')
                        } catch (e) {
                            window.showErrorMessage(`Failed to restart server: ${e.message}`)
                        }
                    }
                )
            }),

            // Open imported file (quick fix)
            commands.registerCommand('jass.openImportedFile', async (uriStr) => {
                if (!uriStr) return
                try {
                    const uri = Uri.parse(uriStr)
                    const doc = await workspace.openTextDocument(uri)
                    await window.showTextDocument(doc, {preview: true})
                } catch (e) {
                    window.showErrorMessage(`Cannot open file: ${e.message}`)
                }
            }),

            // UjAPI download
            commands.registerCommand('ujapi.download', async (uriStr, relPath) => {
                if (!uriStr || !relPath) {
                    window.showWarningMessage('Missing URI or path for UjAPI download.')
                    return
                }
                await window.withProgress(
                    {
                        location: ProgressLocation.Notification,
                        title: 'Downloading UjAPI common.j…',
                        cancellable: false
                    },
                    async () => {
                        try {
                            const result = await client.sendRequest('ujapi/download', {
                                uri: uriStr,
                                path: relPath
                            })
                            if (result && result.ok) {
                                window.showInformationMessage(`✓ ${result.message}`)
                            } else {
                                window.showErrorMessage(`✗ ${result ? result.message : 'Download failed'}`)
                            }
                        } catch (e) {
                            window.showErrorMessage(`UjAPI download error: ${e.message}`)
                        }
                    }
                )
            })
        )

        await clientReady
    },

    async deactivate() {
        if (!client) return
        await client.stop()
        client = undefined
    }
}
