// noinspection JSUnusedGlobalSymbols

// noinspection NpmUsedModulesInstalled
const {
    workspace,
    window,
    Uri, ExtensionMode
} = require('vscode')

const {LanguageClient, Trace} = require('vscode-languageclient')

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
                binName += 'win.exe'
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

        const binPath = path.join(context.extensionPath, 'dist', binName)
        const binUri = Uri.file(binPath)

        try {
            await workspace.fs.stat(binUri)
        } catch (error) {
            window.showErrorMessage(`LSP binary not found:\n${binUri.fsPath}\n\n${error.message}`)
            return
        }

        const options = context.extensionMode === ExtensionMode.Production ? {
            command: binUri.fsPath,
        } : {
            command: process.execPath, // node
            args: [path.join(context.extensionPath, 'lsp-proxy.js')],
            options: {
                env: {
                    ...process.env,
                    REAL_LSP_PATH: binPath
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
                    {
                        scheme: 'file',
                        language: 'lua',
                    },
                ],
                outputChannelName: 'JASS-Tree-Sitter-Rust Logs',
                traceOutputChannel: window.createOutputChannel('JASS-Tree-Sitter-Rust Trace'),
                trace: Trace.Verbose
            }
        )

        client.onNotification('window/logMessage', params => {
            console.log(`${params.message}`)
        })

        await client.start()
    },

    async deactivate() {
        if (client) return
        await client.stop()
    }
}
