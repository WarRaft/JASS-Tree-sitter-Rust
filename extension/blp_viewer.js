/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('vscode-languageclient').LanguageClient} client
 */
async function resolveBlpEditor(document, webviewPanel, _token, client) {
    const result = await client.sendRequest('blp/render', {
        uri: document.uri.toString()
    })

    webviewPanel.webview.html = `
        <!DOCTYPE html>
        <html>
        <body style="margin: 0; background: #1e1e1e; display: flex; align-items: center; justify-content: center;">
            <img src="data:image/png;base64,${result.base64}" style="max-width: 100vw; max-height: 100vh;" />
        </body>
        </html>
    `
}

module.exports = {
    resolveBlpEditor
}
