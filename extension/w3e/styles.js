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
            color: var(--vscode-descriptionForeground, #888);
            background: rgba(30, 30, 30, 0.85);
            border-top: 1px solid rgba(255, 255, 255, 0.08);
            backdrop-filter: blur(8px);
            min-height: 24px;
            pointer-events: none;
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
    `
}

module.exports = {editorStyles}

