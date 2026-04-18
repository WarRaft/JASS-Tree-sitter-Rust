# `//set` — 文件配置指令

`//set` 指令用于为语言服务器配置单文件级别的设置。
它必须出现在文件的**最开头**，与 `//import` 指令一起，在任何语言语句之前。

## 语法

```jass
//set <键> <值>
```

* `//set` 必须从**第 0 列**开始（没有前导空格）。
* `<键>` 是设置名称（键中不允许有空格）。
* `<值>` 是键之后到行末的所有内容（会去除首尾空格）。

## 示例

```jass
//import common/natives.j
//set hint ref type
//set build-jass ./out/war3map.j

globals
    integer count = 0
endglobals
```

## 可用设置

| 键 | 类型 | 默认 | 说明 |
|----|------|------|------|
| `hint` | `ref` `type` | | 要显示的内嵌提示类型。`ref` — 引用 ID（用于调试符号解析），`type` — 类型注释（例如 `: integer`、`: constant real array`）。无指令时不显示提示（ujapi 除外）。 |
| `lens` | `fn` `var` `arg` | | 要显示的代码透镜。`fn` — 函数/native 上方的引用计数，`var` — 变量（全局+局部）上方的引用计数，`arg` — 函数参数上方的引用计数。无指令时不显示代码透镜。 |
| `build-jass` | `<路径>` | `./` | JASS 构建的输出路径。将整个导入树合并为单个 `.j` 文件：类型 → native → globals → 函数（拓扑排序）→ `main`。如果路径是目录，则追加 `war3map.j`。当路径指向 `.w3x` 或 `.w3m` 存档时，脚本会直接注入到地图中。 |
| `build-as` | `<路径>` | `./` | AngelScript 构建的输出路径。相同的合并逻辑，但输出 `.as` 语法。保留字冲突通过追加数字后缀解决。当路径指向 `.w3x` 或 `.w3m` 存档时，脚本会直接注入到地图中。 |
| `backup` | `<路径>` | `./` | 地图存档备份路径。在向 `.w3x` / `.w3m` 文件注入脚本之前，原始存档的副本会保存到此路径，文件名带有日期前缀：`YYYY_MM_DD_原始文件名.w3x`。如果路径是目录，则带日期前缀的文件名会放入该目录。 |
| `build-opts` | `uglify` `nolocal` | | 构建选项标签。`uglify` 启用构建输出中的标识符压缩。`nolocal` 将“返回局部变量”泄漏修复切换为无局部 temp 的策略（使用临时全局变量而不是引入临时局部变量）。一条指令中可以组合多个标签。 |
| `build-uglify` | `0 \| 1` | `0` | `//set build-opt uglify` 的旧别名。为兼容性保留。 |
| `build-before` | `<命令>` | | 构建**之前**执行的终端命令。通过 `sh -c`（Unix）或 `cmd /C`（Windows）执行。工作目录为 `//entry` 文件所在目录。支持 `{{变量}}` 模板占位符（见下文）。 |
| `build-after` | `<命令>` | | 构建**之后**执行的终端命令（仅在成功时）。执行规则与 `build-before` 相同。 |

## 模板变量

`build-before` 和 `build-after` 中的命令支持 `{{变量}}`
占位符，在执行前会被展开为完整的规范化路径。
这使您可以可靠地将构建路径传递给外部脚本。

| 变量 | 说明 |
|------|------|
| `{{entry}}` | `//entry` 文件的完整规范化路径。 |
| `{{entry-dir}}` | 包含 `//entry` 文件的目录的完整规范化路径。 |
| `{{target-jass}}` | JASS 构建输出文件的完整规范化路径（来自 `//set build-jass`）。未配置时为空。 |
| `{{target-as}}` | AngelScript 构建输出文件的完整规范化路径（来自 `//set build-as`）。未配置时为空。 |

### 模板示例

```jass
//entry
//set build-jass ./out/war3map.j
//set build-before echo "正在从 {{entry}} 构建..."
//set build-after my-post-build.sh {{target-jass}}
```

## 行为

* 设置仅作用于单个文件 — 不会通过 `//import` 传播。
* 无法识别的键会被静默接受（为了向前兼容）。
* 缺少值会产生警告诊断。


