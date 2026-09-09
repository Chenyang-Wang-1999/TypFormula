# Typst 附件布局适配器

[返回模块总览](../README.md) · [架构与数据流](architecture.md)

已移除 stretch 专用 SVG 路径和 Raw 映射 box。附件服务仅返回位置，所有 Raw 均由文档编译与透明标签映射提供 SVG。

## 触发与数据流

1. 原有解析器照常生成 Script(base, upper, lower)。不查询 LSP 的 operator 高亮，不匹配 sum、lim、op 或 stretch 名称。
2. Rust 视图仅为顶层 Script 提供该分支的 Typst 源码。中心槽为空或分支包含未确认命令时暂停查询。嵌套 Script 暂保持原布局，避免丢失分母、脚本等数学样式。
3. 浏览器在 160 ms 防抖后请求 POST /api/attachments，单个在途请求。按完整分支、公式词法前缀和 display 模式缓存；扫描包含非活动公式在内的各公式，光标移动不触发重复求值，旧响应不能覆盖已刷新的缓存记录。编辑附件期间暂缓查询。
4. 附件位置查询调用单次 Rust 适配器进程，12 秒超时。适配器复用固定版本 Typst 的求值、数学 IR 和字体代码，不调用 Tinymist LSP，也不解析 hover 文本。文档 SVG 渲染另走同一适配器的常驻模式。
5. 读取公开的 ScriptsItem.top/bottom/top_right/bottom_right，分别返回上、下槽位置。结果只影响视图，不替换编辑树，不改变源码或撤销历史。

请求：

```json
{"expression":"sum_(i=0)^(n)","definitions":"","display":true}
```

响应示意：

```json
{"engine":"Typst math IR 59b5999","upper":"limits","lower":"limits"}
```

upper/lower 为 limits、scripts 或 null（没有该附件）。失败返回 error，编辑器保留右侧槽位，底部直接显示错误，悬停可看完整文字；重新确认命令后可重试。混合位置可分别绘制；Typst 合并出无法一一对应的多附件、左附件时暂不投影。

## 中心 bbox 与刷新

前端网格以中心项的实际显示 bbox 安排上下标：limits 放在中心上方／下方，scripts 放在侧边并沿中心高度排列。Raw SVG 尺寸更新后，CSS 重新布局；位置服务不返回像素坐标。

Raw 按源码文本共享成功 SVG 缓存。编辑上下标并离开附件区域或输入失焦时，刷新中心项内的 Raw，同时失效对应附件位置；只移动光标不刷新。上下标之间移动属于同一编辑会话。“更新全部 SVG”清除这两类缓存，未确认命令存在时等待确认或取消。

## stretch

已删除适配器中的 stretch 判断、两次布局生成中心 SVG，以及浏览器的中心 SVG 替换分支；`editor_math_frame` 入口与 re-export 也已移除。

最初去掉专用处理后，Raw 外层 box 隔断了父级 Script 对中心字形的伸展设置，短／长上标的箭头均宽 24pt。现在编译投影改用带标签的嵌套公式；引擎在数学 IR 阶段透明解析其内容，保留原有字形、数学类别和伸展信息，到排版结束才给输出 frame 加映射标签。

回归结果：`stretch(arrow.r)` 分别带短上标 `"a"` 和长上标 `"a much longer label"` 时，提取的 SVG 宽度为 24pt／144.66pt，与未加映射的 Typst 文档一致。普通 Raw、上下标定位、全局刷新与失焦刷新保留；SVG 仍按已有规则缓存。

伸展对照用例已纳入默认测试集，也可单独运行：

```powershell
cargo test --offline --locked --manifest-path native-adapter/Cargo.toml --target-dir target/adapter stretch_document_probe -- --nocapture
```

## 构建

- build.cmd 仍只编译 WASM。
- start.cmd 现在先调用 build-native.cmd 构建适配器，再编译并启动原服务。
- 首次准备源码需要 Git 和网络，并会编译 Typst 依赖。prepare.ps1 在公共的 `prototypes/typst-engine` 检出 59b5999da8e74e74583069408d2564fc1f9bc973；后续各原型通过路径依赖共享它，不会重复 clone。
- `engine-patches.json` 保存透明标签的引擎补丁，`layout-entry.rs` 将最终片段标记为可提取的 frame，字形在此之前仍保留原生排版属性。准备脚本移除旧 box 桥接及 `editor_math_frame` 导出；重复运行不重复打补丁，并校验固定 revision 和补丁锚点。
- 适配器的依赖、锁文件与构建产物独立于 WASM，首次构建生成 native-adapter/Cargo.lock。源码仍只保存为 .typ。

附件位置查询使用公式与词法前缀，读取 IR 时使用 EmptyIntrospector；文档 SVG 则编译完整内存主文档。两者提供随附 New Computer Modern Math 字体，不提供其他文件导入、包加载或日期。相关表达式失败时保留原编辑结构。Tinymist 仅用于补全，Raw SVG 统一使用固定版本原生引擎。

## 手动检查

- 分别插入行间和行内公式，输入 `sum_i^n`：位置跟随各自公式类型。当前没有行内／行间切换按钮。
- `$ lim_(x->0) $`：内置文本算子。
- `$ scripts(sum)_i^n $`：显式改用右侧。
- `$limits(A)_i^n$`：普通字母也能强制 limits，无 operator 前置筛选。
- `#let my = math.op("my", limits: true)` 后的 `$ my_i $`：从定义求值。
- `$ stretch(arrow.r)^("a much longer label") $`：编辑上下标后离开附件区域，或按“更新全部 SVG”，箭头跟随附件宽度刷新。
- 在分式内部输入 sum_i：本阶段不发起附件适配请求。
- 改变定义、快速编辑、撤销：附件位置缓存按词法前缀、公式类型和分支源码区分；Raw SVG 继续按源码文本共享，需要时按“更新全部 SVG”。

原生适配器测试已通过，包含 stretch 与未加映射文档的宽度对照、普通 Raw、宏、脚本字号、空内容、上下文样式和错误恢复。界面伸展效果已由用户确认。

依据：[Typst attachments](https://typst.app/docs/reference/math/attach/)、[stretch](https://typst.app/docs/reference/math/stretch/)。固定源码中的关键入口是 typst-library/src/math/ir/resolve.rs 的 resolve_inner_attach、typst-layout/src/math/scripts.rs 的 layout_scripts。

## 公式提取修正

适配器从求值后的文档内容中沿 SequenceElem / StyledElem 收集顶层 EquationElem，取最后一个请求公式并保留外层样式，避免把词法前缀中的公式当成查询目标。公式外的空行、段落分隔不进入数学 IR。适配器测试覆盖 lim_(x -> oo) 在空定义、空行和前置定义下的结果。
