// Test for collision detection in leak fix
//
// Это тестовый файл для проверки защиты от коллизий при фиксинге утечек.
//
// Сценарий:
// - Функция Anal возвращает handle локальную переменную
// - Уже существует глобальная переменная Anal_ret
// - Защита должна создать Anal_ret_2

function Anal takes nothing returns unit
    local unit A = CreateUnit('null', 0, 0., 0., 0.)
    return A
endfunction

// Это глобальная переменная, которая создаст коллизию
integer Anal_ret = 33

// При фиксинге утечки функции Anal, должна быть создана Anal_ret_2
// вместо попытки использовать Anal_ret

