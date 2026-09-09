# Typst 附件布局适配器

源码已接入；本轮按用户要求没有构建、运行服务或执行测试。

## 触发与数据流

1. 原有解析器照常生成 Script(base, upper, lower)。不查询 LSP 的 operator 高亮，不匹配 sum、lim、op 或 stretch 名称。
2. Rust 视图仅为顶层 Script 提供该分支的 Typst 源码。中心槽为空或分支包含未确认命令时暂停查询。嵌套 Script 暂保持原布局，避免丢失分母、脚本等数学样式。
3. 浏览器在 160 ms 防抖后请求 POST /api/attachments，单个在途请求。按完整分支、定义和 display 模式缓存；光标移动不触发重复求值，旧响应不能覆盖新分支。
4. 原生服务调用独立的 Rust 适配器进程，12 秒超时。适配器复用固定版本 Typst 的求值、数学 IR、字体和布局代码，不调用 Tinymist LSP，也不解析 hover 文本。
5. 读取公开的 ScriptsItem.top/bottom/top_right/bottom_right，分别返回上、下槽位置。结果只影响视图，不替换编辑树，不改变源码或撤销历史。

请求：

```json
{"expression":"sum_(i=0)^(n)","definitions":"","display":true}
```

响应示意：

```json
{"engine":"Typst math IR 59b5999","upper":"limits","lower":"limits","stretch":false,"base":null}
```

upper/lower 为 limits、scripts 或 null（没有该附件）。失败返回 error，编辑器保留右侧槽位，底部直接显示错误，悬停可看完整文字；重新确认命令后可重试。混合位置可分别绘制；Typst 合并出无法一一对应的多附件、左附件时暂不投影。

## stretch

解析到中心字形的显式水平 stretch 属性后，先调用 Typst 的正常 Script 布局。其 layout_scripts 测量上、下 limits 的宽度，设置中心字形的伸展参考宽度；随后复用同一个 IR 中心对象生成独立 SVG。适配器不复制 limits 策略，也不自己拼接或横向缩放箭头。

响应的 base 此时包含 svg/width/height（pt）。浏览器仅更新原子中心对象的显示，保留其源码、光标边界和所有上下标输入槽。没有 SVG 到源码的反向定位。没有上下标的独立 stretch(...) 继续走现有 Raw 编译。

编辑器的槽位字体和间距仍是近似显示，因此与 Typst 完整排版并非像素一致；字形伸展依据的是 Typst 对同一分支的实际测量。

## 构建

- build.cmd 仍只编译 WASM。
- start.cmd 现在先调用 build-native.cmd 构建适配器，再编译并启动原服务。
- 首次准备源码需要 Git 和网络，并会编译 Typst 依赖。prepare.ps1 在公共的 `prototypes/typst-engine` 检出 59b5999da8e74e74583069408d2564fc1f9bc973；后续各原型通过路径依赖共享它，不会重复 clone。
- 补丁只有 layout-entry.rs 中的 editor_math_frame 入口及其 re-export；它调用现有 MathContext。准备脚本可以重复执行，并校验固定 revision。
- 适配器的依赖、锁文件与构建产物独立于 WASM，首次构建生成 native-adapter/Cargo.lock。源码仍只保存为 .typ。

当前适配器只提供本地公式和前置定义、随附 New Computer Modern Math 字体；不提供文件导入、包加载、日期或完整文档 introspection 环境。相关表达式失败时保留原编辑结构。LSP / 普通 Raw SVG 仍使用已安装的 Tinymist，其版本可能与固定 IR 引擎不同。

## 手动检查

- `$ sum_i^n $` 与 `$sum_i^n$`：切换行间／行内后位置跟随 Typst。
- `$ lim_(x->0) $`：内置文本算子。
- `$ scripts(sum)_i^n $`：显式改用右侧。
- `$limits(A)_i^n$`：普通字母也能强制 limits，无 operator 前置筛选。
- `#let my = math.op("my", limits: true)` 后的 `$ my_i $`：从定义求值。
- `$ stretch(arrow.r)^("a much longer label") $`：中心箭头随附件宽度伸展，附件仍能编辑。
- 在分式内部输入 sum_i：本阶段不发起附件适配请求。
- 改变定义、切换 display、快速编辑、撤销：缓存按上下文隔离。

已加入 tests/attachments.rs 和适配器内部测试，但本轮未执行。

依据：[Typst attachments](https://typst.app/docs/reference/math/attach/)、[stretch](https://typst.app/docs/reference/math/stretch/)。固定源码中的关键入口是 typst-library/src/math/ir/resolve.rs 的 resolve_inner_attach、typst-layout/src/math/scripts.rs 的 layout_scripts。

## 公式提取修正

适配器从求值后的文档内容中沿 SequenceElem / StyledElem 提取唯一的顶层 EquationElem，并保留外层样式。公式外的空行、段落分隔不再进入数学 IR，避免将它们误认为上下标分支的一部分。新增 lim_(x -> oo) 在空定义、空行和前置定义下的回归用例；本次仍未执行构建或测试。
