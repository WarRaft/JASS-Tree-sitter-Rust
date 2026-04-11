const {esc, indexToRgb, TILESET_NAMES, DOODAD_CATEGORIES, DESTRUCTABLE_CATEGORIES} = require('./utils.js')
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
 * @param {Object}      mapInfo     — { mapName, binaries, currentFile, isArchive, isMap, archiveFiles, elementsSrc, canvasListSrc, appSrc }
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
    let cliffTypesSlkSource = ''
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
            legendItems = terrainData.ground_tiles.map((raw, i) => {
                const code = typeof raw === 'string' ? raw : raw.text || String(raw.raw)
                const [r, g, b] = indexToRgb(i)
                const info = slkMap[code]
                const name = info ? info.comment : ''
                const tilePath = info && info.dir && info.file
                    ? info.dir + '\\' + info.file + (info.ext || '') : ''
                return `<tile-item index="${i}" code="${esc(code)}" tile-name="${esc(name)}" tile-path="${esc(tilePath)}" swatch-color="${r},${g},${b}"></tile-item>`
            }).join('\n')
        }

        // Build cliff type rawcode → data lookup (needed for cliff legend below)
        const cliffTypesMap = (terrainData._cliffTypesSlk && terrainData._cliffTypesSlk.cliffTypes) || {}

        if (terrainData.cliff_tiles) {
            cliffLegendItems = terrainData.cliff_tiles.map((raw, i) => {
                const code = typeof raw === 'string' ? raw : raw.text || String(raw.raw)
                const ct = cliffTypesMap[code]
                const parts = []
                if (ct && ct.cliffModelDir) parts.push(ct.cliffModelDir)
                if (ct && ct.cliffClass) parts.push(ct.cliffClass)
                const name = parts.length > 0 ? parts.join(' \u2014 ') : ''
                const texPath = ct && ct.texDir && ct.texFile ? ct.texDir + '\\' + ct.texFile + '.blp' : ''
                const texSource = ct && ct.texSource ? ct.texSource : ''
                return `<tile-item index="${i}" code="${esc(code)}"${name ? ' tile-name="' + esc(name) + '"' : ''}${texPath ? ' tile-path="' + esc(texPath) + '"' : ''}${texSource ? ' tile-source="' + esc(texSource) + '"' : ''}></tile-item>`
            }).join('\n')
        }

        renderData = {
            w, h, totalTiles,
            offsetX: terrainData.offset_x,
            offsetY: terrainData.offset_y,
            tileTextures: terrainData._tileTextures || [],
            // base64-encoded TypedArrays (packed by Rust)
            groundHeight: terrainData._packed.groundHeight,
            waterHeight: terrainData._packed.waterHeight,
            groundTexture: terrainData._packed.groundTexture,
            groundVariation: terrainData._packed.groundVariation,
            cliffVariation: terrainData._packed.cliffVariation,
            cliffTexture: terrainData._packed.cliffTexture,
            layerHeight: terrainData._packed.layerHeight,
            flags: terrainData._packed.flags,
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
    const doodadCategoriesSet = new Set()
    const doodadTilesetsSet = new Set()
    const doodadsMap = (terrainData && terrainData._doodadsSlk && terrainData._doodadsSlk.doodads) || {}
    const doodadsValues = Object.values(doodadsMap)
    if (terrainData && terrainData._doodadsSlk) {
        for (const d of doodadsValues) {
            if (d.category) doodadCategoriesSet.add(d.category)
            if (d.tilesets) {
                for (const ch of d.tilesets) {
                    if (ch !== ',' && ch !== '*') doodadTilesetsSet.add(ch)
                }
            }
        }
    }
    const hasDoodadsSlk = !!(terrainData && terrainData._doodadsSlk)

    // Build category & tileset checkbox HTML for the doodads sidebar
    const categoryCheckboxes = Object.keys(DOODAD_CATEGORIES).sort().map(code => {
        const label = DOODAD_CATEGORIES[code] || code
        return `<label class="menu-cb"><input type="checkbox" class="ds-cat-cb" data-cat="${esc(code)}" checked /> <span class="ds-ts-badge">${esc(code)}</span> ${esc(label)}</label>`
    }).join('\n')

    const sortedTilesets = Array.from(doodadTilesetsSet).sort()
    const tilesetCheckboxes = sortedTilesets.map(code => {
        const label = TILESET_NAMES[code] || code
        return `<label class="menu-cb"><input type="checkbox" class="ds-ts-cb" data-ts="${esc(code)}" checked /> <span class="ds-ts-badge">${esc(code)}</span> ${esc(label)}</label>`
    }).join('\n')

    // Build units SLK data
    const unitsMap = (terrainData && terrainData._unitsSlk && terrainData._unitsSlk.units) || {}
    let unitsSlkSource = ''
    const unitSlkSources = (terrainData && terrainData._unitsSlk && terrainData._unitsSlk.sources) || []
    const unitRacesSet = new Set()
    const unitsValues = Object.values(unitsMap)
    if (terrainData && terrainData._unitsSlk) {
        unitsSlkSource = terrainData._unitsSlk.source || ''
        for (const u of unitsValues) {
            if (u.race) unitRacesSet.add(u.race)
        }
    }
    const hasUnitsSlk = !!(terrainData && terrainData._unitsSlk)

    // Build sources HTML for the sidebar
    let unitSourcesHtml = ''
    if (unitSlkSources.length > 0) {
        unitSourcesHtml = unitSlkSources.map(s =>
            `<div class="ts-source" style="margin:1px 0;font-size:11px;">${esc(s.source)} <span style="opacity:0.5;">(${s.rows})</span></div>`
        ).join('')
    } else if (!hasUnitsSlk) {
        unitSourcesHtml = '<div class="ts-source ts-no-slk">UnitData.slk not found \u2014 set Game Path</div>'
    }

    const UNIT_RACE_NAMES = {
        human: 'Human', orc: 'Orc', undead: 'Undead', nightelf: 'Night Elf',
        creeps: 'Creeps', commoner: 'Commoner', other: 'Other', demon: 'Demon',
        critters: 'Critters', naga: 'Naga',
    }
    const unitRaceCheckboxes = Array.from(unitRacesSet).sort().map(code => {
        const label = UNIT_RACE_NAMES[code] || code
        return `<label class="menu-cb"><input type="checkbox" class="us-race-cb" data-race="${esc(code)}" checked /> ${esc(label)}</label>`
    }).join('\n')

    // Build destructables SLK data
    let destructablesSlkSource = ''
    const destructableCategoriesSet = new Set()
    const destructableTilesetsSet = new Set()
    const destructablesMap = (terrainData && terrainData._destructablesSlk && terrainData._destructablesSlk.destructables) || {}
    const destructablesValues = Object.values(destructablesMap)
    if (terrainData && terrainData._destructablesSlk) {
        destructablesSlkSource = terrainData._destructablesSlk.source || ''
        for (const d of destructablesValues) {
            if (d.category) destructableCategoriesSet.add(d.category)
            if (d.tilesets) {
                for (const ch of d.tilesets) {
                    if (ch !== ',' && ch !== '*') destructableTilesetsSet.add(ch)
                }
            }
        }
    }
    const hasDestructablesSlk = !!(terrainData && terrainData._destructablesSlk)

    // Build category & tileset checkbox HTML for the destructables sidebar
    const destCategoryCheckboxes = Object.keys(DESTRUCTABLE_CATEGORIES).sort().map(code => {
        const label = DESTRUCTABLE_CATEGORIES[code] || code
        return `<label class="menu-cb"><input type="checkbox" class="dt-cat-cb" data-cat="${esc(code)}" checked /> <span class="ds-ts-badge">${esc(code)}</span> ${esc(label)}</label>`
    }).join('\n')

    const destSortedTilesets = Array.from(destructableTilesetsSet).sort()
    const destTilesetCheckboxes = destSortedTilesets.map(code => {
        const label = TILESET_NAMES[code] || code
        return `<label class="menu-cb"><input type="checkbox" class="dt-ts-cb" data-ts="${esc(code)}" checked /> <span class="ds-ts-badge">${esc(code)}</span> ${esc(label)}</label>`
    }).join('\n')

    // Build rawcode → model file maps for placing objects on terrain
    const doodadFileMap = {}
    for (const [rawId, d] of Object.entries(doodadsMap)) {
        if (d.file) doodadFileMap[rawId] = {file: d.file, numVar: d.numVar || 1}
    }
    const destructableFileMap = {}
    for (const [rawId, d] of Object.entries(destructablesMap)) {
        if (d.file) destructableFileMap[rawId] = {file: d.file, numVar: d.numVar || 1, texId: d.texId || 0, texFile: d.texFile || ''}
    }
    const unitFileMap = {}
    for (const [rawId, u] of Object.entries(unitsMap)) {
        if (u.file) unitFileMap[rawId] = u.file
    }

    // Build cliff type rawcode → model dirs map
    const cliffTypesMap = (terrainData && terrainData._cliffTypesSlk && terrainData._cliffTypesSlk.cliffTypes) || {}
    const cliffTypeMap = {}
    for (const [id, ct] of Object.entries(cliffTypesMap)) {
        cliffTypeMap[id] = {cliffModelDir: ct.cliffModelDir || '', rampModelDir: ct.rampModelDir || '', texDir: ct.texDir || '', texFile: ct.texFile || '', texSource: ct.texSource || '', groundTile: ct.groundTile || ''}
    }
    const hasCliffTypesSlk = !!(terrainData && terrainData._cliffTypesSlk)
    if (hasCliffTypesSlk) {
        cliffTypesSlkSource = terrainData._cliffTypesSlk.source || ''
    }

    // Cliff model variations (pattern → max variation index)
    const cliffVariations = (terrainData && terrainData._cliffVariations) || null

    // Extract full DOO items for placed-object categorization (doodad vs destructable)
    const doodadDooItems = mapInfo.doodadDooData && mapInfo.doodadDooData.items
        ? mapInfo.doodadDooData.items.map((it, i) => ({
            raw: it.rawcode.raw,
            text: it.rawcode.text,
            variation: it.variation,
            index: i,
            position: it.position,
            angle: it.angle,
            scale: it.scale,
            skin: it.skin != null ? it.skin : null,
            flag: it.flag,
            health: it.doodad ? it.doodad.health : null,
            num: it.doodad ? it.doodad.num : null,
        })) : []

    // Extract minimal DOO placement data
    const doodadPlacements = mapInfo.doodadDooData && mapInfo.doodadDooData.items
        ? mapInfo.doodadDooData.items.map((it, i) => ({
            r: it.rawcode.raw,
            t: it.rawcode.text,
            v: it.variation,
            i: i,
            p: [it.position.x, it.position.y, it.position.z],
            a: it.angle,
            s: [it.scale.x, it.scale.y, it.scale.z]
        })) : []
    const unitPlacements = mapInfo.unitDooData && mapInfo.unitDooData.items
        ? mapInfo.unitDooData.items.map(it => ({
            r: it.rawcode.raw,
            p: [it.position.x, it.position.y, it.position.z],
            a: it.angle,
            s: [it.scale.x, it.scale.y, it.scale.z]
        })) : []

    // Extract full unit DOO items for placed-object canvas rendering
    const unitDooItems = mapInfo.unitDooData && mapInfo.unitDooData.items
        ? mapInfo.unitDooData.items.map((it, i) => ({
            raw: it.rawcode.raw,
            text: it.rawcode.text,
            variation: it.variation,
            index: i,
            position: it.position,
            angle: it.angle,
            scale: it.scale,
            skin: it.skin != null ? it.skin : null,
            flag: it.flag,
            player: it.unit ? it.unit.player : null,
        })) : []

    const nonce = mapInfo.nonce || ''
    const cspSource = mapInfo.cspSource || ''
    const elementsSrc = mapInfo.elementsSrc || ''
    const canvasListSrc = mapInfo.canvasListSrc || ''
    const utilsSrc = mapInfo.utilsSrc || ''
    const stateSrc = mapInfo.stateSrc || ''
    const tilesetSrc = mapInfo.tilesetSrc || ''
    const doodadsSrc = mapInfo.doodadsSrc || ''
    const destructablesSrc = mapInfo.destructablesSrc || ''
    const unitsSrc = mapInfo.unitsSrc || ''
    const placedSrc = mapInfo.placedSrc || ''
    const gamePathSrc = mapInfo.gamePathSrc || ''
    const pathTexSrc = mapInfo.pathTexSrc || ''
    const modelViewerSrc = mapInfo.modelViewerSrc || ''
    const orbitSrc = mapInfo.orbitSrc || ''
    const appSrc = mapInfo.appSrc || ''
    const terrainSrc = mapInfo.terrainSrc || ''

    const connectSrc = mapInfo.binaryServer ? `connect-src http://127.0.0.1:${mapInfo.binaryServer.port};` : ''
    const imgSrcExtra = mapInfo.binaryServer ? ` http://127.0.0.1:${mapInfo.binaryServer.port}` : ''

    // Build the binary fetch URL (if the HTTP server is available)
    let binaryTerrainUrl = 'null'
    if (hasTerrain && mapInfo.binaryServer && mapInfo.terrainUri) {
        const bs = mapInfo.binaryServer
        const params = new URLSearchParams({
            token: bs.token,
            uri: mapInfo.terrainUri,
        })
        if (mapInfo.archivePath) params.set('archive', mapInfo.archivePath)
        binaryTerrainUrl = JSON.stringify(`http://127.0.0.1:${bs.port}/w3e/terrain?${params}`)
    }

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; ${connectSrc} img-src ${cspSource}${imgSrcExtra} data: blob:; script-src 'nonce-${nonce}'; style-src 'unsafe-inline'; font-src ${cspSource};" />
    <style>${editorStyles()}</style>
</head>
<body>
    <div id="globalLoadingBar"></div>
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
        <button class="menu-item" data-action="toggleWindow" data-target="destructablesSlkWindow"
                title="Destructables catalog (DestructableData.slk)">\ud83c\udfda Destructables</button>
        <button class="menu-item menu-child" data-action="toggleWindow" data-target="destructableDooWindow"
                title="Placed destructables (war3map.doo)">\ud83d\udccd Placed</button>
        <button class="menu-item${hasTerrain ? '' : ' disabled'}" ${hasTerrain ? 'data-action="toggleWindow" data-target="terrainWindow"' : ''}
                title="${hasTerrain ? 'Terrain metadata' : 'No terrain data available'}">\ud83d\uddfa Terrain</button>
        <button class="menu-item menu-child menu-child-cont${hasTerrain ? '' : ' disabled'}" ${hasTerrain ? 'data-action="toggleWindow" data-target="tilesetWindow"' : ''}
                title="${hasTerrain ? 'Tileset info' : 'No terrain data available'}">\ud83e\uddf1 Tileset</button>
        <button class="menu-item menu-child${hasTerrain ? '' : ' disabled'}" ${hasTerrain ? 'data-action="toggleWindow" data-target="cliffsWindow"' : ''}
                title="${hasTerrain ? 'Cliff types' : 'No terrain data available'}">\u26f0 Cliffs</button>
        ${mapInfo.isArchive ? '<button class="menu-item" data-action="toggleWindow" data-target="filesWindow" title="Archive file list">\ud83d\udcc2 Files</button>' : ''}
        ${mapInfo.isArchive ? `<button class="menu-item menu-child${mapInfo.isArchiveFile ? '' : ' disabled'}" ${mapInfo.isArchiveFile ? 'id="browseMpqBtn"' : ''}
                title="${mapInfo.isArchiveFile ? 'Browse archive as folder' : 'Already a folder on disk'}">\ud83d\udcc1 Browse</button>` : ''}
        <button class="menu-item" data-action="toggleWindow" data-target="modelViewerWindow" title="3D Model Viewer">\ud83c\udfae Model</button>
        <button class="menu-item" data-action="toggleWindow" data-target="blpViewerWindow" title="BLP Image Viewer">\ud83d\uddbc BLP</button>
    </div>

    <!-- ── Floating windows (Custom Elements) ─────────────────── -->

    <float-window id="gamePathWindow" title-text="\u2699 Game Path" hidden style="left:140px;top:16px;">
        <reload-button slot="actions"></reload-button>
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

    <float-window id="unitDooWindow" title-text="\ud83d\udccd Placed Units" no-padding ${isDoo && mapInfo.isDooUnit ? '' : 'hidden'} style="left:140px;top:16px;width:800px;height:70vh;">
        ${hasUnitDoo ? unitDooContent : '<div class="fi-empty">\u26a0 war3mapUnits.doo not found</div>'}
    </float-window>

    <float-window id="unitsSlkWindow" title-text="\ud83d\udde1 Units" no-padding hidden style="left:140px;top:16px;width:750px;height:70vh;">
        <reload-button slot="actions"></reload-button>
        <div style="display:flex;height:100%;overflow:hidden;">
            <div class="ds-sidebar" id="usSidebar">
                <collapse-group group-title="SLK Sources (${unitSlkSources.length})" id="usSlkSources">
                    ${unitSourcesHtml}
                </collapse-group>
                <div class="ds-filter-group">
                    <div class="ds-filter-title">Races</div>
                    <div class="terrain-checks" id="usRaceChecks">${unitRaceCheckboxes}</div>
                </div>
            </div>
            <div style="flex:1;display:flex;flex-direction:column;min-width:0;">
                <input type="text" id="usSearchInput" placeholder="Search by name or ID\u2026" class="ds-search" />
                <div class="ds-sort-bar">
                    <span class="ds-sort-col us-sort-col" data-sort="unitId">ID</span>
                    <span class="ds-sort-col us-sort-col ds-sort-name" data-sort="name">Name</span>
                    <span class="ds-sort-col us-sort-col ds-sort-cat" data-sort="race">Race</span>
                    <span class="ds-sort-info">(<span id="usUnitCount">${unitsValues.length}</span> / <span id="usUnitTotal">${unitsValues.length}</span>)</span>
                </div>
                <div class="legend" id="usUnitList" style="overflow:hidden;flex:1;min-height:0;"></div>
            </div>
        </div>
    </float-window>

    <float-window id="unitDetailWindow" title-text="\ud83d\udde1 Unit" hidden style="left:200px;top:60px;width:560px;">
        <div id="unitDetailBody"></div>
    </float-window>

    <float-window id="doodadDooWindow" title-text="\ud83d\udccd Placed Doodads" no-padding ${isDoo && !mapInfo.isDooUnit ? '' : 'hidden'} style="left:140px;top:16px;width:800px;height:70vh;">
        ${hasDoodadDoo ? doodadDooContent : '<div class="fi-empty">\u26a0 war3map.doo not found</div>'}
    </float-window>

    <float-window id="destructableDooWindow" title-text="\ud83d\udccd Placed Destructables" no-padding hidden style="left:140px;top:16px;width:800px;height:70vh;">
        <div style="display:flex;flex-direction:column;height:100%;overflow:hidden;">
            <div id="destDooTitle" class="tw-section-title" style="padding:8px 10px 4px;flex-shrink:0;">\ud83c\udfda Placed Destructables</div>
            <div class="legend" id="destructableDooList" style="flex:1;min-height:0;overflow:hidden;"></div>
        </div>
    </float-window>

    <float-window id="doodadsSlkWindow" title-text="\ud83c\udf33 Doodads" no-padding hidden style="left:140px;top:16px;width:750px;height:70vh;">
        <reload-button slot="actions"></reload-button>
        <div style="display:flex;height:100%;overflow:hidden;">
             <div class="ds-sidebar" id="dsSidebar">
                <slk-source-list id="dsSlkSource"></slk-source-list>
                <div class="ds-filter-group">
                    <div class="ds-filter-title">Status</div>
                    <div class="terrain-checks" id="dsStatusChecks"></div>
                </div>
                <div class="ds-filter-group">
                    <div class="ds-filter-title">Categories</div>
                    <div class="terrain-checks" id="dsCatChecks">${categoryCheckboxes}</div>
                </div>
                <div class="ds-filter-group">
                    <div class="ds-filter-title">Tilesets</div>
                    <div class="terrain-checks" id="dsTsChecks">${tilesetCheckboxes}</div>
                </div>
            </div>
            <div style="flex:1;display:flex;flex-direction:column;min-width:0;">
                <input type="text" id="dsSearchInput" placeholder="Search by name or ID\u2026" class="ds-search" />
                <div class="ds-sort-bar">
                    <span class="ds-sort-col" data-sort="doodId">ID</span>
                    <span class="ds-sort-col ds-sort-name" data-sort="name">Name</span>
                    <span class="ds-sort-col ds-sort-cat" data-sort="category">Category</span>
                    <span class="ds-sort-info">(<span id="dsDoodadCount">${doodadsValues.length}</span> / <span id="dsDoodadTotal">${doodadsValues.length}</span>)</span>
                </div>
                <div class="legend" id="dsDoodadList" style="overflow:hidden;flex:1;min-height:0;"></div>
            </div>
        </div>
    </float-window>

    <float-window id="doodadDetailWindow" title-text="\ud83c\udf33 Doodad" hidden style="left:200px;top:60px;width:560px;">
        <div id="doodadDetailBody"></div>
    </float-window>

    <float-window id="doodadErrorsWindow" title-text="\u26a0 Doodad Errors" hidden style="left:220px;top:80px;width:600px;max-height:60vh;">
        <div id="doodadErrorsBody" style="padding:8px;overflow:auto;max-height:55vh;font-size:12px;"></div>
    </float-window>

    <float-window id="destructablesSlkWindow" title-text="\ud83c\udfda Destructables" no-padding hidden style="left:140px;top:16px;width:750px;height:70vh;">
        <reload-button slot="actions"></reload-button>
        <div style="display:flex;height:100%;overflow:hidden;">
            <div class="ds-sidebar" id="dtSidebar">
                <div id="dtSlkSource" class="${destructablesSlkSource ? 'ts-source' : 'ts-source ts-no-slk'}">${destructablesSlkSource ? esc(destructablesSlkSource) : 'DestructableData.slk not found \u2014 set Game Path'}</div>
                <div class="ds-filter-group">
                    <div class="ds-filter-title">Status</div>
                    <div class="terrain-checks" id="dtStatusChecks"></div>
                </div>
                <div class="ds-filter-group">
                    <div class="ds-filter-title">Categories</div>
                    <div class="terrain-checks" id="dtCatChecks">${destCategoryCheckboxes}</div>
                </div>
                <div class="ds-filter-group">
                    <div class="ds-filter-title">Tilesets</div>
                    <div class="terrain-checks" id="dtTsChecks">${destTilesetCheckboxes}</div>
                </div>
            </div>
            <div style="flex:1;display:flex;flex-direction:column;min-width:0;">
                <input type="text" id="dtSearchInput" placeholder="Search by name or ID\u2026" class="ds-search" />
                <div class="ds-sort-bar">
                    <span class="ds-sort-col dt-sort-col" data-sort="destructableId">ID</span>
                    <span class="ds-sort-col dt-sort-col ds-sort-name" data-sort="name">Name</span>
                    <span class="ds-sort-col dt-sort-col ds-sort-cat" data-sort="category">Category</span>
                    <span class="ds-sort-info">(<span id="dtDestCount">${destructablesValues.length}</span> / <span id="dtDestTotal">${destructablesValues.length}</span>)</span>
                </div>
                <div class="legend" id="dtDestList" style="overflow:hidden;flex:1;min-height:0;"></div>
            </div>
        </div>
    </float-window>

    <float-window id="destructableDetailWindow" title-text="\ud83c\udfda Destructable" hidden style="left:200px;top:60px;width:560px;">
        <div id="destructableDetailBody"></div>
    </float-window>

    <float-window id="destructableErrorsWindow" title-text="\u26a0 Destructable Errors" hidden style="left:220px;top:80px;width:600px;max-height:60vh;">
        <div id="destructableErrorsBody" style="padding:8px;overflow:auto;max-height:55vh;font-size:12px;"></div>
    </float-window>

    <float-window id="gameStringInfoWindow" title-text="\ud83d\udd17 GameString" hidden style="left:240px;top:100px;width:400px;">
        <div id="gsInfoBody"></div>
    </float-window>

    <float-window id="pathTexWindow" title-text="\ud83d\udea7 Path Texture" hidden style="left:260px;top:80px;width:auto;">
        <div id="pathTexBody"></div>
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
            <label class="menu-cb"><input type="checkbox" id="cbWater" checked /> Water</label>
            <label class="menu-cb"><input type="checkbox" id="cbBoundary" /> Boundary</label>
            <label class="menu-cb"><input type="checkbox" id="cbBlight" /> Blight</label>
            <label class="menu-cb"><input type="checkbox" id="cbRamp" /> Ramp</label>
            <label class="menu-cb"><input type="checkbox" id="cbWireframe" checked /> Wireframe</label>
            <label class="menu-cb"><input type="checkbox" id="cbTextures" checked /> Textures</label>
            <label class="menu-cb"><input type="checkbox" id="cbDeformation" checked /> Deformation</label>
            <label class="menu-cb"><input type="checkbox" id="cbSlopes" checked /> Slopes</label>
            <label class="menu-cb"><input type="checkbox" id="cbCliffs" checked /> Cliffs</label>
            <label class="menu-cb"><input type="checkbox" id="cbObjects" checked /> Objects</label>
        </div>
    </float-window>
    ` : ''}

    ${hasTerrain ? `
    <float-window id="tilesetWindow" title-text="\ud83e\uddf1 Tileset" hidden style="left:140px;top:16px;">
        <reload-button slot="actions"></reload-button>
        <div id="tsSlkSource" class="${terrainSlkSource ? 'ts-source' : 'ts-source ts-no-slk'}">${terrainSlkSource ? esc(terrainSlkSource) : 'TerrainArt\\Terrain.slk \u2014 not found, set Game Path'}</div>
        <div class="tw-section-title">Ground Tiles (<span id="tsGroundCount">${totalTiles}</span>)</div>
        <div class="legend" id="tsGroundTiles">${legendItems}</div>
    </float-window>
    ` : ''}

    ${hasTerrain ? `
    <float-window id="cliffsWindow" title-text="\u26f0 Cliffs" hidden style="left:160px;top:32px;">
        <reload-button slot="actions"></reload-button>
        <div id="ctSlkSource" class="${cliffTypesSlkSource ? 'ts-source' : 'ts-source ts-no-slk'}">${cliffTypesSlkSource ? esc(cliffTypesSlkSource) : 'TerrainArt\\CliffTypes.slk \u2014 not found, set Game Path'}</div>
        <div id="ctCliffSection">${totalCliffTiles > 0 ? '<div class="tw-section-title">Cliff Tiles (' + totalCliffTiles + ')</div><div class="legend">' + cliffLegendItems + '</div>' : ''}</div>
    </float-window>
    ` : ''}

    ${mapInfo.isArchive ? `
    <float-window id="filesWindow" title-text="\ud83d\udcc2 Files (${fileCount})" no-padding hidden style="right:16px;top:16px;left:auto;">
        <button slot="actions" class="float-action" id="browseBtn" title="Mount as workspace folder">\ud83d\udcc1</button>
        <input type="text" id="fileFilter" placeholder="Filter files\u2026" class="file-filter" />
        <div class="files-list" id="filesList">${filesRows}</div>
    </float-window>
    ` : ''}

    <float-window id="modelViewerWindow" title-text="\ud83c\udfae Model Viewer" no-padding ${mapInfo.isMdx ? '' : 'hidden'} style="left:160px;top:32px;width:800px;height:650px;">
        <div style="display:flex;height:100%;">
            <div class="mv-sidebar" id="mvSidebar">
                <button class="mv-sb-item" id="mvWireBtn" title="Wireframe overlay">\ud83d\udd32 Wire</button>
                <button class="mv-sb-item active" id="mvAxesBtn" title="Show axes helper">\ud83d\udccf Axes</button>
                <button class="mv-sb-item active" id="mvGridBtn" title="Show grid">\u229e Grid</button>
                <div class="mv-sb-sep"></div>
                <button class="mv-sb-item" id="mvResetCamera" title="Reset camera">\ud83c\udfaf Reset</button>
                <button class="mv-sb-item" id="mvGeosetBtn" title="Geoset visibility">\ud83e\udde9 Geosets</button>
                <button class="mv-sb-item" id="mvMaterialBtn" title="Materials & textures">\ud83c\udfa8 Material</button>
                <button class="mv-sb-item" id="mvBonesBtn" title="Bones & helpers">\ud83e\uddb4 Bones</button>
                <div class="mv-sb-sep"></div>
                <button class="mv-sb-item" id="mvSkeletonBtn" title="Toggle skeleton visibility">\u2620 Skeleton</button>
            </div>
            <div style="display:flex;flex-direction:column;flex:1;min-width:0;">
                <div class="mv-toolbar" id="mvToolbar">
                    <strong id="modelName">Model</strong>
                    <span class="mv-info" id="modelInfo"></span>
                </div>
                <div class="mv-canvas-container" id="modelCanvasContainer">
                    <canvas id="modelCanvas"></canvas>
                    <div class="mv-materials-panel" id="mvGeosetsPanel" hidden>
                        <div class="mv-panel-resize-handle" data-resize-panel="mvGeosetsPanel"></div>
                        <div class="mv-mat-title">Geosets</div>
                        <div class="mv-mat-list" id="mvGeosetList"></div>
                    </div>
                    <div class="mv-materials-panel" id="mvMaterialsPanel" hidden>
                        <div class="mv-panel-resize-handle" data-resize-panel="mvMaterialsPanel"></div>
                        <div class="mv-mat-title">Materials</div>
                        <div class="mv-mat-list" id="mvMaterialList"></div>
                    </div>
                    <div class="mv-materials-panel" id="mvBonesPanel" hidden>
                        <div class="mv-panel-resize-handle" data-resize-panel="mvBonesPanel"></div>
                        <div class="mv-mat-title">Bones & Helpers</div>
                        <div class="mv-mat-list" id="mvBonesList"></div>
                    </div>
                </div>
            </div>
        </div>
    </float-window>

    <float-window id="blpViewerWindow" title-text="\ud83d\uddbc BLP Viewer" no-padding hidden style="left:180px;top:48px;width:640px;height:500px;">
        <div class="blp-viewer" id="blpViewerBody">
            <div class="blp-toolbar" id="blpToolbar">
                <label class="blp-toggle"><input type="checkbox" id="blpCheckerToggle" /> Checker</label>
                <label class="blp-toggle">Bg:&nbsp;<input type="color" id="blpBgColor" value="#000000" /></label>
            </div>
            <div class="blp-empty" id="blpEmpty">Click a <code>.blp</code> file to preview</div>
            <div class="blp-mipmaps" id="blpMipmaps"></div>
        </div>
    </float-window>

    <script nonce="${nonce}" src="${elementsSrc}"></script>
    <script nonce="${nonce}" src="${canvasListSrc}"></script>
    <script nonce="${nonce}" src="${utilsSrc}"></script>
    <script nonce="${nonce}" src="${stateSrc}"></script>
    <script nonce="${nonce}" src="${tilesetSrc}"></script>
    <script nonce="${nonce}" src="${doodadsSrc}"></script>
    <script nonce="${nonce}" src="${destructablesSrc}"></script>
    <script nonce="${nonce}" src="${unitsSrc}"></script>
    <script nonce="${nonce}" src="${gamePathSrc}"></script>
    <script nonce="${nonce}" src="${pathTexSrc}"></script>
    <script nonce="${nonce}" src="${placedSrc}"></script>
    <script nonce="${nonce}" src="${threeSrc}"></script>
    <script nonce="${nonce}" src="${orbitSrc}"></script>
    <script nonce="${nonce}" src="${modelViewerSrc}"></script>
    <script nonce="${nonce}" src="${appSrc}"></script>
    <script nonce="${nonce}">
    window.__W3E_DATA__ = {
        hasTerrain: ${hasTerrain},
        renderData: ${renderData ? JSON.stringify(renderData) : 'null'},
        binaryTerrainUrl: ${binaryTerrainUrl},
        groundTileCodes: ${hasTerrain && terrainData.ground_tiles ? JSON.stringify(terrainData.ground_tiles.map(c => typeof c === 'string' ? c : c.text || String(c.raw))) : '[]'},
        cliffTileCodes: ${hasTerrain && terrainData.cliff_tiles ? JSON.stringify(terrainData.cliff_tiles.map(c => typeof c === 'string' ? c : c.text || String(c.raw))) : '[]'},
        isArchive: ${!!mapInfo.isArchive},
        binaryServer: ${mapInfo.binaryServer ? JSON.stringify({port: mapInfo.binaryServer.port, token: mapInfo.binaryServer.token}) : 'null'},
        archivePath: ${mapInfo.archivePath ? JSON.stringify(mapInfo.archivePath) : 'null'},
        doodadFileMap: ${JSON.stringify(doodadFileMap)},
        destructableFileMap: ${JSON.stringify(destructableFileMap)},
        unitFileMap: ${JSON.stringify(unitFileMap)},
        doodadPlacements: ${JSON.stringify(doodadPlacements)},
        unitPlacements: ${JSON.stringify(unitPlacements)},
        doodadDooItems: ${JSON.stringify(doodadDooItems)},
        unitDooItems: ${JSON.stringify(unitDooItems)},
        initialDoodadsSlk: ${hasDoodadsSlk ? JSON.stringify(terrainData._doodadsSlk) : 'null'},
        initialDestructablesSlk: ${hasDestructablesSlk ? JSON.stringify(terrainData._destructablesSlk) : 'null'},
        initialUnitsSlk: ${hasUnitsSlk ? JSON.stringify(terrainData._unitsSlk) : 'null'},
        cliffTypeMap: ${JSON.stringify(cliffTypeMap)},
        initialCliffTypesSlk: ${hasCliffTypesSlk ? JSON.stringify(terrainData._cliffTypesSlk) : 'null'},
        cliffVariations: ${JSON.stringify(cliffVariations)},
        tileset: ${hasTerrain ? JSON.stringify(terrainData.tileset) : 'null'},
        waterSlk: ${hasTerrain && terrainData._waterSlk ? JSON.stringify(terrainData._waterSlk) : 'null'}
    };
    </script>
    <script nonce="${nonce}" src="${terrainSrc}"></script>
</body>
</html>`
}

module.exports = {renderMapEditor}

