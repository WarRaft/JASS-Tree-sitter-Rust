function onDidChangeStateMessage(oldState, newState) {
    // Strict coverage of all transitions
    if (oldState === 1) { // Stopped
        if (newState === 1) {
            return '🟡 Server was already stopped.\nRestart attempt may have failed.'
        }
        if (newState === 2) {
            return '⚠️ Server jumped from Stopped to Running.\nNormally there should be a Starting phase.'
        }
        if (newState === 3) {
            return // Expected start
        }
    }

    if (oldState === 2) { // Running
        if (newState === 1) {
            return // Normal shutdown
        }
        if (newState === 2) {
            return '🔁 Server was already running.\nRepeated transition to Running — unusual behavior.'
        }
        if (newState === 3) {
            return '🤨 Server started starting while already active.\nThis looks odd and may indicate a bug.'
        }
    }

    if (oldState === 3) { // Starting
        if (newState === 1) {
            return '❌ Server failed to finish starting\nand returned to Stopped.'
        }
        if (newState === 2) {
            return // Successful start
        }
        if (newState === 3) {
            return '🌀 Server keeps starting...\nIt seems stuck in Starting state.'
        }
    }

    // All states are known — should never reach here
    throw new Error(`Unexpected transition: ${oldState} → ${newState}`)
}

module.exports = {
    onDidChangeStateMessage,
}