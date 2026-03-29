const fs = require('fs')
const path = require('path')

const SUPPORTED_BINARIES = [
    {file: 'war3map.w3e', label: 'Terrain'},
    {file: 'war3map.w3i', label: 'Map Info'},
    {file: 'war3map.doo', label: 'Doodads'},
    {file: 'war3mapUnits.doo', label: 'Units'},
    {file: 'war3map.wts', label: 'Strings'},
    {file: 'war3mapMap.blp', label: 'Minimap'},
    {file: 'war3map.w3r', label: 'Regions'},
    {file: 'war3map.w3c', label: 'Cameras'},
    {file: 'war3map.w3s', label: 'Sounds'},
    {file: 'war3map.shd', label: 'Shadow Map'},
    {file: 'war3mapPath.tga', label: 'Pathing Map'},
    {file: 'war3map.mmp', label: 'Menu Minimap'},
]

/** Walk up from filePath looking for a parent folder named *.w3x / *.w3m */
function findMapRoot(filePath) {
    let dir = path.dirname(filePath)
    while (dir && dir !== path.dirname(dir)) {
        const base = path.basename(dir)
        if (/\.(w3x|w3m)$/i.test(base)) {
            return dir
        }
        dir = path.dirname(dir)
    }
    return null
}

/** Check which supported binaries exist in a folder-based map root */
function scanMapBinaries(mapRoot) {
    return SUPPORTED_BINARIES.map(entry => {
        const fullPath = path.join(mapRoot, entry.file)
        let exists = false
        try {
            exists = fs.existsSync(fullPath)
        } catch (_) {
        }
        return {...entry, exists}
    })
}

module.exports = {SUPPORTED_BINARIES, findMapRoot, scanMapBinaries}

