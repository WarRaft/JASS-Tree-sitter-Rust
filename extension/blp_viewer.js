// noinspection CssUnresolvedCustomProperty

/**
 * Ответ от LSP метода 'blp/render'.
 *
 * @typedef {Object} BlpRenderResult
 * @property {string} uri - URI исходного BLP-файла.
 * @property {Array<BlpMipmap>} mipmaps - Список мипмапов, начиная с самого большого.
 */

/**
 * Информация об одном мипмапе BLP-изображения.
 *
 * @typedef {Object} BlpMipmap
 * @property {number} width - Ширина мипмапа.
 * @property {number} height - Высота мипмапа.
 * @property {string} [image_data_url] - PNG-изображение, закодированное в виде `data:image/png;base64,...`.
 *                                       Может отсутствовать, если изображение не загружено.
 */


/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('vscode-languageclient').LanguageClient} client
 */
// eslint-disable-next-line no-unused-vars
async function resolveBlpEditor(document, webviewPanel, _token, client) {

    /** @type {BlpRenderResult} */
    const result = await client.sendRequest('blp/render', {
        uri: document.uri.toString()
    })

    const items = result.mipmaps.map((mip, index) => {
        return `
            <div class="mipmap">
                <div class="meta">
                    <span class="label">#${index + 1}</span>
                    <span class="size">${mip.width} × ${mip.height}</span>
                </div>
                ${
            mip.image_data_url
                ? `<img src="${mip.image_data_url}" alt="${mip.width}x${mip.height}" />`
                : '<div class="no-image">No image</div>'
        }
            </div>
        `
    }).join('')


    webviewPanel.webview.html = `
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
                body {
                    background-color: var(--vscode-editor-background);
                    color: var(--vscode-editor-foreground);
                    font-family: var(--vscode-font-family),serif;
                    font-size: 13px;
                    margin: 0;
                    padding: 1rem;
                }

                .mipmap {
                    border: 1px solid var(--vscode-editorWidget-border);
                    background: var(--vscode-editorWidget-background);
                    padding: 0.5rem;
                    margin-bottom: 1rem;
                    border-radius: 4px;
                }

                .meta {
                    margin-bottom: 0.5rem;
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    font-weight: bold;
                }

                .size {
                    color: var(--vscode-descriptionForeground);
                }

                .no-image {
                    padding: 1rem;
                    text-align: center;
                    color: var(--vscode-disabledForeground);
                    background-color: var(--vscode-editor-background);
                    border: 1px dashed var(--vscode-editorWidget-border);
                    border-radius: 4px;
                }

                img {
                    max-width: 100%;
                    height: auto;
                    display: block;
                    border: 1px solid var(--vscode-editorGroup-border);
                    border-radius: 2px;
                    image-rendering: pixelated;
                }
            </style>
        </head>
        <body>
            ${items}
        </body>
        </html>
    `
}

module.exports = {
    resolveBlpEditor
}
