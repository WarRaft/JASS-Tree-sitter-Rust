const {spawn} = require('child_process')

const realServerPath = process.env.REAL_LSP_PATH
if (!realServerPath) {
    console.error('REAL_LSP_PATH not set')
    process.exit(1)
}

const server = spawn(realServerPath, [], {
    stdio: ['pipe', 'pipe', 'inherit']
})

let clientBuffer = ''
let serverBuffer = ''

function format(msg) {
    const j = JSON.parse(msg)
    if (j['method'] === 'initialize') return '🔥initialize'
    return JSON.stringify(j, null, 4)
}

process.stdin.on('data', chunk => {
    clientBuffer += chunk.toString()
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
    serverBuffer += chunk.toString()
    tryParseMessages(serverBuffer, msg => {
        console.error('⬅️ From Server:\n', format(msg))
        const msgBuf = Buffer.from(msg, 'utf8')
        process.stdout.write(`Content-Length: ${msgBuf.length}\r\n\r\n`)
        process.stdout.write(msgBuf)
    }, remaining => {
        serverBuffer = remaining
    })
})

process.stdin.on('end', () => {
    console.log('stdin closed, shutting down the server...')
    server.stdin.end()  // Закрыть stdin потока серверного процесса
    server.kill('SIGTERM')  // Отправить сигнал завершения процессу
})

function tryParseMessages(buffer, onMessage, onRemaining) {
    while (true) {
        const headerEnd = buffer.indexOf('\r\n\r\n')
        if (headerEnd === -1) break

        const header = buffer.slice(0, headerEnd)
        const match = header.match(/Content-Length: (\d+)/i)
        if (!match) break

        const length = parseInt(match[1], 10)
        const totalLength = headerEnd + 4 + length
        if (buffer.length < totalLength) break

        const message = buffer.slice(headerEnd + 4, totalLength)
        onMessage(message)
        buffer = buffer.slice(totalLength)
    }
    onRemaining(buffer)
}
