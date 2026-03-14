// noinspection JSUnusedGlobalSymbols
// noinspection NpmUsedModulesInstalled
const {
    window,
    Uri, ExtensionMode, commands
} = require('vscode')

const {LanguageClient, Trace} = require('vscode-languageclient')
const {resolveBlpEditor} = require('./resolveBlpEditor.js')
const {onDidChangeStateMessage} = require('./onDidChangeStateMessage.js')
const {showImportGraph} = require('./importGraphPanel.js')
const {showCallGraph} = require('./callGraphPanel.js')

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
            command: binUri.fsPath,
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

        context.subscriptions.push(
            // https://code.visualstudio.com/api/references/vscode-api#window.registerCustomEditorProvider
            window.registerCustomEditorProvider(
                'blp.preview',
                {
                    openCustomDocument(uri) {
                        return {
                            uri,
                            dispose: () => {
                            }
                        }
                    },
                    async resolveCustomEditor(document, webviewPanel, _token) {
                        webviewPanel.webview.options = {
                            enableScripts: true
                        }
                        return resolveBlpEditor(document, webviewPanel, _token, client)
                    }
                },
                {
                    webviewOptions: {
                        retainContextWhenHidden: true,
                    },
                    supportsMultipleEditorsPerDocument: false
                }
            ),

            // Import Graph panel
            commands.registerCommand('importGraph.show', () => {
                showImportGraph(client, context.extensionUri)
            }),

            // Call Graph panel
            commands.registerCommand('callGraph.show', () => {
                showCallGraph(client, context.extensionUri)
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
            })
        )

        client.onDidChangeState(({oldState, newState}) => {
            const message = onDidChangeStateMessage(oldState, newState)
            if (message) {
                window.showWarningMessage(message)
            }
        })

        try {
            await client.start()
        } catch (err) {
            window.showErrorMessage(`❌ Failed to start LSP client:\n\n${err.message}`)
        }
    },

    async deactivate() {
        if (!client) return
        await client.stop()
        client = undefined
    }
}
