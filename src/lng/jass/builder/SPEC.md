# Техзадание / актуальная спецификация JASS Builder (`src/lng/jass/builder`)

## Статус

`src/lng/jass/builder` уже является активным новым контуром сборки для JASS/AS.
Документ ниже описывает **текущее состояние проекта**, уже принятые архитектурные
правила и ближайшие этапы развития.

Старый `src/lng/jass/build` всё ещё существует как legacy-слой и пока частично
используется как источник старой логики/совместимости, но развитие должно идти
через `builder`.

---

## Цель

Builder должен стать единым пайплайном для:

1. **сбора проекта** в один builder-level snapshot;
2. **анализа** в режиме диагностики или сборки;
3. **локальных модификаций AST/плана сборки**;
4. **отдельного прохода записи результата** только когда это явно требуется.

Ключевая идея:

- сначала собирается весь доступный контекст проекта;
- затем выполняется проход builder-а в одном из режимов;
- затем, если `write_output = true`, результат рендерится и записывается в файл;
- локальные однофайловые проблемы могут фикситься отдельно без знания второго файла.

---

## Основные принципы

1. **`mod.rs` — только публичный API.**
   Никакой бизнес-логики в `mod.rs`.
   `mod.rs` содержит:
   - объявления подмодулей;
   - общие типы (`BuildResult`, `BuildOptions`, `PipelineMode`);
   - тонкие публичные обёртки, делегирующие вызов в подмодули.

2. **Builder работает в двух режимах.**
   Через `BuildOptions` и `PipelineMode`:
   - `PipelineMode::Diagnostics` — анализ без записи файла;
   - `PipelineMode::Build` — режим сборки/модификации;
   - `write_output: bool` — отдельный флаг записи результата.

3. **Сначала собирается project snapshot.**
   Builder сначала собирает `ProjectAst` из `project.rs`, а не начинает сразу
   писать файл.

4. **Отдельный проход записи.**
   Рендер и запись в файл — это финальный шаг, который выполняется только когда
   явно указано `write_output = true`.

5. **Локальные фиксы должны уметь работать по одному файлу.**
   Если проблема по своей природе однофайловая (например, `fix_leaks`), builder
   обязан уметь пройти только один файл и исправить его без знания остального
   проекта.

6. **Кросс-файловые диагностики делаются только после сбора полного проекта.**
   Ошибки вида “неиспользованная функция/переменная”, глобальные reachability-
   проверки и cross-file symbol analysis делаются только имея весь видимый проект.

7. **Никакого бизнес-решения через случайный текстовый скан файла.**
   Структурная информация для фиксов и рендера должна опираться на AST и/или
   единый builder-index. Текст может использоваться только как final render/apply
   слой либо для leaf-токенов.

---

## Текущее состояние реализации

### 1. Публичный API (`mod.rs`)

Сейчас доступны:

- `build_jass(uri)`
- `diagnose_jass(uri)`
- `build_jass_preview(uri)`
- `build_as(uri)`
- `diagnose_as(uri)`
- `fix_local(uri)`
- `fix_local_preview(uri)`
- `has_build_setting(uri, key)`
- `resolve_hooks(uri)`

Общие типы:

- `BuildResult`
- `BuildOptions`
- `PipelineMode`

### 2. Сбор проекта (`project.rs`)

Уже реализован `ProjectAst` как **lifetime-free project snapshot**.

Текущая форма:

- `ProjectAst`
  - `trigger_uri`
  - `out_path`
  - `files: Vec<ProjectFile>`
- `ProjectFile`
  - `uri`
  - `source`
  - `is_frozen`
  - `function_callees`

Важно: это **не** один giant `Ast<'tree>` на весь проект. Из-за lifetime-модели
`tree-sitter::Node<'tree>` builder пока хранит owned snapshot файлов и metadata,
а typed AST конкретного файла пересобирается внутри pipeline passes.

Это считается допустимым текущим компромиссом.

### 3. JASS pipeline (`build_jass.rs`)

Текущее поведение уже разделено на стадии:

1. `collect_project(...)`
2. `analyze_project(...)`
3. `render_plan(...)`
4. опциональная запись результата в файл

Внутри анализа собирается `JassBuildPlan`:

- `globals`
- `functions`
- `function_order`
- `function_callees`
- `bare_stmts`
- `frozen_import_directives`
- `sorted_funcs`

Текущее рендер-поведение:

- `type` / `native` не попадают в итоговый JASS output;
- `globals` объединяются;
- bare top-level statements собираются и вставляются в `main`;
- frozen import directives переэмитятся в заголовок с консистентным относительным путём;
- финальный output проходит через нормализацию пустых строк/окончаний строк.

### 4. AS pipeline (`build_as.rs`)

`AS` уже подключён к тому же pipeline-каркасу:

1. `collect_project(...)`
2. `project.clone()`
3. `transform_as(...)`
4. опциональная запись

Пока `transform_as(...)` — заглушка, которая пишет однострочный комментарий,
но архитектурно AS уже должен работать **от копии project snapshot**, а не от
случайной отдельной логики.

### 5. Render layer (`render.rs`)

Текущее состояние:

- структурные узлы рендерятся вручную из AST-полей;
- прямой dump compound-node текста из дерева запрещён;
- допустимы только leaf-level чтения текста из `src`:
  - `Id`
  - literal nodes
  - operator token между выражениями

Это соответствует правилам: builder не должен работать как `snippet(src, node)`-
машина для структурных узлов.

### 6. Локальные фиксы (`local_fix.rs`)

Сейчас реализован local single-file fixer для локальных проблем, не требующих
знания второго файла.

Текущее покрытие:

- leak fixes (`code == "leak"`)

Принятые правила:

- сначала строится единый AST-index по файлу (`AstFixIndex`);
- затем diagnostics используют этот индекс;
- не допускается повторный ad-hoc AST scan на каждый diagnostic;
- тесты хранятся в `local_fix_test.rs` (а не inline внутри файла).

---

## Текущая архитектура проходов

### Режим диагностики

```text
collect_project
    ↓
analyze_project / transform_as
    ↓
вернуть report / BuildResult без записи файла
```

### Режим сборки без записи

```text
collect_project
    ↓
analyze_project / transform_as
    ↓
render_plan / render_as
    ↓
вернуть preview без записи файла
```

### Режим сборки с записью

```text
collect_project
    ↓
analyze_project / transform_as
    ↓
render_plan / render_as
    ↓
write_output
```

### Локальный однофайловый fixer

```text
parse single file
    ↓
build AST index for fixes
    ↓
collect diagnostics
    ↓
produce edits
    ↓
optional write back
```

---

## Что уже считается решённым

- [x] `builder` подключён как основной современный API для build endpoint-ов
- [x] `mod.rs` очищен от бизнес-логики
- [x] введены `BuildOptions` и `PipelineMode`
- [x] введён `ProjectAst` snapshot
- [x] JASS pipeline разделён на collect/analyze/render/write
- [x] AS pipeline переведён на общий каркас и работает от копии проекта
- [x] local single-file fixer добавлен
- [x] frozen import header и относительные пути перенесены в новый builder
- [x] render для структурных AST-узлов не сводится к dump целого node slice

---

## Что ещё не завершено

### 1. Builder-level diagnostics report

Сейчас `BuildResult` остаётся минимальным типом результата.
Нужен отдельный report-тип, который будет хранить:

- builder diagnostics
- статистику
- modified plan/tree
- rendered output preview (optional)
- список выполненных локальных фиксов

### 2. Единый global analysis pass

Пока каркас уже есть, но полноценный cross-file diagnostics pass ещё не завершён.
Нужно добавить:

- неиспользованные функции
- неиспользованные глобалы
- unreachable / dead code на project level
- cross-file duplicate symbols
- undeclared symbols на уровне всего project snapshot

### 3. Общий анализ для leak diagnostics и leak fixing

Сейчас логика leak analysis исторически живёт в двух местах:

- diagnostics path (`cursor.rs`)
- build auto-fix path (`build/fix_leaks.rs`, legacy)

Целевое состояние:

- один общий leak-analysis слой;
- parser/LSP использует его для diagnostics;
- builder использует те же факты для фикса;
- локальный fixer использует тот же слой для single-file patching.

### 4. Работа не только с snapshot, но и с builder-owned transform tree

Сейчас builder хранит project snapshot и внутри проходов пересобирает AST файла.
Следующий шаг — ввести builder-owned transform representation, которую можно:

- модифицировать во время build-pass;
- передавать в AS/JASS render pass;
- использовать для write pass без повторного semantic анализа.

### 5. Полное вытеснение legacy `build/`

Пока legacy-код ещё не убран полностью.
В частности, `resolve_hooks(...)` пока делегируется в старый `build`.

Целевое состояние:

- `resolve_hooks` живёт в `builder`;
- legacy `build` больше не нужен как runtime dependency;
- старые IR passes либо удалены, либо формально признаны obsolete.

### 6. Out-of-process запуск

Пока это future-work.
Целевой контракт остаётся:

- сериализуемый project snapshot / request
- запуск отдельного builder process
- возврат diagnostics/result через stdin/stdout

---

## Актуальная структура модуля

```text
src/lng/jass/builder/
    mod.rs             — публичный API и общие типы
    collect.rs         — поиск build settings, file order, read_source, output path
    project.rs         — ProjectAst / ProjectFile / collect_project
    build_jass.rs      — JASS pipeline: analyze → render → optional write
    build_as.rs        — AS pipeline: clone project → transform → optional write
    render.rs          — AST → text render layer
    sort.rs            — topological sort for functions
    local_fix.rs       — local single-file fixes
    local_fix_test.rs  — тесты local_fix
    SPEC.md            — актуальная спецификация builder
```

---

## Критерии ближайшего следующего этапа

Следующий этап считается готовым, когда:

- [ ] появится `BuilderReport` / аналогичный result-type поверх `BuildResult`
- [ ] builder diagnostics будут собираться project-wide одним проходом
- [ ] leak analysis будет вынесен в общий слой для parser + builder + local fixer
- [ ] build-mode сможет работать по модифицируемому builder-owned tree/plan,
      а write-pass будет только рендерить уже подготовленный результат
- [ ] `resolve_hooks` будет перенесён из legacy `build` в `builder`

---

## Запрещённые архитектурные откаты

Нельзя возвращаться к следующим практикам:

1. писать бизнес-логику в `mod.rs`;
2. рендерить compound AST nodes через целиковый `src[node.start..node.end]`;
3. делать отдельный AST scan на каждый diagnostic/fix;
4. смешивать analysis-pass и write-pass в один неразделимый шаг;
5. реализовывать локальные однофайловые фиксы через глобальный project builder,
   если знание второго файла не требуется.
