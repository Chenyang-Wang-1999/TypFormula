#set heading(numbering: "1.")

当前的前后端职责划分不合理。后端持有了一些与编辑有关的内容，而前端又需要频繁和渲染器通信。应该重新设计。

= 梳理现有前后端职责

== 名词与进程

- *前端*：`desktop/` 的 PyQt5 窗口（`window.py` 的 `Window`、`editor.py` 的 `Editor`、`mathview.py` 的 `Typesetter` / `FormulaObject` / `MathCanvas`）。一个窗口一个 `Window`，可分成两栏，两栏共享同一份源码。
- *后端*：Rust host 进程，一个窗口三个，都由 `desktop/bridge.py` 启动：
  - `--desktop-core`（`bridge.Core`）：`src/document.rs` 的 `Document` + `src/desktop.rs` 的协议 + 内核 `crates/core`。文档级请求与*唯一一个*活动公式会话都在这里。
  - `--stdio`（`bridge.Services`，窗口里叫 `self.services`）：`src/rpc.rs` 路由到 `services.rs` / `packages.rs`，即 `/api/render`、`/api/preview`、`/api/pdf`、`/api/attachments`、`/api/glyphs`、`/api/packages`。
  - `--stdio`（窗口里叫 `self.lsp`）：`/api/lsp`、`/api/completion`、`/api/preview/live`。
  - 分成两条服务管道只有一个理由：语言服务与实时预览不要排在取图后面。`rpc.rs` 里每请求一线程，但前端 `Services` 自己一次只发一条。
- *引擎*：host 按需再起的子进程——取图 / 整页 / PDF 走一个常驻 `render_adapter`，附件位置与字形走另一个常驻 `math_adapter`，外加一个 Tinymist LSP 会话（语言方法与实时预览共用）。前端不直接跟适配器说话。
- *内核*：`crates/core`（`typformula-core`）。host 依赖它，它不依赖 host，这条由 cargo 强制。

同一份文档在四处存在不同形态：前端 `Window.source`（*权威*）、Qt `QTextDocument`（显示文本，每个公式是一个替换字符）、host `Document.source` 加 `typst_syntax::Source`（镜像 + 增量语法树）、活动会话的 `Editor.root`（可编辑树）。它们靠回包里的 `source` / `revision` / `reparsed_range` 对齐。

== 一次按键经过什么

正文里按一个字符（`editor.py::Editor.changed` → `window.py::Window.replace` → `Window.update_analysis`）：

+ 前端把 Qt 显示文本的差异换算成源码字节区间（`model.difference_at_caret` / `to_byte`），先改自己的 `Window.source`，压一次撤销栈；
+ `edit_source` 给后端：`Document.source.replace_range` 加 `syntax.edit(...)`，回包带 `reparsed_range`；
+ `scan` 再全篇轻量扫一遍（公式表，以及 `let` / 注释 / 标题等 `styles`）；
+ 前端用 `incremental.py::merge` 自己判定哪些公式失效，逐个 `analyze_formula`（失败则退回整篇 `analyze`）；
+ 160 ms 后 `load_raw` 把「当前可见公式的 Raw 片段」合成*一次* `/api/render`；
+ 400 ms 后 `/api/lsp` 取诊断，550 ms 后 `background()` 逐公式取附件位置与语义高亮。

公式里按一个字符（`mathview.py::MathCanvas.keyPressEvent`）：

+ 前端只做三件事：Qt 键名翻译成内核的 `key` 名、Ctrl+C/V/X 走剪贴板、Esc 退出；其余一律 `math_action(...)` 转发；
+ 后端 `Editor::apply(Action)` 执行编辑动作，回包一整棵新的显示树 `view`，加 `cursor` / `pending` / `command` / `candidates` / `message`；
+ 前端用 `Typesetter.layout` 把这棵树排成 Box（尺寸全由 Qt 文字度量算出），再把量出来的 stop 坐标用 `geometry` 动作回传；
+ 后端存进 `Editor.geometry`，上下键找落点时读它（`cursor.rs::move_vertical`）。

所以一次公式内按键是*两次往返*（动作 + `geometry`），而且*后端的编辑决定依赖前端的测量*。

== 逐项对账

=== 编辑
#table(
  columns: (1.6fr, 3fr, 3fr),
  [*事项*], [*后端*], [*前端*],
  [增删改], [内核 `Action` 的全部变体：`input` / `key` / `paste` / `insert` / `complete` / `click` / `edit_source` / 增删行列 …], [把 Qt 事件翻译成动作名并转发；`MathCanvas` 是近乎纯粹的转发器],
  [视图定位], [只把 `geometry` 当提示（`cursor.rs` 注释：可过期，绝不是光标来源）], [命中测试与坐标：正文按 Qt 光标矩形与公式对象求交，公式框按 `(dx² + 4·dy²)` 找最近 stop],
  [光标跳转], [规则与实现都在 `editing.rs` / `cursor.rs`：进入点、左右、上下、可达性、Home/End/Tab、按数学类跳词], [只提供 stop 的实测坐标；纵向落点的 x 来自前端],
  [文本回写], [结构变化后立刻序列化并写回活动区间，新源码随回包给前端], [公式外的正文回写由前端发起（`edit_source`）；采纳新源码、记撤销、重建投影],
  [选区], [公式内的选区（`anchor` / `selection`），剪贴板文本由回包 `selected_source` 给], [文档级就是 Qt 自己的选区；复制从 `Window.source` 取值],
  [撤销], [公式会话内一份栈（上限 200，连续输入合并）], [文档级一份栈（上限 500，含选区）；Qt 自身的撤销被显式关掉],
  [命令草稿], [`pending` / `command` / `candidates` 都在 `Editor`；草稿不写回源码；草稿未确认时其它动作一律被拒], [画候选列表，把 LSP 候选回传（`lsp_completions`）],
)

前端独有的两处「控件级」判断（后端没有对应物）：

- *宏定义折叠块*：`definitions.py::blocks` 拿后端 `styles` 里的 `let` 区间自己合成控件，连 `definition_block` / `_object_id` 这些标记也是前端造的；草稿确认后当作一次普通文档替换写回。
- *列表工具栏可见性*：`mathview.LIST_KINDS = ("table", "multiline")` 是前端自己的白名单；后端那份判据是「不在矩阵或对齐公式的格子里就拒绝增删」（`cursor.rs::grow_grid` / `shrink_grid`）。两份判据指的是同一件事，但键不同：前端按*线上名字*（`table` / `multiline`），后端按存储 `Kind`（`Table` / `Multiline`），两者的对应写在 `view_atom` 里。

=== 解析
#table(
  columns: (1.6fr, 3fr, 3fr),
  [*事项*], [*后端*], [*前端*],
  [语法与解析器管理], [host `Document` 常驻 `typst_syntax::Source`；每次编辑 `Source::edit` 增量重解析，实际范围记进 `last_reparsed` 并随回包给出], [不做任何 Typst 解析],
  [上下文管理], [内核 `context.rs` 把文档归约成「这段源码还看得见的东西」；host 在三处调用：激活公式、取图批次、附件请求], [只把回包里的 `context` 摘要当缓存键],
  [语法结点类型的抽象], [内核三张表加配置：`math.rs`（可编辑树）、`slots.rs`（形状 / 槽位 / 拼写）、`editing.rs`（编辑规则），键是形状名；`config/*.json` 由 `build.rs` 编译期嵌入], [另一份*名字*白名单：`ARRANGEMENTS`（排布名）、`MARKERS`（画法名）；遇到不认识的名字只报告一次并退回横排],
  [管理用户规则], [`typst.rs` 的宏注册表：按归约后的定义文本缓存、进程内共享、按作用域收集 `let`、判定可展 / 不可展], [只负责定义块的显示与草稿确认],
  [源码 → 树], [`parse_formula_node_with` 复用 host 语法树里的 `SyntaxNode`（不再切成字符串二次解析）], [不参与],
  [哪些公式要重新投影], [只给事实：`scan` 的公式表、`edit_source` 的 `reparsed_range`、变更后的 `styles`], [*判定在前端*：`incremental.py::merge` 按重解析范围、公式源码是否变化、`let` 绑定文本是否变化、`#[]{}` 是否改动四条自己算 `rebuild`],
  [取图用源码区间], [`document.rs::annotate` 加 `Locator` 给每个 Raw 算区间与 `render_id` / `render_request`], [按 `render_id` 字符串切分来做诊断对齐与上下文分组],
  [高亮与诊断], [给数据：`scan_syntax` 的 `styles`、Tinymist 诊断的转发], [绘制：`ExtraSelection` / `QTextCharFormat`],
)

=== 渲染
- *内置渲染*：前端。`mathview.Typesetter` 把后端的显示树排成 `Box` 并用 `QPainter` 画：尺寸、基线、脚标移位、空槽、失败框全在前端。后端只给结构与名字（`kind` / `role` / `marker` / `columns` / `row_lengths` / `border` / `is_mat` / `display_glyph`），`View` 里*没有任何尺寸或坐标字段*。
- *Typst 自动渲染*：引擎。取图 `/api/render`（标记区间、编译、按组切 SVG）、整页 SVG `/api/preview`、PDF `/api/pdf`、附件位置 `/api/attachments`、字形替换 `/api/glyphs`。前端拿到 SVG 文本与盒子度量，自己栅格化（`BitmapCache`，64 MiB LRU）。
- *实时预览*：Tinymist 自己的进程与 WebSocket，前端只把页面装进 `QWebEngineView`，地址由 host 从 LSP 回包里取。
- *缓存机制*：主体在前端。后端只有三样：内核宏注册表（进程级，多窗口共享）、host 的公式索引（按语法树代次失效）、适配器的常驻 world。
- *跨层常量*：引擎量出来的东西在前端硬编码或一次性抄写——脚标 .36 / .25 em、`cancel` 的盒子不变高、字体变体的码位表只能问引擎、环境字号靠引擎补丁在标签尾上传回。内核 `Slot::scale`（每槽千分比）声明了但*无人读*，前端各排布分支另写 ×0.9 / ×0.7 等比例。

前端缓存（按"谁失效"归类）：

#table(
  columns: (1.8fr, 3fr, 2fr),
  [*缓存*], [*键*], [*谁失效*],
  [Raw 片段图 / 被拒], [`raw_key(node)`：`("raw", text[, 脚本形状摘要])` 或 `("raw-instance", text, origin, 调用身份, occurrence, context)`], [前端：revision、定义文本变化、上下文不再活跃、显式刷新],
  [脚本编辑会话], [一个 `scripts` 节点里基底片段 key 的元组], [前端：脚本形状摘要变了*且*光标离开该脚本才丢弃],
  [拒绝与重试], [`Window.raw_error` / `Window.retry`，按片段 key], [前端：新 revision 丢拒绝；进入失败框或确认草稿放行一次],
  [字形], [`("", 调用拼写, display)`], [三态：缺省 / `None` 在途 / `False` 失败 / `""` 成功空串],
  [附件位置], [`(definitions, expression, display)`], [前端],
  [公式 Box], [`id(view)` 加 `_draw_revision` 与 `typesetter.version`], [前端],
  [位图], [SVG 摘要加目标尺寸], [前端],
  [公式视图合并], [公式区间], [前端（按源码 / 定义 / 作用域判据）],
  [片段上下文], [`render_id` 前缀 → 脚本形状摘要], [前端：analysis 换掉即重建],
)

`context::digest` 是唯一一条后端算、前端直接当键的东西。

=== 时序
- *渲染时机*：全在前端。四个单次定时器（取图 160 ms、命令补全 300 ms、诊断 400 ms、附件与高亮 550 ms），加滚动触发、进入公式触发；同键请求在 `Services` 队列里合并（`raw-batch` / `glyphs-batch` / `diagnostics` / `preview`）。
- *取哪一片*：前端决定——只取可见公式的片段、把 `context_end` 截到最后需要的那个公式、定义体里的片段改成整篇。
- *编辑-回写时机*：前端决定何时发；后端决定何时写回（树变了就立刻序列化并写回活动区间）。命令草稿与宏定义草稿都不写回。
- *后端没有 debounce，也没有批处理*：`--desktop-core` 是一行进一行出的阻塞循环；`--stdio` 虽每请求一线程，但前端一次只发一条。
- *失败与重试*：两端各一半。前端管「被拒的片段只在下一次编辑或一次刻意动作后重试」，后端管「合成上下文编译不过就退回整段前缀」「整批失败就折半重试」。
- *重启与重放*：只有前端有。`bridge.Core.restore` 重启核心后按 `set_source` → `activate_formula` → 重放已确认会话动作的顺序恢复，并核对最终源码；后端失败时只是丢掉热进程（Tinymist 会话、适配器）。
- *超时*：前端 `Core` 10 s / 60 s 两档、`Services` 65 s；后端 LSP 与适配器 30 s。

== 跨边界传的是什么

`--desktop-core`（一行一个 JSON，回 `{"result": …}` 或 `{"error": …}`）：

+ 文档级：`set_source`、`edit_source{start,end,text}`、`scan`、`analyze`、`analyze_formula{start}`
+ 会话级：`activate_formula{start}`、`deactivate_formula`、`state`
+ 动作级：内核 `Action` 的全部变体（serde 打平成 `{"action": …}`）：`input` / `key{key,shift,ctrl}` / `paste` / `insert` / `complete` / `click{cursor,shift}` / `edit_source{cursor,source}` / `geometry{stops}` / `undo` / `redo` / `clear` / `add_row` 等四个结构动作 / `preview_result(s)` / `lsp_completions` / `set_display` / `set_definitions` / `import`

`--stdio`（`{"id","route","body"}` → `{"id","result"|"error"}`）：`/api/status`、`/api/render`、`/api/preview`、`/api/pdf`、`/api/attachments`、`/api/glyphs`、`/api/completion`、`/api/lsp`、`/api/preview/live`、`/api/packages`。

回包里的*名字契约*（前端按字符串认，编译器管不到）：`view.kind`（排布名）、`marker`（画法名）、`role`、`columns` / `row_lengths` / `border` / `is_mat`、`render_id` 的 `"start:end[:call:call]:occurrence"` 字符串格式、`context` 摘要。

== 现状与「高内聚、低耦合」的差距

开头那句判断成立，而且两头都能指出具体位置。以下是第一步的结论，供第 2 节重新划分时对照：

+ *编辑动作整体在后端*：内核 `cursor.rs` 是编辑模型本身（约一千行，`Action` 二十多个变体，含导航、选区、草稿、历史、结构编辑），前端 `MathCanvas` 只是转发器。这与"前端主要负责编辑"正好相反。
+ *后端的编辑反过来依赖前端的测量*：`Action::Geometry` 是唯一一批"前端 → 后端"的数据，`move_vertical` 的落点 x 完全来自它。后端无法独立决定纵向导航。
+ *前端镜像后端规则*，至少八处：`incremental.py::merge`（失效判定）、`Window.glyph_expressions`（镜像 `desktop::style_expressions`）、`formula_error` 样式两端各合成一次、`mathview.LIST_KINDS`、`ARRANGEMENTS`、`MARKERS`、`rawcache.raw_key` / `signature`（后端节点的身份在前端重新定义）、`definitions.py::blocks`（用后端区间重新合成定义块）。
+ *同一份文档四种形态、撤销两份栈、公式身份两套*：后端 `render_id` 与前端 `_object_id` / `_raw_key` 各自编号，靠区间与字符串格式对齐。
+ *往返次数与时序都偏多*：正文一次按键至少两次核心往返（`edit_source` + `scan`），公式内一次按键两次（动作 + `geometry`），再加三类异步服务请求；节拍全部由前端定时器给。
+ *缓存与失效策略的主体在前端*（上表九类），后端只提供键与事实。
+ *渲染数据分家*：后端给结构名字，前端给几何、绘制与引擎量测常量；`Slot::scale` 这类声明在中间落了空。
+ *控件级概念只在前端存在*：宏定义折叠块、列表工具栏可见性、消息面板、两个并存的预览路径；后端不知道它们存在。

== 文档中与代码不符的描述（本次已修正）

#table(
  columns: (1.6fr, 2.6fr, 3fr, 1.4fr),
  [*位置*], [*原文*], [*实际*], [*处置*],
  [`docs/architecture.md`（增删行列那段）], ["判据本身仍只有核心有"], [前端另有一份列表判据 `mathview.LIST_KINDS`（只管工具栏可见性），后端那份是"不在格子里就拒绝"], [已改，写明两份判据与各自用途],
  [`docs/desktop.md`（列表工具栏那行）], ["所以不另立一份『什么是列表』的判断"], [同上：节点种类来自后端显示树，但"哪些种类算列表"是前端写下的名字白名单], [已改],
  [`docs/architecture.md`（后端那节）], ["整页编译不会把公式取图和语言请求排在它后面"], [语言请求确实不受影响（另一条管道）；但 `/api/preview`、`/api/pdf` 与 `/api/render` 共用 `render_adapter`（一把锁、一个常驻进程），在前端又是同一条 `Services` 队列，所以整页编译*会*把这期间的取图排在后面], [已改],
  [`AGENTS.md`（`services.rs` 那行）], ["`ask_adapter` 是三条适配器请求的公共入口"], [`ask_adapter` 只接附件与字形两条；取图自己拿 `render_adapter`], [已改],
  [`AGENTS.md`（`window.py` 那行）], [未提文档所有权], [权威源码、撤销 / 重做栈、脏标记都在窗口], [已补],
)

核对无误、因此不改的描述（本次逐条对过代码）：内核不依赖外围（cargo 强制）、只有 `native-adapter` 链接 Typst 编译器、每窗口一核心两服务、host 不监听端口、位置映射的三套编码（Python Unicode / Rust UTF-8 / Qt 与 LSP UTF-16）、命令草稿不写回源码、`View` 不含尺寸与坐标。若后续代码与本文冲突，以代码为准并回来改这份梳理。

另记三处*本次未改*的过期说法（都不属于"前后端职责"，但读文档时会撞上）：

- `crates/core/src/slots.rs` 的注释提到并不存在的 `view::ViewTemplate::Deferred`（延迟判定现在由 `Projector::material` 的重新投影实现）。
- `crates/core/src/context.rs` 的注释把 host 的 `Document` 说成内核的（`Document` 在 `src/document.rs`）。
- `docs/kind-inventory.md` 第 12 条仍按 `Accent` / `Line` 两个 `Kind` 说后端产生什么（它们早已从 `Kind` 里删掉，现在是 `decoration` / `line` 两个形状）；这份清单需要整篇重核，不是改一行的事。

= 重新划分职责
原则：
- 后端主要负责持有源文档与语法树。
- 前端主要负责编辑与控件绘制。

= 构建最小可行案例
- 先确定接口规则。可以考虑将前端可编辑可显示的对象抽象成控件，与后端交互。
- 只保留最基本的编辑功能，去掉如实时预览等辅助功能，跑通前后端：
  - 按接口规则写一个最小前端，用合成数据测试它能否正确通过接口收发消息，并与现有程序对比。
  - 然后，同样是按接口规则写一个最小后端。
  - 当上述两个步骤受阻时，优化接口规则，直至方案收敛。

= 正式迁移
- 在最小可行案例跑通后，将全部测试套按功能重新分组到前端和后端。
