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

/// Get the localized detail for a `SetDef` by key.
pub fn set_def_detail(key: &str) -> &'static str {
    match key {
        "ref-tip" => set_def_ref_tip(),
        "type-tip" => set_def_type_tip(),
        "build-jass" => set_def_build_jass(),
        "build-as" => set_def_build_as(),
        _ => "",
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

