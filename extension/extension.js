// noinspection JSUnusedGlobalSymbols
// noinspection NpmUsedModulesInstalled
const {
    window,
    Uri, commands, ProgressLocation, workspace,
    languages, InlayHint, InlayHintKind, Position, Range, Location, EventEmitter,
    ShellExecution, Task, TaskScope, tasks,
    DocumentSymbol: VscDocumentSymbol, SymbolKind,
    FoldingRange, FoldingRangeKind,
    Diagnostic: VscDiagnostic, DiagnosticSeverity, DiagnosticTag,
    SemanticTokensLegend,
    DocumentLink: VscDocumentLink,
    ColorInformation, Color, ColorPresentation: VscColorPresentation, TextEdit,
} = require('vscode')

const {ServerClient} = require('./serverClient.js')
const {resolveBlpEditor} = require('./mapEditor/resolveBlpEditor.js')
const {resolveMapEditor} = require('./mapEditor/index.js')
const {showImportGraph} = require('./importGraphPanel.js')
const {showCallGraph} = require('./callGraphPanel.js')
const {showTypeGraph} = require('./typeGraphPanel.js')

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
/** @type {import('vscode').Disposable[]} */
const fileWatcherDisposables = []

/** Map severity (1-4) to VS Code DiagnosticSeverity */
function _mapSeverity(sev) {
    switch (sev) {
        case 1: return DiagnosticSeverity.Error
        case 2: return DiagnosticSeverity.Warning
        case 3: return DiagnosticSeverity.Information
        case 4: return DiagnosticSeverity.Hint
        default: return DiagnosticSeverity.Information
    }
}

/** Map SymbolKind (1-26) to VS Code SymbolKind */
function _mapSymbolKind(kind) {
    // SymbolKind values map 1:1 to VS Code SymbolKind
    return kind || SymbolKind.Object
}

/** Recursively map a server DocumentSymbol to a VS Code DocumentSymbol */
function _mapSymbol(s) {
    const range = new Range(
        s.range.start.line, s.range.start.character,
        s.range.end.line, s.range.end.character
    )
    const selRange = new Range(
        s.selectionRange.start.line, s.selectionRange.start.character,
        s.selectionRange.end.line, s.selectionRange.end.character
    )
    const sym = new VscDocumentSymbol(
        s.name,
        s.detail || '',
        _mapSymbolKind(s.kind),
        range,
        selRange
    )
    if (s.tags) {
        sym.tags = s.tags
    }
    if (s.deprecated) {
        sym.tags = [1] // SymbolTag.Deprecated = 1
    }
    if (s.children && s.children.length > 0) {
        sym.children = s.children.map(c => _mapSymbol(c))
    }
    return sym
}

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

        // ── Supported languages for document sync ─────────────────────
        const SUPPORTED_LANGUAGES = new Set(['bni', 'jass', 'angelscript', 'wts', 'slk'])

        // ── Diagnostics collection ────────────────────────────────────
        const diagnosticCollection = languages.createDiagnosticCollection('jass')

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

        /** @type {Map<string, import('vscode').ColorInformation[]>} */
        const colorsCache = new Map()

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

        // ── Handle unified custom/parseResult notification ────────────
        client.onNotification('custom/parseResult', (params) => {
            const uriStr = params.uri

            // Semantic tokens
            semanticCache.set(uriStr, params.semanticTokens || [])
            semanticChanged.fire()

            // Diagnostics
            const diags = (params.diagnostics || []).map(d => {
                const range = new Range(
                    d.range.start.line, d.range.start.character,
                    d.range.end.line, d.range.end.character
                )
                const diag = new VscDiagnostic(range, d.message, _mapSeverity(d.severity))
                if (d.source) diag.source = d.source
                if (d.code != null) {
                    if (d.codeDescription && d.codeDescription.href) {
                        diag.code = {value: d.code, target: Uri.parse(d.codeDescription.href)}
                    } else {
                        diag.code = d.code
                    }
                }
                if (d.tags) {
                    diag.tags = d.tags.map(t =>
                        t === 1 ? DiagnosticTag.Unnecessary
                            : t === 2 ? DiagnosticTag.Deprecated
                                : t
                    )
                }
                if (d.relatedInformation) {
                    diag.relatedInformation = d.relatedInformation.map(ri => ({
                        location: new Location(
                            Uri.parse(ri.location.uri),
                            new Range(
                                ri.location.range.start.line, ri.location.range.start.character,
                                ri.location.range.end.line, ri.location.range.end.character,
                            )
                        ),
                        message: ri.message,
                    }))
                }
                return diag
            })
            diagnosticCollection.set(Uri.parse(uriStr), diags)

            // Inlay hints
            const vscHints = (params.inlayHints || []).map(h => {
                const hint = new InlayHint(
                    new Position(h.position.line, h.position.character),
                    h.label,
                    h.kind === 1 ? InlayHintKind.Type
                        : h.kind === 2 ? InlayHintKind.Parameter
                            : undefined
                )
                if (h.paddingLeft != null) hint.paddingLeft = h.paddingLeft
                if (h.paddingRight != null) hint.paddingRight = h.paddingRight
                return hint
            })
            inlayHintsCache.set(uriStr, vscHints)
            inlayHintsChanged.fire()

            // Folding ranges
            const folds = (params.folding || []).map(f => {
                const kind = f.kind === 'comment' ? FoldingRangeKind.Comment
                    : f.kind === 'imports' ? FoldingRangeKind.Imports
                        : f.kind === 'region' ? FoldingRangeKind.Region
                            : undefined
                return new FoldingRange(f.startLine, f.endLine, kind)
            })
            foldingCache.set(uriStr, folds)

            // Document symbols
            const symbols = (params.symbols || []).map(s => _mapSymbol(s))
            symbolsCache.set(uriStr, symbols)

            // Document links
            const links = (params.documentLinks || []).map(l => {
                const range = new Range(
                    l.range.start.line, l.range.start.character,
                    l.range.end.line, l.range.end.character
                )
                const link = new VscDocumentLink(range, l.target ? Uri.parse(l.target) : undefined)
                if (l.tooltip) link.tooltip = l.tooltip
                return link
            })
            linksCache.set(uriStr, links)

            // Colors
            const colors = (params.colors || []).map(c => {
                const range = new Range(
                    c.range.start.line, c.range.start.character,
                    c.range.end.line, c.range.end.character
                )
                return new ColorInformation(
                    range,
                    new Color(c.color.red, c.color.green, c.color.blue, c.color.alpha)
                )
            })
            colorsCache.set(uriStr, colors)
        })


        // ── Binary HTTP server (parallel data channel) ───────────────
        // Server info (port + token) is available immediately after start().
        /** @returns {{port: number, token: string} | null} */
        function getBinaryServer() { return client.getServerInfo() }

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
                    const data = semanticCache.get(document.uri.toString())
                    if (!data || data.length === 0) return undefined
                    return {data: new Uint32Array(data)}
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
            provideDocumentLinks(document) {
                return linksCache.get(document.uri.toString()) || []
            }
        })

        const colorProvider = languages.registerColorProvider(allSelector, {
            provideDocumentColors(document) {
                return colorsCache.get(document.uri.toString()) || []
            },
            provideColorPresentations(color, ctx) {
                const uri = ctx.document.uri.toString()
                return client.sendRequest('color/presentation', {
                    textDocument: {uri},
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

        // ── Start the client and perform document sync ────────────────
        const clientReady = client.start().catch(err => {
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

        function _sendDidOpen(doc) {
            const key = doc.uri.toString()
            if (openedDocs.has(key)) return
            openedDocs.add(key)
            client.sendNotification('document/open', {
                textDocument: {
                    uri: key,
                    languageId: doc.languageId,
                    version: doc.version,
                    text: doc.getText(),
                }
            })
        }

        const docOpenDisposable = workspace.onDidOpenTextDocument(doc => {
            if (!SUPPORTED_LANGUAGES.has(doc.languageId)) return
            if (doc.uri.scheme !== 'file' && doc.uri.scheme !== 'mpq') return
            _sendDidOpen(doc)
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

            // Cancel all in-flight requests for this URI — they're working
            // with stale data.  The server does the same on its side, but
            // cancelling client-side avoids waiting for doomed responses.
            client.cancelUri(doc.uri.toString())

            client.sendNotification('document/change', {
                textDocument: {
                    uri: doc.uri.toString(),
                    version: doc.version,
                },
                contentChanges: e.contentChanges.map(c => ({
                    range: {
                        start: {line: c.range.start.line, character: c.range.start.character},
                        end: {line: c.range.end.line, character: c.range.end.character},
                    },
                    text: c.text,
                })),
            })
        })

        const docCloseDisposable = workspace.onDidCloseTextDocument(doc => {
            const key = doc.uri.toString()
            if (!openedDocs.has(key)) return
            openedDocs.delete(key)
            client.sendNotification('document/close', {
                textDocument: {uri: key}
            })
        })

        // ── File watchers ─────────────────────────────────────────────
        // Handle server's watchers/register request for file watchers
        client.onNotification('watchers/register', (params) => {
            if (!params || !params.registrations) return
            for (const reg of params.registrations) {
                if (reg.method !== 'files/changed') continue
                const watchers = reg.registerOptions?.watchers || []
                for (const w of watchers) {
                    const watcher = workspace.createFileSystemWatcher(w.globPattern)
                    const sendEvent = (uri, type) => {
                        client.sendNotification('files/changed', {
                            changes: [{uri: uri.toString(), type}]
                        })
                    }
                    if (!w.kind || w.kind & 1) watcher.onDidCreate(uri => sendEvent(uri, 1))
                    if (!w.kind || w.kind & 2) watcher.onDidChange(uri => sendEvent(uri, 2))
                    if (!w.kind || w.kind & 4) watcher.onDidDelete(uri => sendEvent(uri, 3))
                    fileWatcherDisposables.push(watcher)
                }
            }
        })

        /** Helper: open-custom-document boilerplate */
        const openCustomDocument = uri => ({uri, dispose: () => {}})

        // ── MPQ virtual filesystem ───────────────────────────────
        const mpqProvider = new MpqFileSystemProvider(() => client, clientReady)

        context.subscriptions.push(
            diagnosticCollection,
            inlayHintsProvider,
            inlayHintsChanged,
            semanticTokensProvider,
            semanticChanged,
            foldingProvider,
            symbolProvider,
            linkProvider,
            colorProvider,
            docOpenDisposable,
            docChangeDisposable,
            docCloseDisposable,

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


            // Rescan all files
            commands.registerCommand('rescan.execute', async () => {
                const editor = window.activeTextEditor
                if (!editor) {
                    window.showWarningMessage('No active editor.')
                    return
                }
                const uri = editor.document.uri.toString()
                await window.withProgress(
                    {
                        location: ProgressLocation.Notification,
                        title: 'Rescanning all files…',
                        cancellable: false
                    },
                    async () => {
                        try {
                            const result = await client.sendRequest('rescan/execute', {uri})
                            if (result && result.ok) {
                                window.showInformationMessage(`↻ ${result.message}`)
                            } else if (result && result.errors && result.errors.length > 0) {
                                const summary = `✗ Rescanned ${result.message.split('\n')[0]}`
                                const action = await window.showErrorMessage(summary, 'Show Details')
                                if (action === 'Show Details') {
                                    const ch = window.createOutputChannel('JASS Rescan')
                                    ch.clear()
                                    ch.appendLine('Rescan errors:')
                                    for (const e of result.errors) {
                                        ch.appendLine(`  • ${e}`)
                                    }
                                    ch.show()
                                }
                            } else {
                                window.showErrorMessage(`✗ ${result ? result.message : 'Rescan failed'}`)
                            }
                        } catch (e) {
                            window.showErrorMessage(`Rescan error: ${e.message}`)
                        }
                    }
                )
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
        for (const d of fileWatcherDisposables) d.dispose()
        await client.stop()
        client = undefined
    }
}
