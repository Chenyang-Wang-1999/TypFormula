# 架构与数据流

本文按当前源码说明各模块的职责和协作方式。[返回模块总览](../README.md)。

## 运行组成与状态归属

编辑器有三部分运行产物：浏览器中的 `web/core.wasm`、本地 HTTP 服务 `lyx-typst-demo`、原生适配器 `lyx-typst-layout-adapter`。完整 Typst 编译器只链接到原生适配器，WASM 使用 `typst-syntax` 进行语法分析。

| 状态 | 所属模块 | 生命周期与用途 |
|---|---|---|
| 多公式、上下文、全局宏定义、活动公式和文档撤销历史 | `Document` | 浏览器 WASM 实例内；构成当前文档的编辑状态 |
| 单公式编辑树、光标、选区、命令草稿和测量数据 | `Editor` | 每个公式各一份；处理当前公式的结构操作 |
| 宏登记表和共享模板 | `typst.rs` | 最近定义集的线程局部缓存；相同定义复用，变化时复用未变前缀 |
| Raw SVG、Blob URL、附件位置、异步请求状态 | `web/app.js` | 页面内存；不进入文档撤销历史或导出文件 |
| 常驻文档渲染进程 | `Services` | 首次渲染时启动，后续请求复用；传输失败或超时后重建 |
| Typst 主源码、字体与编译缓存 | 原生适配器 | 常驻模式保留 World，更新内存 Source 后再次编译；每次请求后清理旧缓存项 |

`source` 是编辑状态的序列化输出。上下文与 Raw 保留自己的源码，已识别的结构按规则重写，例如 `a/b` 可以导出成 `frac(a, b)`。SVG、光标和结构块边界没有单独的持久化格式。

## 编辑核心

### 文档协调：src/document.rs

`Document` 在既有单公式 `Editor` 外增加文档层。它维护 `formulas: Vec<Editor>` 和 `contexts: Vec<String>`，始终满足“上下文段数 = 公式数 + 1”：

```text
全局 definitions
context[0] → formula[0] → context[1] → formula[1] → context[2]
```

插入公式时按 UTF-8 字节位置拆开上下文；删除公式时合并两侧上下文。行内或行间类型在插入时指定。激活另一公式只切换操作目标，保留各自光标；尚有未确认草稿时限制跨公式切换。

模块还负责两件跨公式的工作：

- 将全局定义、此前上下文和此前公式拼成各公式的词法前缀。定义或上下文变化时，重新解析前缀改变的公式，更新宏分类；普通结构输入只同步前缀。
- 在文档级记录快照并执行撤销／重做，覆盖公式输入、正文、定义以及公式的增删。浏览器通过 Document 操作，单公式 Editor 的历史不是页面的全局撤销入口。

显式导入扫描顶层 Equation 节点，将其转换成结构公式，其他内容保留为上下文。上下文文本框的输入直接走 `set_context`，不会因为出现 `$` 自动调用导入逻辑。

`response()` 聚合所有公式的视图并生成完整源码。随后 `annotate()` 把可见 Raw 关联到源码区间：普通 Raw 利用编辑树光标定位，宏模板内固定 Raw 利用定义来源和语法节点匹配。重复出现的视图有各自 `render_id`，但可能对应同一个源码区间。

### 数据模型：src/math.rs

`MathData = Vec<MathAtom>`，每个原子由 `Kind` 和子槽 `cells` 组成：

| 对象类别 | 表达内容 |
|---|---|
| Char／Symbol／Raw | 原始字符、配置显示符号、整体保存源码的未知表达式 |
| Frac／Sqrt／Root／Script | 分式、根式、上下标等可进入的结构槽 |
| Delim／Grid／Aligned／Decoration／Text | 括号、矩阵、对齐、装饰和字符串结构 |
| MacroCall | 宏调用及唯一一份实参槽 |
| TemplateCall／Parameter | 仅用于宏模板的定义版本引用和参数引用 |
| Unknown | 尚未确认的命令草稿，保存草稿光标及取消时要恢复的内容 |

`Cursor` 由 `CursorSlice` 路径和槽内 `pos` 组成，位置是对象之间的插入点，不是 Typst 字节位置。`occurrence` 区分宏重复参数的显示位置，使点击某个参数副本后光标仍停留在该副本。

本模块也提供槽位进入规则、上下标槽顺序、光标有效性检查和符号字典查询。

### 公式编辑：src/cursor.rs

`Editor::apply(Action)` 是单公式操作入口。它实现普通输入、命令／字符串状态、确认与取消、补全填入、水平和垂直移动、选区、剪贴板、删除及矩阵扩展。

普通按键直接修改编辑树。命令确认和导入才交给语法模块解析；补全填入不等于确认。Raw 编译状态回传后，失败块允许重新进入源码编辑，成功块按整体跨越和删除。

浏览器测得的槽位坐标通过 `geometry` 回传，用于垂直导航和命中判断。数学对象的编辑规则留在 Rust，像素测量留在浏览器。

### 语法与宏：src/typst.rs

本模块使用固定引擎中的 `typst-syntax`，负责 `parse_formula`、`parse_command`、`parse_document` 和 `write_cell`／`write_document_mode` 等转换。

识别出的结构转换为本地槽；符号配置命中时保留源码并改变显示；不能结构化的表达式作为 Raw。它做语法分析和受限的宏模板分析，任意 Typst 代码的求值交给原生引擎。

宏登记表按定义顺序记录不可变版本，当前名称表处理覆盖关系。可展模板使用 `Rc` 共享，嵌套调用引用早先定义版本；参数引用最终投影到调用的实参槽，不复制可编辑参数。

当前缓存保留最近一个定义集：定义相同时复用；定义改变时仍全量解析源码，但从第一条变化的定义起重建后缀模板。多公式拥有不同前缀，因此不能理解为所有公式都各自拥有长期缓存。展开视图有 4096 节点及深度保护，超预算时显示调用与实参。

### 视图投影与 WASM：src/view.rs、src/wasm.rs

`view.rs` 将编辑树转换成 `View`：结构类型、显示文字、子节点、光标停靠点、选择状态，以及 Raw 编辑位置和宏定义来源。它在显示时展开可展宏，固定模板内容不可在调用处修改，重复参数绑定同一个实参槽。

满足条件的顶层 Script 带有 `attachment` 源码，供前端查询 Typst 的上下标位置。嵌套 Script 暂不提供这类查询，保留默认布局。

`wasm.rs` 持有单个 Document，通过以下接口同步交换 UTF-8 JSON：

| 导出函数 | 功能 |
|---|---|
| `alloc(size)` | 为请求分配内存 |
| `dispatch(ptr, len)` | 接收一次操作，执行 Document.apply，返回响应字节地址 |
| `output_len()` | 读取响应字节长度 |

`src/lib.rs` 导出核心模块；条件编译让原生服务只出现在原生目标中，让 WASM 桥只出现在 wasm32 目标中。

## 浏览器与本地服务

### 页面交互：web/

`index.html` 提供工具栏、上下文与公式容器、宏管理器、源码导入导出和操作说明。`app.js` 实现浏览器侧行为：

| 功能组 | 主要函数 | 职责 |
|---|---|---|
| 核心调用 | `call`、`send` | JSON 与 WASM 内存交换，接收最新状态后安排绘制和辅助查询 |
| 文档与公式绘制 | `renderDocument`、`draw`、`render` | 保持上下文文本框，绘制活动与非活动公式、结构槽和 Raw |
| 几何与输入 | `measure`、`paintCaret`、事件监听器 | DOM 坐标测量、光标绘制、鼠标点击、键盘、输入法组合输入及剪贴板 |
| Raw 预览 | `schedulePreviews`、`runPreviews` | 找出待生成 Raw，批量请求文档编译，校验 SVG 并管理 Blob URL |
| 附件位置 | `scheduleAttachments`、`runAttachments` | 对各公式可查询的 Script 请求 limits／scripts 位置 |
| 刷新规则 | `refreshAllSvg`、`updateAttachmentEdits` | 全局刷新，以及编辑上下标后退出或失焦时使中心 Raw 失效 |
| 补全与宏列表 | `scheduleCompletion`、`renderMacros` | 异步 LSP 补全、宏列表草稿与应用操作 |

`style.css` 负责编辑器布局。上下标采用以中心项 bbox 为基准的网格：居中的 limits 在中心上方／下方，侧边 scripts 围绕中心高度排列。`math-font.js` 只转换数学字母显示字形，配置字典值、草稿与字符串不受额外转换。

### HTTP 与进程桥接：src/main.rs、src/services.rs

HTTP 服务默认绑定 `127.0.0.1:4320`，提供固定静态资源并校验 Host、Origin 和 JSON 请求类型。每个 API 请求交由工作线程处理，编辑按键本身不通过 HTTP。

| API | 请求内容 | 返回内容／执行者 |
|---|---|---|
| `GET /api/status` | 无 | Tinymist 可用状态和适配器是否存在 |
| `POST /api/completion` | 命令投影源码及草稿起止／光标位置 | Tinymist LSP 候选，Services 转换位置与替换范围 |
| `POST /api/render` | 完整源码、Raw 区间、结构公式区间 | 常驻 Typst 适配器返回 Raw SVG、尺寸、基线、页码和位置 |
| `POST /api/attachments` | 分支表达式、词法前缀、公式类型 | 单次适配器返回 upper／lower 的位置类别 |

补全每次创建独立 Tinymist LSP 会话，前端防抖并限制同时在途的请求。Services 负责 UTF-8 字节位置与 LSP UTF-16 位置之间的转换；为了让不完整草稿可补全，临时补足的括号只存在于查询投影中。

文档渲染通过 `RenderAdapter` 管理常驻 `--server` 子进程，以每行一个 JSON 的协议通信。正常编译错误保留进程和 World；协议失败或超时会释放进程，后续请求重新创建。附件位置查询目前仍是每请求一个子进程，不能把它与常驻渲染会话混为一谈。

### 原生适配器：native-adapter/

`src/main.rs` 定义 `FormulaWorld`，向 Typst 提供内存主文档、标准库和随附 New Computer Modern Math 字体。它不提供其他文件、包或日期。

无 `--server` 参数时，适配器求值附件请求，从文档内容中提取公式并保留外层样式，读取数学 IR 中最外层 `ScriptsItem` 的位置。返回 `limits`、`scripts` 或 null，不判断算子名字，也不返回中心 SVG。左附件或不能对应单个编辑槽的合并结果会报错。

`--server` 模式持续调用 `src/render.rs`：

1. 检查 Raw 区间是否为有效、互不重叠的 UTF-8 数学语法节点区间。
2. 在编译副本中为 Raw 添加带标签的嵌套公式，为结构公式添加范围标签；保存与导出的源码不变。
3. 更新内存 Source，调用 Typst 编译完整文档。
4. 遍历页面 frame，通过标签找到 Raw 的最终排版片段，并结合所在结构公式与出现次数生成返回 ID。
5. 导出该片段的 SVG，同时返回宽、高、基线、页码和位置；空内容也保留可提取的 frame。

编译单位始终是整份文档，Raw 列表只决定提取哪些片段。因此上下文或其他公式中的编译错误也可能使新 SVG 生成失败；已经缓存的成功 SVG 仍可继续显示。

### 透明标签桥接：prepare.ps1、engine-patches.json、layout-entry.rs

`prepare.ps1` 在共享的 `prototypes/typst-engine` 准备并校验固定 revision，移除旧桥接，再按明确锚点应用 `engine-patches.json`。重复运行不会重复插入补丁；维护补丁应修改这些受版本控制的文件，不能只修改共享引擎目录。

补丁把映射标签从内容实现阶段带入数学 IR 的属性中，同时保留原来的数学对象类型。字形仍可参与原生 stretch、上下标和字体度量；到生成最终 frame 时，`layout-entry.rs` 才附加可提取的标签。

这里没有隔离 Raw 的 box，也没有按 stretch 名称判断或单独编译箭头的分支。回归用例对照提取结果与无标签文档：短／长上标的箭头宽度分别为 24pt／144.6648pt。

## 接口约定与缓存

### Document 响应中的关键字段

| 字段 | 用途 |
|---|---|
| `source`、`contexts` | 完整 Typst 源码及可编辑上下文段 |
| `blocks`、`active_formula` | 全部公式的视图、类型、词法前缀和源码，以及当前操作目标 |
| `view`、`cursor`、`pending` | 活动公式的视图、光标和未确认命令状态 |
| `definitions`、`formula_definitions` | 宏管理器中的全局定义；活动公式实际使用的完整词法前缀 |
| `render.source` | 此次映射快照使用的完整源码 |
| `render.raw` | Raw 源码区间，每项为 id／start／end，位置使用 UTF-8 字节 |
| `render.formulas` | 结构公式的源码区间，用于区分宏等内容的渲染出现位置 |
| Raw 视图的 `render_id` | 区间身份、公式索引与出现序号，关联对应编译快照的 SVG |

`render_id` 是编译快照内的定位标识，不是 SVG 长期缓存键。前端发请求时保存目标 ID 与缓存记录对象的关系，返回后确认该记录仍然有效，避免插入公式、源码偏移变化或刷新后旧响应覆盖新结果。

### SVG 与附件缓存

| 缓存／操作 | 规则 |
|---|---|
| Raw SVG | 只按 Raw 源码文本缓存；相同文本共享首个成功结果，跨源码偏移和上下文变化保留 |
| 附件位置 | 按词法前缀、公式类型、完整上下标分支源码缓存 |
| 普通输入或光标移动 | 不使已有 Raw SVG 失效；新 Raw 或改写为未缓存文本的 Raw 可以请求编译 |
| 更新全部 SVG | 清除两类缓存并重新请求；未确认命令存在时等待确认或取消 |
| 上下标编辑后退出／失焦 | 比较附件内容是否实际变化；变化后失效中心项内的 Raw 和相关附件位置 |
| 上下标互相切换 | 保持同一次编辑会话，不在切换时立即刷新中心 |
| 编译失败 | 保留源码和错误；明确确认源码或全局刷新可重试，避免重绘时反复请求 |

这是一套允许预览滞后的编辑器策略。同样 Raw 即使处于不同样式或附件中也共享 SVG；按需刷新提高操作稳定性，不承诺每个出现位置一直与当前最终排版一致。

## 配置、构建与测试

`config/symbols.json` 经 `build.rs` 校验并生成 Rust 字典，编译进 WASM。它只影响显示和解析回退，不参与任意源码替换，也不把普通输入的多个字符自动合成符号。

| 构建入口 | 产物／作用 |
|---|---|
| `build.cmd` | release WASM，复制到 `web/core.wasm` |
| `build-native.cmd` | 准备共享引擎，构建 `target/adapter/debug/` 中的适配器 |
| `start.cmd` | 补建缺失 WASM，构建适配器，再用 cargo run 启动本地服务 |
| `build-release.cmd` | 构建 WASM、`target/adapter/release/` 适配器及 `target/release/` 服务 |
| `start-release.cmd` | 检查既有 release 产物并启动，不执行构建或下载 |

测试按模块组织：

| 文件／入口 | 主要验证 |
|---|---|
| `tests/lyx_traces.rs`、`structured_input.rs` | 编辑路径、结构输入、导航和删除 |
| `tests/command_mode.rs`、`source_modes.rs`、`failed_block.rs` | 命令、字符串、源码转换和失败块重新编辑 |
| `tests/document.rs` | 多公式、独立光标、上下文拆分、文档撤销及 Raw 区间 |
| `tests/attachments.rs` | 视图产生的附件请求源码与适用范围 |
| `tests/wasm-command.mjs` | 真实 WASM JSON 接口、宏与编辑行为 |
| `tests/preview-cache.mjs` | 真实 WASM 状态配合前端逻辑；模拟 API 验证缓存、失焦刷新和旧响应竞争 |
| 原生适配器内部测试 | 真实 Typst 编译、源码映射、脚本字号、上下文、空 SVG、错误恢复及 stretch 对照 |
| `tests/services.rs` | 本机进程集成；默认忽略，运行需要已构建适配器，补全用例还需要 Tinymist |
| `tests/math-font.mjs` | 数学字体显示映射 |
| `tests/layout-fixture.mjs` | 生成 `target/layout-preview.html` 供人工检查；SVG 是占位图，不验证编译器结果 |

Node 中使用 WASM 的测试需要 `web/core.wasm` 与核心源码同步。本机服务集成测试可通过 `cargo test --offline --locked --test services -- --ignored` 单独运行；它直接调用子进程，不启动 HTTP 服务。

## 修改功能时从哪里开始

| 想修改的功能 | 首先查看 |
|---|---|
| 新增结构类型或改变槽位语义 | math.rs → cursor.rs → typst.rs → view.rs → 前端 draw／CSS |
| 调整多公式、上下文或全局撤销 | document.rs → app.js 的文档事件与 renderDocument |
| 扩展宏支持或改变可展判定 | typst.rs；共享实参与显示位置再看 view.rs |
| 调整 SVG 缓存与触发规则 | app.js 的 previews／attachmentEdits，配合 preview-cache.mjs |
| 改变 Raw 的映射或编译结果 | document.rs 的 annotate → render.rs → 引擎桥接补丁 |
| 修改上下标位置策略或视觉布局 | 适配器 main.rs 的位置读取 → view.rs 的请求条件 → app.js／style.css |
| 更换补全后端或改善进程管理 | services.rs；HTTP 接口变化再看 main.rs |
