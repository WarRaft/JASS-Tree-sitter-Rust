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
 * @param {import('vscode-languageclient').LanguageClient} client
 * @param {import('vscode').ExtensionContext} context
 */
async function resolveSlkEditor(document, webviewPanel, _token, client, context) {
    const webview = webviewPanel.webview
    webview.options = {enableScripts: true}

    const vendorDir = Uri.file(path.join(context.extensionPath, 'extension', 'vendor'))
    const canvasDatagridJsUri = webview.asWebviewUri(Uri.joinPath(vendorDir, 'canvas-datagrid.js'))

    let suppressRefresh = false

    /**
     * Fetch fresh data from LSP and return parsed schema + rows.
     * @returns {Promise<{schema: object[], rowData: object[], cols: number, rows: number, fname: string}|null>}
     */
    async function fetchData() {
        /** @type {SlkRenderResult} */
        const result = await client.sendRequest('slk/render', {
            uri: document.uri.toString()
        })

        if (result.error) return null

        const {cols, rows, grid} = result
        const fname = document.uri.fsPath.split(/[\\/]/).pop() || 'slk'
        const {settings} = readSettings(document)
        const savedWidths = settings.columnWidths || {}
        const hiddenCols = settings.hiddenColumns || []

        const schema = []
        for (let c = 0; c < cols; c++) {
            const headerVal = grid[0] && grid[0][c] ? grid[0][c].value : `Col ${c + 1}`
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

        return {schema, rowData, cols, rows, fname}
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
 * @param {{schema: object[], rowData: object[], cols: number, rows: number, fname: string}} data
 * @param {import('vscode').Uri} canvasDatagridJsUri
 * @param {boolean} defaultEditorChecked
 */
function buildHtml(data, canvasDatagridJsUri, defaultEditorChecked) {
    const {schema, rowData, cols, rows, fname} = data
    const schemaJson = JSON.stringify(schema)
    const rowDataJson = JSON.stringify(rowData)

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
        var vscode = acquireVsCodeApi();
        var schema = ${schemaJson};
        var rowData = ${rowDataJson};

        // ── preloader helpers ──
        var preloader = document.getElementById('preloader');
        var steps = {
            parse:  document.getElementById('step-parse'),
            grid:   document.getElementById('step-grid'),
            theme:  document.getElementById('step-theme'),
            layout: document.getElementById('step-layout')
        };
        function markDone(id) {
            var el = steps[id];
            if (!el) return;
            el.classList.remove('active');
            el.classList.add('done');
            el.querySelector('.icon').textContent = '\u2713';
        }
        function markActive(id) {
            var el = steps[id];
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
        var cs = getComputedStyle(document.documentElement);
        function cv(name, fallback) {
            return cs.getPropertyValue(name).trim() || fallback;
        }
        var editorBg    = cv('--vscode-editor-background', '#1e1e1e');
        var editorFg    = cv('--vscode-editor-foreground', '#cccccc');
        var widgetBg    = cv('--vscode-editorWidget-background', '#252526');
        var widgetBorder= cv('--vscode-editorWidget-border', '#454545');
        var headerBg    = cv('--vscode-editorGroupHeader-tabsBackground', '#2d2d2d');
        var descFg      = cv('--vscode-descriptionForeground', '#969696');
        var selBg       = cv('--vscode-editor-selectionBackground', 'rgba(38,79,120,0.7)');
        var selFg       = cv('--vscode-list-activeSelectionForeground', editorFg);
        var focusBorder = cv('--vscode-focusBorder', '#007fd4');
        var inputBg     = cv('--vscode-input-background', '#3c3c3c');
        var inputFg     = cv('--vscode-input-foreground', '#cccccc');
        var fontFamily  = cv('--vscode-font-family', 'sans-serif');
        markDone('theme');

        // ── helpers ──
        function stripMeta(rows) {
            return rows.map(function(row) {
                var clean = {};
                for (var key in row) {
                    if (!key.startsWith('_')) clean[key] = row[key];
                }
                return clean;
            });
        }

        var cleanData = stripMeta(rowData);
        // Hidden index column — used to restore original file order
        schema.push({name: '_idx', hidden: true, type: 'number'});
        for (var i = 0; i < cleanData.length; i++) cleanData[i]._idx = i;

        var currentRowData = rowData;
        var container = document.getElementById('table-container');
        var grid; // assigned once layout is ready
        var lastW = 0, lastH = 0;

        var gridStyle = {
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

        var gridElement = document.createElement('div');
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
            var h = container.offsetHeight;
            var w = container.offsetWidth;
            if (h > 0 && w > 0) {
                lastH = h; lastW = w;
                grid.style.height = h + 'px';
                grid.style.width  = w + 'px';
                if (typeof grid.resize === 'function') grid.resize(true);
                if (typeof grid.draw === 'function') grid.draw();
            }
        }

        var attempts = 0;
        function pump() {
            applySize();
            window.dispatchEvent(new Event('resize'));
            attempts++;

            // grid.visibleCells is populated only after a successful draw
            var rendered = grid.visibleCells && grid.visibleCells.length > 0;

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
            var nh = container.offsetHeight;
            var nw = container.offsetWidth;
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

            // ── 3-state sort: asc → desc → original file order ──
            var lastSortCol = null;
            var lastSortDir = null;

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
                var settings = {};

                // Column widths — take user-resized width first, fall back to schema width
                var widths = {};
                var s = grid.schema;
                for (var i = 0; i < s.length; i++) {
                    var col = s[i];
                    if (col.name === '_idx') continue;
                    var w = grid.sizes.columns[i] || col.width;
                    if (w && col.name) widths[col.name] = w;
                }
                if (Object.keys(widths).length > 0) settings.columnWidths = widths;

                // Hidden columns (exclude the internal _idx)
                var hidden = [];
                for (var i = 0; i < s.length; i++) {
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
                var cell = e.cell;
                if (!cell) return;
                var field = cell.header ? cell.header.name : null;
                if (!field) return;

                var meta = getMetaForCell(cell.rowIndex, field);
                if (!meta || meta.start == null) return;

                var newValue = e.value;
                if (newValue === e.oldValue) return;

                vscode.postMessage({
                    command: 'edit',
                    start: meta.start,
                    len: meta.len,
                    value: String(newValue)
                });
            });

            // ── column resize persistence (debounced) ──
            var resizeTimer = null;
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
                var clickedCol = e.cell.header;
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
                var hasHidden = false;
                var s = grid.schema;
                for (var i = 0; i < s.length; i++) {
                    if (s[i].hidden && s[i].name !== '_idx') { hasHidden = true; break; }
                }
                if (hasHidden) {
                    e.items.push({
                        title: 'Show all columns',
                        click: function() {
                            var s = grid.schema;
                            for (var i = 0; i < s.length; i++) {
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
                var msg = event.data;
                if (msg.command === 'updateData') {
                    currentRowData = msg.rowData;
                    var newClean = stripMeta(msg.rowData);
                    for (var i = 0; i < newClean.length; i++) newClean[i]._idx = i;
                    cleanData = newClean;
                    grid.data = newClean;
                }
            });

            // ── default editor checkbox ──
            document.getElementById('defaultEditor').addEventListener('change', function() {
                vscode.postMessage({ command: 'setDefaultEditor', enabled: this.checked });
            });

            // ── search / filter ──
            var searchBox = document.getElementById('search');
            searchBox.addEventListener('input', function() {
                var val = this.value.toLowerCase();
                if (!val) {
                    grid.data = cleanData.slice();
                    currentRowData = rowData;
                    return;
                }
                var filteredClean = [];
                var filteredRaw = [];
                for (var i = 0; i < rowData.length; i++) {
                    var row = rowData[i];
                    var match = false;
                    for (var key in row) {
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
