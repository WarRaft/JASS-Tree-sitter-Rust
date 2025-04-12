// noinspection JSUnusedGlobalSymbols

// noinspection NpmUsedModulesInstalled
const {
    workspace,
    window,
    Uri
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
        let binName;

        switch (process.platform) {
            case "win32":
                binName = "JASS-Tree-sitter-Rust-win.exe";
                break;
            case "darwin":
                binName = "JASS-Tree-sitter-Rust-macos";
                break;
            case "linux":
                binName = "JASS-Tree-sitter-Rust-linux";
                break;
            default:
                window.showErrorMessage(`Unsupported platform: ${process.platform}`);
                return;
        }

        const binPath = Uri.file(
            path.join(context.extensionPath, "dist", binName)
        );

        try {
            await workspace.fs.stat(binPath);
        } catch (error) {
            window.showErrorMessage(`LSP binary not found:\n${binPath.fsPath}\n\n${error.message}`);
            return;
        }

        client = new LanguageClient(
            'JassTreeSitterRustLsp',
            'JassTreeSitterRustLspClient',
            {
                command: binPath.fsPath,
            },
            {
                progressOnInitialization: true,
                initializationOptions: {},
                documentSelector: [
                    {
                        scheme: 'file',
                        language: 'luz',
                    },
                ],
                outputChannelName: "JASS-Tree-Sitter-Rust Logs",
                traceOutputChannel: window.createOutputChannel("JASS-Tree-Sitter-Rust Trace"),
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
