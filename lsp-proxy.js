const {spawn} = require('child_process')
const {Buffer} = require('buffer')

const realServerPath = process.env.REAL_LSP_PATH
if (!realServerPath) {
    console.error('REAL_LSP_PATH not set')
    process.exit(1)
}

const server = spawn(realServerPath, [], {
    stdio: ['pipe', 'pipe', 'pipe']
})

let clientBuffer = Buffer.alloc(0)
let serverBuffer = Buffer.alloc(0)

function format(msg) {
    try {
        const j = JSON.parse(msg)
        if (j.method === 'initialize') return '🔥initialize'
        return JSON.stringify(j, null, 4)
    } catch (e) {
        return msg
    }
}

process.stdin.on('data', chunk => {
    clientBuffer = Buffer.concat([clientBuffer, chunk])
    tryParseMessages(clientBuffer, msg => {
        console.error('➡️ To Server:\n', format(msg))
        const msgBuf = Buffer.from(msg, 'utf8')
        server.stdin.write(`Content-Length: ${msgBuf.length}\r\n\r\n`)
        server.stdin.write(msgBuf)
    }, remaining => {
        clientBuffer = remaining
    })
})

server.stdout.on('data', chunk => {
    serverBuffer = Buffer.concat([serverBuffer, chunk])
    tryParseMessages(serverBuffer, msg => {
        console.error('⬅️ From Server:\n', format(msg))
        const msgBuf = Buffer.from(msg, 'utf8')
        process.stdout.write(`Content-Length: ${msgBuf.length}\r\n\r\n`)
        process.stdout.write(msgBuf)
    }, remaining => {
        serverBuffer = remaining
    })
})

server.stderr.on('data', chunk => {
    console.error('⚠️', chunk.toString())
})

process.stdin.on('end', () => {
    server.stdin.end()
    server.kill('SIGTERM')
})

function tryParseMessages(buffer, onMessage, onRemaining) {
    let offset = 0
    while (true) {
        const headerEnd = buffer.indexOf('\r\n\r\n', offset)
        if (headerEnd === -1) break

        const headerStr = buffer.slice(offset, headerEnd).toString('utf8')
        const match = headerStr.match(/Content-Length: (\d+)/i)
        if (!match) break

        const contentLength = parseInt(match[1], 10)
        const messageStart = headerEnd + 4
        const messageEnd = messageStart + contentLength

        if (buffer.length < messageEnd) break

        const message = buffer.slice(messageStart, messageEnd).toString('utf8')
        onMessage(message)

        offset = messageEnd
    }

    // slice remaining unprocessed buffer
    onRemaining(buffer.slice(offset))
}
