# `//ignore` — Директива придушення діагностик

Директива `//ignore` придушує певні діагностики для **всього файлу**.
Вона повинна знаходитися **на самому початку** файлу, поруч із директивами
`//import` та `//set`, перед будь-якими мовними конструкціями.

Для придушення діагностики окремої декларації використовуйте `//@ignore` над функцією або змінною.

## Синтаксис

```jass
//ignore <тег…>
```

* Токен `//ignore` повинен починатися з **колонки 0** (без початкових пробілів).
* Можна вказати один або кілька тегів через пробіл в одному рядку.

## Приклад

```jass
//import common/natives.j
//ignore unused leak

function Helper takes nothing returns nothing
endfunction
```

## Доступні теги

| Тег | Файл (`//ignore`) | Функція (`//@ignore`) | Змінна (`//@ignore`) |
|-----|:-:|:-:|:-:|
| `unused` | ✔ | ✔ | — |
| `leak` | ✔ | ✔ | ✔ |
| `cycle` | ✔ | ✔ | — |

* **`unused`** — придушити діагностику **невикористаних функцій**.
* **`leak`** — придушити діагностику **витоків handle**.
* **`cycle`** — придушити діагностику **циклічних викликів**.

## Придушення для окремої декларації

Використовуйте `//@ignore` в коментарі безпосередньо над декларацією.
Теги можна комбінувати в одному рядку: `//@ignore unused cycle`.

### Рівень функції

`//@ignore` над функцією придушує діагностику лише для неї:

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

### Рівень змінної

`//@ignore leak` над оголошенням `local` придушує діагностику витоку лише для цієї змінної:

```jass
function Foo takes nothing returns nothing
    //@ignore leak
    local unit u = CreateUnit()
    local unit v = CreateUnit()  // ← все ще діагностується
endfunction
```

## Поведінка

* Теги діють лише в межах одного файлу — вони не поширюються через `//import`.
* Нерозпізнані теги приймаються без помилок (для прямої сумісності).
* Відсутній тег викликає попередження (warning).


