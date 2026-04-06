// noinspection CssUnresolvedCustomProperty

/**
 * Response from the server method 'blp/render'.
 *
 * @typedef {Object} BlpRenderResult
 * @property {string} uri - URI of the source BLP file.
 * @property {Array<BlpMipmap>} mipmaps - List of mipmaps, starting with the largest.
 */

/**
 * Information about a single BLP image mipmap.
 *
 * @typedef {Object} BlpMipmap
 * @property {number} width - Mipmap width.
 * @property {number} height - Mipmap height.
 * @property {string} [image_data_url] - PNG image encoded as `data:image/png;base64,...`.
 *                                       May be absent if the image was not loaded.
 */


/**
 * @param {import('vscode').CustomDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('../serverClient.js').ServerClient} client
 */
// eslint-disable-next-line no-unused-vars
async function resolveBlpEditor(document, webviewPanel, _token, client) {
    /** @type {BlpRenderResult} */
    let result
    try {
        result = await client.sendRequest('blp/render', {
            uri: document.uri.toString()
        })
    } catch (e) {
        webviewPanel.webview.html = errorHtml(`Failed to render BLP: ${e}`)
        return
    }

    if (result.error) {
        webviewPanel.webview.html = errorHtml(result.error.message || JSON.stringify(result.error))
        return
    }

    if (!result.mipmaps) {
        webviewPanel.webview.html = errorHtml('No mipmaps returned by server.')
        return
    }

    const items = result.mipmaps.map((mip, index) => {
        return `
        <div class="mipmap">
            <div class="meta">
                <span class="size">${mip.width} × ${mip.height}</span>
                <span class="label">#${index + 1}</span>
            </div>
            ${
            mip.image_data_url
                ? `<div class="img-wrapper">
                            <img class="image" src="${mip.image_data_url}" alt="${mip.width}x${mip.height}" />
                       </div>`
                : '<div class="no-image">No image</div>'
        }
        </div>
        `
    }).join('')

    webviewPanel.webview.html = `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <style>
        :root {
            --checker-color: transparent;
        }

        body {
            background-color: var(--vscode-editor-background);
            color: var(--vscode-editor-foreground);
            font-family: var(--vscode-font-family), sans-serif;
            font-size: 13px;
            margin: 0;
            padding: 1rem;
        }

        .toolbar {
            margin-bottom: 1rem;
            display: flex;
            align-items: center;
            gap: 1rem;
        }

        .toggle-container {
            display: flex;
            align-items: center;
            gap: 0.5rem;
        }

        .toggle-container input[type="checkbox"] {
            width: 48px;
            height: 24px;
            position: relative;
            appearance: none;
            background-color: var(--vscode-editorWidget-border);
            border-radius: 9999px;
            outline: none;
            cursor: pointer;
            transition: background-color 0.2s ease-in-out;
        }

        .toggle-container input[type="checkbox"]:focus-visible {
            box-shadow: 0 0 0 1.5px var(--vscode-focusBorder, #007acc);
        }

        .toggle-container input[type="checkbox"]::before {
            content: "";
            position: absolute;
            top: 3px;
            left: 3px;
            width: 18px;
            height: 18px;
            background-color: var(--vscode-editor-background);
            border-radius: 50%;
            transition: transform 0.2s ease-in-out;
        }

        .toggle-container input[type="checkbox"]:checked {
            background-color: var(--vscode-button-background);
        }

        .toggle-container input[type="checkbox"]:checked::before {
            transform: translateX(24px);
        }

        .toggle-container label {
            user-select: none;
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

        .img-wrapper {
            display: inline-flex;
            background-color: var(--checker-color);
        }

        .img-wrapper.checker {
           background-image: repeating-conic-gradient(#888 0% 25%, #444 0% 50%);
    background-size: 16px 16px;
    background-position: 0 0;
    background-repeat: repeat;
    background-color: white;
        }

        img.image {
            max-width: 100%;
            height: auto;
            display: block;
            border: 0;            
            image-rendering: pixelated;
        }
        label{
        display: inline-flex;
        align-items: center;
        }
        
    </style>
</head>
<body>
    <div class="toolbar">
        <div class="toggle-container">
            <input type="checkbox" id="checker-toggle" />
            <label for="checker-toggle">Checker background</label>
        </div>
        <label>
            <span>Background:&nbsp;</span>
            <input type="color" id="bg-color" value="#000000" />
        </label>
    </div>

    <div id="mipmaps-container">
        ${items}
    </div>

    <script>
        const toggle = document.getElementById('checker-toggle')
        const bgColorPicker = document.getElementById('bg-color')
        const wrappers = document.querySelectorAll('.img-wrapper')

        // Restore from localStorage
        const savedChecker = localStorage.getItem('checker') === 'true'
        const savedColor = localStorage.getItem('bg') || '#000000'
        toggle.checked = savedChecker
        bgColorPicker.value = savedColor
        document.documentElement.style.setProperty('--checker-color', savedColor)
        wrappers.forEach(w => w.classList.toggle('checker', savedChecker))

        toggle.addEventListener('change', () => {
            const enabled = toggle.checked
            localStorage.setItem('checker', enabled)
            wrappers.forEach(w => w.classList.toggle('checker', enabled))
        })

        bgColorPicker.addEventListener('input', () => {
            const color = bgColorPicker.value
            localStorage.setItem('bg', color)
            document.documentElement.style.setProperty('--checker-color', color)
        })
    </script>
</body>
</html>`
}

module.exports = {
    resolveBlpEditor
}

function errorHtml(msg) {
    const s = String(msg).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
    return `<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"/></head>
<body style="background:var(--vscode-editor-background);color:var(--vscode-errorForeground);font-family:var(--vscode-font-family);padding:2rem;">
<h2>⚠ Error</h2><pre>${s}</pre>
</body></html>`
}

