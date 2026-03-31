const {esc, indexToRgb, TILESET_NAMES} = require('./utils.js')
const {renderHeaderContent, renderGamePathContent, renderFilesRows, renderW3iContent, renderDooContent} = require('./panels.js')
const {editorStyles} = require('./styles.js')

function renderMeta(meta) {
    if (!meta) return ''
    if (meta.remaining === 0) {
        return `<div class="meta-banner ok">✓ All ${meta.total} bytes read</div>`
    }
    return `<div class="meta-banner warn">⚠ ${meta.remaining} of ${meta.total} bytes not read (parser stopped at 0x${meta.read.toString(16).toUpperCase()})</div>`
}

/**
 * Build the full HTML page for the map editor webview.
 *
 * @param {Object|null} terrainData  — parsed w3e data, or null if unavailable
 * @param {string}      fname       — display file name
 * @param {string}      threeSrc    — webview URI to three.min.js
 * @param {Object}      mapInfo     — { mapName, binaries, currentFile, isArchive, isMap, archiveFiles, componentsSrc }
 */
function renderMapEditor(terrainData, fname, threeSrc, mapInfo) {
    const hasTerrain = !!terrainData

    let renderData = null
    let totalTiles = 0
    let totalCliffTiles = 0
    let tilesetName = ''
    let legendItems = ''
    let cliffLegendItems = ''
    let terrainSlkSource = ''
    let w = 0, h = 0

    // Build a tileID → SLK row lookup
    const slkMap = {}
    if (terrainData && terrainData._terrainSlk && terrainData._terrainSlk.tiles) {
        terrainSlkSource = terrainData._terrainSlk.source || ''
        for (const t of terrainData._terrainSlk.tiles) {
            slkMap[t.tileId] = t
        }
    }

    if (hasTerrain) {
        w = terrainData.map_width
        h = terrainData.map_height
        totalTiles = terrainData.ground_tiles ? terrainData.ground_tiles.length : 0
        totalCliffTiles = terrainData.cliff_tiles ? terrainData.cliff_tiles.length : 0
        tilesetName = TILESET_NAMES[terrainData.tileset] || terrainData.tileset

        if (terrainData.ground_tiles) {
            legendItems = terrainData.ground_tiles.map((code, i) => {
                const [r, g, b] = indexToRgb(i)
                const info = slkMap[code]
                const name = info ? info.comment : ''
                const tilePath = info && info.dir && info.file
                    ? info.dir + '\\' + info.file + (info.ext || '') : ''
                return `<tile-item index="${i}" code="${esc(code)}" tile-name="${esc(name)}" tile-path="${esc(tilePath)}" swatch-color="${r},${g},${b}"></tile-item>`
            }).join('\n')
        }

        if (terrainData.cliff_tiles) {
            cliffLegendItems = terrainData.cliff_tiles.map((code, i) => {
                return `<tile-item index="${i}" code="${esc(code)}"></tile-item>`
            }).join('\n')
        }

        renderData = {
            w, h, totalTiles,
            offsetX: terrainData.offset_x,
            offsetY: terrainData.offset_y,
            tileTextures: terrainData._tileTextures || [],
            groundTexture: terrainData.points.map(p => p.ground_texture),
            groundVariation: terrainData.points.map(p => p.ground_variation),
            groundHeight: terrainData.points.map(p => p.ground_height),
            waterFlag: terrainData.points.map(p => p.water ? 1 : 0),
            boundaryFlag: terrainData.points.map(p => p.boundary ? 1 : 0),
            blightFlag: terrainData.points.map(p => p.blight ? 1 : 0),
            rampFlag: terrainData.points.map(p => p.ramp ? 1 : 0),
            cliffVariation: terrainData.points.map(p => p.cliff_variation),
            cliffTexture: terrainData.points.map(p => p.cliff_texture),
            layerHeight: terrainData.points.map(p => p.layer_height),
        }
    }

    const headerContent = renderHeaderContent(mapInfo.archiveHeader)
    const gamePathContent = renderGamePathContent(mapInfo.gamePath, mapInfo.mpqStatus)
    const w3iContent = renderW3iContent(mapInfo.w3iData)
    const hasW3i = !!mapInfo.w3iData
    const unitDooContent = renderDooContent(mapInfo.unitDooData, true)
    const hasUnitDoo = !!mapInfo.unitDooData
    const doodadDooContent = renderDooContent(mapInfo.doodadDooData, false)
    const hasDoodadDoo = !!mapInfo.doodadDooData
    const isDoo = !!mapInfo.isDoo
    const fileCount = mapInfo.archiveFiles ? mapInfo.archiveFiles.length : 0
    const filesRows = mapInfo.isArchive ? renderFilesRows(mapInfo.archiveFiles) : ''

    // Build doodads SLK data
    let doodadsSlkSource = ''
    let doodadsSlkItems = ''
    if (terrainData && terrainData._doodadsSlk) {
        doodadsSlkSource = terrainData._doodadsSlk.source || ''
        if (terrainData._doodadsSlk.doodads && terrainData._doodadsSlk.doodads.length > 0) {
            doodadsSlkItems = terrainData._doodadsSlk.doodads.map(d => {
                return `<doodad-item dood-id="${esc(d.doodId)}" dood-name="${esc(d.name)}" comment="${esc(d.comment)}" dood-class="${esc(d.doodClass)}" category="${esc(d.category)}" file="${esc(d.file)}" tilesets="${esc(d.tilesets)}" num-var="${d.numVar}" def-scale="${d.defScale}" min-scale="${d.minScale}" max-scale="${d.maxScale}"></doodad-item>`
            }).join('\n')
        }
    }
    const hasDoodadsSlk = !!(terrainData && terrainData._doodadsSlk)

    // Build units SLK data
    let unitsSlkSource = ''
    let unitsSlkItems = ''
    if (terrainData && terrainData._unitsSlk) {
        unitsSlkSource = terrainData._unitsSlk.source || ''
        if (terrainData._unitsSlk.units && terrainData._unitsSlk.units.length > 0) {
            unitsSlkItems = terrainData._unitsSlk.units.map(u => {
                return `<unit-item unit-id="${esc(u.unitId)}" comment="${esc(u.comment)}" race="${esc(u.race)}" move-tp="${esc(u.moveTp)}" threat="${u.threat}" points="${u.points}"></unit-item>`
            }).join('\n')
        }
    }
    const hasUnitsSlk = !!(terrainData && terrainData._unitsSlk)

    const nonce = mapInfo.nonce || ''
    const cspSource = mapInfo.cspSource || ''
    const componentsSrc = mapInfo.componentsSrc || ''
    const terrainSrc = mapInfo.terrainSrc || ''

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${cspSource} data: blob:; script-src 'nonce-${nonce}'; style-src 'unsafe-inline'; font-src ${cspSource};" />
    <style>${editorStyles()}</style>
</head>
<body>
    <canvas id="terrain"></canvas>
    <div id="cursor-info" class="cursor-info"></div>

    <!-- ── Menu bar ───────────────────────────────────────────── -->
    <div class="menubar" id="menubar">
        <button class="menu-item" data-action="toggleWindow" data-target="gamePathWindow" title="Warcraft III installation path">\u2699 Game Path</button>
        <button class="menu-item${mapInfo.isArchive ? '' : ' disabled'}" ${mapInfo.isArchive ? 'data-action="toggleWindow" data-target="headerWindow"' : ''}
                title="${mapInfo.isArchive ? 'Archive header info' : 'Available only for archives (.w3x, .w3m, .w3n, .mpq)'}">\ud83d\udce6 Header</button>
        <button class="menu-item${hasW3i ? '' : ' disabled'}" ${hasW3i ? 'data-action="toggleWindow" data-target="w3iWindow"' : ''}
                title="${hasW3i ? 'Map info (war3map.w3i)' : 'No map info available'}">\ud83d\udcdc Map Info</button>
        <button class="menu-item" data-action="toggleWindow" data-target="unitsSlkWindow"
                title="Units catalog (UnitData.slk)">\ud83d\udde1 Units</button>
        <button class="menu-item menu-child" data-action="toggleWindow" data-target="unitDooWindow"
                title="Placed units (war3mapUnits.doo)">\ud83d\udccd Placed</button>
        <button class="menu-item" data-action="toggleWindow" data-target="doodadsSlkWindow"
                title="Doodads catalog (Doodads.slk)">\ud83c\udf33 Doodads</button>
        <button class="menu-item menu-child" data-action="toggleWindow" data-target="doodadDooWindow"
                title="Placed doodads (war3map.doo)">\ud83d\udccd Placed</button>
        <button class="menu-item${hasTerrain ? '' : ' disabled'}" ${hasTerrain ? 'data-action="toggleWindow" data-target="terrainWindow"' : ''}
                title="${hasTerrain ? 'Terrain metadata' : 'No terrain data available'}">\ud83d\uddfa Terrain</button>
        <button class="menu-item menu-child${hasTerrain ? '' : ' disabled'}" ${hasTerrain ? 'data-action="toggleWindow" data-target="tilesetWindow"' : ''}
                title="${hasTerrain ? 'Tileset info' : 'No terrain data available'}">\ud83e\uddf1 Tileset</button>
        ${mapInfo.isArchive ? '<button class="menu-item" data-action="toggleWindow" data-target="filesWindow" title="Archive file list">\ud83d\udcc2 Files</button>' : ''}
        ${mapInfo.isArchive ? `<button class="menu-item menu-child${mapInfo.isArchiveFile ? '' : ' disabled'}" ${mapInfo.isArchiveFile ? 'id="browseMpqBtn"' : ''}
                title="${mapInfo.isArchiveFile ? 'Browse archive as folder' : 'Already a folder on disk'}">\ud83d\udcc1 Browse</button>` : ''}
    </div>

    <!-- ── Floating windows (Custom Elements) ─────────────────── -->

    <float-window id="gamePathWindow" title-text="\u2699 Game Path" hidden style="left:140px;top:16px;">
        <div id="gpBody">${gamePathContent}</div>
    </float-window>

    ${mapInfo.isArchive ? `
    <float-window id="headerWindow" title-text="\ud83d\udce6 Header \u2014 ${esc(fname)}" hidden style="left:140px;top:16px;">
        ${headerContent}
    </float-window>
    ` : ''}

    ${hasW3i ? `
    <float-window id="w3iWindow" title-text="\ud83d\udcdc Map Info" ${mapInfo.isW3i ? '' : 'hidden'} style="left:140px;top:16px;">
        ${w3iContent}
    </float-window>
    ` : ''}

    <float-window id="unitDooWindow" title-text="\ud83d\udccd Placed Units" ${isDoo && mapInfo.isDooUnit ? '' : 'hidden'} style="left:140px;top:16px;">
        ${hasUnitDoo ? unitDooContent : '<div class="fi-empty">\u26a0 war3mapUnits.doo not found</div>'}
    </float-window>

    <float-window id="unitsSlkWindow" title-text="\ud83d\udde1 Units" hidden style="left:140px;top:16px;width:600px;height:70vh;">
        <div id="usSlkSource" class="${unitsSlkSource ? 'ts-source' : 'ts-source ts-no-slk'}">${unitsSlkSource ? 'UnitData.slk: <span class="code">' + esc(unitsSlkSource) + '</span>' : 'UnitData.slk not found \u2014 set Game Path'}</div>
        <div class="tw-section-title">Units (<span id="usUnitCount">${hasUnitsSlk && terrainData._unitsSlk.units ? terrainData._unitsSlk.units.length : 0}</span>)</div>
        <div class="legend" id="usUnitList">${unitsSlkItems}</div>
    </float-window>

    <float-window id="doodadDooWindow" title-text="\ud83d\udccd Placed Doodads" ${isDoo && !mapInfo.isDooUnit ? '' : 'hidden'} style="left:140px;top:16px;">
        ${hasDoodadDoo ? doodadDooContent : '<div class="fi-empty">\u26a0 war3map.doo not found</div>'}
    </float-window>

    <float-window id="doodadsSlkWindow" title-text="\ud83c\udf33 Doodads" hidden style="left:140px;top:16px;width:600px;height:70vh;">
        <div id="dsSlkSource" class="${doodadsSlkSource ? 'ts-source' : 'ts-source ts-no-slk'}">${doodadsSlkSource ? 'Doodads.slk: <span class="code">' + esc(doodadsSlkSource) + '</span>' : 'Doodads.slk not found \u2014 set Game Path'}</div>
        <div class="tw-section-title">Doodads (<span id="dsDoodadCount">${hasDoodadsSlk && terrainData._doodadsSlk.doodads ? terrainData._doodadsSlk.doodads.length : 0}</span>)</div>
        <div class="legend" id="dsDoodadList">${doodadsSlkItems}</div>
    </float-window>

    ${hasTerrain ? `
    <float-window id="terrainWindow" title-text="\ud83d\uddfa Terrain" ${mapInfo.isW3e ? '' : 'hidden'} style="left:140px;top:16px;">
        ${renderMeta(terrainData._meta)}
        <table class="info">
            <tr><td class="key">Magic</td><td><code>${esc(terrainData.magic)}</code></td></tr>
            <tr><td class="key">Version</td><td>${terrainData.version}</td></tr>
            <tr><td class="key">Tileset</td><td>${esc(terrainData.tileset)} \u2014 ${esc(tilesetName)}</td></tr>
            <tr><td class="key">Custom</td><td>${terrainData.custom_tileset ? 'Yes' : 'No'}</td></tr>
            <tr><td class="key">Size</td><td>${w} \u00d7 ${h} (${w * h} pts)</td></tr>
            <tr><td class="key">Offset</td><td>X: ${terrainData.offset_x.toFixed(2)}, Y: ${terrainData.offset_y.toFixed(2)}</td></tr>
        </table>
        <div class="tw-section-title">Layers</div>
        <div class="terrain-checks">
            <label class="menu-cb"><input type="checkbox" id="cbWater" /> Water</label>
            <label class="menu-cb"><input type="checkbox" id="cbBoundary" /> Boundary</label>
            <label class="menu-cb"><input type="checkbox" id="cbBlight" /> Blight</label>
            <label class="menu-cb"><input type="checkbox" id="cbRamp" /> Ramp</label>
            <label class="menu-cb"><input type="checkbox" id="cbWireframe" checked /> Wireframe</label>
            <label class="menu-cb"><input type="checkbox" id="cbTextures" checked /> Textures</label>
            <label class="menu-cb"><input type="checkbox" id="cbDeformation" checked /> Deformation</label>
        </div>
    </float-window>
    ` : ''}

    ${hasTerrain ? `
    <float-window id="tilesetWindow" title-text="\ud83e\uddf1 Tileset" hidden style="left:140px;top:16px;">
        <div id="tsSlkSource" class="${terrainSlkSource ? 'ts-source' : 'ts-source ts-no-slk'}">${terrainSlkSource ? 'Terrain.slk: <span class="code">' + esc(terrainSlkSource) + '</span>' : 'Terrain.slk not found \u2014 set Game Path'}</div>
        <div class="tw-section-title">Ground Tiles (<span id="tsGroundCount">${totalTiles}</span>)</div>
        <div class="legend" id="tsGroundTiles">${legendItems}</div>
        <div id="tsCliffSection">${totalCliffTiles > 0 ? '<div class="tw-section-title">Cliff Tiles (' + totalCliffTiles + ')</div><div class="legend">' + cliffLegendItems + '</div>' : ''}</div>
    </float-window>
    ` : ''}

    ${mapInfo.isArchive ? `
    <float-window id="filesWindow" title-text="\ud83d\udcc2 Files (${fileCount})" no-padding hidden style="right:16px;top:16px;left:auto;">
        <button slot="actions" class="float-action" id="browseBtn" title="Mount as workspace folder">\ud83d\udcc1</button>
        <input type="text" id="fileFilter" placeholder="Filter files\u2026" class="file-filter" />
        <div class="files-list" id="filesList">${filesRows}</div>
    </float-window>
    ` : ''}

    <script nonce="${nonce}" src="${componentsSrc}"></script>
    <script nonce="${nonce}" src="${threeSrc}"></script>
    <script nonce="${nonce}">
    window.__W3E_DATA__ = {
        hasTerrain: ${hasTerrain},
        renderData: ${renderData ? JSON.stringify(renderData) : 'null'},
        groundTileCodes: ${hasTerrain && terrainData.ground_tiles ? JSON.stringify(terrainData.ground_tiles) : '[]'},
        cliffTileCodes: ${hasTerrain && terrainData.cliff_tiles ? JSON.stringify(terrainData.cliff_tiles) : '[]'},
        isArchive: ${!!mapInfo.isArchive}
    };
    </script>
    <script nonce="${nonce}" src="${terrainSrc}"></script>
</body>
</html>`
}

module.exports = {renderMapEditor}

