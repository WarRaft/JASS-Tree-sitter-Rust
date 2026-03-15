[![](https://dcbadge.limes.pink/api/server/https://discord.gg/CNeQmXAgVq)](https://discord.gg/CNeQmXAgVq)

[English](README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | **简体中文** | [繁體中文](README.zh-tw.md)

# JASS Tree-sitter Rust

没错，名称就是对技术栈的直白描述——用 Rust 编写的 Tree-sitter 语法。  
把 JASS 放在最前面是为了吸引眼球（和怀旧），当然，我们也完全支持它。

## [VSCode](https://code.visualstudio.com)

本插件汇集了各种用于处理经典 WarCraft III 内容的工具，提供语法支持、编辑器功能以及一些现代化的便利特性。

👉 [VSCode Marketplace](https://marketplace.visualstudio.com/items?itemName=WarRaft.jass-tree-sitter-rust)

---

## 支持的语言

### [JASS](https://github.com/WarRaft/tree-sitter-jass) — `.j`、`.pld`

WarCraft III 的主要脚本语言。基于专用的
[tree-sitter-jass](https://github.com/WarRaft/tree-sitter-jass) 语法提供完整支持。

### [AngelScript](https://github.com/WarRaft/tree-sitter-as) — `.as`

为基于 UJAPI 的 WarCraft III 模组开发提供 AngelScript 支持。语法 —
[tree-sitter-as](https://github.com/WarRaft/tree-sitter-as)。

### [BNI](https://github.com/WarRaft/tree-sitter-bni) — `.bni`

**BNI**（Blizzard Notation Ini）— Warcraft III 模组开发中使用的结构化配置格式。  
语法 — [tree-sitter-bni](https://github.com/WarRaft/tree-sitter-bni)。

### BLP — `.blp`

内置的 **BLP** 纹理格式图像查看器，BLP 是 WarCraft III 使用的纹理格式。

---

## LSP 功能

扩展附带独立的 Rust 编写的 LSP 服务器（支持 Linux、macOS、Windows）：

| 功能 | JASS | AngelScript | BNI |
|------|:----:|:-----------:|:---:|
| **语义高亮** | ✅ | ✅ | ✅ |
| **代码折叠** | ✅ | ✅ | ✅ |
| **文档符号** | ✅ | ✅ | ✅ |
| **诊断** | ✅ | ✅ | — |
| **跳转到定义** | ✅ | ✅ | — |
| **查找所有引用** | ✅ | ✅ | — |
| **高亮出现位置** | ✅ | ✅ | — |
| **重命名** | ✅ | ✅ | — |
| **悬停提示** | ✅ | ✅ | — |
| **自动补全** | ✅ | ✅ | — |
| **内联提示** | ✅ | ✅ | — |
| **文档链接** | ✅ | ✅ | — |

---

## 导入系统

JASS 文件可以通过特殊的注释指令相互链接：

```jass
//import path/to/file.j
//import! blizzard/common.j
```

- `//import` — 将另一个文件链接到共享作用域。所有顶层声明（函数、全局变量、类型、原生函数）都将可用。
- `//import!` — **冻结**导入。与 `//import` 相同，但目标文件被视为只读，不会被重构或自动重命名修改。

指令必须出现在文件最开头，在任何语言语句之前。

### 导入功能

- **路径补全** — `//import` 后的文件路径自动补全。
- **Ctrl+Click** — 在编辑器中打开导入的文件。
- **无效路径诊断** — 不存在的路径会被高亮为错误。
- **重命名/移动时自动更新** — 当导入的文件被重命名或移动时，所有引用文件中的路径会自动重写。
- **跨平台路径** — `/` 和 `\` 可互换；支持相对路径、绝对路径和 Windows 风格路径（`C://`）。
- **循环检测** — 检测并报告循环导入。

---

## `//set` — 文件级配置

```jass
//set ref-tip 1
//set build-jass ./out/war3map.j
//set build-as ./out/war3map.as
```

| 键 | 值 | 说明 |
|----|-----|------|
| `ref-tip` | `1` / `0` | 显示/隐藏每个标识符旁的引用 ID 内联提示——对调试符号解析很有用。 |
| `build-jass` | `<路径>` | JASS 构建的输出路径。将整个导入树合并为单个 `.j` 文件。 |
| `build-as` | `<路径>` | AngelScript 构建的输出路径。相同的合并逻辑，但输出 `.as` 语法。 |

---

## 跨文件智能

所有通过 `//import` 链接的文件形成一个**连通分量**——共享的全局作用域：

- **作用域解析器** — 跨所有导入文件的持久 O(1) 名称查找，在服务器重启之间保持。
- **两阶段解析** — 第一阶段在本地解析符号；第二阶段将未解析的引用与导入的符号链接。
- **导出差异检测** — 仅在导出声明集实际发生变化时才重新解析依赖文件。
- **推送诊断** — 立即为受影响的文件报告错误，即使它们未在编辑器中打开。

---

## 调用图

服务器在整个连通分量中构建函数调用图：

- **未使用函数检测** — 从 `main` / `config` 入口点不可达的函数会被标记。
- **循环检测** — 通过诊断报告循环调用链。
- **拓扑排序** — 构建系统使用它来确保被调用函数出现在调用函数之前（JASS 要求）。

基于 D3.js 的**调用图**面板可通过编辑器标题栏按钮访问。

---

## 导入图可视化

基于 D3.js 的**导入图**面板显示当前文件的依赖树。可通过编辑器标题栏按钮访问。所有可视化资源已内置——无需互联网连接。

---

## 构建系统

`//set build-jass <路径>` 和 `//set build-as <路径>` 指令触发构建：

1. 收集导入树中的所有文件。
2. 对函数执行拓扑排序。
3. 将所有内容合并到单个输出文件：**类型 → 全局变量 → 函数 → `main`**。
4. 跳过 `native` 声明和类型定义（它们由引擎提供）。
5. 顶层的裸调用表达式被折叠到 `main` 中。

---

## 持久缓存

所有重量级数据结构通过 **bincode** 序列化到磁盘，并在服务器重启时恢复：

- **导入图** — 文件依赖图（基于 petgraph）。
- **作用域解析器** — 全局符号索引。
- **符号缓存** — 按文件存储的函数/变量/类型声明。
- **引用缓存** — 按文件存储的引用映射。

这意味着即使对于大型项目也能几乎即时启动。

---

## 架构

- **Tree-sitter** — 所有支持语法的增量解析。
- **`ParseSnapshot`** — 每个文件所有 LSP 数据的原子不可变快照，存储在 `Arc<ParseSnapshot>` 中以实现无锁并发读取。
- **`CancellationToken`** — 按文件取消：新编辑立即中止过时的解析任务。
- **DashMap** — 所有快照的并发文件存储。
- **petgraph** — 导入图和调用图分析。

---

## 键盘快捷键

所有命令都可通过编辑器标题栏按钮使用，但您也可以分配自定义键盘快捷键。

打开**键盘快捷方式**（`Ctrl+K Ctrl+S` / `⌘K ⌘S`），搜索命令名称，然后绑定任意组合键。

| 命令 ID | 说明 |
|---------|------|
| `importGraph.show` | 显示导入图 |
| `callGraph.show` | 显示调用图 |
| `build.execute` | 构建（JASS / AngelScript） |

或者直接在 `keybindings.json` 中添加绑定（`Ctrl+Shift+P` → *Preferences: Open Keyboard Shortcuts (JSON)*）：

```json
[
  { "key": "ctrl+shift+i", "command": "importGraph.show", "when": "resourceLangId == jass || resourceLangId == angelscript" },
  { "key": "ctrl+shift+g", "command": "callGraph.show",   "when": "resourceLangId == jass || resourceLangId == angelscript" },
  { "key": "ctrl+shift+b", "command": "build.execute",    "when": "resourceLangId == jass || resourceLangId == angelscript" }
]
```

---

## 许可证

[MIT](LICENSE)

