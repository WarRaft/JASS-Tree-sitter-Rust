# `//ignore` — Директива подавления диагностик

Директива `//ignore` подавляет определённые диагностики для **всего файла**.
Она должна находиться **в самом начале** файла, рядом с директивами `//import`
и `//set`, до любых языковых конструкций.

Для подавления диагностики отдельной декларации используйте `//@ignore` над функцией или переменной.

## Синтаксис

```jass
//ignore <тег…>
```

* Токен `//ignore` должен начинаться с **колонки 0** (без ведущих пробелов).
* Можно указать один или несколько тегов через пробел в одной строке.

## Пример

```jass
//import common/natives.j
//ignore unused leak

function Helper takes nothing returns nothing
endfunction
```

## Доступные теги

| Тег | Файл (`//ignore`) | Функция (`//@ignore`) | Переменная (`//@ignore`) |
|-----|:-:|:-:|:-:|
| `unused` | ✔ | ✔ | — |
| `leak` | ✔ | ✔ | ✔ |
| `cycle` | ✔ | ✔ | — |

* **`unused`** — подавить диагностику **неиспользуемых функций**.
* **`leak`** — подавить диагностику **утечек handle**.
* **`cycle`** — подавить диагностику **циклических вызовов**.

## Подавление для отдельной декларации

Используйте `//@ignore` в комментарии непосредственно над декларацией.
Теги можно комбинировать в одной строке: `//@ignore unused cycle`.

### Уровень функции

`//@ignore` над функцией подавляет диагностику только для неё:

```jass
//@ignore unused
function Helper takes nothing returns nothing
endfunction

//@ignore cycle
function Recursive takes nothing returns nothing
    call Recursive()
endfunction

//@ignore leak
function Setup takes nothing returns nothing
    local unit u = CreateUnit()
endfunction
```

### Уровень переменной

`//@ignore leak` над объявлением `local` подавляет диагностику утечки только для этой переменной:

```jass
function Foo takes nothing returns nothing
    //@ignore leak
    local unit u = CreateUnit()
    local unit v = CreateUnit()  // ← по-прежнему диагностируется
endfunction
```

## Поведение

* Теги действуют только в пределах одного файла — они не распространяются через `//import`.
* Нераспознанные теги принимаются без ошибок (для прямой совместимости).
* Отсутствующий тег вызывает предупреждение (warning).


