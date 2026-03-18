// noinspection JSUnusedGlobalSymbols
// noinspection NpmUsedModulesInstalled
const {
    window,
    Uri, ExtensionMode, commands, ProgressLocation, workspace
} = require('vscode')

const {LanguageClient, Trace} = require('vscode-languageclient')
const {resolveBlpEditor} = require('./resolveBlpEditor.js')
const {resolveDooEditor} = require('./resolveDooEditor.js')
const {resolveW3iEditor} = require('./resolveW3iEditor.js')
const {onDidChangeStateMessage} = require('./onDidChangeStateMessage.js')
const {showImportGraph} = require('./importGraphPanel.js')
const {showCallGraph} = require('./callGraphPanel.js')
const {showTypeGraph} = require('./typeGraphPanel.js')
const {MpqFileSystemProvider} = require('./mpqFileSystemProvider.js')
const {resolveMpqEditor} = require('./resolveMpqEditor.js')

const path = require('path')

/**
 * @typedef {import('vscode').Uri} Uri
 * @typedef {import('vscode-languageclient').LanguageClientOptions}
 */

/** @type {LanguageClient} */ let client

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
        const binUri = Uri.file(binPath)

        const options = context.extensionMode === ExtensionMode.Production || true ? {
            command: binUri.fsPath.toString(),
        } : {
            command: process.execPath, // node
            args: [path.join(context.extensionPath, 'lsp-proxy.js')],
            options: {
                env: {
                    ...process.env,
                    REAL_LSP_PATH: binPath,
                    RUST_LOG: 'debug'
                }
            }
        }

        client = new LanguageClient(
            'JassTreeSitterRustLsp',
            'JassTreeSitterRustLspClient',
            options,
            {
                progressOnInitialization: true,
                initializationOptions: {},
                documentSelector: [
                    {scheme: 'file', language: 'bni'},
                    {scheme: 'file', language: 'jass'},
                    {scheme: 'file', language: 'angelscript'},
                ],
                outputChannelName: 'JASS-Tree-Sitter-Rust Logs',
                traceOutputChannel: window.createOutputChannel('JASS-Tree-Sitter-Rust Trace'),
                trace: Trace.Verbose
            }
        )

        client.onNotification('window/logMessage', params => {
            console.log(`${params.message}`)
        })

        client.onDidChangeState(({oldState, newState}) => {
            const message = onDidChangeStateMessage(oldState, newState)
            if (message) {
                window.showWarningMessage(message)
            }
        })

        // Start the client early so custom editors can send requests.
        const clientReady = client.start().catch(err => {
            window.showErrorMessage(`❌ Failed to start LSP client:\n\n${err.message}`)
        })

        /** Helper: open-custom-document boilerplate */
        const openCustomDocument = uri => ({uri, dispose: () => {}})

        // ── MPQ virtual filesystem ───────────────────────────────
        const mpqProvider = new MpqFileSystemProvider(() => client, clientReady)

        context.subscriptions.push(
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

                // Wait for LSP to be ready before mounting.
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
            })
        )

        /** Helper: register a binary-file custom editor that talks to LSP */
        function binaryEditor(viewType, resolver) {
            return window.registerCustomEditorProvider(
                viewType,
                {
                    openCustomDocument,
                    async resolveCustomEditor(document, webviewPanel, _token) {
                        webviewPanel.webview.options = {enableScripts: true}
                        await clientReady
                        return resolver(document, webviewPanel, _token, client)
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
            binaryEditor('doo.preview', resolveDooEditor),
            binaryEditor('w3i.preview', resolveW3iEditor),
            binaryEditor('mpq.preview', resolveMpqEditor),

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
                    const result = await client.sendRequest('build/execute', {uri})
                    if (result && result.ok) {
                        window.showInformationMessage(`✓ ${result.message}`)
                    } else {
                        window.showErrorMessage(`✗ ${result ? result.message : 'Build failed'}`)
                    }
                } catch (e) {
                    window.showErrorMessage(`Build error: ${e.message}`)
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
