// noinspection JSUnusedGlobalSymbols
/**
 * HTTP transport layer for communicating with the Rust backend.
 *
 * All methods use a persistent keep-alive Agent to reuse TCP connections,
 * eliminating per-request handshake overhead on localhost.
 */

const http = require('http')

class HttpTransport {
    constructor() {
        /** @private */
        this._agent = new http.Agent({
            keepAlive: true,
            maxSockets: 8,
            keepAliveMsecs: 30_000,
        })
        /** @private @type {{port: number, token: string} | null} */
        this._serverInfo = null
    }

    /**
     * Set connection info after server starts.
     * @param {{port: number, token: string}} info
     */
    init(info) {
        this._serverInfo = info
    }

    /** @returns {{port: number, token: string} | null} */
    get serverInfo() {
        return this._serverInfo
    }

    /**
     * POST raw binary body and receive binary response.
     * @param {string} path
     * @param {Record<string, string>} queryParams
     * @param {Buffer} body
     * @param {AbortSignal} [signal]
     * @returns {Promise<Buffer>}
     */
    postBinary(path, queryParams, body, signal) {
        return this._request('POST', path, queryParams, body, 'application/octet-stream', signal)
    }

    /**
     * POST JSON body and receive binary response.
     * @param {string} path
     * @param {*} body
     * @param {AbortSignal} [signal]
     * @returns {Promise<Buffer>}
     */
    postJson(path, body, signal) {
        const data = Buffer.from(JSON.stringify(body), 'utf8')
        return this._request('POST', path, {}, data, 'application/json', signal)
    }

    /**
     * POST JSON body, fire-and-forget.
     * @param {string} path
     * @param {*} body
     * @returns {Promise<void>}
     */
    post(path, body) {
        return this.postJson(path, body).then(() => {})
    }

    /**
     * GET binary response.
     * @param {string} path
     * @param {Record<string, string>} [queryParams]
     * @returns {Promise<Buffer>}
     */
    getBinary(path, queryParams = {}) {
        return this._request('GET', path, queryParams, null, null, null)
    }

    /**
     * Core request method.
     * @param {string} method
     * @param {string} path
     * @param {Record<string, string>} extraParams
     * @param {Buffer|null} body
     * @param {string|null} contentType
     * @param {AbortSignal|null} [signal]
     * @returns {Promise<Buffer>}
     * @private
     */
    _request(method, path, extraParams, body, contentType, signal) {
        return new Promise((resolve, reject) => {
            if (!this._serverInfo) return reject(new Error('Server not started'))
            if (signal?.aborted) return reject(new DOMException('Aborted', 'AbortError'))

            const qs = new URLSearchParams({token: this._serverInfo.token, ...extraParams})
            const headers = {}
            if (contentType) headers['Content-Type'] = contentType
            if (body) headers['Content-Length'] = body.length

            const req = http.request({
                hostname: '127.0.0.1',
                port: this._serverInfo.port,
                path: `${path}?${qs.toString()}`,
                method,
                agent: this._agent,
                headers,
            }, res => {
                const chunks = []
                res.on('data', chunk => chunks.push(chunk))
                res.on('end', () => {
                    if (res.statusCode >= 400) {
                        return reject(new Error(`HTTP ${res.statusCode}`))
                    }
                    resolve(Buffer.concat(chunks))
                })
            })

            if (signal) {
                const onAbort = () => {
                    req.destroy()
                    reject(new DOMException('Aborted', 'AbortError'))
                }
                signal.addEventListener('abort', onAbort, {once: true})
                req.on('close', () => signal.removeEventListener('abort', onAbort))
            }

            req.on('error', err => {
                if (signal?.aborted) {
                    reject(new DOMException('Aborted', 'AbortError'))
                } else {
                    reject(err)
                }
            })

            if (body) req.write(body)
            req.end()

            // Safety timeout — prevent requests from hanging indefinitely
            const timeout = setTimeout(() => {
                req.destroy()
                reject(new Error('Request timeout (60s)'))
            }, 60_000)
            req.on('close', () => clearTimeout(timeout))
        })
    }

    /** Destroy keep-alive connections. */
    destroy() {
        this._agent.destroy()
    }
}

module.exports = {HttpTransport}

