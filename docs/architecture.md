# 正式版架构

## 原生桌面前端

`desktop/` 是 Qt Widgets / PyQt5 原生前端，通过 `--desktop-core` 管道使用 Rust Document，通过独立 `--stdio` 管道使用编译/包服务与 Tinymist。没有 WebView 或 HTTP 端口。完整源码及文档撤销由窗口持有，QTextDocument 只是带自定义公式对象的投影；位置映射显式转换 Python Unicode、Rust UTF-8 和 Qt/LSP UTF-16。一个窗口只保留一个活动公式会话和绘图控件，分栏共享源码。

Rust Document 常驻 `typst_syntax::Source`。源码编辑调用 `Source::edit`，使用 Typst 增量解析器返回的实际重解析范围；桌面公式索引只扫描更新后的轻量语法节点，平移范围外已有投影，只为重解析范围相交的新建/改变公式构造 View。`let` 变化会使其后的宏投影失效。此策略沿用 Typst 保持远处 span 稳定的增量解析边界，而不是按输入字符猜测影响范围。参见 [Typst 编译器架构](https://github.com/typst/typst/blob/main/docs/dev/architecture.md) 与 [comemo](https://github.com/typst/comemo)。

桌面端不包含实时页面预览。F5 通过原生 `typst-pdf` 编译当前内存源码并交给系统默认阅读器；公式 Raw、宏预热和附件位置继续使用独立后台服务，不依赖 PDF 编译。详情及范围见 [desktop.md](desktop.md)。下面保留 Web/VS Code 前端的架构说明。

## 状态所有权

下表描述独立版本。VS Code 扩展沿用公式核心与视图，但文档权威状态和撤销交给 VS Code TextDocument，详见下文。

| 层 | 保存什么 | 不保存什么 |
| --- | --- | --- |
| CodeMirror EditorState | 完整源码、选区、文件撤销历史 | 逐公式 Editor |
| Rust Document | 完整源码镜像、活动公式字节区间、全局 Editor | contexts / Vec<Editor> |
| Rust Editor | 活动公式的 MathData、槽位光标、草稿和测量 | 其他公式的状态 |
| Decoration / Widget | 区间和静态显示；活动控件复用同一个 DOM host | 文档状态 |
| 本地 Services | Tinymist 文档会话、独立公式补全、常驻 Typst 渲染器 | 正文编辑状态 |

正常代码输入由 CodeMirror 事务提交，WASM `set_source` 无损保存源码并用 Typst AST 扫描公式区间。扫描忽略注释、字符串和 Raw 中的 `$`。源码有错也允许保留。按钮调用 `activate_formula(start)`，只解析活动范围并读取前缀中的宏定义。

宏绑定以 Typst 语法树中的作用域边界为依据：只收集活动公式祖先作用域中已经出现的 `let`，跳过已经结束的兄弟内容块/代码块；函数自身和形参遮蔽外层绑定。绑定表按顺序更新，模板依赖边仍引用不可变的定义版本，因此后续同名定义不会改变早先宏的捕获。当前仅支持 `let` 的静态结构展开，不尝试执行任意导入、循环或动态函数来推导展开结构。

Web 在源码变化后防抖调用 `/api/prewarm`。后端为可展开定义创建独立源码投影，在定义之后显式插入 `#name("", …)`，补齐祖先括号后交给原生引擎；编辑文档、磁盘与撤销历史均不参与这个过程。固定 Raw 按定义版本和源码区间分别映射 SVG，后端缓存还区分文件路径与未保存依赖。超预算、编译失败或缺失固定 Raw 排版结果会降级为普通调用；定义/依赖版本改变后重新预热。实际调用的排版结果优先于模板缓存，命令草稿期间暂缓应用降级。

结构修改通过唯一 Editor 执行；若序列化结果实际改变，Document 仅替换活动范围。JS 将这次替换提交为 CodeMirror 事务，标记为公式来源，避免再次 `set_source` 清空活动树。源码进入和退出不做隐式格式化。撤销和重做统一回到 CodeMirror，随后重建源码镜像。隐藏公式只保存 DOM 投影，不保存单独的光标和撤销栈。

Rust / Typst 字节区间使用 UTF-8；CodeMirror 字符位置与 LSP character 使用 UTF-16。所有跨层转换集中于 `web/source.js` 和 `src/services.rs`。LSP 返回替换范围后检查边界与重叠；异步请求返回时核对源码和文件身份，避免过期结果写入当前文档。

## VS Code 扩展

`extensions/vscode/src/extension.cjs` 注册 `CustomTextEditorProvider`，将同一前端置于 Webview。扩展通过 `WorkspaceEdit` 更新 TextDocument；`web/document-sync.js` 只允许一个在途提交，连续输入在确认后继续提交。自己的文档回传不会重建活动公式；来自另一编辑器的修改在无本地未确认输入时同步，否则保留输入并进入显式冲突处理。扩展模式没有第二份 CodeMirror 撤销栈。

文件保存、脏标记、撤销、文件树和 Git 使用 VS Code 能力。`web/transport.js` 在独立版本使用 HTTP，在扩展中使用 request/reply 消息。扩展宿主启动随包的后端 `--stdio <workspace>`，`src/rpc.rs` 转发已有原生服务；扩展不监听网络端口。所有资源经 asWebviewUri 加载，前端不引用原型或仓库绝对路径。

`config/editor-commands.json` 是按钮与命令的统一登记表。构建脚本生成扩展 commands/keybindings；所有按钮都可通过 VS Code 原生快捷键配置改键。独立版本提供本地快捷键配置对话框。`web/navigation.js` 管理公式外方向键选择目标和进入槽位：水平边界进入根槽首/尾，垂直进入公式顶/底行中最接近当前 x 坐标的位置。

公式内容采用 max-content 自然尺寸，不参与 flex 压缩；外层 Widget 是有宽高上限的滚动视口。`web/formula-layout.js` 根据编辑列宽度更新上限，输入时仅滚动公式框以露出当前槽位。手动滚动时重测几何，固定定位光标裁剪在公式视口内。结构上下标字号按基准字号设置下限，不再无限累计 0.7em 缩小；Raw SVG 的度量已含 Typst 的上下标字号，前端按公式基准单位显示，避免再乘一遍父槽字号。

Web 版显示设置由 `web/typography.js` 管理。Ctrl+滚轮及工具栏只改变编辑字号，不改 Typst 源码。每个 Raw 的图片尺寸由后端的归一化比值决定，已经移除手填全局文章字号。

在数学 IR 解析 Raw 标记的边界，使用传入 StyleChain 临时以 MathSize::Text 解析 TextElem::size，只排除上下标字号因子；不修改排版所用样式。此时读取的环境字号不受 Raw 内部显式 text 样式影响，局部环境的绝对／相对字号仍被保留。基准值放入编辑器私有 frame 标签的 `:base-font-pt:` 后缀，沿现有透明标签桥传至 SVG 提取，不新增布局盒子。源码中的标签不变。

返回字段中，`environment_font_size_pt` 是实际环境字号；`base_font_size_pt`、`base_font_height_pt`、`base_font_baseline_pt` 分别为原始宽／高／基线除以环境字号。按用户指定的兼容命名，这三个 `_pt` 字段实际是无量纲比值。前端用 `编辑字号(px) × 对应比值 × 微调` 得到像素尺寸；Raw 内部的上下标和显式局部放大仍体现在 SVG 中。

Raw 缓存按进入会话和出现位置区分，不再让相同源码的不同出现位置共用首个 SVG。静态公式投影的缓存键也包含文件和环境前缀。不同位置可以保留各自的原始尺寸及归一化分母；已有成功结果在当前会话内仍沿用按需刷新策略。

`web/file-access.js` 使用 File System Access API 获取用户明确授权的任意文件句柄。打开和另存为不受服务工作区路径限制；写入通过临时 writable 完成。浏览器不暴露绝对路径文本。缺少该 API 时使用 file input 和下载回退；项目内文件继续走后端的乐观并发保存。

扩展优先桥接 Tinymist 注册的 VS Code 语言提供器；未安装 Tinymist 扩展时使用已有原生 LSP 桥。临时公式命令投影仍交给独立补全会话。

## 整页预览

`web/preview.js` 防抖请求当前源码，返回后检查请求代次，旧结果不覆盖新编辑。以 SVG 图片显示整页，失败保留上次成功页面并显示错误。图片 Blob URL 在替换和关闭时释放。

原生 `RenderRequest.preview` 为 true 时，直接编译原文，不插入结构编辑器的 24pt 字号或映射标签。FontStore 加载随附字体及系统字体；日期由系统提供。扩展把已打开的项目内文档作为 overlays 传入，未保存的 import/include 依赖也参与预览。页面尺寸、字体和正文样式由原文决定。

## 后端

本地 HTTP 只绑定回环地址，并检查 Host、Origin 和 POST JSON 类型。静态资源固定白名单。项目目录由 `VISUAL_TYPST_WORKSPACE` 指定，默认 `workspace/`；文件接口使用规范化路径限制项目边界，保存时对照打开时的磁盘内容，避免静默覆盖外部修改。

`/api/lsp` 提供 completion / hover / definition / formatting / diagnostics。一个文档使用一个常驻 Tinymist 进程，版本递增并全文同步；换文件或协议失败时重建。诊断来自 publishDiagnostics，前端防抖、版本检查和过期结果丢弃。公式补全仍使用隔离的临时源码投影，以保留原型已验证的命令行为。

`/api/packages` 从官方索引查版本，安装精确版本到标准缓存。下载有超时与大小限制；包解压仅接收普通文件和目录，拒绝链接、越界和过大归档。先解压到临时目录并校验 manifest 存在，再重命名发布缓存，避免半安装状态。

`/api/render` 和 `/api/attachments` 使用正式版自己的 native-adapter。数学 IR 标签桥接保存在 `vendor/typst`；构建不再准备原型引擎或修改其他目录。源码、资源与缓存失败不影响代码区继续输入。

## 当前边界

- 公式静态投影在按钮或方向键进入后缓存；源码输入 `$` 不自动变成控件。
- 宏的结构展开仍沿用受限静态分析，任意 Typst 求值由引擎处理。
- Raw SVG / 附件位置沿用按需刷新策略；不保证任何源码修改后都自动刷新全部数学投影。
- 代码高亮为轻量 Typst token 高亮，诊断和语义查询由 Tinymist 提供。
- 独立版 F12 可打开项目内目标；扩展版交给 VS Code 打开定义文件。
- 当前预览没有到源码的双向定位，尚无 PDF 导出按钮或包卸载。扩展版文件管理使用 VS Code；独立版仍不提供目录创建/重命名。
