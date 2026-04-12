// noinspection JSUnusedGlobalSymbols
/**
 * Process-based client for the Rust backend.
 *
 * The server process prints a JSON line to stdout on startup:
 *   {"port": <number>, "token": "<string>"}
 *
 * All communication uses HTTP via {@link HttpTransport}.
 * stdin is kept open only for process lifecycle — when VS Code exits,
 * stdin closes, and the server shuts down.
 */

const {spawn} = require('child_process')
const {HttpTransport} = require('./httpTransport')

class ServerClient {
    /**
     * @param {string} command  Path to the server binary
     * @param {string[]} [args]
     * @param {object} [spawnOpts]
     */
    constructor(command, args = [], spawnOpts = {}) {
        /** @private */ this._command = command
        /** @private */ this._args = args
        /** @private */ this._spawnOpts = spawnOpts
        /** @private */ this._proc = null
        /** @private */ this._started = false

        /** HTTP transport layer */
        this.http = new HttpTransport()

        /**
         * When true, messages are logged to `_traceOutputChannel`.
         * Toggle at runtime: `client.traceMessages = true`
         */
        this.traceMessages = false

        /**
         * Optional VS Code OutputChannel for structured trace output.
         * @type {import('vscode').OutputChannel | null}
         */
        this._traceOutputChannel = null
    }

    /**
     * Start the server process.
     * @returns {Promise<void>}
     */
    async start() {
        if (this._started) return

        this._proc = spawn(this._command, this._args, {
            stdio: ['pipe', 'pipe', 'pipe'],
            ...this._spawnOpts,
        })

        this._proc.stderr.on('data', chunk => {
            console.error(`[server stderr] ${chunk.toString()}`)
        })

        this._proc.on('exit', code => {
            console.log(`Server exited with code ${code}`)
        })

        // Read the first line from stdout: {"port": ..., "token": "..."}
        const info = await this._readStartupInfo()
        this.http.init(info)

        this._started = true
    }

    /**
     * Read the startup JSON from the server's stdout.
     * @returns {Promise<{port: number, token: string}>}
     * @private
     */
    _readStartupInfo() {
        return new Promise((resolve, reject) => {
            let buffer = ''
            const onData = chunk => {
                buffer += chunk.toString()
                const newlineIdx = buffer.indexOf('\n')
                if (newlineIdx !== -1) {
                    this._proc.stdout.removeListener('data', onData)
                    const line = buffer.substring(0, newlineIdx).trim()
                    try {
                        const info = JSON.parse(line)
                        if (info.port && info.token) {
                            resolve(info)
                        } else {
                            reject(new Error(`Invalid startup JSON: ${line}`))
                        }
                    } catch {
                        reject(new Error(`Failed to parse startup JSON: ${line}`))
                    }
                }
            }

            this._proc.stdout.on('data', onData)

            setTimeout(() => {
                this._proc.stdout.removeListener('data', onData)
                reject(new Error('Timeout waiting for server startup'))
            }, 10000)
        })
    }

    /**
     * Send a JSON request and return the parsed response.
     * @param {string} method  Route path (e.g. 'build/execute')
     * @param {*} params
     * @returns {Promise<any>}
     */
    async sendRequest(method, params) {
        if (!this._started) return null
        const buf = await this.http.postJson(`/${method}`, params)
        return JSON.parse(buf.toString('utf8'))
    }

    /**
     * Send a JSON request and return the raw binary response (no JSON parse).
     * @param {string} method  Route path
     * @param {*} params
     * @returns {Promise<Buffer>}
     */
    async sendBinaryRequest(method, params) {
        if (!this._started) return null
        return this.http.postJson(`/${method}`, params)
    }

    /**
     * Get the binary HTTP server info.
     * @returns {{port: number, token: string} | null}
     */
    getServerInfo() {
        return this.http.serverInfo
    }

    /**
     * Log a trace line when `traceMessages` is enabled.
     * @param {'→' | '←'} direction
     * @param {string} text
     */
    trace(direction, text) {
        if (!this.traceMessages) return
        const ts = new Date().toISOString()
        const line = `[${ts}] ${direction} ${text}`
        if (this._traceOutputChannel) {
            this._traceOutputChannel.appendLine(line)
        }
    }

    /**
     * Stop the server process.
     * @returns {Promise<void>}
     */
    async stop() {
        if (!this._proc) return
        this.http.destroy()
        if (this._proc.stdin) {
            this._proc.stdin.end()
        }
        await new Promise(resolve => setTimeout(resolve, 200))
        if (this._proc && !this._proc.killed) {
            this._proc.kill('SIGTERM')
        }
        this._proc = null
        this._started = false
    }

    /**
     * Restart the server process.
     * @returns {Promise<void>}
     */
    async restart() {
        await this.stop()
        this.http = new HttpTransport()
        await this.start()
    }
}

module.exports = {ServerClient}
