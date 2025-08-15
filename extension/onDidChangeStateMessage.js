function onDidChangeStateMessage(oldState, newState) {
    // Строгое покрытие всех переходов
    if (oldState === 1) { // Stopped
        if (newState === 1) {
            return '🟡 LSP клиент уже был остановлен.\nВозможно, попытка перезапуска не сработала.'
        }
        if (newState === 2) {
            return '⚠️ LSP клиент перескочил из Stopped прямо в Running.\nОбычно должен быть этап запуска (Starting).'
        }
        if (newState === 3) {
            return // Ожидаемый запуск
        }
    }

    if (oldState === 2) { // Running
        if (newState === 1) {
            return // Корректное завершение
        }
        if (newState === 2) {
            return '🔁 LSP клиент уже был запущен.\nПовторный переход в Running — необычное поведение.'
        }
        if (newState === 3) {
            return '🤨 LSP клиент начал запуск, уже будучи активным.\nЭто выглядит странно и может указывать на баг.'
        }
    }

    if (oldState === 3) { // Starting
        if (newState === 1) {
            return '❌ LSP клиент не смог завершить запуск\nи вернулся в Stopped.'
        }
        if (newState === 2) {
            return // Успешный запуск
        }
        if (newState === 3) {
            return '🌀 LSP клиент продолжает запускаться...\nПохоже, он застрял в состоянии Starting.'
        }
    }

    // Все состояния известны, сюда попасть нельзя
    throw new Error(`Непредусмотренный переход: ${oldState} → ${newState}`)
}

module.exports = {
    onDidChangeStateMessage,
}