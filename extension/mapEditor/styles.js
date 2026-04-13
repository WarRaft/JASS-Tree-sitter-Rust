function editorStyles() {
    return `
        * { box-sizing: border-box; margin: 0; padding: 0; }

        body {
            overflow: hidden;
            width: 100vw;
            height: 100vh;
            background: #1e1e1e;
            font-family: var(--vscode-font-family, 'Segoe UI', sans-serif);
            font-size: 13px;
            color: var(--vscode-editor-foreground, #ccc);
        }

        #terrain {
            position: absolute;
            top: 0; left: 0;
            width: 100vw;
            height: 100vh;
            z-index: 0;
        }

        /* ── Global loading bar (terrain texture loading) ─────── */
        #globalLoadingBar {
            position: absolute;
            top: 0; left: 0; right: 0;
            height: 3px;
            z-index: 30;
            overflow: hidden;
            pointer-events: none;
            opacity: 0;
            transition: opacity 0.2s;
        }
        #globalLoadingBar.active {
            opacity: 1;
        }
        #globalLoadingBar::after {
            content: '';
            position: absolute;
            top: 0; left: -40%;
            width: 40%; height: 100%;
            background: var(--vscode-progressBar-background, #0e70c0);
            animation: global-loading-slide 1.2s ease-in-out infinite;
        }
        @keyframes global-loading-slide {
            0% { left: -40%; }
            100% { left: 100%; }
        }

        /* ── Sidebar menu ───────────────────────────────────────── */
        .menubar {
            position: absolute;
            top: 0; left: 0; bottom: 24px;
            z-index: 20;
            display: flex;
            flex-direction: column;
            gap: 2px;
            padding: 6px 4px;
            width: auto;
            background: rgba(30, 30, 30, 0.92);
            border-right: 1px solid rgba(255, 255, 255, 0.08);
            backdrop-filter: blur(8px);
            -webkit-backdrop-filter: blur(8px);
            overflow-y: auto;
        }
        .menu-item {
            background: none;
            border: 1px solid transparent;
            border-radius: 4px;
            color: var(--vscode-editor-foreground, #ccc);
            cursor: pointer;
            padding: 5px 14px;
            font-family: inherit;
            font-size: 12px;
            line-height: 1;
            white-space: nowrap;
            text-align: left;
        }
        .menu-item:hover:not(.disabled) { background: rgba(255, 255, 255, 0.08); border-color: rgba(255, 255, 255, 0.1); }
        .menu-item.active {
            background: rgba(255, 255, 255, 0.12);
            border-color: var(--vscode-focusBorder, #007acc);
            color: #fff;
        }
        .menu-item.disabled {
            opacity: 0.35;
            cursor: default;
        }
        .menu-child {
            position: relative;
            padding-left: 22px;
        }
        .menu-child::before {
            content: '';
            position: absolute;
            left: 14px;
            top: 0;
            bottom: 50%;
            width: 1px;
            background: rgba(255, 255, 255, 0.18);
        }
        .menu-child::after {
            content: '';
            position: absolute;
            left: 14px;
            top: 50%;
            width: 6px;
            height: 1px;
            background: rgba(255, 255, 255, 0.18);
        }
        .menu-child-cont::before {
            bottom: 0;
        }
        .menu-sep { height: 1px; background: rgba(255, 255, 255, 0.12); margin: 4px 0; }
        .menu-cb {
            display: flex; align-items: center; gap: 4px;
            cursor: pointer; font-size: 12px;
            color: var(--vscode-editor-foreground, #ccc);
            padding: 3px 6px; border-radius: 3px;
            white-space: nowrap;
        }
        .menu-cb:hover { background: rgba(255, 255, 255, 0.06); }
        .menu-cb input[type="checkbox"] { outline: none; }
        .menu-cb input[type="checkbox"]:focus-visible {
            outline: 1.5px solid var(--vscode-focusBorder, #007acc);
            outline-offset: 1px;
            border-radius: 2px;
        }

        /* ── Cursor info bar ────────────────────────────────────── */
        .cursor-info {
            position: absolute;
            bottom: 0; left: 0; right: 0;
            z-index: 25;
            padding: 4px 12px;
            font-family: var(--vscode-editor-font-family, monospace);
            font-size: 12px;
            color: var(--vscode-editor-foreground, #ccc);
            background: rgba(30, 30, 30, 0.9);
            border-top: 1px solid rgba(255, 255, 255, 0.08);
            backdrop-filter: blur(8px);
            min-height: 24px;
            pointer-events: none;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }
        .cursor-info .ci-label {
            color: var(--vscode-descriptionForeground, #888);
            margin-right: 3px;
        }
        .cursor-info .ci-dim {
            color: var(--vscode-descriptionForeground, #666);
            font-size: 11px;
        }
        .cursor-info .ci-sep {
            color: rgba(255, 255, 255, 0.15);
            margin: 0 6px;
        }
        .cursor-info code {
            background: rgba(255, 255, 255, 0.08);
            padding: 0 3px;
            border-radius: 2px;
            font-size: 11px;
            color: var(--vscode-textPreformat-foreground, #d7ba7d);
        }

        /* ── Floating window action buttons (slotted into Shadow DOM) ── */
        .float-action {
            background: none; border: none;
            color: var(--vscode-editor-foreground, #ccc);
            cursor: pointer; font-size: 14px; line-height: 1;
            padding: 0 4px; border-radius: 3px; opacity: 0.6;
        }
        .float-action:hover { opacity: 1; background: rgba(255, 255, 255, 0.1); }


        /* ── Game Path ──────────────────────────────────────────── */
        .gp-hint {
            font-size: 11px;
            color: var(--vscode-descriptionForeground, #888);
            margin-bottom: 8px;
        }
        .gp-path {
            font-family: var(--vscode-editor-font-family, monospace);
            font-size: 12px;
            color: var(--vscode-editor-foreground, #ccc);
            background: rgba(255, 255, 255, 0.04);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 3px;
            padding: 5px 8px;
            margin-bottom: 8px;
            word-break: break-all;
        }
        .gp-no-path {
            font-size: 12px;
            color: var(--vscode-descriptionForeground, #666);
            font-style: italic;
            margin-bottom: 8px;
        }
        .gp-mpq-list {
            margin-bottom: 8px;
        }
        .gp-mpq-row {
            font-size: 11px;
            padding: 2px 0;
            display: flex;
            align-items: center;
            gap: 6px;
        }
        .gp-mpq-row.gp-ok { color: var(--vscode-editor-foreground, #ccc); }
        .gp-mpq-row.gp-missing { color: var(--vscode-errorForeground, #f48771); }
        .gp-actions {
            display: flex;
            gap: 6px;
        }
        .gp-browse, .gp-clear {
            padding: 4px 14px;
            border: none;
            border-radius: 3px;
            font-family: inherit;
            font-size: 12px;
            cursor: pointer;
        }
        .gp-browse {
            background: var(--vscode-button-background, #0e639c);
            color: var(--vscode-button-foreground, #fff);
        }
        .gp-browse:hover { background: var(--vscode-button-hoverBackground, #1177bb); }
        .gp-clear {
            background: transparent;
            border: 1px solid rgba(255, 255, 255, 0.15);
            color: var(--vscode-descriptionForeground, #888);
        }
        .gp-clear:hover { border-color: rgba(255, 255, 255, 0.3); color: var(--vscode-editor-foreground, #ccc); }

        /* ── Terrain info ───────────────────────────────────────── */
        table.info { border-collapse: collapse; margin-bottom: 8px; width: 100%; }
        table.info td { padding: 2px 8px 2px 0; font-size: 12px; }
        table.info .key { color: var(--vscode-descriptionForeground, #888); white-space: nowrap; }
        table.info .dd-default {
            color: var(--vscode-descriptionForeground, #888);
            opacity: 0.6;
            font-size: 11px;
            white-space: nowrap;
            text-decoration: line-through;
        }
        .gs-resolved {
            color: var(--vscode-textLink-foreground, #3794ff);
            cursor: pointer;
            text-decoration: none;
            border-bottom: 1px dotted var(--vscode-textLink-foreground, #3794ff);
        }
        .gs-resolved:hover {
            color: var(--vscode-textLink-activeForeground, #3794ff);
            border-bottom-style: solid;
        }
        .tw-section-title {
            font-size: 11px; font-weight: 600;
            color: var(--vscode-descriptionForeground, #888);
            margin: 8px 0 4px; text-transform: uppercase; letter-spacing: 0.5px;
        }
        .legend { display: flex; flex-direction: column; gap: 4px; }
        .code {
            font-family: var(--vscode-editor-font-family, monospace);
            font-size: 11px; color: var(--vscode-textLink-foreground, #3794ff);
        }
        .terrain-checks { display: flex; flex-wrap: wrap; gap: 2px 4px; }

        /* ── Tileset window ─────────────────────────────────────── */
        .ts-source {
            font-size: 11px;
            color: var(--vscode-descriptionForeground, #888);
            margin-bottom: 8px;
            padding: 4px 6px;
            background: rgba(255, 255, 255, 0.03);
            border-radius: 3px;
        }
        .ts-no-slk {
            color: var(--vscode-errorForeground, #f48771);
            font-style: italic;
        }
        .ts-slk-link {
            color: var(--vscode-textLink-foreground, #3794ff);
            text-decoration: none;
            cursor: pointer;
        }
        .ts-slk-link:hover {
            text-decoration: underline;
        }
        .ts-slk-source-line {
            font-size: 10px;
            color: var(--vscode-descriptionForeground, #888);
            opacity: 0.7;
            margin-top: 2px;
            word-break: break-all;
        }

        /* ── Doodads sidebar ───────────────────────────────────── */
        .ds-sidebar {
            display: flex;
            flex-direction: column;
            gap: 0;
            width: 170px;
            min-width: 140px;
            background: rgba(30, 30, 30, 0.92);
            border-right: 1px solid rgba(255, 255, 255, 0.08);
            flex-shrink: 0;
            overflow-y: auto;
            padding: 6px 4px;
        }
        .ds-sidebar::-webkit-scrollbar { width: 6px; }
        .ds-sidebar::-webkit-scrollbar-track { background: transparent; }
        .ds-sidebar::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.15); border-radius: 3px; }
        .ds-sidebar::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,0.25); }
        .ds-sidebar slk-source-list {
            margin: 0 0 4px;
        }
        .ds-filter-group {
            margin-bottom: 4px;
            padding: 0 6px;
        }
        .ds-filter-title {
            font-size: 10px;
            font-weight: 600;
            color: var(--vscode-descriptionForeground, #888);
            text-transform: uppercase;
            letter-spacing: 0.5px;
            margin: 6px 0 3px;
            padding-bottom: 2px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.06);
        }
        .ds-sidebar .terrain-checks {
            flex-direction: column;
            gap: 1px;
        }
        .ds-sidebar .menu-cb {
            font-size: 11px;
            padding: 1px 0;
        }
        .ds-ts-badge {
            display: inline-block;
            width: 16px;
            height: 16px;
            line-height: 16px;
            text-align: center;
            font-size: 10px;
            font-family: var(--vscode-editor-font-family, monospace);
            font-weight: 600;
            border-radius: 3px;
            vertical-align: middle;
            background: rgba(255, 255, 255, 0.08);
            color: var(--vscode-descriptionForeground, #999);
            flex-shrink: 0;
        }
        .ds-search {
            display: block;
            width: 100%;
            padding: 6px 10px;
            border: none;
            border-bottom: 1px solid rgba(255, 255, 255, 0.06);
            background: rgba(255, 255, 255, 0.04);
            color: var(--vscode-editor-foreground, #ccc);
            font-family: var(--vscode-editor-font-family, monospace);
            font-size: 12px;
            outline: none;
            flex-shrink: 0;
        }
        .ds-search::placeholder { color: var(--vscode-descriptionForeground, #666); }
        .ds-search:focus { background: rgba(255, 255, 255, 0.07); }
        #dsDoodadList::-webkit-scrollbar,
        #usUnitList::-webkit-scrollbar { width: 6px; }
        #dsDoodadList::-webkit-scrollbar-track,
        #usUnitList::-webkit-scrollbar-track { background: transparent; }
        #dsDoodadList::-webkit-scrollbar-thumb,
        #usUnitList::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.15); border-radius: 3px; }
        #dsDoodadList::-webkit-scrollbar-thumb:hover,
        #usUnitList::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,0.25); }

        /* ── Doodads sort bar ──────────────────────────────────── */
        .ds-sort-bar {
            display: flex;
            align-items: center;
            gap: 0;
            padding: 0 6px;
            background: rgba(255, 255, 255, 0.03);
            border-bottom: 1px solid rgba(255, 255, 255, 0.06);
            flex-shrink: 0;
            font-size: 11px;
            font-weight: 600;
            color: var(--vscode-descriptionForeground, #888);
            user-select: none;
        }
        .ds-sort-col {
            cursor: pointer;
            padding: 3px 6px;
            border-radius: 3px;
            white-space: nowrap;
        }
        .ds-sort-col:hover { background: rgba(255, 255, 255, 0.06); color: var(--vscode-foreground, #ccc); }
        .ds-sort-col.ds-sort-active { color: var(--vscode-textLink-foreground, #3794ff); }
        .ds-sort-col::after { content: ''; margin-left: 2px; }
        .ds-sort-col.ds-sort-asc::after { content: ' ▲'; }
        .ds-sort-col.ds-sort-desc::after { content: ' ▼'; }
        .ds-sort-name { flex: 1; }
        .ds-sort-cat { min-width: 80px; text-align: right; }
        .ds-sort-info {
            font-weight: normal;
            font-size: 10px;
            padding: 0 4px;
            white-space: nowrap;
        }

        /* ── Files window ───────────────────────────────────────── */
        .file-filter {
            display: block;
            width: 100%;
            padding: 6px 10px;
            border: none;
            border-bottom: 1px solid rgba(255, 255, 255, 0.06);
            background: rgba(255, 255, 255, 0.04);
            color: var(--vscode-editor-foreground, #ccc);
            font-family: var(--vscode-editor-font-family, monospace);
            font-size: 12px;
            outline: none;
        }
        .file-filter::placeholder { color: var(--vscode-descriptionForeground, #666); }
        .file-filter:focus { background: rgba(255, 255, 255, 0.07); }
        .files-list {
            max-height: 50vh;
            overflow-y: auto;
        }
        .file-row {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 4px 10px;
            cursor: pointer;
            font-size: 12px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.03);
        }
        .file-row:hover {
            background: var(--vscode-list-hoverBackground, rgba(255, 255, 255, 0.05));
        }
        .file-row:active {
            background: var(--vscode-list-activeSelectionBackground, rgba(0, 122, 204, 0.3));
        }
        .file-num {
            color: var(--vscode-descriptionForeground, #666);
            font-size: 10px;
            min-width: 24px;
            text-align: right;
            font-variant-numeric: tabular-nums;
        }
        .file-name {
            flex: 1;
            font-family: var(--vscode-editor-font-family, monospace);
            font-size: 12px;
            color: var(--vscode-textLink-foreground, #3794ff);
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
        }
        .file-row:hover .file-name { text-decoration: underline; }
        .file-size {
            color: var(--vscode-descriptionForeground, #888);
            font-size: 11px;
            font-variant-numeric: tabular-nums;
            white-space: nowrap;
        }
        .fi-empty {
            padding: 20px;
            text-align: center;
            color: var(--vscode-descriptionForeground, #888);
        }

        /* ── File source badges & filter ────────────────────────── */
        .file-badge {
            display: inline-block;
            padding: 1px 5px;
            border-radius: 3px;
            font-size: 9px;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 0.3px;
            flex-shrink: 0;
        }
        .file-badge-discovered {
            background: rgba(78, 201, 176, 0.2);
            color: #4ec9b0;
        }
        .file-badge-found {
            background: rgba(220, 160, 50, 0.2);
            color: #dca032;
        }
        .file-source-filters {
            display: flex;
            gap: 12px;
            padding: 4px 10px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.06);
            background: rgba(255, 255, 255, 0.02);
        }
        .file-source-label {
            display: flex;
            align-items: center;
            gap: 4px;
            font-size: 11px;
            cursor: pointer;
            color: var(--vscode-descriptionForeground, #888);
            user-select: none;
        }
        .file-source-label input[type="checkbox"] {
            margin: 0;
            cursor: pointer;
        }

        /* ── Header flag tags ─────────────────────────────────────── */
        .flag-tags { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 8px; }
        .flag-tag {
            display: inline-block;
            padding: 2px 7px;
            border-radius: 3px;
            font-size: 11px;
            background: var(--vscode-badge-background, rgba(255,255,255,0.1));
            color: var(--vscode-badge-foreground, #ccc);
        }

        /* ── Folder tree in Files ─────────────────────────────────── */
        .folder-row {
            display: flex;
            align-items: center;
            gap: 4px;
            padding: 4px 10px;
            cursor: pointer;
            font-size: 12px;
            font-weight: 600;
            border-bottom: 1px solid rgba(255, 255, 255, 0.03);
            user-select: none;
            color: var(--vscode-editor-foreground, #ccc);
        }
        .folder-row:hover {
            background: var(--vscode-list-hoverBackground, rgba(255, 255, 255, 0.05));
        }
        .folder-chevron {
            display: inline-block;
            width: 12px;
            text-align: center;
            font-size: 10px;
            transition: transform 0.15s;
            color: var(--vscode-descriptionForeground, #888);
        }
        .folder-row.collapsed .folder-chevron { transform: rotate(-90deg); }
        .folder-icon { font-size: 13px; }
        .folder-name {
            flex: 1;
            font-family: var(--vscode-editor-font-family, monospace);
            font-size: 12px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
        }
        .folder-count {
            color: var(--vscode-descriptionForeground, #888);
            font-size: 10px;
            font-weight: normal;
        }
        .folder-children { }
        .folder-children.collapsed { display: none; }
        .folder-children .file-row { padding-left: 28px; }
        .folder-children .folder-row { padding-left: 28px; }
        .folder-children .folder-children .file-row { padding-left: 46px; }
        .folder-children .folder-children .folder-row { padding-left: 46px; }
        .folder-children .folder-children .folder-children .file-row { padding-left: 64px; }
        .folder-children .folder-children .folder-children .folder-row { padding-left: 64px; }

        /* ── Scrollbar styling ──────────────────────────────────── */
        .files-list::-webkit-scrollbar { width: 6px; }
        .files-list::-webkit-scrollbar-track { background: transparent; }
        .files-list::-webkit-scrollbar-thumb {
            background: rgba(255, 255, 255, 0.15); border-radius: 3px;
        }
        .files-list::-webkit-scrollbar-thumb:hover {
            background: rgba(255, 255, 255, 0.25);
        }

        /* ── W3i Map Info ──────────────────────────────────────── */
        .w3i-desc {
            background: rgba(255, 255, 255, 0.03);
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 4px;
            padding: 6px 8px;
            white-space: pre-wrap;
            word-break: break-word;
            margin: 4px 0;
            font-size: 12px;
            font-family: var(--vscode-editor-font-family, monospace);
            color: var(--vscode-editor-foreground, #ccc);
        }
        .w3i-sub-group {
            margin: 4px 0;
        }
        .w3i-sub-group em {
            display: block;
            margin-bottom: 2px;
            color: var(--vscode-descriptionForeground, #888);
            font-size: 11px;
        }
        #w3iWindow .table-wrap {
            overflow-x: auto;
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 4px;
            margin-bottom: 4px;
        }
        #w3iWindow table {
            width: 100%;
            border-collapse: collapse;
            white-space: nowrap;
        }
        #w3iWindow thead {
            position: sticky;
            top: 0;
            z-index: 1;
        }
        #w3iWindow th {
            background: rgba(255, 255, 255, 0.04);
            color: var(--vscode-descriptionForeground, #888);
            text-align: left;
            padding: 3px 6px;
            border-bottom: 2px solid rgba(255, 255, 255, 0.08);
            font-weight: 600;
            font-size: 11px;
        }
        #w3iWindow td {
            padding: 2px 6px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.04);
            font-size: 12px;
        }
        #w3iWindow tr:hover td {
            background: rgba(255, 255, 255, 0.04);
        }
        #w3iWindow .num { text-align: right; font-variant-numeric: tabular-nums; }
        #w3iWindow .mono { font-family: var(--vscode-editor-font-family, monospace); font-size: 11px; }
        #w3iWindow .tags, #w3iWindow .flag-tags { margin-bottom: 8px; }

        /* ── DOO Placed windows (doodads / units) ────────────── */
        .doo-content {
            display: flex;
            flex-direction: column;
            height: 100%;
            overflow: hidden;
            padding: 6px 10px;
        }
        .doo-content > .meta-banner,
        .doo-content > .info,
        .doo-content > .tw-section-title {
            flex-shrink: 0;
        }
        #doodadDooWindow .table-wrap,
        #destructableDooWindow .table-wrap,
        #unitDooWindow .table-wrap {
            flex: 1;
            min-height: 0;
            overflow: auto;
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 4px;
        }
        #doodadDooWindow table,
        #destructableDooWindow table,
        #unitDooWindow table {
            width: 100%;
            border-collapse: collapse;
            white-space: nowrap;
        }
        #doodadDooWindow thead,
        #destructableDooWindow thead,
        #unitDooWindow thead {
            position: sticky;
            top: 0;
            z-index: 1;
        }
        #doodadDooWindow th,
        #destructableDooWindow th,
        #unitDooWindow th {
            background: rgba(255, 255, 255, 0.04);
            color: var(--vscode-descriptionForeground, #888);
            text-align: left;
            padding: 3px 6px;
            border-bottom: 2px solid rgba(255, 255, 255, 0.08);
            font-weight: 600;
            font-size: 11px;
        }
        #doodadDooWindow td,
        #destructableDooWindow td,
        #unitDooWindow td {
            padding: 2px 6px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.04);
            font-size: 12px;
        }
        #doodadDooWindow tr:hover td,
        #destructableDooWindow tr:hover td,
        #unitDooWindow tr:hover td {
            background: rgba(255, 255, 255, 0.04);
        }
        #doodadDooWindow .num,
        #destructableDooWindow .num,
        #unitDooWindow .num { text-align: right; font-variant-numeric: tabular-nums; }
        #doodadDooWindow .mono,
        #destructableDooWindow .mono,
        #unitDooWindow .mono { font-family: var(--vscode-editor-font-family, monospace); font-size: 11px; }
        #doodadDooWindow .code,
        #destructableDooWindow .code,
        #unitDooWindow .code { font-family: var(--vscode-editor-font-family, monospace); font-size: 11px; }
        .doo-highlight td {
            background: rgba(55, 148, 255, 0.2) !important;
        }

        /* ── Meta banner (bytes read) ─────────────────────────── */
        .meta-banner {
            display: inline-flex;
            align-items: center;
            gap: 0.5rem;
            padding: 0.3rem 0.75rem;
            border-radius: 4px;
            font-size: 12px;
            margin-bottom: 0.75rem;
            font-variant-numeric: tabular-nums;
        }
        .meta-banner.ok {
            background: rgba(78, 201, 176, 0.12);
            color: #4ec9b0;
            border: 1px solid rgba(78, 201, 176, 0.3);
        }
        .meta-banner.warn {
            background: rgba(224, 108, 64, 0.12);
            color: #e06c40;
            border: 1px solid rgba(224, 108, 64, 0.3);
        }
        .meta-banner.error {
            background: rgba(244, 71, 71, 0.12);
            color: #f44747;
            border: 1px solid rgba(244, 71, 71, 0.3);
        }

        /* ── Model viewer ──────────────────────────────────────── */
        .mv-sidebar {
            display: flex;
            flex-direction: column;
            gap: 2px;
            padding: 6px 4px;
            width: auto;
            background: rgba(30, 30, 30, 0.92);
            border-right: 1px solid rgba(255, 255, 255, 0.08);
            flex-shrink: 0;
            overflow-y: auto;
        }
        .mv-sb-item {
            background: none;
            border: 1px solid transparent;
            border-radius: 4px;
            color: var(--vscode-editor-foreground, #ccc);
            cursor: pointer;
            padding: 5px 14px;
            font-family: inherit;
            font-size: 12px;
            line-height: 1;
            white-space: nowrap;
            text-align: left;
        }
        .mv-sb-item:hover { background: rgba(255, 255, 255, 0.08); border-color: rgba(255, 255, 255, 0.1); }
        .mv-sb-item.active {
            background: rgba(255, 255, 255, 0.12);
            border-color: var(--vscode-focusBorder, #007acc);
            color: #fff;
        }
        .mv-sb-sep { height: 1px; background: rgba(255, 255, 255, 0.12); margin: 4px 0; }
        .mv-toolbar {
            display: flex;
            align-items: center;
            gap: 0.6rem;
            padding: 4px 8px;
            background: rgba(255, 255, 255, 0.03);
            border-bottom: 1px solid rgba(255, 255, 255, 0.06);
            flex-shrink: 0;
            font-size: 12px;
        }
        .mv-toolbar strong {
            font-size: 12px;
            max-width: 200px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
        }
        .mv-info {
            color: var(--vscode-descriptionForeground, #888);
            margin-left: auto;
            font-size: 11px;
            white-space: nowrap;
        }
        .mv-canvas-container {
            flex: 1;
            position: relative;
            overflow: hidden;
            min-height: 0;
        }
        .mv-canvas-container canvas {
            display: block;
            width: 100%;
            height: 100%;
        }
        .mv-mat-title {
            font-size: 11px; font-weight: 600;
            color: var(--vscode-descriptionForeground, #888);
            padding: 8px 10px 4px;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }
        .mv-mat-list { padding: 0 6px 6px; }
        .mv-mat-row {
            display: flex;
            align-items: center;
            gap: 6px;
            padding: 4px 6px;
            border-radius: 3px;
            font-size: 12px;
            cursor: pointer;
        }
        .mv-mat-row:hover { background: rgba(255, 255, 255, 0.06); }
        .mv-mat-row.mv-hidden { opacity: 0.35; }
        .mv-mat-swatch {
            width: 14px; height: 14px;
            border-radius: 3px;
            border: 1px solid rgba(255, 255, 255, 0.2);
            flex-shrink: 0;
        }
        .mv-mat-label { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
        .mv-mat-eye { font-size: 11px; opacity: 0.5; }
        .mv-team-color-label {
            display: flex;
            align-items: center;
            gap: 6px;
            padding: 5px 14px;
            font-size: 12px;
            color: var(--vscode-editor-foreground, #ccc);
            cursor: pointer;
            white-space: nowrap;
        }
        .mv-team-color-label input[type="color"] {
            -webkit-appearance: none;
            appearance: none;
            width: 22px;
            height: 22px;
            border: 1px solid rgba(255, 255, 255, 0.2);
            border-radius: 4px;
            padding: 0;
            cursor: pointer;
            background: none;
            flex-shrink: 0;
        }
        .mv-team-color-label input[type="color"]::-webkit-color-swatch-wrapper {
            padding: 2px;
        }
        .mv-team-color-label input[type="color"]::-webkit-color-swatch {
            border: none;
            border-radius: 2px;
        }
        .mv-sidebar::-webkit-scrollbar { width: 6px; }
        .mv-sidebar::-webkit-scrollbar-track { background: transparent; }
        .mv-sidebar::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.15); border-radius: 3px; }
        .mv-sidebar::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,0.25); }

        /* ── Model viewer: material items ─────────────────────── */
        .mv-mat-item {
            border-bottom: 1px solid rgba(255, 255, 255, 0.06);
            padding: 6px;
        }
        .mv-mat-item.mv-hidden {
            opacity: 0.35;
        }
        .mv-mat-item-header {
            display: flex;
            align-items: center;
            gap: 4px;
            font-size: 11px;
            font-weight: 600;
            color: var(--vscode-editor-foreground, #ccc);
            margin-bottom: 4px;
        }
        .mv-mat-header-label {
            flex: 1;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
        }
        .mv-mat-eye-btn {
            font-size: 11px;
            opacity: 0.4;
            cursor: pointer;
            flex-shrink: 0;
            padding: 0 2px;
            border-radius: 3px;
        }
        .mv-mat-eye-btn:hover {
            opacity: 1;
            background: rgba(255, 255, 255, 0.1);
        }
        .mv-mat-layer {
            background: rgba(255, 255, 255, 0.03);
            border: 1px solid rgba(255, 255, 255, 0.06);
            border-radius: 3px;
            padding: 4px 6px;
            margin: 4px 0;
            font-size: 11px;
        }
        .mv-mat-layer-row {
            display: flex;
            gap: 4px;
            padding: 1px 0;
            min-width: 0;
        }
        .mv-mat-layer-row > span:last-child {
            min-width: 0;
            word-break: break-all;
        }
        .mv-mat-layer-label {
            color: var(--vscode-descriptionForeground, #888);
            white-space: nowrap;
        }
        .mv-mat-thumb-wrap {
            display: inline-flex;
            margin-top: 4px;
            border-radius: 3px;
            border: 1px solid rgba(255, 255, 255, 0.1);
            overflow: hidden;
            cursor: pointer;
            background-image: repeating-conic-gradient(#555 0% 25%, #333 0% 50%);
            background-size: 12px 12px;
            background-position: 0 0;
            background-repeat: repeat;
        }
        .mv-mat-thumb-wrap:hover {
            border-color: var(--vscode-focusBorder, #007acc);
            box-shadow: 0 0 0 1px var(--vscode-focusBorder, #007acc);
        }
        .mv-mat-thumb {
            display: block;
            max-width: 100%;
            max-height: 96px;
            object-fit: contain;
        }
        .mv-mat-thumb-placeholder {
            font-size: 10px;
            color: var(--vscode-descriptionForeground, #666);
            font-style: italic;
            padding: 4px 0;
        }

        /* ── Custom context menu ──────────────────────────────── */
        .ctx-menu {
            position: fixed;
            z-index: 100;
            min-width: 180px;
            background: rgba(37, 37, 38, 0.96);
            border: 1px solid rgba(255, 255, 255, 0.12);
            border-radius: 6px;
            box-shadow: 0 6px 24px rgba(0, 0, 0, 0.5);
            backdrop-filter: blur(12px);
            -webkit-backdrop-filter: blur(12px);
            padding: 4px 0;
            font-size: 12px;
            overflow: hidden;
        }
        .ctx-menu[hidden] { display: none !important; }
        .ctx-item {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 6px 14px;
            cursor: pointer;
            color: var(--vscode-editor-foreground, #ccc);
            white-space: nowrap;
        }
        .ctx-item:hover {
            background: var(--vscode-list-activeSelectionBackground, rgba(0, 122, 204, 0.3));
            color: #fff;
        }
        .ctx-sep {
            height: 1px;
            background: rgba(255, 255, 255, 0.08);
            margin: 4px 0;
        }

        /* ── Doodad detail model links ───────────────────────── */
        .dd-model-link {
            display: block;
            font-family: var(--vscode-editor-font-family, monospace);
            font-size: 11px;
            color: var(--vscode-textLink-foreground, #3794ff);
            text-decoration: none;
            padding: 1px 0;
            cursor: pointer;
        }
        .dd-model-link:hover {
            text-decoration: underline;
            color: var(--vscode-textLink-activeForeground, #4fc3f7);
        }

        /* ── Placed doodad ID links ─────────────────────────── */
        .doo-id-link, .doo-unit-link {
            color: var(--vscode-textLink-foreground, #3794ff);
            text-decoration: none;
            cursor: pointer;
        }
        .doo-id-link:hover, .doo-unit-link:hover {
            text-decoration: underline;
            color: var(--vscode-textLink-activeForeground, #4fc3f7);
        }
        .doo-resolved-name {
            opacity: 0.7;
            font-style: italic;
            font-family: inherit;
        }
        .doo-error-row {
            background: rgba(255, 80, 80, 0.10);
        }
        .doo-error-row td:first-child::before {
            content: '\u26a0 ';
            color: var(--vscode-errorForeground, #f44);
        }

        /* ── Doodad detail color badge ────────────────────────── */

        .dd-color-badge {
            display: inline-block;
            width: 14px;
            height: 14px;
            border-radius: 3px;
            border: 1px solid rgba(255, 255, 255, 0.25);
            vertical-align: middle;
            margin-left: 4px;
        }

        /* ── Path texture link ─────────────────────────────────── */
        .dd-pathtex-link {
            display: block;
            font-family: var(--vscode-editor-font-family, monospace);
            font-size: 11px;
            color: var(--vscode-textLink-foreground, #3794ff);
            text-decoration: none;
            padding: 1px 0;
            cursor: pointer;
        }
        .dd-pathtex-link:hover {
            text-decoration: underline;
            color: var(--vscode-textLink-activeForeground, #4fc3f7);
        }

        /* ── Path texture viewer ───────────────────────────────── */
        .ptex-legend {
            display: flex;
            gap: 16px;
            padding: 8px 10px;
            font-size: 12px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.08);
            flex-wrap: wrap;
            align-items: center;
        }
        .ptex-legend-row {
            display: flex;
            align-items: center;
            gap: 6px;
        }
        .ptex-legend-cell {
            display: inline-grid;
            grid-template-columns: 1fr 1fr;
            grid-template-rows: 1fr 1fr;
            width: 20px;
            height: 20px;
            border: 1px solid rgba(255,255,255,0.2);
            border-radius: 2px;
            overflow: hidden;
        }
        .ptex-legend-cell > span {
            width: 10px;
            height: 10px;
        }
        .ptex-source {
            padding: 4px 10px;
            font-size: 11px;
            opacity: 0.6;
        }
        .ptex-grid {
            display: grid;
            gap: 1px;
            padding: 10px;
            background: rgba(0,0,0,0.2);
        }
        .ptex-cell {
            display: grid;
            grid-template-columns: 1fr 1fr;
            grid-template-rows: 1fr 1fr;
            width: 24px;
            height: 24px;
            border: 1px solid rgba(255,255,255,0.08);
            border-radius: 2px;
            overflow: hidden;
        }
        .ptex-cell > span {
            width: 12px;
            height: 12px;
        }
        .ptex-loading {
            padding: 16px;
            text-align: center;
            opacity: 0.6;
        }
        .ptex-error {
            padding: 16px;
            text-align: center;
            color: var(--vscode-errorForeground, #f44);
        }

        /* ── BLP Viewer ───────────────────────────────────── */
        .blp-viewer {
            display: flex;
            flex-direction: column;
            height: 100%;
            overflow: hidden;
        }
        .blp-toolbar {
            display: flex;
            align-items: center;
            gap: 1rem;
            padding: 6px 10px;
            border-bottom: 1px solid var(--vscode-editorWidget-border, #444);
            flex-shrink: 0;
        }
        .blp-toggle {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            font-size: 12px;
            cursor: pointer;
            user-select: none;
        }
        .blp-empty {
            padding: 2rem;
            text-align: center;
            color: var(--vscode-descriptionForeground, #888);
        }
        .blp-mipmaps {
            flex: 1;
            overflow: auto;
            padding: 10px;
        }
        .blp-mipmap {
            border: 1px solid var(--vscode-editorWidget-border, #444);
            background: var(--vscode-editorWidget-background, #252526);
            padding: 8px;
            margin-bottom: 10px;
            border-radius: 4px;
        }
        .blp-mip-meta {
            display: flex;
            justify-content: space-between;
            align-items: center;
            font-weight: 600;
            margin-bottom: 6px;
            font-size: 12px;
        }
        .blp-mip-size {
            color: var(--vscode-descriptionForeground, #888);
        }
        .blp-img-wrap {
            display: inline-flex;
        }
        .blp-img-wrap.checker {
            background-image: repeating-conic-gradient(#888 0% 25%, #444 0% 50%);
            background-size: 16px 16px;
            background-position: 0 0;
            background-repeat: repeat;
            background-color: white;
        }
        .blp-img-wrap img {
            max-width: 100%;
            height: auto;
            display: block;
            border: 0;
            image-rendering: pixelated;
        }
        .blp-no-image {
            padding: 1rem;
            text-align: center;
            color: var(--vscode-disabledForeground, #666);
            background: var(--vscode-editor-background, #1e1e1e);
            border: 1px dashed var(--vscode-editorWidget-border, #444);
            border-radius: 4px;
        }
        .blp-mip-actions {
            display: inline-flex;
            align-items: center;
            gap: 6px;
        }
        .blp-alpha-btn {
            font-size: 11px;
            padding: 1px 6px;
            border: 1px solid rgba(255, 255, 255, 0.15);
            border-radius: 3px;
            background: transparent;
            color: var(--vscode-editor-foreground, #ccc);
            cursor: pointer;
            user-select: none;
            line-height: 1.4;
            font-family: inherit;
        }
        .blp-alpha-btn:hover {
            background: rgba(255, 255, 255, 0.1);
        }

        /* ── Alpha Test window ──────────────────────────────── */
        .blp-at-body {
            display: flex;
            flex-direction: column;
            height: 100%;
            overflow: hidden;
        }
        .blp-at-toolbar {
            display: flex;
            align-items: center;
            gap: 1rem;
            padding: 6px 10px;
            border-bottom: 1px solid var(--vscode-editorWidget-border, #444);
            flex-shrink: 0;
            font-size: 12px;
        }
        .blp-at-slider-wrap {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            font-size: 12px;
            cursor: pointer;
            user-select: none;
        }
        .blp-at-slider-wrap input[type="range"] {
            width: 120px;
            accent-color: var(--vscode-focusBorder, #007acc);
        }
        .blp-at-slider-wrap span {
            font-family: monospace;
            min-width: 32px;
            text-align: right;
        }
        .blp-at-canvas-wrap {
            flex: 1;
            overflow: auto;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 10px;
        }
        .blp-at-canvas-wrap.checker {
            background-image: repeating-conic-gradient(#888 0% 25%, #444 0% 50%);
            background-size: 16px 16px;
            background-position: 0 0;
            background-repeat: repeat;
        }
        .blp-at-canvas-wrap canvas {
            display: block;
            image-rendering: pixelated;
            max-width: 100%;
            height: auto;
        }

        /* ── Animation panel ──────────────────────────────────── */
        .mv-anim-list {
            padding: 0;
            overflow-y: auto;
            max-height: 100%;
        }
        .mv-anim-list::-webkit-scrollbar { width: 6px; }
        .mv-anim-list::-webkit-scrollbar-track { background: transparent; }
        .mv-anim-list::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.15); border-radius: 3px; }
        .mv-anim-list::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,0.25); }
        .mv-anim-item {
            border-bottom: 1px solid rgba(255, 255, 255, 0.06);
            padding: 6px 8px;
            transition: background 0.1s;
        }
        .mv-anim-item:hover {
            background: rgba(255, 255, 255, 0.03);
        }
        .mv-anim-item.mv-anim-active {
            background: rgba(0, 122, 204, 0.15);
            border-left: 2px solid var(--vscode-focusBorder, #007acc);
        }
        .mv-anim-header {
            display: flex;
            align-items: center;
            gap: 6px;
            margin-bottom: 4px;
        }
        .mv-anim-play-btn {
            width: 22px;
            height: 22px;
            border: none;
            border-radius: 4px;
            background: rgba(255, 255, 255, 0.08);
            color: var(--vscode-editor-foreground, #ccc);
            font-size: 12px;
            cursor: pointer;
            display: flex;
            align-items: center;
            justify-content: center;
            flex-shrink: 0;
            padding: 0;
            line-height: 1;
        }
        .mv-anim-play-btn:hover {
            background: rgba(255, 255, 255, 0.16);
        }
        .mv-anim-play-btn.playing {
            background: var(--vscode-button-background, #0e639c);
            color: var(--vscode-button-foreground, #fff);
        }
        .mv-anim-name {
            flex: 1;
            font-size: 12px;
            font-weight: 600;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            cursor: pointer;
        }
        .mv-anim-duration {
            font-size: 10px;
            color: var(--vscode-descriptionForeground, #888);
            white-space: nowrap;
            flex-shrink: 0;
        }
        .mv-anim-flags {
            display: flex;
            gap: 4px;
            font-size: 9px;
            margin-left: 4px;
            flex-shrink: 0;
        }
        .mv-anim-flag {
            padding: 1px 4px;
            border-radius: 2px;
            background: rgba(255, 255, 255, 0.06);
            color: var(--vscode-descriptionForeground, #888);
        }
        .mv-anim-slider-row {
            display: flex;
            align-items: center;
            gap: 6px;
        }
        .mv-anim-slider {
            flex: 1;
            height: 4px;
            -webkit-appearance: none;
            appearance: none;
            background: rgba(255, 255, 255, 0.1);
            border-radius: 2px;
            outline: none;
            cursor: pointer;
        }
        .mv-anim-slider::-webkit-slider-thumb {
            -webkit-appearance: none;
            appearance: none;
            width: 10px;
            height: 10px;
            border-radius: 50%;
            background: var(--vscode-button-background, #0e639c);
            cursor: pointer;
            border: none;
        }
        .mv-anim-slider::-webkit-slider-thumb:hover {
            background: var(--vscode-button-hoverBackground, #1177bb);
            transform: scale(1.3);
        }
        .mv-anim-frame-label {
            font-size: 10px;
            font-family: var(--vscode-editor-font-family, monospace);
            color: var(--vscode-descriptionForeground, #888);
            min-width: 60px;
            text-align: right;
            white-space: nowrap;
            font-variant-numeric: tabular-nums;
        }
        .mv-anim-empty {
            padding: 20px;
            text-align: center;
            color: var(--vscode-descriptionForeground, #888);
            font-size: 12px;
        }
    `
}

module.exports = {editorStyles}

