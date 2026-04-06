// noinspection NpmUsedModulesInstalled
const vscode = require('vscode')

/**
 * A read-only FileSystemProvider that exposes MPQ archives (.w3x, .w3m, .w3n, .mpq)
 * as virtual folders in VS Code's explorer.
 *
 * URI scheme: mpq
 * URI format: mpq://<base64url-encoded archive absolute path>/<internal path>
 *
 * The server handles the actual archive I/O via two custom requests:
 *   - mpq/list  → { entries: [{ name, size }] }
 *   - mpq/read  → { content: "<base64>" }
 */
class MpqFileSystemProvider {

    /**
     * @param {() => import('./serverClient.js').ServerClient | undefined} getClient
     * @param {Promise<void>} clientReady
     */
    constructor(getClient, clientReady) {
        /** @private */
        this._getClient = getClient
        /** @private */
        this._clientReady = clientReady
        /** @private  @type {vscode.EventEmitter<vscode.FileChangeEvent[]>} */
        this._onDidChangeFile = new vscode.EventEmitter()
        this.onDidChangeFile = this._onDidChangeFile.event

        /**
         * Cache: archivePath → flat file list from last mpq/list call.
         * @private @type {Map<string, {name: string, size: number}[]>}
         */
        this._listCache = new Map()
        /** @private @type {vscode.OutputChannel | undefined} */
        this._log = undefined
    }

    /** @private */
    _getLog() {
        if (!this._log) {
            this._log = vscode.window.createOutputChannel('MPQ FileSystem')
        }
        return this._log
    }

    // ── helpers ──────────────────────────────────────────────────

    /**
     * Encode an absolute archive path into the URI authority component.
     * Uses hex encoding because URI authorities are case-insensitive
     * (RFC 3986) and VS Code lowercases them, which would corrupt base64.
     * @param {string} archivePath
     * @returns {string}
     */
    static encodeAuthority(archivePath) {
        return Buffer.from(archivePath, 'utf-8').toString('hex')
    }

    /**
     * Decode the URI authority back to an absolute archive path.
     * @param {string} authority
     * @returns {string}
     */
    static decodeAuthority(authority) {
        return Buffer.from(authority, 'hex').toString('utf-8')
    }

    /**
     * Build an mpq URI for a given archive and internal path.
     * @param {string} archivePath absolute fs path to the MPQ archive
     * @param {string} [internalPath=''] path inside the archive
     * @returns {vscode.Uri}
     */
    static makeUri(archivePath, internalPath = '') {
        const p = '/' + internalPath.replace(/\\/g, '/')
        return vscode.Uri.from({
            scheme: 'mpq',
            authority: MpqFileSystemProvider.encodeAuthority(archivePath),
            path: p,
        })
    }

    /**
     * Ensure the client is ready, then return it.
     * @private
     * @returns {Promise<import('./serverClient.js').ServerClient>}
     */
    async _ensureClient() {
        await this._clientReady
        const client = this._getClient()
        if (!client) {
            throw vscode.FileSystemError.Unavailable('Server client not available')
        }
        return client
    }

    /**
     * Fetch the flat file list for an archive (cached).
     * @private
     * @param {string} archivePath
     * @returns {Promise<{name: string, size: number}[]>}
     */
    async _fetchList(archivePath) {
        if (this._listCache.has(archivePath)) {
            return this._listCache.get(archivePath)
        }

        const client = await this._ensureClient()
        const log = this._getLog()

        log.appendLine(`mpq/list → archivePath=${archivePath}`)

        let result
        try {
            result = await client.sendRequest('mpq/list', {archivePath})
        } catch (e) {
            log.appendLine(`mpq/list error: ${e}`)
            throw vscode.FileSystemError.Unavailable(`mpq/list failed: ${e}`)
        }

        log.appendLine(`mpq/list ← ${JSON.stringify(result).slice(0, 500)}`)

        if (result.error) {
            log.appendLine(`mpq/list server error: ${result.error}`)
            throw vscode.FileSystemError.FileNotFound(result.error)
        }

        const entries = result.entries || []
        log.appendLine(`mpq/list: ${entries.length} entries`)
        this._listCache.set(archivePath, entries)
        return entries
    }

    /**
     * Clear cache for an archive (call when unmounting).
     * @param {string} archivePath
     */
    clearCache(archivePath) {
        this._listCache.delete(archivePath)
    }

    // ── FileSystemProvider interface ─────────────────────────────

    watch(_uri, _options) {
        // Archives are read-only, no watching needed.
        return new vscode.Disposable(() => {})
    }

    /**
     * @param {vscode.Uri} uri
     * @returns {Promise<vscode.FileStat>}
     */
    async stat(uri) {
        const archivePath = MpqFileSystemProvider.decodeAuthority(uri.authority)
        const internal = uri.path.replace(/^\//, '')

        // Root of the archive → directory
        if (!internal) {
            return {
                type: vscode.FileType.Directory,
                ctime: 0,
                mtime: 0,
                size: 0,
            }
        }

        // Reject VS Code housekeeping paths that don't exist in MPQ archives
        if (internal.startsWith('.vscode/') || internal === '.vscode') {
            throw vscode.FileSystemError.FileNotFound(uri)
        }

        const entries = await this._fetchList(archivePath)

        // Normalise separators for comparison
        const norm = internal.replace(/\\/g, '/').toLowerCase()

        // Exact file match?
        const file = entries.find(e => e.name.replace(/\\/g, '/').toLowerCase() === norm)
        if (file) {
            return {
                type: vscode.FileType.File,
                ctime: 0,
                mtime: 0,
                size: file.size || 0,
            }
        }

        // Virtual directory? Check if any entry starts with this prefix.
        const prefix = norm.endsWith('/') ? norm : norm + '/'
        const isDir = entries.some(e => e.name.replace(/\\/g, '/').toLowerCase().startsWith(prefix))
        if (isDir) {
            return {
                type: vscode.FileType.Directory,
                ctime: 0,
                mtime: 0,
                size: 0,
            }
        }

        throw vscode.FileSystemError.FileNotFound(uri)
    }

    /**
     * @param {vscode.Uri} uri
     * @returns {Promise<[string, vscode.FileType][]>}
     */
    async readDirectory(uri) {
        const archivePath = MpqFileSystemProvider.decodeAuthority(uri.authority)
        const internal = uri.path.replace(/^\//, '')
        const prefix = internal ? internal.replace(/\\/g, '/') + '/' : ''
        const prefixLower = prefix.toLowerCase()

        const entries = await this._fetchList(archivePath)

        /** @type {Map<string, vscode.FileType>} */
        const children = new Map()

        for (const entry of entries) {
            const name = entry.name.replace(/\\/g, '/')

            if (prefixLower && !name.toLowerCase().startsWith(prefixLower)) continue

            const relative = prefix ? name.slice(prefix.length) : name
            if (!relative) continue

            const slashIdx = relative.indexOf('/')
            if (slashIdx === -1) {
                // Direct child file
                children.set(relative, vscode.FileType.File)
            } else {
                // Intermediate directory
                const dirName = relative.slice(0, slashIdx)
                children.set(dirName, vscode.FileType.Directory)
            }
        }

        return Array.from(children.entries())
    }

    /**
     * @param {vscode.Uri} uri
     * @returns {Promise<Uint8Array>}
     */
    async readFile(uri) {
        const archivePath = MpqFileSystemProvider.decodeAuthority(uri.authority)
        const filePath = uri.path.replace(/^\//, '')

        if (!filePath) {
            throw vscode.FileSystemError.FileIsADirectory(uri)
        }

        // Reject VS Code housekeeping paths
        if (filePath.startsWith('.vscode/') || filePath.startsWith('.')) {
            throw vscode.FileSystemError.FileNotFound(uri)
        }

        const client = await this._ensureClient()
        const log = this._getLog()

        log.appendLine(`mpq/read → ${filePath}`)

        let result
        try {
            result = await client.sendRequest('mpq/read', {archivePath, filePath})
        } catch (e) {
            log.appendLine(`mpq/read error: ${e}`)
            throw vscode.FileSystemError.Unavailable(`mpq/read failed: ${e}`)
        }

        if (result.error) {
            log.appendLine(`mpq/read server error: ${result.error}`)
            throw vscode.FileSystemError.FileNotFound(result.error)
        }

        return Buffer.from(result.content, 'base64')
    }

    // ── Read-only: all mutating operations throw ─────────────────

    createDirectory(_uri) {
        throw vscode.FileSystemError.NoPermissions('MPQ archives are read-only')
    }

    writeFile(_uri, _content, _options) {
        throw vscode.FileSystemError.NoPermissions('MPQ archives are read-only')
    }

    delete(_uri, _options) {
        throw vscode.FileSystemError.NoPermissions('MPQ archives are read-only')
    }

    rename(_oldUri, _newUri, _options) {
        throw vscode.FileSystemError.NoPermissions('MPQ archives are read-only')
    }
}

module.exports = {MpqFileSystemProvider}
