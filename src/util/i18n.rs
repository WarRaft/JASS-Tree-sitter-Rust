//! Centralized localization module.
//!
//! Detects the user's locale once (from `$LANG` / `$LC_ALL` / `$LC_MESSAGES`
//! / `$LANGUAGE`) and provides accessor functions for every translatable
//! user-facing string in the LSP.
//!
//! Supported locales: English (default), Russian, Ukrainian,
//! Simplified Chinese, Traditional Chinese.

use std::sync::OnceLock;

// ─── Locale enum ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Ru,
    Uk,
    Zh,
    Tc,
}

static LOCALE: OnceLock<Locale> = OnceLock::new();

/// Return the cached locale for this process.
pub fn locale() -> Locale {
    *LOCALE.get_or_init(detect_locale)
}

fn detect_locale() -> Locale {
    let lang = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANGUAGE"))
        .unwrap_or_default()
        .to_lowercase();

    if lang.starts_with("ru") {
        Locale::Ru
    } else if lang.starts_with("uk") {
        Locale::Uk
    } else if lang.starts_with("zh_tw")
        || lang.starts_with("zh_hant")
        || lang.starts_with("zh-tw")
        || lang.starts_with("zh-hant")
    {
        Locale::Tc
    } else if lang.starts_with("zh") {
        Locale::Zh
    } else {
        Locale::En
    }
}

/// Pick a value based on the current locale — helper for `include_str!` docs.
pub fn pick<T>(en: T, ru: T, uk: T, zh: T, tc: T) -> T {
    match locale() {
        Locale::En => en,
        Locale::Ru => ru,
        Locale::Uk => uk,
        Locale::Zh => zh,
        Locale::Tc => tc,
    }
}

// ─── Syntax / AST diagnostics ───────────────────────────────────────────────

pub fn syntax_error() -> &'static str {
    pick(
        "Syntax error",
        "Синтаксическая ошибка",
        "Синтаксична помилка",
        "语法错误",
        "語法錯誤",
    )
}

pub fn missing_token(kind: &str) -> String {
    match locale() {
        Locale::En => format!("Missing `{}`", kind),
        Locale::Ru => format!("Отсутствует `{}`", kind),
        Locale::Uk => format!("Відсутній `{}`", kind),
        Locale::Zh => format!("缺少 `{}`", kind),
        Locale::Tc => format!("缺少 `{}`", kind),
    }
}

pub fn unexpected_node(kind: &str) -> String {
    match locale() {
        Locale::En => format!("Unexpected `{}`", kind),
        Locale::Ru => format!("Неожиданный `{}`", kind),
        Locale::Uk => format!("Неочікуваний `{}`", kind),
        Locale::Zh => format!("意外的 `{}`", kind),
        Locale::Tc => format!("意外的 `{}`", kind),
    }
}

pub fn else_if_should_be_elseif() -> &'static str {
    match locale() {
        Locale::En => "`else if` → use `elseif`",
        Locale::Ru => "`else if` → используйте `elseif`",
        Locale::Uk => "`else if` → використовуйте `elseif`",
        Locale::Zh => "`else if` → 请使用 `elseif`",
        Locale::Tc => "`else if` → 請使用 `elseif`",
    }
}

pub fn fix_else_if_to_elseif() -> &'static str {
    pick(
        "Replace `else if` with `elseif`",
        "Заменить `else if` на `elseif`",
        "Замінити `else if` на `elseif`",
        "将 `else if` 替换为 `elseif`",
        "將 `else if` 替換為 `elseif`",
    )
}

pub fn fix_add_endif() -> &'static str {
    pick(
        "Add missing `endif`",
        "Добавить `endif`",
        "Додати `endif`",
        "添加缺少的 `endif`",
        "添加缺少的 `endif`",
    )
}

// ─── Undeclared symbol ──────────────────────────────────────────────────────

fn undeclared_label_type() -> &'static str {
    pick("type", "тип", "тип", "类型", "類型")
}
fn undeclared_label_function() -> &'static str {
    pick("function", "функция", "функція", "函数", "函式")
}
fn undeclared_label_variable() -> &'static str {
    pick("variable", "переменная", "змінна", "变量", "變數")
}

/// Returns the localized label for an undeclared symbol category.
pub fn undeclared_label(is_type_ref: bool, is_func: bool) -> &'static str {
    if is_type_ref {
        undeclared_label_type()
    } else if is_func {
        undeclared_label_function()
    } else {
        undeclared_label_variable()
    }
}

pub fn undeclared_symbol(label: &str, name: &str) -> String {
    match locale() {
        Locale::En => format!("Undeclared {} `{}`", label, name),
        Locale::Ru => format!("Необъявленный {} `{}`", label, name),
        Locale::Uk => format!("Неоголошений {} `{}`", label, name),
        Locale::Zh => format!("未声明的{} `{}`", label, name),
        Locale::Tc => format!("未宣告的{} `{}`", label, name),
    }
}

// ─── Type system diagnostics ────────────────────────────────────────────────

pub fn cannot_assign_type(expr_type: &str, declared_type: &str) -> String {
    match locale() {
        Locale::En => format!("Cannot assign type `{}` to `{}`", expr_type, declared_type),
        Locale::Ru => format!("Невозможно присвоить тип `{}` к `{}`", expr_type, declared_type),
        Locale::Uk => format!("Неможливо присвоїти тип `{}` до `{}`", expr_type, declared_type),
        Locale::Zh => format!("无法将类型 `{}` 赋值给 `{}`", expr_type, declared_type),
        Locale::Tc => format!("無法將類型 `{}` 賦值給 `{}`", expr_type, declared_type),
    }
}

pub fn return_type_mismatch(expr_type: &str, declared_type: &str) -> String {
    match locale() {
        Locale::En => format!("Cannot return type `{}` from function returning `{}`", expr_type, declared_type),
        Locale::Ru => format!("Невозможно вернуть тип `{}` из функции, возвращающей `{}`", expr_type, declared_type),
        Locale::Uk => format!("Неможливо повернути тип `{}` з функції, що повертає `{}`", expr_type, declared_type),
        Locale::Zh => format!("无法从返回 `{}` 的函数返回类型 `{}`", declared_type, expr_type),
        Locale::Tc => format!("無法從返回 `{}` 的函數返回類型 `{}`", declared_type, expr_type),
    }
}

pub fn return_value_in_nothing() -> String {
    match locale() {
        Locale::En => "Cannot return a value from a function that returns `nothing`".to_string(),
        Locale::Ru => "Невозможно вернуть значение из функции, возвращающей `nothing`".to_string(),
        Locale::Uk => "Неможливо повернути значення з функції, що повертає `nothing`".to_string(),
        Locale::Zh => "无法从返回 `nothing` 的函数返回值".to_string(),
        Locale::Tc => "無法從返回 `nothing` 的函數返回值".to_string(),
    }
}

pub fn return_missing_value(declared_type: &str) -> String {
    match locale() {
        Locale::En => format!("Function returns `{}`, but `return` has no value", declared_type),
        Locale::Ru => format!("Функция возвращает `{}`, но `return` не содержит значения", declared_type),
        Locale::Uk => format!("Функція повертає `{}`, але `return` не містить значення", declared_type),
        Locale::Zh => format!("函数返回 `{}`, 但 `return` 没有值", declared_type),
        Locale::Tc => format!("函數返回 `{}`, 但 `return` 沒有值", declared_type),
    }
}

pub fn operator_binary_error(op: &str, left: &str, right: &str) -> String {
    match locale() {
        Locale::En => format!("Operator `{}` cannot be applied to `{}` and `{}`", op, left, right),
        Locale::Ru => format!("Оператор `{}` не применим к `{}` и `{}`", op, left, right),
        Locale::Uk => format!("Оператор `{}` не застосовний до `{}` та `{}`", op, left, right),
        Locale::Zh => format!("运算符 `{}` 不能用于 `{}` 和 `{}`", op, left, right),
        Locale::Tc => format!("運算子 `{}` 不能用於 `{}` 和 `{}`", op, left, right),
    }
}

pub fn operator_unary_error(op: &str, operand: &str) -> String {
    match locale() {
        Locale::En => format!("Operator `{}` cannot be applied to `{}`", op, operand),
        Locale::Ru => format!("Оператор `{}` не применим к `{}`", op, operand),
        Locale::Uk => format!("Оператор `{}` не застосовний до `{}`", op, operand),
        Locale::Zh => format!("运算符 `{}` 不能用于 `{}`", op, operand),
        Locale::Tc => format!("運算子 `{}` 不能用於 `{}`", op, operand),
    }
}

// ─── Handle leak diagnostics ────────────────────────────────────────────────

pub fn handle_leak_function_end(name: &str, type_name: &str) -> String {
    match locale() {
        Locale::En => format!(
            "Handle leak: local `{}` (`{}`) is not set to `null` before function end",
            name, type_name
        ),
        Locale::Ru => format!(
            "Утечка хэндла: локальная `{}` (`{}`) не установлена в `null` перед концом функции",
            name, type_name
        ),
        Locale::Uk => format!(
            "Витік хендла: локальна `{}` (`{}`) не встановлена в `null` перед кінцем функції",
            name, type_name
        ),
        Locale::Zh => format!(
            "句柄泄漏：局部变量 `{}`（`{}`）在函数结束前未设置为 `null`",
            name, type_name
        ),
        Locale::Tc => format!(
            "句柄洩漏：局部變數 `{}`（`{}`）在函式結束前未設置為 `null`",
            name, type_name
        ),
    }
}

pub fn handle_leak_before_return(name: &str, type_name: &str) -> String {
    match locale() {
        Locale::En => format!(
            "Handle leak: local `{}` (`{}`) is not set to `null` before `return`",
            name, type_name
        ),
        Locale::Ru => format!(
            "Утечка хэндла: локальная `{}` (`{}`) не установлена в `null` перед `return`",
            name, type_name
        ),
        Locale::Uk => format!(
            "Витік хендла: локальна `{}` (`{}`) не встановлена в `null` перед `return`",
            name, type_name
        ),
        Locale::Zh => format!(
            "句柄泄漏：局部变量 `{}`（`{}`）在 `return` 前未设置为 `null`",
            name, type_name
        ),
        Locale::Tc => format!(
            "句柄洩漏：局部變數 `{}`（`{}`）在 `return` 前未設置為 `null`",
            name, type_name
        ),
    }
}

// ─── Handle leak quick fix ──────────────────────────────────────────────────

pub fn fix_handle_leak(name: &str) -> String {
    match locale() {
        Locale::En => format!("Set `{}` to `null`", name),
        Locale::Ru => format!("Установить `{}` в `null`", name),
        Locale::Uk => format!("Встановити `{}` в `null`", name),
        Locale::Zh => format!("将 `{}` 设置为 `null`", name),
        Locale::Tc => format!("將 `{}` 設置為 `null`", name),
    }
}

pub fn fix_all_handle_leaks() -> &'static str {
    pick(
        "Fix all handle leaks in file",
        "Исправить все утечки хэндлов в файле",
        "Виправити всі витоки хендлів у файлі",
        "修复文件中所有句柄泄漏",
        "修復檔案中所有句柄洩漏",
    )
}

// ─── Function diagnostics ───────────────────────────────────────────────────

pub fn unused_function(name: &str) -> String {
    match locale() {
        Locale::En => format!("Unused function `{}`", name),
        Locale::Ru => format!("Неиспользуемая функция `{}`", name),
        Locale::Uk => format!("Невикористана функція `{}`", name),
        Locale::Zh => format!("未使用的函数 `{}`", name),
        Locale::Tc => format!("未使用的函式 `{}`", name),
    }
}

pub fn remove_unused_function() -> &'static str {
    pick(
        "Remove unused function",
        "Удалить неиспользуемую функцию",
        "Видалити невикористану функцію",
        "删除未使用的函数",
        "刪除未使用的函式",
    )
}

pub fn remove_all_unused_functions() -> &'static str {
    pick(
        "Remove all unused functions in file",
        "Удалить все неиспользуемые функции в файле",
        "Видалити всі невикористані функції у файлі",
        "删除文件中所有未使用的函数",
        "刪除檔案中所有未使用的函式",
    )
}

pub fn array_in_return(name: &str) -> String {
    match locale() {
        Locale::En => format!("Cannot return array variable `{}`", name),
        Locale::Ru => format!("Нельзя вернуть переменную-массив `{}`", name),
        Locale::Uk => format!("Не можна повернути змінну-масив `{}`", name),
        Locale::Zh => format!("无法返回数组变量 `{}`", name),
        Locale::Tc => format!("無法返回陣列變數 `{}`", name),
    }
}

pub fn array_in_argument(name: &str) -> String {
    match locale() {
        Locale::En => format!("Cannot pass array variable `{}` as an argument", name),
        Locale::Ru => format!("Нельзя передать переменную-массив `{}` аргументом", name),
        Locale::Uk => format!("Не можна передати змінну-масив `{}` аргументом", name),
        Locale::Zh => format!("无法将数组变量 `{}` 作为参数传递", name),
        Locale::Tc => format!("無法將陣列變數 `{}` 作為參數傳遞", name),
    }
}

pub fn array_no_init(name: &str) -> String {
    match locale() {
        Locale::En => format!("Array `{}` cannot have an initializer", name),
        Locale::Ru => format!("Массив `{}` не может иметь инициализатор", name),
        Locale::Uk => format!("Масив `{}` не може мати ініціалізатор", name),
        Locale::Zh => format!("数组 `{}` 不能有初始值", name),
        Locale::Tc => format!("陣列 `{}` 不能有初始值", name),
    }
}

pub fn array_no_init_fix() -> &'static str {
    pick(
        "Remove initializer",
        "Удалить инициализатор",
        "Видалити ініціалізатор",
        "删除初始值",
        "刪除初始值",
    )
}

pub fn array_set_no_index(name: &str) -> String {
    match locale() {
        Locale::En => format!("Cannot assign to array `{}` without an index", name),
        Locale::Ru => format!("Нельзя присвоить массиву `{}` без индекса", name),
        Locale::Uk => format!("Не можна присвоїти масиву `{}` без індексу", name),
        Locale::Zh => format!("不能给数组 `{}` 赋值而不指定索引", name),
        Locale::Tc => format!("不能給陣列 `{}` 賦值而不指定索引", name),
    }
}

pub fn array_set_no_index_fix() -> &'static str {
    pick(
        "Add index []",
        "Добавить индекс []",
        "Додати індекс []",
        "添加索引 []",
        "新增索引 []",
    )
}

pub fn dead_code() -> &'static str {
    pick(
        "Unreachable code",
        "Недостижимый код",
        "Недосяжний код",
        "不可达代码",
        "不可達代碼",
    )
}

pub fn cyclic_call_chain(name: &str) -> String {
    match locale() {
        Locale::En => format!(
            "Function `{}` is part of a cyclic call chain — cannot be ordered",
            name
        ),
        Locale::Ru => format!(
            "Функция `{}` является частью циклической цепочки вызовов — невозможно упорядочить",
            name
        ),
        Locale::Uk => format!(
            "Функція `{}` є частиною циклічного ланцюга викликів — неможливо впорядкувати",
            name
        ),
        Locale::Zh => format!(
            "函数 `{}` 属于循环调用链 — 无法排序",
            name
        ),
        Locale::Tc => format!(
            "函式 `{}` 屬於循環呼叫鏈 — 無法排序",
            name
        ),
    }
}

pub fn inlinable_function(name: &str) -> String {
    match locale() {
        Locale::En => format!(
            "Function `{}` can be inlined — single return, called once",
            name
        ),
        Locale::Ru => format!(
            "Функция `{}` может быть заинлайнена — единственный return, единственный вызов",
            name
        ),
        Locale::Uk => format!(
            "Функція `{}` може бути заінлайнена — єдиний return, єдиний виклик",
            name
        ),
        Locale::Zh => format!(
            "函数 `{}` 可以内联 — 单一返回，仅调用一次",
            name
        ),
        Locale::Tc => format!(
            "函式 `{}` 可以內聯 — 單一返回，僅呼叫一次",
            name
        ),
    }
}

pub fn inline_function_action() -> &'static str {
    pick(
        "Inline function",
        "Заинлайнить функцию",
        "Заінлайнити функцію",
        "内联函数",
        "內聯函式",
    )
}

pub fn inline_all_functions_action() -> &'static str {
    pick(
        "Inline all single-call functions in file",
        "Заинлайнить все функции с единственным вызовом в файле",
        "Заінлайнити всі функції з єдиним викликом у файлі",
        "内联文件中所有单次调用函数",
        "內聯檔案中所有單次呼叫函式",
    )
}

// ─── Import directive diagnostics ───────────────────────────────────────────

pub fn missing_import_path() -> &'static str {
    pick(
        "Missing import path",
        "Отсутствует путь импорта",
        "Відсутній шлях імпорту",
        "缺少导入路径",
        "缺少匯入路徑",
    )
}

pub fn file_not_found(path: &str) -> String {
    match locale() {
        Locale::En => format!("File not found: {}", path),
        Locale::Ru => format!("Файл не найден: {}", path),
        Locale::Uk => format!("Файл не знайдено: {}", path),
        Locale::Zh => format!("文件未找到：{}", path),
        Locale::Tc => format!("檔案未找到：{}", path),
    }
}

pub fn cannot_resolve_import(path: &str) -> String {
    match locale() {
        Locale::En => format!("Cannot resolve import path: {}", path),
        Locale::Ru => format!("Невозможно разрешить путь импорта: {}", path),
        Locale::Uk => format!("Неможливо розв'язати шлях імпорту: {}", path),
        Locale::Zh => format!("无法解析导入路径：{}", path),
        Locale::Tc => format!("無法解析匯入路徑：{}", path),
    }
}

// ─── //set directive diagnostics ────────────────────────────────────────────

pub fn missing_setting_key() -> &'static str {
    pick(
        "Missing setting key",
        "Отсутствует ключ настройки",
        "Відсутній ключ налаштування",
        "缺少设置键",
        "缺少設定鍵",
    )
}

pub fn missing_setting_value(key: &str) -> String {
    match locale() {
        Locale::En => format!("Missing value for setting `{}`", key),
        Locale::Ru => format!("Отсутствует значение для настройки `{}`", key),
        Locale::Uk => format!("Відсутнє значення для налаштування `{}`", key),
        Locale::Zh => format!("缺少设置 `{}` 的值", key),
        Locale::Tc => format!("缺少設定 `{}` 的值", key),
    }
}

pub fn invalid_bool_value(value: &str, key: &str) -> String {
    match locale() {
        Locale::En => format!("Invalid value `{}` for `{}`: expected `0` or `1`", value, key),
        Locale::Ru => format!("Недопустимое значение `{}` для `{}`: ожидается `0` или `1`", value, key),
        Locale::Uk => format!("Недопустиме значення `{}` для `{}`: очікується `0` або `1`", value, key),
        Locale::Zh => format!("`{}` 的值 `{}` 无效：应为 `0` 或 `1`", key, value),
        Locale::Tc => format!("`{}` 的值 `{}` 無效：應為 `0` 或 `1`", key, value),
    }
}

// ─── //ignore directive diagnostics ─────────────────────────────────────────

pub fn missing_ignore_tag() -> &'static str {
    pick(
        "Missing ignore tag (e.g. `unused`, `leak`)",
        "Отсутствует тег игнорирования (напр. `unused`, `leak`)",
        "Відсутній тег ігнорування (напр. `unused`, `leak`)",
        "缺少忽略标签（例如 `unused`、`leak`）",
        "缺少忽略標籤（例如 `unused`、`leak`）",
    )
}

// ─── UjAPI diagnostics ──────────────────────────────────────────────────────

pub fn ujapi_missing_path() -> &'static str {
    pick(
        "Missing destination path for UjAPI import",
        "Отсутствует путь назначения для импорта UjAPI",
        "Відсутній шлях призначення для імпорту UjAPI",
        "缺少 UjAPI 导入的目标路径",
        "缺少 UjAPI 匯入的目標路徑",
    )
}

pub fn ujapi_file_not_found(path: &str) -> String {
    match locale() {
        Locale::En => format!("UjAPI file not found: `{}`", path),
        Locale::Ru => format!("Файл UjAPI не найден: `{}`", path),
        Locale::Uk => format!("Файл UjAPI не знайдено: `{}`", path),
        Locale::Zh => format!("UjAPI 文件未找到：`{}`", path),
        Locale::Tc => format!("UjAPI 檔案未找到：`{}`", path),
    }
}

pub fn ujapi_no_version_tag() -> &'static str {
    pick(
        "UjAPI file has no version tag",
        "Файл UjAPI не содержит тег версии",
        "Файл UjAPI не містить тег версії",
        "UjAPI 文件没有版本标签",
        "UjAPI 檔案沒有版本標籤",
    )
}

pub fn ujapi_outdated(local_tag: &str, latest_tag: &str) -> String {
    match locale() {
        Locale::En => format!("UjAPI outdated: local `{}`, latest `{}`", local_tag, latest_tag),
        Locale::Ru => format!("UjAPI устарел: локальный `{}`, последний `{}`", local_tag, latest_tag),
        Locale::Uk => format!("UjAPI застарів: локальний `{}`, останній `{}`", local_tag, latest_tag),
        Locale::Zh => format!("UjAPI 已过期：本地 `{}`，最新 `{}`", local_tag, latest_tag),
        Locale::Tc => format!("UjAPI 已過期：本地 `{}`，最新 `{}`", local_tag, latest_tag),
    }
}

pub fn ujapi_cannot_resolve(path: &str) -> String {
    match locale() {
        Locale::En => format!("Cannot resolve UjAPI path: {}", path),
        Locale::Ru => format!("Невозможно разрешить путь UjAPI: {}", path),
        Locale::Uk => format!("Неможливо розв'язати шлях UjAPI: {}", path),
        Locale::Zh => format!("无法解析 UjAPI 路径：{}", path),
        Locale::Tc => format!("無法解析 UjAPI 路徑：{}", path),
    }
}

// UjAPI tooltips

pub fn ujapi_tooltip_up_to_date(tag: &str) -> String {
    match locale() {
        Locale::En => format!("UjAPI {} ✓ (up to date)", tag),
        Locale::Ru => format!("UjAPI {} ✓ (актуально)", tag),
        Locale::Uk => format!("UjAPI {} ✓ (актуально)", tag),
        Locale::Zh => format!("UjAPI {} ✓（最新）", tag),
        Locale::Tc => format!("UjAPI {} ✓（最新）", tag),
    }
}

pub fn ujapi_tooltip_update_available(local_tag: &str, latest_tag: &str) -> String {
    match locale() {
        Locale::En => format!("UjAPI {} → {} available", local_tag, latest_tag),
        Locale::Ru => format!("UjAPI {} → {} доступно", local_tag, latest_tag),
        Locale::Uk => format!("UjAPI {} → {} доступно", local_tag, latest_tag),
        Locale::Zh => format!("UjAPI {} → {} 可用", local_tag, latest_tag),
        Locale::Tc => format!("UjAPI {} → {} 可用", local_tag, latest_tag),
    }
}

pub fn ujapi_tooltip_no_tag() -> &'static str {
    pick(
        "UjAPI (no version tag)",
        "UjAPI (без тега версии)",
        "UjAPI (без тегу версії)",
        "UjAPI（无版本标签）",
        "UjAPI（無版本標籤）",
    )
}

// UjAPI code actions

pub fn ujapi_download() -> &'static str {
    pick(
        "⬇ Download UjAPI",
        "⬇ Скачать UjAPI",
        "⬇ Завантажити UjAPI",
        "⬇ 下载 UjAPI",
        "⬇ 下載 UjAPI",
    )
}

pub fn ujapi_update() -> &'static str {
    pick(
        "⬇ Update UjAPI",
        "⬇ Обновить UjAPI",
        "⬇ Оновити UjAPI",
        "⬇ 更新 UjAPI",
        "⬇ 更新 UjAPI",
    )
}

pub fn ujapi_downloaded(tag: &str, dest: &str) -> String {
    match locale() {
        Locale::En => format!("Downloaded UjAPI {} to {}", tag, dest),
        Locale::Ru => format!("UjAPI {} загружен в {}", tag, dest),
        Locale::Uk => format!("UjAPI {} завантажено до {}", tag, dest),
        Locale::Zh => format!("已下载 UjAPI {} 到 {}", tag, dest),
        Locale::Tc => format!("已下載 UjAPI {} 到 {}", tag, dest),
    }
}

pub fn ujapi_cannot_resolve_download_path(path: &str) -> String {
    match locale() {
        Locale::En => format!("Cannot resolve path: {}", path),
        Locale::Ru => format!("Невозможно разрешить путь: {}", path),
        Locale::Uk => format!("Неможливо розв'язати шлях: {}", path),
        Locale::Zh => format!("无法解析路径：{}", path),
        Locale::Tc => format!("無法解析路徑：{}", path),
    }
}

pub fn ujapi_download_failed(err: &str) -> String {
    match locale() {
        Locale::En => format!("Download failed: {}", err),
        Locale::Ru => format!("Ошибка загрузки: {}", err),
        Locale::Uk => format!("Помилка завантаження: {}", err),
        Locale::Zh => format!("下载失败：{}", err),
        Locale::Tc => format!("下載失敗：{}", err),
    }
}

// UjAPI completion

pub fn ujapi_completion_detail() -> &'static str {
    pick(
        "Download & import UjAPI common.j",
        "Скачать и импортировать UjAPI common.j",
        "Завантажити та імпортувати UjAPI common.j",
        "下载并导入 UjAPI common.j",
        "下載並匯入 UjAPI common.j",
    )
}

// ─── Completion item details ────────────────────────────────────────────────

pub fn completion_import_file() -> &'static str {
    pick(
        "Import a file",
        "Импортировать файл",
        "Імпортувати файл",
        "导入文件",
        "匯入檔案",
    )
}

pub fn completion_import_frozen() -> &'static str {
    pick(
        "Import a frozen (read-only) file",
        "Импортировать замороженный (только для чтения) файл",
        "Імпортувати заморожений (лише для читання) файл",
        "导入冻结（只读）文件",
        "匯入凍結（唯讀）檔案",
    )
}

pub fn completion_set_config() -> &'static str {
    pick(
        "Set a file-local configuration value",
        "Установить локальное значение конфигурации",
        "Встановити локальне значення конфігурації",
        "设置文件本地配置值",
        "設定檔案本地設定值",
    )
}

pub fn completion_suppress_file() -> &'static str {
    pick(
        "Suppress diagnostics for the entire file",
        "Подавить диагностику для всего файла",
        "Придушити діагностику для всього файлу",
        "抑制整个文件的诊断",
        "抑制整個檔案的診斷",
    )
}

pub fn completion_suppress_decl() -> &'static str {
    pick(
        "Suppress diagnostics for the next declaration",
        "Подавить диагностику для следующего объявления",
        "Придушити діагностику для наступного оголошення",
        "抑制下一个声明的诊断",
        "抑制下一個宣告的診斷",
    )
}

pub fn completion_entry_point() -> &'static str {
    pick(
        "Mark file as a build entry point",
        "Отметить файл как точку входа сборки",
        "Позначити файл як точку входу збірки",
        "标记文件为构建入口点",
        "標記檔案為建構入口點",
    )
}

pub fn completion_enable() -> &'static str {
    pick("Enable", "Включить", "Увімкнути", "启用", "啟用")
}

pub fn completion_disable() -> &'static str {
    pick("Disable", "Выключить", "Вимкнути", "禁用", "停用")
}

pub fn completion_function_snippet() -> &'static str {
    pick(
        "function … endfunction",
        "function … endfunction",
        "function … endfunction",
        "function … endfunction",
        "function … endfunction",
    )
}

// ─── SetDef details (localized) ─────────────────────────────────────────────

pub fn set_def_ref_tip() -> &'static str {
    pick(
        "Show / hide reference-ID inlay hints (debug)",
        "Показать / скрыть подсказки ID ссылок (отладка)",
        "Показати / сховати підказки ID посилань (зневадження)",
        "显示/隐藏引用ID内嵌提示（调试）",
        "顯示/隱藏參考ID內嵌提示（除錯）",
    )
}

pub fn set_def_type_tip() -> &'static str {
    pick(
        "Show / hide type-annotation inlay hints",
        "Показать / скрыть подсказки аннотаций типов",
        "Показати / сховати підказки анотацій типів",
        "显示/隐藏类型注释内嵌提示",
        "顯示/隱藏類型註解內嵌提示",
    )
}

pub fn set_def_build_jass() -> &'static str {
    pick(
        "Output path for the JASS build",
        "Путь вывода для сборки JASS",
        "Шлях виводу для збірки JASS",
        "JASS 构建输出路径",
        "JASS 建構輸出路徑",
    )
}

pub fn set_def_build_as() -> &'static str {
    pick(
        "Output path for the AngelScript build",
        "Путь вывода для сборки AngelScript",
        "Шлях виводу для збірки AngelScript",
        "AngelScript 构建输出路径",
        "AngelScript 建構輸出路徑",
    )
}

pub fn set_def_backup() -> &'static str {
    pick(
        "Backup path for map archive before injecting script",
        "Путь для резервной копии карты перед записью скрипта",
        "Шлях для резервної копії карти перед записом скрипта",
        "注入脚本前地图存档的备份路径",
        "注入腳本前地圖存檔的備份路徑",
    )
}

pub fn set_def_build_uglify() -> &'static str {
    pick(
        "Minify identifiers in build output",
        "Минифицировать идентификаторы в сборке",
        "Мініфікувати ідентифікатори у збірці",
        "在构建输出中压缩标识符",
        "在建構輸出中壓縮標識符",
    )
}

pub fn set_def_build_before() -> &'static str {
    pick(
        "Shell command to run before the build",
        "Команда терминала, выполняемая перед сборкой",
        "Команда терміналу, що виконується перед збіркою",
        "构建前执行的终端命令",
        "建構前執行的終端命令",
    )
}

pub fn set_def_build_after() -> &'static str {
    pick(
        "Shell command to run after the build",
        "Команда терминала, выполняемая после сборки",
        "Команда терміналу, що виконується після збірки",
        "构建后执行的终端命令",
        "建構後執行的終端命令",
    )
}

/// Get the localized detail for a `SetDef` by key.
pub fn set_def_detail(key: &str) -> &'static str {
    match key {
        "ref-tip" => set_def_ref_tip(),
        "type-tip" => set_def_type_tip(),
        "build-jass" => set_def_build_jass(),
        "build-as" => set_def_build_as(),
        "backup" => set_def_backup(),
        "build-uglify" => set_def_build_uglify(),
        "build-before" => set_def_build_before(),
        "build-after" => set_def_build_after(),
        _ => "",
    }
}

// ─── Template variable details (localized) ──────────────────────────────────

pub fn template_var_entry() -> &'static str {
    pick(
        "Full normalized path to the `//entry` file",
        "Полный нормализованный путь к файлу `//entry`",
        "Повний нормалізований шлях до файлу `//entry`",
        "`//entry` 文件的完整规范化路径",
        "`//entry` 檔案的完整正規化路徑",
    )
}

pub fn template_var_entry_dir() -> &'static str {
    pick(
        "Full normalized path to the directory containing the `//entry` file",
        "Полный нормализованный путь к каталогу с файлом `//entry`",
        "Повний нормалізований шлях до каталогу з файлом `//entry`",
        "包含 `//entry` 文件的目录的完整规范化路径",
        "包含 `//entry` 檔案的目錄的完整正規化路徑",
    )
}

pub fn template_var_target_jass() -> &'static str {
    pick(
        "Full normalized path to the JASS build output file (from `//set build-jass`)",
        "Полный нормализованный путь к файлу сборки JASS (из `//set build-jass`)",
        "Повний нормалізований шлях до файлу збірки JASS (з `//set build-jass`)",
        "JASS 构建输出文件的完整规范化路径（来自 `//set build-jass`）",
        "JASS 建構輸出檔案的完整正規化路徑（來自 `//set build-jass`）",
    )
}

pub fn template_var_target_as() -> &'static str {
    pick(
        "Full normalized path to the AngelScript build output file (from `//set build-as`)",
        "Полный нормализованный путь к файлу сборки AngelScript (из `//set build-as`)",
        "Повний нормалізований шлях до файлу збірки AngelScript (з `//set build-as`)",
        "AngelScript 构建输出文件的完整规范化路径（来自 `//set build-as`）",
        "AngelScript 建構輸出檔案的完整正規化路徑（來自 `//set build-as`）",
    )
}

/// Get the localized detail for a `TemplateVar` by name.
pub fn template_var_detail(name: &str) -> &'static str {
    match name {
        "entry" => template_var_entry(),
        "entry-dir" => template_var_entry_dir(),
        "target-jass" => template_var_target_jass(),
        "target-as" => template_var_target_as(),
        _ => "",
    }
}

pub fn unknown_template_var(name: &str) -> String {
    match locale() {
        Locale::En => format!("Unknown template variable `{{{{{}}}}}`. Known: `{{{{entry}}}}`, `{{{{entry-dir}}}}`, `{{{{target-jass}}}}`, `{{{{target-as}}}}`.", name),
        Locale::Ru => format!("Неизвестная переменная шаблона `{{{{{}}}}}`. Доступные: `{{{{entry}}}}`, `{{{{entry-dir}}}}`, `{{{{target-jass}}}}`, `{{{{target-as}}}}`.", name),
        Locale::Uk => format!("Невідома змінна шаблону `{{{{{}}}}}`. Доступні: `{{{{entry}}}}`, `{{{{entry-dir}}}}`, `{{{{target-jass}}}}`, `{{{{target-as}}}}`.", name),
        Locale::Zh => format!("未知模板变量 `{{{{{}}}}}`. 可用：`{{{{entry}}}}`, `{{{{entry-dir}}}}`, `{{{{target-jass}}}}`, `{{{{target-as}}}}`.", name),
        Locale::Tc => format!("未知模板變數 `{{{{{}}}}}`. 可用：`{{{{entry}}}}`, `{{{{entry-dir}}}}`, `{{{{target-jass}}}}`, `{{{{target-as}}}}`.", name),
    }
}

// ─── IgnoreTagDef details (localized) ───────────────────────────────────────

pub fn ignore_tag_unused() -> &'static str {
    pick(
        "Suppress unused-function diagnostic",
        "Подавить диагностику неиспользуемых функций",
        "Придушити діагностику невикористаних функцій",
        "抑制未使用函数诊断",
        "抑制未使用函式診斷",
    )
}

pub fn ignore_tag_leak() -> &'static str {
    pick(
        "Suppress handle-leak diagnostic",
        "Подавить диагностику утечек хэндлов",
        "Придушити діагностику витоків хендлів",
        "抑制句柄泄漏诊断",
        "抑制句柄洩漏診斷",
    )
}

pub fn ignore_tag_cycle() -> &'static str {
    pick(
        "Suppress cyclic-call-chain diagnostic",
        "Подавить диагностику циклических вызовов",
        "Придушити діагностику циклічних викликів",
        "抑制循环调用链诊断",
        "抑制循環呼叫鏈診斷",
    )
}

/// Get the localized detail for an `IgnoreTagDef` by tag name.
pub fn ignore_tag_detail(tag: &str) -> &'static str {
    match tag {
        "unused" => ignore_tag_unused(),
        "leak" => ignore_tag_leak(),
        "cycle" => ignore_tag_cycle(),
        _ => "",
    }
}

// ─── Build messages ─────────────────────────────────────────────────────────

pub fn build_requires_entry(key: &str) -> String {
    match locale() {
        Locale::En => format!("`//set {}` requires `//entry` directive in this file.", key),
        Locale::Ru => format!("`//set {}` требует директиву `//entry` в этом файле.", key),
        Locale::Uk => format!("`//set {}` потребує директиву `//entry` у цьому файлі.", key),
        Locale::Zh => format!("`//set {}` 需要在此文件中使用 `//entry` 指令。", key),
        Locale::Tc => format!("`//set {}` 需要在此檔案中使用 `//entry` 指令。", key),
    }
}

pub fn build_no_setting_jass() -> &'static str {
    pick(
        "No `//set build-jass <path>` directive found in the import tree.",
        "Директива `//set build-jass <path>` не найдена в дереве импортов.",
        "Директиву `//set build-jass <path>` не знайдено в дереві імпортів.",
        "在导入树中未找到 `//set build-jass <path>` 指令。",
        "在匯入樹中未找到 `//set build-jass <path>` 指令。",
    )
}

pub fn build_no_setting_as() -> &'static str {
    pick(
        "No `//set build-as <path>` directive found in the import tree.",
        "Директива `//set build-as <path>` не найдена в дереве импортов.",
        "Директиву `//set build-as <path>` не знайдено в дереві імпортів.",
        "在导入树中未找到 `//set build-as <path>` 指令。",
        "在匯入樹中未找到 `//set build-as <path>` 指令。",
    )
}

pub fn build_no_parent_dir() -> &'static str {
    pick(
        "Cannot determine parent directory.",
        "Невозможно определить родительский каталог.",
        "Неможливо визначити батьківський каталог.",
        "无法确定父目录。",
        "無法確定父目錄。",
    )
}

pub fn build_not_file_path() -> &'static str {
    pick(
        "URI is not a file path.",
        "URI не является путём к файлу.",
        "URI не є шляхом до файлу.",
        "URI 不是文件路径。",
        "URI 不是檔案路徑。",
    )
}

pub fn build_ok(globals: usize, functions: usize, bare_stmts: usize) -> String {
    let stmts_part = if bare_stmts > 0 {
        match locale() {
            Locale::En => format!(", {} statements → main", bare_stmts),
            Locale::Ru => format!(", {} инструкций → main", bare_stmts),
            Locale::Uk => format!(", {} інструкцій → main", bare_stmts),
            Locale::Zh => format!("，{} 条语句 → main", bare_stmts),
            Locale::Tc => format!("，{} 條語句 → main", bare_stmts),
        }
    } else {
        String::new()
    };
    match locale() {
        Locale::En => format!("Build OK — {} globals, {} functions{}", globals, functions, stmts_part),
        Locale::Ru => format!("Сборка ОК — {} глобальных, {} функций{}", globals, functions, stmts_part),
        Locale::Uk => format!("Збірка ОК — {} глобальних, {} функцій{}", globals, functions, stmts_part),
        Locale::Zh => format!("构建成功 — {} 个全局变量，{} 个函数{}", globals, functions, stmts_part),
        Locale::Tc => format!("建構成功 — {} 個全域變數，{} 個函式{}", globals, functions, stmts_part),
    }
}

pub fn build_write_failed(path: &str, err: &str) -> String {
    match locale() {
        Locale::En => format!("Failed to write {}: {}", path, err),
        Locale::Ru => format!("Ошибка записи {}: {}", path, err),
        Locale::Uk => format!("Помилка запису {}: {}", path, err),
        Locale::Zh => format!("写入 {} 失败：{}", path, err),
        Locale::Tc => format!("寫入 {} 失敗：{}", path, err),
    }
}

#[allow(dead_code)]
pub fn build_hook_failed(phase: &str, cmd: &str, detail: &str) -> String {
    match locale() {
        Locale::En => format!("`build-{}` command failed: `{}` — {}", phase, cmd, detail),
        Locale::Ru => format!("Команда `build-{}` завершилась ошибкой: `{}` — {}", phase, cmd, detail),
        Locale::Uk => format!("Команда `build-{}` завершилася помилкою: `{}` — {}", phase, cmd, detail),
        Locale::Zh => format!("`build-{}` 命令失败：`{}` — {}", phase, cmd, detail),
        Locale::Tc => format!("`build-{}` 命令失敗：`{}` — {}", phase, cmd, detail),
    }
}

#[allow(dead_code)]
pub fn build_hook_spawn_failed(phase: &str, cmd: &str, err: &str) -> String {
    match locale() {
        Locale::En => format!("Failed to start `build-{}` command: `{}` — {}", phase, cmd, err),
        Locale::Ru => format!("Не удалось запустить команду `build-{}`: `{}` — {}", phase, cmd, err),
        Locale::Uk => format!("Не вдалося запустити команду `build-{}`: `{}` — {}", phase, cmd, err),
        Locale::Zh => format!("无法启动 `build-{}` 命令：`{}` — {}", phase, cmd, err),
        Locale::Tc => format!("無法啟動 `build-{}` 命令：`{}` — {}", phase, cmd, err),
    }
}

pub fn build_archive_ok(globals: usize, functions: usize, bare_stmts: usize, script_name: &str) -> String {
    let stmts_part = if bare_stmts > 0 {
        match locale() {
            Locale::En => format!(", {} statements → main", bare_stmts),
            Locale::Ru => format!(", {} инструкций → main", bare_stmts),
            Locale::Uk => format!(", {} інструкцій → main", bare_stmts),
            Locale::Zh => format!("，{} 条语句 → main", bare_stmts),
            Locale::Tc => format!("，{} 條語句 → main", bare_stmts),
        }
    } else {
        String::new()
    };
    match locale() {
        Locale::En => format!("Build OK → {} — {} globals, {} functions{}", script_name, globals, functions, stmts_part),
        Locale::Ru => format!("Сборка ОК → {} — {} глобальных, {} функций{}", script_name, globals, functions, stmts_part),
        Locale::Uk => format!("Збірка ОК → {} — {} глобальних, {} функцій{}", script_name, globals, functions, stmts_part),
        Locale::Zh => format!("构建成功 → {} — {} 个全局变量，{} 个函数{}", script_name, globals, functions, stmts_part),
        Locale::Tc => format!("建構成功 → {} — {} 個全域變數，{} 個函式{}", script_name, globals, functions, stmts_part),
    }
}

pub fn build_backup_failed(path: &str, err: &str) -> String {
    match locale() {
        Locale::En => format!("Failed to create backup {}: {}", path, err),
        Locale::Ru => format!("Ошибка создания резервной копии {}: {}", path, err),
        Locale::Uk => format!("Помилка створення резервної копії {}: {}", path, err),
        Locale::Zh => format!("创建备份 {} 失败：{}", path, err),
        Locale::Tc => format!("建立備份 {} 失敗：{}", path, err),
    }
}

pub fn build_archive_open_failed(path: &str, err: &str) -> String {
    match locale() {
        Locale::En => format!("Failed to open archive {}: {}", path, err),
        Locale::Ru => format!("Ошибка открытия архива {}: {}", path, err),
        Locale::Uk => format!("Помилка відкриття архіву {}: {}", path, err),
        Locale::Zh => format!("打开存档 {} 失败：{}", path, err),
        Locale::Tc => format!("開啟存檔 {} 失敗：{}", path, err),
    }
}

pub fn build_archive_inject_failed(script_name: &str, err: &str) -> String {
    match locale() {
        Locale::En => format!("Failed to inject {} into archive: {}", script_name, err),
        Locale::Ru => format!("Ошибка записи {} в архив: {}", script_name, err),
        Locale::Uk => format!("Помилка запису {} в архів: {}", script_name, err),
        Locale::Zh => format!("向存档注入 {} 失败：{}", script_name, err),
        Locale::Tc => format!("向存檔注入 {} 失敗：{}", script_name, err),
    }
}

// ─── Hover: //import-ujapi! ─────────────────────────────────────────────────

pub fn ujapi_hover_latest_release(tag: &str, html_url: &str, name: &str) -> String {
    match locale() {
        Locale::En => format!("**Latest release:** [`{}`]({}) — {}", tag, html_url, name),
        Locale::Ru => format!("**Последний релиз:** [`{}`]({}) — {}", tag, html_url, name),
        Locale::Uk => format!("**Останній реліз:** [`{}`]({}) — {}", tag, html_url, name),
        Locale::Zh => format!("**最新版本：** [`{}`]({}) — {}", tag, html_url, name),
        Locale::Tc => format!("**最新版本：** [`{}`]({}) — {}", tag, html_url, name),
    }
}

pub fn ujapi_hover_fetching() -> &'static str {
    pick(
        "*Fetching latest release info…*",
        "*Получение информации о последнем релизе…*",
        "*Отримання інформації про останній реліз…*",
        "*正在获取最新版本信息…*",
        "*正在取得最新版本資訊…*",
    )
}

pub fn ujapi_hover_body(version_line: &str) -> String {
    match locale() {
        Locale::En => format!(
            "### `//import-ujapi!`\n\n\
             Download `uJAPIFiles/common.j` from the latest \
             [UjAPI](https://github.com/UnryzeC/UjAPI) GitHub release \
             and treat it as a frozen import.\n\n\
             {version_line}\n\n\
             The first line of the downloaded file contains `//<tag>` for version tracking.\n\n\
             Use code action (**Alt+Enter**) to download / re-download."
        ),
        Locale::Ru => format!(
            "### `//import-ujapi!`\n\n\
             Скачать `uJAPIFiles/common.j` из последнего релиза \
             [UjAPI](https://github.com/UnryzeC/UjAPI) на GitHub \
             и обработать как замороженный импорт.\n\n\
             {version_line}\n\n\
             Первая строка загруженного файла содержит `//<тег>` для отслеживания версии.\n\n\
             Используйте действие кода (**Alt+Enter**) для загрузки / повторной загрузки."
        ),
        Locale::Uk => format!(
            "### `//import-ujapi!`\n\n\
             Завантажити `uJAPIFiles/common.j` з останнього релізу \
             [UjAPI](https://github.com/UnryzeC/UjAPI) на GitHub \
             та обробити як заморожений імпорт.\n\n\
             {version_line}\n\n\
             Перший рядок завантаженого файлу містить `//<тег>` для відстеження версії.\n\n\
             Використовуйте дію коду (**Alt+Enter**) для завантаження / повторного завантаження."
        ),
        Locale::Zh => format!(
            "### `//import-ujapi!`\n\n\
             从最新的 [UjAPI](https://github.com/UnryzeC/UjAPI) GitHub 版本下载 \
             `uJAPIFiles/common.j` 并作为冻结导入处理。\n\n\
             {version_line}\n\n\
             下载文件的第一行包含 `//<标签>` 用于版本跟踪。\n\n\
             使用代码操作（**Alt+Enter**）下载/重新下载。"
        ),
        Locale::Tc => format!(
            "### `//import-ujapi!`\n\n\
             從最新的 [UjAPI](https://github.com/UnryzeC/UjAPI) GitHub 版本下載 \
             `uJAPIFiles/common.j` 並作為凍結匯入處理。\n\n\
             {version_line}\n\n\
             下載檔案的第一行包含 `//<標籤>` 用於版本追蹤。\n\n\
             使用程式碼動作（**Alt+Enter**）下載/重新下載。"
        ),
    }
}

// ─── String format conversion code actions ──────────────────────────────────

pub fn convert_to_triple_quoted() -> &'static str {
    pick(
        "Convert to triple-quoted string \"\"\"…\"\"\"",
        "Преобразовать в строку с тройными кавычками \"\"\"…\"\"\"",
        "Перетворити на рядок з потрійними лапками \"\"\"…\"\"\"",
        "转换为三引号字符串 \"\"\"…\"\"\"",
        "轉換為三引號字串 \"\"\"…\"\"\"",
    )
}

pub fn convert_to_single_quoted() -> &'static str {
    pick(
        "Convert to regular string \"…\"",
        "Преобразовать в обычную строку \"…\"",
        "Перетворити на звичайний рядок \"…\"",
        "转换为普通字符串 \"…\"",
        "轉換為普通字串 \"…\"",
    )
}

pub fn convert_all_to_triple_quoted() -> &'static str {
    pick(
        "Convert all strings in file to \"\"\"…\"\"\"",
        "Преобразовать все строки в файле в \"\"\"…\"\"\"",
        "Перетворити всі рядки у файлі на \"\"\"…\"\"\"",
        "将文件中所有字符串转换为 \"\"\"…\"\"\"",
        "將檔案中所有字串轉換為 \"\"\"…\"\"\"",
    )
}

pub fn convert_all_to_single_quoted() -> &'static str {
    pick(
        "Convert all strings in file to \"…\"",
        "Преобразовать все строки в файле в \"…\"",
        "Перетворити всі рядки у файлі на \"…\"",
        "将文件中所有字符串转换为 \"…\"",
        "將檔案中所有字串轉換為 \"…\"",
    )
}

// ─── Simplify if-return ──────────────────────────────────────────────────────

pub fn simplify_if_return() -> &'static str {
    pick(
        "Simplify: replace if-return with a single return",
        "Упрощение: заменить if-return одним return",
        "Спрощення: замінити if-return одним return",
        "简化：将 if-return 替换为单个 return",
        "簡化：將 if-return 替換為單個 return",
    )
}

pub fn simplify_if_return_action() -> &'static str {
    pick(
        "Simplify to single return",
        "Упростить до одного return",
        "Спростити до одного return",
        "简化为单个 return",
        "簡化為單個 return",
    )
}

pub fn simplify_all_if_return_action() -> &'static str {
    pick(
        "Simplify all if-returns in file",
        "Упростить все if-return в файле",
        "Спростити всі if-return у файлі",
        "简化文件中所有 if-return",
        "簡化檔案中所有 if-return",
    )
}

// ─── Redundant parentheses ───────────────────────────────────────────────────

pub fn redundant_parens() -> &'static str {
    pick(
        "Redundant parentheses",
        "Лишние скобки",
        "Зайві дужки",
        "多余的括号",
        "多餘的括號",
    )
}

pub fn remove_redundant_parens() -> &'static str {
    pick(
        "Remove redundant parentheses",
        "Убрать лишние скобки",
        "Прибрати зайві дужки",
        "删除多余括号",
        "刪除多餘括號",
    )
}

pub fn remove_all_redundant_parens() -> &'static str {
    pick(
        "Remove all redundant parentheses in file",
        "Убрать все лишние скобки в файле",
        "Прибрати всі зайві дужки у файлі",
        "删除文件中所有多余括号",
        "刪除檔案中所有多餘括號",
    )
}

// ─── Redundant boolean comparison ────────────────────────────────────────────

pub fn redundant_bool_cmp() -> &'static str {
    pick(
        "Redundant boolean comparison",
        "Лишнее сравнение с булевым значением",
        "Зайве порівняння з булевим значенням",
        "多余的布尔比较",
        "多餘的布林比較",
    )
}

pub fn simplify_bool_cmp() -> &'static str {
    pick(
        "Simplify boolean comparison",
        "Упростить сравнение с булевым значением",
        "Спростити порівняння з булевим значенням",
        "简化布尔比较",
        "簡化布林比較",
    )
}

pub fn simplify_all_bool_cmp() -> &'static str {
    pick(
        "Simplify all boolean comparisons in file",
        "Упростить все сравнения с булевым значением в файле",
        "Спростити всі порівняння з булевим значенням у файлі",
        "简化文件中所有布尔比较",
        "簡化檔案中所有布林比較",
    )
}

// ─── Collapse and-chain ──────────────────────────────────────────────────────

pub fn collapse_and_chain() -> &'static str {
    pick(
        "Simplify: collapse if-not-return-false chain into a single return with `and`",
        "Упрощение: свернуть цепочку if-not-return-false в один return с `and`",
        "Спрощення: згорнути ланцюжок if-not-return-false в один return з `and`",
        "简化：将 if-not-return-false 链折叠为带 `and` 的单个 return",
        "簡化：將 if-not-return-false 鏈折疊為帶 `and` 的單個 return",
    )
}

pub fn collapse_and_chain_action() -> &'static str {
    pick(
        "Collapse into single return with `and`",
        "Свернуть в один return с `and`",
        "Згорнути в один return з `and`",
        "折叠为带 `and` 的单个 return",
        "折疊為帶 `and` 的單個 return",
    )
}

pub fn collapse_all_and_chains_action() -> &'static str {
    pick(
        "Collapse all and-chains in file",
        "Свернуть все and-цепочки в файле",
        "Згорнути всі and-ланцюжки у файлі",
        "折叠文件中所有 and 链",
        "折疊檔案中所有 and 鏈",
    )
}

// ─── Collapse or-chain ───────────────────────────────────────────────────────

pub fn collapse_or_chain() -> &'static str {
    pick(
        "Simplify: collapse if-return-true chain into a single return with `or`",
        "Упрощение: свернуть цепочку if-return-true в один return с `or`",
        "Спрощення: згорнути ланцюжок if-return-true в один return з `or`",
        "简化：将 if-return-true 链折叠为带 `or` 的单个 return",
        "簡化：將 if-return-true 鏈折疊為帶 `or` 的單個 return",
    )
}

pub fn collapse_or_chain_action() -> &'static str {
    pick(
        "Collapse into single return with `or`",
        "Свернуть в один return с `or`",
        "Згорнути в один return з `or`",
        "折叠为带 `or` 的单个 return",
        "折疊為帶 `or` 的單個 return",
    )
}

pub fn collapse_all_or_chains_action() -> &'static str {
    pick(
        "Collapse all or-chains in file",
        "Свернуть все or-цепочки в файле",
        "Згорнути всі or-ланцюжки у файлі",
        "折叠文件中所有 or 链",
        "折疊檔案中所有 or 鏈",
    )
}

// ─── Empty else ──────────────────────────────────────────────────────────────

pub fn empty_else() -> &'static str {
    pick(
        "Empty else block",
        "Пустой блок else",
        "Порожній блок else",
        "空的 else 块",
        "空的 else 區塊",
    )
}

pub fn remove_empty_else() -> &'static str {
    pick(
        "Remove empty else",
        "Удалить пустой else",
        "Видалити порожній else",
        "删除空的 else",
        "刪除空的 else",
    )
}

pub fn remove_all_empty_else() -> &'static str {
    pick(
        "Remove all empty else blocks in file",
        "Удалить все пустые else в файле",
        "Видалити всі порожні else у файлі",
        "删除文件中所有空的 else 块",
        "刪除檔案中所有空的 else 區塊",
    )
}

// ─── Remove else branch ─────────────────────────────────────────────────────

pub fn remove_else_branch() -> &'static str {
    pick(
        "Remove else branch",
        "Удалить ветку else",
        "Видалити гілку else",
        "删除 else 分支",
        "刪除 else 分支",
    )
}

// ─── Fold StringHash ─────────────────────────────────────────────────────────

pub fn fold_string_hash() -> &'static str {
    pick(
        "Compute StringHash",
        "Вычислить StringHash",
        "Обчислити StringHash",
        "计算 StringHash",
        "計算 StringHash",
    )
}

pub fn fold_string_hash_all() -> &'static str {
    pick(
        "Compute all StringHash in file",
        "Вычислить все StringHash в файле",
        "Обчислити всі StringHash у файлі",
        "计算文件中所有 StringHash",
        "計算檔案中所有 StringHash",
    )
}

// ─── ExecuteFunc ─────────────────────────────────────────────────────────────

pub fn execute_func_replace(name: &str) -> String {
    match locale() {
        Locale::En => format!("Replace with `call {name}()`"),
        Locale::Ru => format!("Заменить на `call {name}()`"),
        Locale::Uk => format!("Замінити на `call {name}()`"),
        Locale::Zh => format!("替换为 `call {name}()`"),
        Locale::Tc => format!("替換為 `call {name}()`"),
    }
}

pub fn execute_func_replace_all() -> &'static str {
    pick(
        "Replace all ExecuteFunc in file",
        "Заменить все ExecuteFunc в файле",
        "Замінити всі ExecuteFunc у файлі",
        "替换文件中所有 ExecuteFunc",
        "替換檔案中所有 ExecuteFunc",
    )
}

pub fn execute_func_hint(name: &str) -> String {
    match locale() {
        Locale::En => format!("Use direct call `{name}()` instead of `ExecuteFunc`"),
        Locale::Ru => format!("Используйте прямой вызов `{name}()` вместо `ExecuteFunc`"),
        Locale::Uk => format!("Використовуйте прямий виклик `{name}()` замість `ExecuteFunc`"),
        Locale::Zh => format!("使用直接调用 `{name}()` 代替 `ExecuteFunc`"),
        Locale::Tc => format!("使用直接呼叫 `{name}()` 代替 `ExecuteFunc`"),
    }
}

pub fn execute_func_bad_hack() -> &'static str {
    pick(
        "ExecuteFunc is a bad hack: argument is not a computable string literal",
        "ExecuteFunc — костыль: аргумент не является вычислимой строкой",
        "ExecuteFunc — костиль: аргумент не є обчислюваним рядком",
        "ExecuteFunc 是个糟糕的做法：参数不是可计算的字符串字面量",
        "ExecuteFunc 是個糟糕的做法：參數不是可計算的字串字面量",
    )
}

