// noinspection JSUnusedGlobalSymbols
/**
 * Custom process-based client that communicates with the Rust backend
 * via WebSocket (bidirectional) and HTTP (request/response).
 *
 * The server process prints a JSON line to stdout on startup:
 *   {"port": <number>, "token": "<string>"}
 *
 * The extension connects to ws://127.0.0.1:{port}/ws?token={token}
 * for push notifications and request/response messaging.
 *
 * stdin is kept open only for process lifecycle — when VS Code exits,
 * stdin closes, and the server shuts down.
 */

const {spawn} = require('child_process')

/**
 * Error thrown when a request is cancelled (e.g. because the document changed).
 * Callers can check `err.isCancellation` to distinguish from real errors.
 */
class RequestCancelledError extends Error {
    constructor(id) {
        super(`Request ${id} cancelled`)
        this.isCancellation = true
        this.requestId = id
    }
}

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
        /** @private @type {import('ws').WebSocket | null} */
        this._ws = null
        /** @private */ this._nextId = 1
        /** @private @type {Map<number, {resolve: Function, reject: Function}>} */
        this._pending = new Map()
        /** @private @type {Map<string, Function[]>} */
        this._notificationHandlers = new Map()
        /** @private */ this._started = false
        /** @private @type {{port: number, token: string} | null} */
        this._serverInfo = null

        // ── URI-based request tracking for cancellation ───────────
        /** @private @type {Map<string, Set<number>>} uri → set of request IDs */
        this._pendingByUri = new Map()
        /** @private @type {Map<number, string>} request ID → uri */
        this._idToUri = new Map()
    }

    /**
     * Start the server process and connect via WebSocket.
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

        this._proc.on('exit', (code) => {
            console.log(`Server exited with code ${code}`)
            for (const [, {reject}] of this._pending) {
                reject(new Error(`Server exited with code ${code}`))
            }
            this._pending.clear()
            this._pendingByUri.clear()
            this._idToUri.clear()
        })

        // Read the first line from stdout: {"port": ..., "token": "..."}
        this._serverInfo = await this._readStartupInfo()
        console.log(`Server ready on port ${this._serverInfo.port}`)

        // Connect WebSocket
        await this._connectWebSocket()

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
            const onData = (chunk) => {
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
                    } catch (e) {
                        reject(new Error(`Failed to parse startup JSON: ${line}`))
                    }
                }
            }

            this._proc.stdout.on('data', onData)

            // Timeout after 10 seconds
            setTimeout(() => {
                this._proc.stdout.removeListener('data', onData)
                reject(new Error('Timeout waiting for server startup'))
            }, 10000)
        })
    }

    /**
     * Connect to the server's WebSocket endpoint.
     * @returns {Promise<void>}
     * @private
     */
    _connectWebSocket() {
        return new Promise((resolve, reject) => {
            const url = `ws://127.0.0.1:${this._serverInfo.port}/ws?token=${this._serverInfo.token}`

            // Use the built-in http module to do a raw WebSocket upgrade
            // (VS Code extensions can't use browser WebSocket and we avoid
            // pulling in the 'ws' npm package).
            const http = require('http')
            const crypto = require('crypto')

            const key = crypto.randomBytes(16).toString('base64')
            const urlObj = new URL(url)

            const req = http.request({
                hostname: urlObj.hostname,
                port: urlObj.port,
                path: urlObj.pathname + urlObj.search,
                method: 'GET',
                headers: {
                    'Upgrade': 'websocket',
                    'Connection': 'Upgrade',
                    'Sec-WebSocket-Key': key,
                    'Sec-WebSocket-Version': '13',
                },
            })

            req.on('upgrade', (res, socket, head) => {
                this._socket = socket
                this._wsBuffer = Buffer.alloc(0)

                socket.on('data', (data) => {
                    this._wsBuffer = Buffer.concat([this._wsBuffer, data])
                    this._processWebSocketFrames()
                })

                socket.on('close', () => {
                    console.log('WebSocket closed')
                })

                socket.on('error', (err) => {
                    console.error('WebSocket error:', err)
                })

                // Process any initial data from the upgrade
                if (head && head.length > 0) {
                    this._wsBuffer = Buffer.concat([this._wsBuffer, head])
                    this._processWebSocketFrames()
                }

                resolve()
            })

            req.on('error', (err) => {
                reject(new Error(`WebSocket connection failed: ${err.message}`))
            })

            req.end()
        })
    }

    /**
     * Process incoming WebSocket frames from the raw socket buffer.
     * @private
     */
    _processWebSocketFrames() {
        while (this._wsBuffer.length >= 2) {
            const firstByte = this._wsBuffer[0]
            const secondByte = this._wsBuffer[1]
            const opcode = firstByte & 0x0F
            const isMasked = (secondByte & 0x80) !== 0
            let payloadLength = secondByte & 0x7F
            let offset = 2

            if (payloadLength === 126) {
                if (this._wsBuffer.length < 4) return
                payloadLength = this._wsBuffer.readUInt16BE(2)
                offset = 4
            } else if (payloadLength === 127) {
                if (this._wsBuffer.length < 10) return
                // For simplicity, read as 32-bit (messages > 4GB unlikely)
                payloadLength = this._wsBuffer.readUInt32BE(6)
                offset = 10
            }

            if (isMasked) offset += 4
            if (this._wsBuffer.length < offset + payloadLength) return

            const payload = this._wsBuffer.slice(offset, offset + payloadLength)
            this._wsBuffer = this._wsBuffer.slice(offset + payloadLength)

            if (opcode === 0x01) {
                // Text frame
                const text = payload.toString('utf8')
                try {
                    const msg = JSON.parse(text)
                    this._handleMessage(msg)
                } catch (e) {
                    console.error('Failed to parse WebSocket message:', e, text.substring(0, 200))
                }
            } else if (opcode === 0x08) {
                // Close frame
                console.log('WebSocket close frame received')
            } else if (opcode === 0x09) {
                // Ping — send pong
                this._sendWebSocketFrame(0x0A, payload)
            }
            // opcode 0x0A = pong — ignore
        }
    }

    /**
     * Send a raw WebSocket frame.
     * @param {number} opcode
     * @param {Buffer} payload
     * @private
     */
    _sendWebSocketFrame(opcode, payload) {
        if (!this._socket || this._socket.destroyed) return

        const crypto = require('crypto')
        const mask = crypto.randomBytes(4)
        const maskedPayload = Buffer.alloc(payload.length)
        for (let i = 0; i < payload.length; i++) {
            maskedPayload[i] = payload[i] ^ mask[i % 4]
        }

        let header
        if (payload.length < 126) {
            header = Buffer.alloc(6)
            header[0] = 0x80 | opcode // FIN + opcode
            header[1] = 0x80 | payload.length // MASK + length
            mask.copy(header, 2)
        } else if (payload.length < 65536) {
            header = Buffer.alloc(8)
            header[0] = 0x80 | opcode
            header[1] = 0x80 | 126
            header.writeUInt16BE(payload.length, 2)
            mask.copy(header, 4)
        } else {
            header = Buffer.alloc(14)
            header[0] = 0x80 | opcode
            header[1] = 0x80 | 127
            header.writeUInt32BE(0, 2) // high 32 bits
            header.writeUInt32BE(payload.length, 6) // low 32 bits
            mask.copy(header, 10)
        }

        this._socket.write(Buffer.concat([header, maskedPayload]))
    }

    /**
     * Send a text message via WebSocket.
     * @param {string} text
     * @private
     */
    _sendText(text) {
        this._sendWebSocketFrame(0x01, Buffer.from(text, 'utf8'))
    }

    /** @private */
    _handleMessage(msg) {
        // Response to a request we sent
        if (msg.id != null && (msg.result !== undefined || msg.error !== undefined) && !msg.method) {
            const pending = this._pending.get(msg.id)
            if (pending) {
                this._pending.delete(msg.id)
                this._cleanupUriTracking(msg.id)
                if (msg.error) {
                    pending.reject(new Error(msg.error.message || JSON.stringify(msg.error)))
                } else {
                    pending.resolve(msg.result)
                }
            }
            return
        }

        // Server-initiated request (has id AND method) — auto-respond
        if (msg.id != null && msg.method) {
            const handlers = this._notificationHandlers.get(msg.method)
            if (handlers) {
                for (const handler of handlers) {
                    try { handler(msg.params) } catch (e) {
                        console.error(`Handler error for ${msg.method}:`, e)
                    }
                }
            }
            // Send an empty success response back
            this._sendText(JSON.stringify({jsonrpc: '2.0', id: msg.id, result: null}))
            return
        }

        // Notification from server (no id, has method)
        if (msg.method) {
            const handlers = this._notificationHandlers.get(msg.method)
            if (handlers) {
                for (const handler of handlers) {
                    try {
                        handler(msg.params)
                    } catch (e) {
                        console.error(`Notification handler error for ${msg.method}:`, e)
                    }
                }
            }
            return
        }
    }

    /**
     * Send a JSON-RPC request and wait for the response.
     * @param {string} method
     * @param {*} [params]
     * @param {string} [uri]  Optional document URI — when provided, the request
     *                        is tracked so it can be cancelled via `cancelUri()`.
     * @returns {Promise<*>}
     */
    sendRequest(method, params, uri) {
        return new Promise((resolve, reject) => {
            const id = this._nextId++
            this._pending.set(id, {resolve, reject})

            // Track by URI for bulk cancellation
            if (uri) {
                this._idToUri.set(id, uri)
                if (!this._pendingByUri.has(uri)) {
                    this._pendingByUri.set(uri, new Set())
                }
                this._pendingByUri.get(uri).add(id)
            }

            this._sendText(JSON.stringify({
                jsonrpc: '2.0',
                id,
                method,
                params: params ?? {},
            }))
        })
    }

    /**
     * Cancel all in-flight requests for the given URI.
     *
     * - Sends `$/cancelRequest` to the server for each tracked request ID.
     * - Immediately rejects the client-side promises with `RequestCancelledError`.
     *
     * @param {string} uri
     */
    cancelUri(uri) {
        const ids = this._pendingByUri.get(uri)
        if (!ids || ids.size === 0) return

        for (const id of ids) {
            // Notify the server
            this.sendNotification('$/cancelRequest', {id})

            // Reject the client-side promise
            const pending = this._pending.get(id)
            if (pending) {
                this._pending.delete(id)
                pending.reject(new RequestCancelledError(id))
            }
            this._idToUri.delete(id)
        }
        this._pendingByUri.delete(uri)
    }

    /**
     * Remove a request ID from the URI tracking maps.
     * @param {number} id
     * @private
     */
    _cleanupUriTracking(id) {
        const uri = this._idToUri.get(id)
        if (uri) {
            this._idToUri.delete(id)
            const ids = this._pendingByUri.get(uri)
            if (ids) {
                ids.delete(id)
                if (ids.size === 0) this._pendingByUri.delete(uri)
            }
        }
    }

    /**
     * Send a JSON-RPC notification (fire-and-forget).
     * @param {string} method
     * @param {*} [params]
     */
    sendNotification(method, params) {
        this._sendText(JSON.stringify({
            jsonrpc: '2.0',
            method,
            params: params ?? {},
        }))
    }

    /**
     * Register a handler for a server notification.
     * @param {string} method
     * @param {Function} handler
     */
    onNotification(method, handler) {
        if (!this._notificationHandlers.has(method)) {
            this._notificationHandlers.set(method, [])
        }
        this._notificationHandlers.get(method).push(handler)
    }

    /**
     * Get the binary HTTP server info.
     * @returns {{port: number, token: string} | null}
     */
    getServerInfo() {
        return this._serverInfo
    }

    /**
     * Stop the server process.
     * @returns {Promise<void>}
     */
    async stop() {
        if (!this._proc) return
        // Close WebSocket
        if (this._socket && !this._socket.destroyed) {
            this._sendWebSocketFrame(0x08, Buffer.alloc(0)) // close frame
            this._socket.end()
        }
        // Close stdin → triggers server shutdown
        if (this._proc.stdin) {
            this._proc.stdin.end()
        }
        // Give it a moment to exit gracefully
        await new Promise(resolve => setTimeout(resolve, 200))
        if (this._proc && !this._proc.killed) {
            this._proc.kill('SIGTERM')
        }
        this._proc = null
        this._socket = null
        this._started = false
    }

    /**
     * Restart the server process.
     * @returns {Promise<void>}
     */
    async restart() {
        await this.stop()
        await this.start()
    }
}

module.exports = {ServerClient, RequestCancelledError}

