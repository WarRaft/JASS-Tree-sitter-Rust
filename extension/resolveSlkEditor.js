// noinspection CssUnresolvedCustomProperty

/**
 * @typedef {Object} SlkCell
 * @property {string} value
 * @property {number|null} start
 * @property {number|null} len
 */

/**
 * @typedef {Object} SlkRenderResult
 * @property {number} cols
 * @property {number} rows
 * @property {SlkCell[][]} grid
 * @property {{message:string}} [error]
 */

const {workspace, WorkspaceEdit, Range, Position, Uri} = require('vscode')
const path = require('path')

const SETTINGS_PREFIX = '@cdg:'
const DEFAULT_EDITOR_KEY = 'slk.defaultEditor'

/**
 * Check whether the SLK table editor is set as the default for *.slk files.
 * @param {import('vscode').ExtensionContext} context
 * @returns {boolean}
 */
function isDefaultEditor(context) {
    return !!context.globalState.get(DEFAULT_EDITOR_KEY, false)
}

/**
 * Toggle the default editor association for *.slk files.
 * @param {import('vscode').ExtensionContext} context
 * @param {boolean} enable
 */
async function setDefaultEditor(context, enable) {
    await context.globalState.update(DEFAULT_EDITOR_KEY, enable)
    const config = workspace.getConfiguration()
    const associations = config.get('workbench.editorAssociations') || {}
    const newAssociations = Object.assign({}, associations)

    if (enable) {
        newAssociations['*.slk'] = 'slk.preview'
    } else {
        delete newAssociations['*.slk']
    }

    await config.update('workbench.editorAssociations', newAssociations, true)
}

// ─── In-file settings (stored after the E record) ───────────────────────────

/**
 * Read CDG settings from the SLK document text (the line after E starting with @cdg:).
 * @param {import('vscode').TextDocument} document
 * @returns {{settings: object, line: number|null}}
 */
function readSettings(document) {
    const text = document.getText()
    // Find the E record — a line that is exactly "E" (possibly with \r)
    const lines = text.split('\n')
    for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].replace(/\r$/, '')
        if (trimmed === 'E') {
            // Check the next line for settings
            if (i + 1 < lines.length) {
                const nextLine = lines[i + 1].replace(/\r$/, '')
                if (nextLine.startsWith(SETTINGS_PREFIX)) {
                    try {
                        const json = JSON.parse(nextLine.slice(SETTINGS_PREFIX.length))
                        return {settings: json, line: i + 1}
                    } catch (_) { /* corrupted — ignore */ }
                }
            }
            return {settings: {}, line: null}
        }
    }
    return {settings: {}, line: null}
}

/**
 * Write CDG settings into the SLK document (after the E record).
 * @param {import('vscode').TextDocument} document
 * @param {object} settings
 */
async function writeSettings(document, settings) {
    const text = document.getText()
    const lines = text.split('\n')
    let eLine = -1
    for (let i = 0; i < lines.length; i++) {
        if (lines[i].replace(/\r$/, '') === 'E') {
            eLine = i
            break
        }
    }
    if (eLine < 0) return  // no E record found

    const settingsText = SETTINGS_PREFIX + JSON.stringify(settings)
    const edit = new WorkspaceEdit()

    // Check if next line already has settings
    if (eLine + 1 < lines.length) {
        const nextLine = lines[eLine + 1].replace(/\r$/, '')
        if (nextLine.startsWith(SETTINGS_PREFIX)) {
            // Replace existing settings line
            const start = new Position(eLine + 1, 0)
            const end = new Position(eLine + 1, lines[eLine + 1].length)
            edit.replace(document.uri, new Range(start, end), settingsText)
        } else {
            // Insert new settings line after E
            const pos = new Position(eLine + 1, 0)
            edit.insert(document.uri, pos, settingsText + '\n')
        }
    } else {
        // E is the last line — append settings after it
        const pos = new Position(eLine, lines[eLine].length)
        edit.insert(document.uri, pos, '\n' + settingsText)
    }

    await workspace.applyEdit(edit)
}

/**
 * Resolve the SLK table editor webview using canvas-datagrid (canvas-based).
 *
 * @param {import('vscode').TextDocument} document
 * @param {import('vscode').WebviewPanel} webviewPanel
 * @param {import('vscode').CancellationToken} _token
 * @param {import('./serverClient.js').ServerClient} client
 * @param {import('vscode').ExtensionContext} context
 */
async function resolveSlkEditor(document, webviewPanel, _token, client, context) {
    const webview = webviewPanel.webview
    webview.options = {enableScripts: true}

    const vendorDir = Uri.file(path.join(context.extensionPath, 'extension', 'vendor'))
    const canvasDatagridJsUri = webview.asWebviewUri(Uri.joinPath(vendorDir, 'canvas-datagrid.js'))

    let suppressRefresh = false

    /**
     * Fetch fresh data from the server and return parsed schema + rows.
     * @returns {Promise<{schema: object[], rowData: object[], cols: number, rows: number, fname: string, headers: string[]}|null>}
     */
    async function fetchData() {
        /** @type {SlkRenderResult} */
        const result = await client.sendRequest('render/slk', {
            uri: document.uri.toString()
        })

        if (result.error) return null

        const {cols, rows, grid} = result
        const fname = document.uri.fsPath.split(/[\\/]/).pop() || 'slk'
        const {settings} = readSettings(document)
        const savedWidths = settings.columnWidths || {}
        const hiddenCols = settings.hiddenColumns || []

        const headers = []
        const schema = []
        for (let c = 0; c < cols; c++) {
            const headerVal = grid[0] && grid[0][c] ? grid[0][c].value : `Col ${c + 1}`
            headers.push(headerVal)
            const field = `c${c}`
            const colDef = {
                name: field,
                title: headerVal,
            }
            if (savedWidths[field]) {
                colDef.width = savedWidths[field]
            }
            if (hiddenCols.indexOf(field) !== -1) {
                colDef.hidden = true
            }
            schema.push(colDef)
        }

        const rowData = []
        for (let r = 1; r < rows; r++) {
            const row = {_rowNum: r + 1, _rowIdx: r}
            for (let c = 0; c < cols; c++) {
                const cell = grid[r] && grid[r][c] ? grid[r][c] : {value: '', start: null, len: null}
                row[`c${c}`] = cell.value
                row[`_meta_c${c}`] = {start: cell.start, len: cell.len}
            }
            rowData.push(row)
        }

        return {schema, rowData, cols, rows, fname, headers}
    }

    /**
     * Send incremental data update to webview (preserves scroll position).
     */
    async function refreshData() {
        const data = await fetchData()
        if (!data) return
        webview.postMessage({command: 'updateData', rowData: data.rowData})
    }

    // Initial full render
    const initialData = await fetchData()
    if (!initialData) {
        webview.html = errorHtml('Failed to parse SLK data')
        return
    }

    const defaultEditorChecked = isDefaultEditor(context)
    webview.html = buildHtml(
        initialData, canvasDatagridJsUri, defaultEditorChecked
    )

    // Listen for messages from the webview
    webviewPanel.webview.onDidReceiveMessage(async msg => {
        if (msg.command === 'edit') {
            try {
                const result = await client.sendRequest('slk/edit', {
                    uri: document.uri.toString(),
                    start: msg.start,
                    len: msg.len,
                    value: msg.value
                })

                if (result.ok && result.range && result.new_text != null) {
                    const edit = new WorkspaceEdit()
                    const range = new Range(
                        new Position(result.range.start.line, result.range.start.character),
                        new Position(result.range.end.line, result.range.end.character)
                    )
                    edit.replace(document.uri, range, result.new_text)
                    suppressRefresh = true
                    await workspace.applyEdit(edit)
                    await workspace.save(document.uri)
                    suppressRefresh = false
                    // Incremental update — preserves scroll position
                    setTimeout(() => refreshData(), 150)
                }
            } catch (e) {
                console.error('SLK edit error:', e)
            }
        }

        if (msg.command === 'saveSettings') {
            suppressRefresh = true
            await writeSettings(document, msg.settings)
            suppressRefresh = false
        }

        if (msg.command === 'setDefaultEditor') {
            await setDefaultEditor(context, msg.enabled)
        }
    })

    // Re-render when the document changes externally (debounced)
    let refreshTimer = null
    const changeDisposable = workspace.onDidChangeTextDocument(e => {
        if (e.document.uri.toString() === document.uri.toString() && !suppressRefresh) {
            if (refreshTimer) clearTimeout(refreshTimer)
            refreshTimer = setTimeout(() => refreshData(), 300)
        }
    })

    webviewPanel.onDidDispose(() => {
        changeDisposable.dispose()
    })
}

function errorHtml(msg) {
    return `<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"/></head>
<body style="background:var(--vscode-editor-background);color:var(--vscode-errorForeground);font-family:var(--vscode-font-family);padding:2rem;">
<h2>⚠ Error</h2><pre>${escapeHtml(msg)}</pre>
</body></html>`
}

function escapeHtml(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

/**
 * Build the full HTML page for the canvas-datagrid-based SLK table.
 *
 * @param {{schema: object[], rowData: object[], cols: number, rows: number, fname: string, headers: string[]}} data
 * @param {import('vscode').Uri} canvasDatagridJsUri
 * @param {boolean} defaultEditorChecked
 */
function buildHtml(data, canvasDatagridJsUri, defaultEditorChecked) {
    const {schema, rowData, cols, rows, fname, headers} = data
    const schemaJson = JSON.stringify(schema)
    const rowDataJson = JSON.stringify(rowData)
    const headersJson = JSON.stringify(headers)
    const fnameJson = JSON.stringify(fname)

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <style>
        html, body {
            margin: 0;
            padding: 0;
            background: var(--vscode-editor-background);
            color: var(--vscode-editor-foreground);
            font-family: var(--vscode-font-family), sans-serif;
            font-size: 13px;
            height: 100%;
            overflow: hidden;
        }
        .header-bar {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 6px 12px;
            background: var(--vscode-editorWidget-background);
            border-bottom: 1px solid var(--vscode-editorWidget-border);
            flex-shrink: 0;
        }
        .header-bar h1 {
            font-size: 1.1em;
            margin: 0;
            font-weight: 600;
        }
        .header-bar .dim {
            color: var(--vscode-descriptionForeground);
            font-weight: normal;
            font-size: 0.85em;
        }
        .header-bar .default-editor-label {
            display: flex;
            align-items: center;
            gap: 4px;
            font-size: 12px;
            color: var(--vscode-descriptionForeground);
            cursor: pointer;
            user-select: none;
            white-space: nowrap;
        }
        .header-bar .default-editor-label input[type="checkbox"] {
            accent-color: var(--vscode-focusBorder);
            cursor: pointer;
            outline: none;
        }
        .header-bar .default-editor-label input[type="checkbox"]:focus-visible {
            outline: 1.5px solid var(--vscode-focusBorder);
            outline-offset: 1px;
            border-radius: 2px;
        }
        .header-bar .search-box {
            margin-left: auto;
            padding: 3px 8px;
            background: var(--vscode-input-background);
            color: var(--vscode-input-foreground);
            border: 1px solid var(--vscode-input-border, var(--vscode-editorWidget-border));
            border-radius: 3px;
            font-size: 12px;
            font-family: var(--vscode-font-family), sans-serif;
            outline: none;
            min-width: 180px;
        }
        .header-bar .search-box:focus {
            border-color: var(--vscode-focusBorder);
        }
        .header-bar .search-box::placeholder {
            color: var(--vscode-input-placeholderForeground);
        }
        .slk-color-picker {
            position: fixed;
            opacity: 0;
            width: 0;
            height: 0;
            border: none;
            padding: 0;
            pointer-events: auto;
            z-index: 99999;
        }
        #app {
            display: flex;
            flex-direction: column;
            height: 100vh;
        }
        #table-container {
            flex: 1;
            min-height: 0;
            position: relative;
            overflow: hidden;
        }
        /* canvas-datagrid creates a custom element; make it fill the container */
        #table-container canvas-datagrid {
            display: block !important;
            width: 100% !important;
            height: 100% !important;
        }

        /* ── preloader overlay ── */
        #preloader {
            position: absolute;
            inset: 0;
            z-index: 9999;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            gap: 18px;
            background: var(--vscode-editor-background);
            transition: opacity .35s ease;
        }
        #preloader.hidden {
            opacity: 0;
            pointer-events: none;
        }
        .spinner {
            width: 36px; height: 36px;
            border: 3px solid var(--vscode-editorWidget-border, #444);
            border-top-color: var(--vscode-focusBorder, #007fd4);
            border-radius: 50%;
            animation: spin .8s linear infinite;
        }
        @keyframes spin { to { transform: rotate(360deg); } }
        #preloader .status {
            font-size: 12px;
            color: var(--vscode-descriptionForeground, #888);
            text-align: center;
            line-height: 1.5;
        }
        #preloader .step {
            display: flex;
            align-items: center;
            gap: 6px;
            font-size: 11px;
            color: var(--vscode-descriptionForeground, #888);
            opacity: .5;
            transition: opacity .2s;
        }
        #preloader .step.active {
            opacity: 1;
            color: var(--vscode-editor-foreground, #ccc);
        }
        #preloader .step.done {
            opacity: .7;
            color: var(--vscode-gitDecoration-addedResourceForeground, #73c991);
        }
        #preloader .step .icon {
            width: 14px;
            text-align: center;
            flex-shrink: 0;
        }
    </style>
</head>
<body>
    <div id="app">
    <div class="header-bar">
        <h1>\u{1F4CA} ${escapeHtml(fname)} <span class="dim">${cols}\u00D7${rows}</span></h1>
        <label class="default-editor-label">
            <input type="checkbox" id="defaultEditor" ${defaultEditorChecked ? 'checked' : ''} />
            Default editor
        </label>
        <input class="search-box" id="search" type="text" placeholder="Search\u2026" />
    </div>
    <div id="table-container">
        <div id="preloader">
            <div class="spinner"></div>
            <div class="status">
                <div id="step-parse" class="step active"><span class="icon">\u25CB</span> Parsing data\u2026</div>
                <div id="step-grid" class="step"><span class="icon">\u25CB</span> Creating grid\u2026</div>
                <div id="step-theme" class="step"><span class="icon">\u25CB</span> Applying theme\u2026</div>
                <div id="step-layout" class="step"><span class="icon">\u25CB</span> Layout &amp; render\u2026</div>
            </div>
        </div>
    </div>
    </div>

    <script src="${canvasDatagridJsUri}"><\/script>
    <script>
    (function() {
        let vscode = acquireVsCodeApi();
        let schema = ${schemaJson};
        let rowData = ${rowDataJson};
        let slkHeaders = ${headersJson};
        let slkFname = ${fnameJson};

        // ── SLK metadata: detect known file types by filename ──
        let SLK_META = (function() {
            let headerToField = {};
            for (let i = 0; i < slkHeaders.length; i++) {
                headerToField[slkHeaders[i]] = 'c' + i;
            }
            let slkName = slkFname.replace(/\.slk$/i, '').toLowerCase();

            let boolCols = {};   // field → true
            let colorGroups = []; // [{r:'cN', g:'cM', b:'cK', label:'name'}]

            function markBools(names) {
                for (let i = 0; i < names.length; i++) {
                    let f = headerToField[names[i]];
                    if (f) boolCols[f] = true;
                }
            }

            if (slkName === 'doodads') {
                markBools(['tilesetSpecific','canPlaceRandScale','useClickHelper','ignoreModelClick',
                    'walkable','onCliffs','onWater','floats','shadow','showInFog','animInFog',
                    'showInMM','useMMColor','InBeta']);
                if (headerToField['MMRed'] && headerToField['MMGreen'] && headerToField['MMBlue']) {
                    colorGroups.push({r:headerToField['MMRed'],g:headerToField['MMGreen'],b:headerToField['MMBlue'],label:'MM'});
                }
                for (let i = 1; i <= 10; i++) {
                    let idx = (i<10?'0':'') + i;
                    let rH='vertR'+idx, gH='vertG'+idx, bH='vertB'+idx;
                    if (headerToField[rH] && headerToField[gH] && headerToField[bH]) {
                        colorGroups.push({r:headerToField[rH],g:headerToField[gH],b:headerToField[bH],label:'V'+idx});
                    }
                }
            } else if (slkName === 'destructabledata') {
                markBools(['tilesetSpecific','lightweight','fatLOS','useClickHelper','onCliffs','onWater',
                    'canPlaceDead','walkable','canPlaceRandScale','fogVis','shadow','showInMM',
                    'useMMColor','selectable','InBeta']);
                if (headerToField['colorR'] && headerToField['colorG'] && headerToField['colorB']) {
                    colorGroups.push({r:headerToField['colorR'],g:headerToField['colorG'],b:headerToField['colorB'],label:'Tint'});
                }
                if (headerToField['MMRed'] && headerToField['MMGreen'] && headerToField['MMBlue']) {
                    colorGroups.push({r:headerToField['MMRed'],g:headerToField['MMGreen'],b:headerToField['MMBlue'],label:'MM'});
                }
            } else if (slkName === 'unitdata') {
                markBools(['canSleep','canFlee','isBuildOn']);
            }

            // Reverse: field → colorGroup
            let colorFieldMap = {};
            for (let ci = 0; ci < colorGroups.length; ci++) {
                let cg = colorGroups[ci];
                colorFieldMap[cg.r] = cg;
                colorFieldMap[cg.g] = cg;
                colorFieldMap[cg.b] = cg;
            }

            return {boolCols:boolCols, colorGroups:colorGroups, colorFieldMap:colorFieldMap};
        })();

        // ── preloader helpers ──
        let preloader = document.getElementById('preloader');
        let steps = {
            parse:  document.getElementById('step-parse'),
            grid:   document.getElementById('step-grid'),
            theme:  document.getElementById('step-theme'),
            layout: document.getElementById('step-layout')
        };
        function markDone(id) {
            let el = steps[id];
            if (!el) return;
            el.classList.remove('active');
            el.classList.add('done');
            el.querySelector('.icon').textContent = '\u2713';
        }
        function markActive(id) {
            let el = steps[id];
            if (!el) return;
            el.classList.add('active');
            el.querySelector('.icon').textContent = '\u25CF';
        }
        function hidePreloader() {
            preloader.classList.add('hidden');
            setTimeout(function() {
                if (preloader.parentNode) preloader.parentNode.removeChild(preloader);
            }, 400);
        }

        // ── Step 1: parse ──
        markDone('parse');

        // ── Step 2: read theme (synchronous, fast) ──
        markActive('theme');
        let cs = getComputedStyle(document.documentElement);
        function cv(name, fallback) {
            return cs.getPropertyValue(name).trim() || fallback;
        }
        let editorBg    = cv('--vscode-editor-background', '#1e1e1e');
        let editorFg    = cv('--vscode-editor-foreground', '#cccccc');
        let widgetBg    = cv('--vscode-editorWidget-background', '#252526');
        let widgetBorder= cv('--vscode-editorWidget-border', '#454545');
        let headerBg    = cv('--vscode-editorGroupHeader-tabsBackground', '#2d2d2d');
        let descFg      = cv('--vscode-descriptionForeground', '#969696');
        let selBg       = cv('--vscode-editor-selectionBackground', 'rgba(38,79,120,0.7)');
        let selFg       = cv('--vscode-list-activeSelectionForeground', editorFg);
        let focusBorder = cv('--vscode-focusBorder', '#007fd4');
        let inputBg     = cv('--vscode-input-background', '#3c3c3c');
        let inputFg     = cv('--vscode-input-foreground', '#cccccc');
        let fontFamily  = cv('--vscode-font-family', 'sans-serif');
        markDone('theme');

        // ── helpers ──
        function stripMeta(rows) {
            return rows.map(function(row) {
                let clean = {};
                for (let key in row) {
                    if (!key.startsWith('_')) clean[key] = row[key];
                }
                return clean;
            });
        }

        let cleanData = stripMeta(rowData);
        // Hidden index column — used to restore original file order
        schema.push({name: '_idx', hidden: true, type: 'number'});
        for (let i = 0; i < cleanData.length; i++) cleanData[i]._idx = i;

        let currentRowData = rowData;
        let container = document.getElementById('table-container');
        let grid; // assigned once layout is ready
        let lastW = 0, lastH = 0;

        let gridStyle = {
            gridBackgroundColor: editorBg,
            gridBorderColor: widgetBorder,
            gridBorderWidth: 1,

            cellBackgroundColor: editorBg,
            cellColor: editorFg,
            cellBorderColor: widgetBorder,
            cellBorderWidth: 1,
            cellFont: '13px ' + fontFamily,
            cellHeight: 24,
            cellPaddingLeft: 6,
            cellPaddingRight: 6,
            cellHoverBackgroundColor: widgetBg,
            cellHoverColor: editorFg,
            cellSelectedBackgroundColor: selBg,
            cellSelectedColor: selFg,

            activeCellBackgroundColor: editorBg,
            activeCellColor: editorFg,
            activeCellBorderColor: focusBorder,
            activeCellBorderWidth: 2,
            activeCellFont: '13px ' + fontFamily,
            activeCellHoverBackgroundColor: widgetBg,
            activeCellHoverColor: editorFg,
            activeCellSelectedBackgroundColor: selBg,
            activeCellSelectedColor: selFg,
            activeCellOverlayBorderColor: focusBorder,
            activeCellOverlayBorderWidth: 2,

            columnHeaderCellBackgroundColor: headerBg,
            columnHeaderCellColor: descFg,
            columnHeaderCellBorderColor: widgetBorder,
            columnHeaderCellBorderWidth: 1,
            columnHeaderCellFont: 'bold 13px ' + fontFamily,
            columnHeaderCellHeight: 28,
            columnHeaderCellHoverBackgroundColor: widgetBg,
            columnHeaderCellHoverColor: editorFg,

            rowHeaderCellBackgroundColor: headerBg,
            rowHeaderCellColor: descFg,
            rowHeaderCellBorderColor: widgetBorder,
            rowHeaderCellBorderWidth: 1,
            rowHeaderCellFont: '12px ' + fontFamily,
            rowHeaderCellHoverBackgroundColor: widgetBg,
            rowHeaderCellHoverColor: editorFg,

            cornerCellBackgroundColor: headerBg,
            cornerCellBorderColor: widgetBorder,

            scrollBarBackgroundColor: editorBg,
            scrollBarBoxColor: descFg,
            scrollBarBorderColor: widgetBorder,
            scrollBarActiveColor: focusBorder,
            scrollBarCornerBackgroundColor: editorBg,
            scrollBarCornerBorderColor: widgetBorder,
            scrollBarWidth: 12,

            editCellBackgroundColor: inputBg,
            editCellColor: inputFg,
            editCellBorder: '1px solid ' + focusBorder,
            editCellFontFamily: fontFamily,
            editCellFontSize: '13px',

            contextMenuBackground: widgetBg,
            contextMenuColor: editorFg,
            contextMenuBorder: '1px solid ' + widgetBorder,
            contextMenuHoverBackground: selBg,
            contextMenuHoverColor: selFg,
            contextMenuFontFamily: fontFamily,
            contextMenuFontSize: '13px',

            selectionOverlayBorderColor: focusBorder,
            selectionOverlayBorderWidth: 2,
            moveOverlayBorderColor: focusBorder,
        };

        // ── Step 3: create grid right away (preloader hides it) ──
        // Follow the official pattern: create a wrapper div, instantiate the
        // grid into it, append the wrapper, then set data after the fact.
        // https://canvas-datagrid.js.org/examples/set-data-after-instantiation
        markActive('grid');

        let gridElement = document.createElement('div');
        grid = canvasDatagrid({
            parentNode: gridElement,
            schema: schema,
            editable: true,
            allowColumnReordering: false,
            allowRowReordering: false,
            showFilter: false,
            showRowNumbers: true,
            showRowHeaders: true,
            showColumnHeaders: true,
            allowColumnResize: true,
            allowRowResize: false,
            showCopy: true,
            allowSorting: true,
            selectionMode: 'cell',
            multiLine: false,
            style: gridStyle
        });

        container.append(gridElement);
        grid.data = cleanData;

        markDone('grid');
        markActive('layout');

        // ── Step 4: force correct size and keep retrying until it renders ──
        // canvas-datagrid listens for window "resize" events internally —
        // dispatching one is the most reliable way to make it re-measure.

        function applySize() {
            let h = container.offsetHeight;
            let w = container.offsetWidth;
            if (h > 0 && w > 0) {
                lastH = h; lastW = w;
                grid.style.height = h + 'px';
                grid.style.width  = w + 'px';
                if (typeof grid.resize === 'function') grid.resize(true);
                if (typeof grid.draw === 'function') grid.draw();
            }
        }

        let attempts = 0;
        function pump() {
            applySize();
            window.dispatchEvent(new Event('resize'));
            attempts++;

            // grid.visibleCells is populated only after a successful draw
            let rendered = grid.visibleCells && grid.visibleCells.length > 0;

            if (rendered) {
                markDone('layout');
                // One more rAF so the canvas buffer is composited on screen
                requestAnimationFrame(function() {
                    hidePreloader();
                    // Force a final redraw after the preloader overlay is removed
                    setTimeout(function() {
                        applySize();
                    }, 50);
                });
            } else if (attempts < 60) {
                // keep trying — ~1 s worth of frames
                requestAnimationFrame(pump);
            } else {
                // safety valve — show whatever we have
                markDone('layout');
                hidePreloader();
                setTimeout(function() { applySize(); }, 50);
            }
        }

        // Yield once so the preloader paints, then start pumping
        setTimeout(function() { requestAnimationFrame(pump); }, 0);

        // ── keep tracking container size after initial render ──
        new ResizeObserver(function() {
            let nh = container.offsetHeight;
            let nw = container.offsetWidth;
            if (nh > 0 && nw > 0 && (nh !== lastH || nw !== lastW)) {
                lastH = nh; lastW = nw;
                grid.style.height = nh + 'px';
                grid.style.width  = nw + 'px';
                if (typeof grid.resize === 'function') grid.resize(true);
                if (typeof grid.draw === 'function') grid.draw();
            }
        }).observe(container);

        // ── wire up everything else ──
        setupGrid(grid);

        // ═══════════════════════════════════════════════════
        // All event listeners / features — called once grid exists
        // ═══════════════════════════════════════════════════
        function setupGrid(grid) {

            // ── Auto-size columns without saved widths ──
            (function autoSizeColumns() {
                let ctx = null;
                try { ctx = grid.canvas ? grid.canvas.getContext('2d') : null; } catch(_){}
                if (!ctx) {
                    let tmp = document.createElement('canvas');
                    ctx = tmp.getContext('2d');
                }
                let s = grid.schema;
                let sampleRows = Math.min(cleanData.length, 60);
                for (let i = 0; i < s.length; i++) {
                    let col = s[i];
                    if (col.name === '_idx' || col.hidden) continue;
                    if (col.width) continue;

                    // Measure header
                    ctx.font = 'bold 13px ' + fontFamily;
                    let headerW = ctx.measureText(col.title || col.name).width;

                    // Check if column is boolean or all-numeric
                    let isBool = !!SLK_META.boolCols[col.name];
                    let isNumeric = !isBool;
                    if (isNumeric) {
                        for (let r = 0; r < sampleRows && isNumeric; r++) {
                            let val = cleanData[r] ? String(cleanData[r][col.name] || '') : '';
                            if (val !== '' && isNaN(val)) isNumeric = false;
                        }
                    }

                    let maxW;
                    if (isBool || isNumeric) {
                        // Size by header only
                        maxW = headerW;
                    } else {
                        // Size by max of header and data
                        maxW = headerW;
                        ctx.font = '13px ' + fontFamily;
                        for (let r = 0; r < sampleRows; r++) {
                            let val = cleanData[r] ? String(cleanData[r][col.name] || '') : '';
                            let tw = ctx.measureText(val).width;
                            if (tw > maxW) maxW = tw;
                        }
                    }

                    let computed = Math.ceil(maxW) + 24;
                    if (computed < 50) computed = 50;
                    if (computed > 350) computed = 350;
                    if (!grid.sizes) grid.sizes = {};
                    if (!grid.sizes.columns) grid.sizes.columns = {};
                    grid.sizes.columns[i] = computed;
                }
            })();

            // ── Custom cell rendering: booleans, colors, preview button ──
            grid.addEventListener('rendertext', function(e) {
                if (!e.cell || !e.cell.header) return;
                let field = e.cell.header.name;
                // Boolean columns: draw checkbox instead of text
                if (SLK_META.boolCols[field] && !e.cell.isColumnHeader && !e.cell.isRowHeader) {
                    e.preventDefault();
                    let ctx = e.ctx;
                    let cx = e.cell.x + e.cell.width / 2;
                    let cy = e.cell.y + e.cell.height / 2;
                    let sz = 12;
                    let x0 = cx - sz/2, y0 = cy - sz/2;
                    let checked = (e.cell.value === '1' || e.cell.value === 1);
                    ctx.save();
                    ctx.strokeStyle = descFg;
                    ctx.lineWidth = 1.5;
                    ctx.beginPath();
                    // Rounded rectangle
                    let r = 2;
                    ctx.moveTo(x0+r, y0);
                    ctx.lineTo(x0+sz-r, y0);
                    ctx.arcTo(x0+sz, y0, x0+sz, y0+r, r);
                    ctx.lineTo(x0+sz, y0+sz-r);
                    ctx.arcTo(x0+sz, y0+sz, x0+sz-r, y0+sz, r);
                    ctx.lineTo(x0+r, y0+sz);
                    ctx.arcTo(x0, y0+sz, x0, y0+sz-r, r);
                    ctx.lineTo(x0, y0+r);
                    ctx.arcTo(x0, y0, x0+r, y0, r);
                    ctx.closePath();
                    if (checked) {
                        ctx.fillStyle = focusBorder;
                        ctx.fill();
                        // Draw checkmark
                        ctx.strokeStyle = '#fff';
                        ctx.lineWidth = 2;
                        ctx.beginPath();
                        ctx.moveTo(x0+2.5, cy);
                        ctx.lineTo(x0+5, y0+sz-2.5);
                        ctx.lineTo(x0+sz-2, y0+2.5);
                        ctx.stroke();
                    } else {
                        ctx.stroke();
                    }
                    ctx.restore();
                    return;
                }
            });

            grid.addEventListener('afterrendercell', function(e) {
                if (!e.cell || !e.cell.header || e.cell.isColumnHeader || e.cell.isRowHeader) return;
                let field = e.cell.header.name;
                let ctx = e.ctx;

                // Color swatch for RGB group columns
                let cg = SLK_META.colorFieldMap[field];
                if (cg) {
                    // Compose RGB from sibling columns
                    let rowObj = cleanData[e.cell.rowIndex];
                    if (!rowObj) return;
                    let rv = parseInt(rowObj[cg.r], 10) || 0;
                    let gv = parseInt(rowObj[cg.g], 10) || 0;
                    let bv = parseInt(rowObj[cg.b], 10) || 0;
                    // Draw swatch at right edge
                    let swSz = 14;
                    let sx = e.cell.x + e.cell.width - swSz - 4;
                    let sy = e.cell.y + (e.cell.height - swSz) / 2;
                    ctx.save();
                    ctx.fillStyle = 'rgb(' + rv + ',' + gv + ',' + bv + ')';
                    ctx.fillRect(sx, sy, swSz, swSz);
                    ctx.strokeStyle = descFg;
                    ctx.lineWidth = 1;
                    ctx.strokeRect(sx, sy, swSz, swSz);
                    ctx.restore();
                }
            });

            // ── Prevent default edit for boolean columns ──
            grid.addEventListener('beforebeginedit', function(e) {
                if (!e.cell || !e.cell.header) return;
                let field = e.cell.header.name;
                if (SLK_META.boolCols[field]) {
                    e.preventDefault();
                }
            });

            // ── Click handler: boolean toggle, color picker ──
            grid.addEventListener('click', function(e) {
                if (!e.cell || !e.cell.header || e.cell.isColumnHeader || e.cell.isRowHeader) return;
                let field = e.cell.header.name;
                let rowIdx = e.cell.rowIndex;

                // Boolean toggle
                if (SLK_META.boolCols[field]) {
                    let meta = getMetaForCell(rowIdx, field);
                    if (!meta || meta.start == null) return;
                    let curVal = currentRowData[rowIdx] ? currentRowData[rowIdx][field] : '0';
                    let newVal = (curVal === '1') ? '0' : '1';
                    vscode.postMessage({command:'edit', start:meta.start, len:meta.len, value:newVal});
                    return;
                }

                // Color picker on swatch click
                let cg = SLK_META.colorFieldMap[field];
                if (cg) {
                    // Check if click is in the swatch area (right side of cell)
                    let swSz = 14;
                    let sx = e.cell.x + e.cell.width - swSz - 4;
                    let mouseX = 0;
                    if (e.NativeEvent) {
                        let rect = grid.canvas ? grid.canvas.getBoundingClientRect() : null;
                        if (rect) mouseX = e.NativeEvent.clientX - rect.left;
                    }
                    if (mouseX >= sx) {
                        let rowObj = cleanData[rowIdx];
                        if (!rowObj) return;
                        let rv = parseInt(rowObj[cg.r], 10) || 0;
                        let gv = parseInt(rowObj[cg.g], 10) || 0;
                        let bv = parseInt(rowObj[cg.b], 10) || 0;
                        let hex = '#' + ((1<<24)+(rv<<16)+(gv<<8)+bv).toString(16).slice(1);

                        let picker = document.querySelector('.slk-color-picker');
                        if (!picker) {
                            picker = document.createElement('input');
                            picker.type = 'color';
                            picker.className = 'slk-color-picker';
                            document.body.appendChild(picker);
                        }
                        picker.value = hex;
                        picker._slkCg = cg;
                        picker._slkRow = rowIdx;

                        picker.onchange = function() {
                            let h = this.value;
                            let nr = parseInt(h.slice(1,3),16);
                            let ng = parseInt(h.slice(3,5),16);
                            let nb = parseInt(h.slice(5,7),16);
                            let cgr = this._slkCg;
                            let ri = this._slkRow;
                            let metaR = getMetaForCell(ri, cgr.r);
                            let metaG = getMetaForCell(ri, cgr.g);
                            let metaB = getMetaForCell(ri, cgr.b);
                            if (metaR && metaR.start!=null) vscode.postMessage({command:'edit',start:metaR.start,len:metaR.len,value:String(nr)});
                            if (metaG && metaG.start!=null) vscode.postMessage({command:'edit',start:metaG.start,len:metaG.len,value:String(ng)});
                            if (metaB && metaB.start!=null) vscode.postMessage({command:'edit',start:metaB.start,len:metaB.len,value:String(nb)});
                        };
                        picker.click();
                    }
                }
            });

            // ── 3-state sort: asc → desc → original file order ──
            let lastSortCol = null;
            let lastSortDir = null;

            function resetSort() {
                lastSortCol = null;
                lastSortDir = null;
                grid.order('_idx', 'asc');
            }

            grid.addEventListener('beforesortcolumn', function(e) {
                if (e.name === '_idx') return; // let index sort through
                if (e.name === lastSortCol && lastSortDir === 'desc' && e.direction === 'asc') {
                    // 3rd click on same column → reset to original order
                    e.preventDefault();
                    resetSort();
                    return;
                }
                lastSortCol = e.name;
                lastSortDir = e.direction;
            });

            // ── collect current settings from the grid ──
            function collectSettings() {
                let settings = {};

                // Column widths — take user-resized width first, fall back to schema width
                let widths = {};
                let s = grid.schema;
                for (let i = 0; i < s.length; i++) {
                    let col = s[i];
                    if (col.name === '_idx') continue;
                    let w = grid.sizes.columns[i] || col.width;
                    if (w && col.name) widths[col.name] = w;
                }
                if (Object.keys(widths).length > 0) settings.columnWidths = widths;

                // Hidden columns (exclude the internal _idx)
                let hidden = [];
                for (let i = 0; i < s.length; i++) {
                    if (s[i].hidden && s[i].name !== '_idx') hidden.push(s[i].name);
                }
                if (hidden.length > 0) settings.hiddenColumns = hidden;

                return settings;
            }

            function persistSettings() {
                vscode.postMessage({ command: 'saveSettings', settings: collectSettings() });
            }

            // ── cell editing ──
            function getMetaForCell(rowIndex, field) {
                if (rowIndex < 0 || rowIndex >= currentRowData.length) return null;
                return currentRowData[rowIndex]['_meta_' + field];
            }

            grid.addEventListener('endedit', function(e) {
                let cell = e.cell;
                if (!cell) return;
                let field = cell.header ? cell.header.name : null;
                if (!field) return;

                let meta = getMetaForCell(cell.rowIndex, field);
                if (!meta || meta.start == null) return;

                let newValue = e.value;
                if (newValue === e.oldValue) return;

                vscode.postMessage({
                    command: 'edit',
                    start: meta.start,
                    len: meta.len,
                    value: String(newValue)
                });
            });

            // ── column resize persistence (debounced) ──
            let resizeTimer = null;
            grid.addEventListener('resizecolumn', function() {
                if (resizeTimer) clearTimeout(resizeTimer);
                resizeTimer = setTimeout(persistSettings, 300);
            });

            // ── context menu: hide / show columns + sort reset ──
            grid.addEventListener('contextmenu', function(e) {
                // "Sort: original file order" (always available)
                if (grid.orderBy && grid.orderBy !== '_idx') {
                    e.items.push({
                        title: 'Sort: original file order',
                        click: function() { resetSort(); }
                    });
                }

                if (!e.cell || !e.cell.header) return;
                let clickedCol = e.cell.header;
                if (clickedCol.name === '_idx') return;

                // "Hide column" item
                e.items.push({
                    title: 'Hide column "' + (clickedCol.title || clickedCol.name) + '"',
                    click: function() {
                        clickedCol.hidden = true;
                        grid.draw();
                        persistSettings();
                    }
                });

                // "Show all columns" item (only if something besides _idx is hidden)
                let hasHidden = false;
                let s = grid.schema;
                for (let i = 0; i < s.length; i++) {
                    if (s[i].hidden && s[i].name !== '_idx') { hasHidden = true; break; }
                }
                if (hasHidden) {
                    e.items.push({
                        title: 'Show all columns',
                        click: function() {
                            let s = grid.schema;
                            for (let i = 0; i < s.length; i++) {
                                if (s[i].name !== '_idx') s[i].hidden = false;
                            }
                            grid.draw();
                            persistSettings();
                        }
                    });
                }
            });

            // ── incremental data updates from extension host ──
            window.addEventListener('message', function(event) {
                let msg = event.data;
                if (msg.command === 'updateData') {
                    currentRowData = msg.rowData;
                    let newClean = stripMeta(msg.rowData);
                    for (let i = 0; i < newClean.length; i++) newClean[i]._idx = i;
                    cleanData = newClean;
                    grid.data = newClean;
                }
            });

            // ── default editor checkbox ──
            document.getElementById('defaultEditor').addEventListener('change', function() {
                vscode.postMessage({ command: 'setDefaultEditor', enabled: this.checked });
            });

            // ── search / filter ──
            let searchBox = document.getElementById('search');
            searchBox.addEventListener('input', function() {
                let val = this.value.toLowerCase();
                if (!val) {
                    grid.data = cleanData.slice();
                    currentRowData = rowData;
                    return;
                }
                let filteredClean = [];
                let filteredRaw = [];
                for (let i = 0; i < rowData.length; i++) {
                    let row = rowData[i];
                    let match = false;
                    for (let key in row) {
                        if (key.startsWith('_')) continue;
                        if (String(row[key]).toLowerCase().indexOf(val) !== -1) {
                            match = true;
                            break;
                        }
                    }
                    if (match) {
                        filteredRaw.push(row);
                        filteredClean.push(cleanData[i]);
                    }
                }
                currentRowData = filteredRaw;
                grid.data = filteredClean;
            });

            // Ctrl/Cmd+F to focus search
            document.addEventListener('keydown', function(e) {
                if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
                    e.preventDefault();
                    searchBox.focus();
                    searchBox.select();
                }
            });
        }
    })();
    <\/script>
</body>
</html>`
}

module.exports = {resolveSlkEditor}
