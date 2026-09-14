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
原则（不变）：
- 后端主要负责持有源文档与语法树。
- 前端主要负责编辑与控件绘制。

== 一句话分工与判据

- *后端 = 文档与结构的服务*：持有源码、增量语法树、宏与上下文、可编辑树与显示树；*编排渲染*（决定要哪些图、按什么身份缓存、什么时候失效）；*转发外部服务*（Tinymist、包、PDF）。它只回答关于文档的问题、只执行改变文档的意图，*不持有会话*。
- *前端 = 会话与交互的客户端*：持有活动公式、光标路径、选区、命令草稿、撤销栈、几何测量与可见性；负责键盘与鼠标、命中测试、排版与绘制，以及全部控件（源码栏、定义块、工具栏、消息面板、预览壳）。
- 判据一句话：*这个状态描述的是文档，还是描述某个人此刻在看/在改这份文档？* 前者归后端，后者归前端。第 1 节列出的耦合几乎都能用这一句判出归属。

== 三条关键取舍

=== 取舍一：树不搬到前端，搬走的是会话

可编辑树、拼写（`write_atom`）、宏注册表、上下文归约、显示树（`view_atom`）是同一件事的几面，都要 `typst-syntax`。把树搬到 Python 只有两条路：把 `write_atom` / `view_atom` / 宏分析在 Python 里重写一遍（等于把内核的核心不变量复制一份，`tests/round_trip.rs` 与 `tests/stored_kinds.rs` 也要复制），或者每次编辑都回后端投影一次（省不下往返，还多一份树状态）。

所以：*树留在后端，搬走的是会话状态*——活动公式、光标、选区、草稿、历史、几何。直接后果：

- 后端从「一个有状态的编辑器」变成「文档 + 纯函数」：`activate_formula` / `deactivate_formula` / `state` 这些会话动作消失，「草稿未确认就拒绝一切」的规则消失；
- 前端 `bridge.Core.restore` 的*重启重放*整段删除：后端没有会话可恢复，重启后只需重发 `set_source` 与当前可见集合；
- 内核 `Editor` 的公开字段（`root` / `cursor` / `anchor` / `definitions` / `macro_text` / `registry` / `display`）已经够用来按请求构造一个临时 Editor，所以这条取舍是*改接线，不是改算法*。

=== 取舍二：走哪一格在后端，落在那一格的什么位置在前端

- 「上下左右走到哪一格」是形状声明的事（`editing.rs` 的规则表 + `slots.rs` 的角色），与树同源，留在后端；一次按键一次往返（今天是一次动作加一次 `geometry`，共两次）。
- 「落在那一格的哪个位置」是度量的事：后端只给*结构落点*（格首或格尾，按已有声明 `Landing` 与 `back_lands_at_end`），前端拿到这个光标后，在自己的 stop 几何里按同一个 x 就近微调——纯前端、无副作用，调整后的光标作为下一次请求的会话参数传回。
- 于是 `Action::Geometry` 与 `Editor.geometry` 这条唯一的「前端 → 后端」测量通道删除，后端不再依赖前端的排版结果。

=== 取舍三：渲染编排搬到后端，只有「看得见什么」留在前端

片段的身份、上下文的截断、批次合成、拒绝与重试、字形与附件的键，都是*文档的事实*——今天前端用 `rawcache.py`（`raw_key`、脚本形状摘要、调用实例）加上第 1 节列的九类缓存把它们重算了一遍。这些都搬到后端：它有计划（显示树与 `annotate`）、有上下文（`context.rs` 的归约）、也有适配器句柄。

- 「哪些公式现在在视口里」是 UI 事实，由前端在滚动、编辑、切换文档后告诉后端；
- 后端据此合并成一次编译，按 `image id` 回包；前端只保留 Qt 的位图 LRU（栅格化是控件的事）；
- 字形（字体变体）与附件位置的键也由后端给，前端不再自己拼 `("", 调用拼写, display)` 或 `(definitions, expression, display)`。

== 边界上的对象：公式控件

第 3 步说的「把前端可编辑可显示的对象抽象成控件」在边界上的形态是*数据*，不是对象：后端给一份控件数据，前端把它实现成控件。

```text
FormulaControl（后端给的数据）          前端实现
  id / range / display / editable        一个 QTextObject（静态投影），或唯一的活动控件
  view（显示树，含 role / marker /        排版与绘制（Typesetter）
        columns / is_list / metrics）
  images / glyphs / placements           画；位图 LRU
  session（仅被编辑时，前端持有）          光标、选区、草稿、撤销、命中测试
```

- 静态公式与活动公式是*同一种控件数据*，差别只是前者没有 `session`——正好对应今天「只有一个 `MathCanvas`，其余是静态对象」。
- 定义块是第二种控件：后端给「哪几段 `#let` 合成一块，每块的区间与原文」，前端只负责块内的草稿控件。

== 状态归属（新旧对照）

#table(
  columns: (1.5fr, 2.2fr, 2.6fr),
  [*状态*], [*现在*], [*新*],
  [源码], [前端权威 + 后端镜像], [不变；规则明确：每次变换的回复都带新源码，写盘只在前端],
  [增量语法树], [后端], [后端],
  [可编辑树 `MathData`], [后端，且是会话的一部分], [后端，*无会话*：按（区间，源码文本）记忆化，随用随建],
  [显示树 `View`], [后端], [后端，与变换同一次回复给出],
  [公式索引], [后端给事实、*前端判定失效*], [后端判定：回复里给整条公式数据，每条带 `id`（身份）与 `view_version`（视图内容变了才涨）——区间平移不换身份，前端因此不必计算平移],
  [光标 / 选区], [后端], [前端，随每次请求显式回传],
  [命令草稿], [后端 `pending`，未确认时拒绝一切动作], [前端；后端只答「候选」与「把这条命令填成槽位」],
  [撤销 / 重做], [两份栈（文档级前端、公式会话内后端）], [一份，全在前端],
  [几何测量], [前端算、回传后端], [前端独占；后端只给结构落点],
  [片段图与身份], [前端（`raw_key`、脚本摘要、调用实例）], [后端（`image id` 加 SVG 缓存）],
  [字形（字体变体）], [前端请求与三态缓存], [后端随公式与可见集合给出],
  [附件位置], [前端建键与缓存], [后端],
  [渲染批次与上下文截断], [前端（`load_raw`）], [后端],
  [可见性], [前端], [前端（唯一保留的渲染输入）],
  [语言服务位置换算], [前端（`lsp_position`）], [后端（前端只发字节偏移）],
  [定义块分组与可展判定], [后端给区间、前端分组], [后端分组，前端只做控件],
  [「是不是列表」], [前端白名单加后端合法性，两份], [后端在 `View` 上标 `is_list`，前端只读],
  [排版与绘制], [前端], [前端],
  [控件、窗口、文件、设置], [前端], [前端],
)

== 接口规则（第 3 步要落进协议的部分）

+ *R1 无会话*：后端进程内不保存「当前公式、光标、草稿、历史」。同一请求在同一源码上必得同一回复（幂等），因此任何请求都可以重发，重启零恢复成本。
+ *R2 请求自带上下文*：需要光标与选区的动词把它们作为参数收下（`session`），而不是靠先前某次 `activate` 建立的状态；回复总是带*变化后的源码*（若变了）与新的显示树。
+ *R3 动词少而稳*：文档级、结构级、渲染级各只有几个动词（见下表），且不因前端控件的增删而增加——控件的增删属于前端。
+ *R4 位置只用字节偏移*：边界上一律 UTF-8 字节；Qt 的 UTF-16 由前端换算（已有 `model.py`），LSP 的 UTF-16 由后端换算。换算各只在一处。
+ *R5 名字契约版本化*：连接建立后先 `hello`，后端给出能力表（协议版本、`view` kinds、`markers`、`roles`、形状与编辑规则、引擎量测常量、动作清单）。前端*启动时对账*，不认识的名字*报错*，而不是静默按横排画——今天 `ARRANGEMENTS` / `MARKERS` 只警告一次就退回横排，属于会静默画错的那一类。
+ *R6 回复与事件同一条协议*：一行一个 JSON；带 `id` 的是回复，带 `event` 的是推送。取图与整页编译在工作线程里完成后以事件回包，前端不阻塞等编译；事件带 `revision`，过期即丢。
+ *R7 失败是数据*：`images`（id 到 svg 或 error）、`glyphs`、`placements`、`failed`、`diagnostics`；前端只画，不再维护「被拒片段属于哪个 revision」这类账。
+ *R8 错误不抛给用户*：沿用今天的形式——错误带动作名、退出码与后端 stderr 尾部；前端显示在消息面板并保留源码。

== 动作表（新）

#table(
  columns: (1fr, 2.7fr, 2.7fr),
  [*动词*], [*请求要点*], [*回复要点*],
  [`hello`], [协议版本], [能力表（R5）],
  [`set_source`], [全文], [revision（不带分析）],
  [`analyze`], [可选区域；上次 revision], [整份公式表（`id`、`view_version`、区间、`view`、`display`、`editable`、上下文身份、`entry`）、`styles`、定义块分组],
  [`edit`], [区间或全文、编辑意图、`session{cursor,selection,draft,display}`], [新源码（若变）、`styles` 与定义块分组、`changed`（整条公式数据）、`removed`（身份）、新光标、`message`],
  [`navigate`], [`session`、方向或跳转种类], [新光标的结构落点],
  [`command`], [草稿文本、caret、动作（候选、确认、取消）], [候选列表，或确认后的新源码与 `view`],
  [`visible`], [revision、可见公式区间], [（事件）`images`、`glyphs`、`placements`、`failed`],
  [`export`], [PDF 或整页 SVG、overlay 列表], [产物页数据（落盘仍在前端）],
  [`lsp`], [方法、字节位置、revision], [诊断、补全、悬停、定义、格式化、语义 token],
  [`packages`], [同现状], [同现状],
)

删除的动作与路由：`activate_formula` / `deactivate_formula` / `state` / `geometry` / `preview_result(s)` / `lsp_completions` / `set_definitions` / `set_display` / `clear` / `import`（并入 `edit` 与 `analyze` 的参数），以及前端侧的三条适配器路由 `/api/render`、`/api/attachments`、`/api/glyphs`。

进程拓扑也随之简化：*两条管道*——`--document`（源码、结构、编辑、渲染编排，自己调用适配器）与 `--services`（Tinymist 的语言服务与实时预览、包管理）。代价是文档进程必须允许并发请求（今天 `--desktop-core` 是一行进一行出的阻塞循环），编译在工作线程里、文档锁只在计划阶段持有。若不接受这点，过渡形态是：文档进程只产出*渲染计划*（id、区间、上下文身份），前端把计划原样转给渲染管道——前端不再理解计划的内容，只是转发。

== 单一实现清单（每件事只有一个出处）

#table(
  columns: (1.7fr, 3.4fr),
  [*事*], [*唯一出处*],
  [源码与树的互转、拼写、宏与上下文、显示树、失效判定、渲染计划与身份、语言服务换算], [后端（Rust）],
  [键盘语义、命中测试、排版、绘制、撤销、可见性、控件], [前端（Python）],
  [名字与引擎量测常量], [后端生成（`hello`），前端启动时对账],
  [位置编码], [边界各转一次：前端 Qt 与字节，后端字节与 LSP UTF-16],
)

== 迁移顺序（第 4 步的骨架，每步独立验收）

+ *A 失效判定回到后端*：`edit` 与 `analyze` 用 `id` 与 `view_version` 说明哪些投影要重建、区间移到哪里；前端删 `incremental.py` 的判据。验收：现有增量行为逐条对照通过（正文编辑不动公式、`let` 改动使其后失效、`#[]{}` 保守失效、公式整体平移时视图复用）。
+ *B 渲染编排回到后端*：`visible` 订阅加 `image id` 加后端缓存；前端删 `rawcache.py`、`load_raw` 的批次合成与三类缓存。验收：第 1 节列的三条实测行为不变——失败框进入即重试、模板 `raw_macro` 按调用实例分帧、`stretch(->)^x` 改脚标即使基底失效。
+ *C 会话搬到前端*：`edit` 与 `navigate` 带显式 `session`；后端删会话与 `geometry`，前端删 `math_state` 的后端镜像与 `bridge.Core.restore` 的重放。验收：光标、选区、草稿、撤销逐条与今天一致（`tests/caret_navigation.rs`、`tests/command_mode.rs`、`tests/lyx_traces.rs` 是判据来源）。
+ *D 能力表与量测常量下发*：前端删 `ARRANGEMENTS` / `MARKERS` / `LIST_KINDS` 与硬编码的 .36 / .25 em、×0.9 / ×0.7；`tools/kind_inventory.py` 的双向对照变成 `hello` 的启动期检查。
+ *E 文档分析事实回到后端*：定义块分组与 `is_list`；`definitions.py` 只剩控件。

测试也随之分组：后端（内核、host、适配器）保留现有 Rust 套件并新增*无会话*与*契约*两类用例；前端用*假后端*（脚本化的 JSON 行，`desktop/test_desktop.py::BridgeTest` 已是这个做法）测控件与交互；两侧共用同一份请求与回复 fixture——这就是第 3 步的「最小前端加合成数据」。

== 代价与风险

+ *往返次数下降，但仍是同步的*：正文一次按键从「`edit_source` 加 `scan` 加每个重建公式一次 `analyze_formula`」降到 1 次，公式内从 3 次（动作加 `scan` 加 `geometry`）降到 1 次；但今天 `Core.call` 是阻塞等回包的，所以渲染必须走事件（R6），否则编译会卡住界面。
+ *文档进程要并发*：这是本次唯一新增的复杂度。替代方案见上（保留渲染管道，只把计划搬进文档进程）。
+ *撤销粒度会变*：公式内撤销从内核的结构步变成前端的源码快照步，必须保留今天「连续输入合并成一步」的规则，否则撤销变碎；`lyx_traces.rs` 的撤销用例要在新栈上重放。
+ *无会话的解析成本*：每次请求由源码重建可编辑树。文档里实测 200 原子的投影约 93µs（`docs/editing-model.md`），按键量级无问题；大公式需要按（区间，源码）记忆化。
+ *「树不搬到前端」是本设计最重要的不采纳项*：若将来要做无后端的离线编辑，那要先决定 `write_atom` 与 `view_atom` 是否也在前端实现一遍——那是另一个量级的决定，不该夹在这次重构里。

== 明确不动的部分

- 内核与外围的 crate 边界，以及「只有 `native-adapter` 链接 Typst 编译器」。
- `View` 作为唯一的显示数据：前端仍然不解析 Typst。
- Qt 的排版与绘制、位图 LRU、文件与设置对话框、窗口与文档对象模型。
- Tinymist 的语言服务与实时预览、包管理。
- 两条行为约定：命令草稿不回写源码；宏定义草稿确认后才回写。

== 备选方案（记录理由，不采纳）

#table(
  columns: (1.7fr, 3.4fr),
  [*方案*], [*为什么不走*],
  [导航表下发前端，移动也在前端算], [省下一次按键往返，但要在 Python 再实现一遍 `entry` / `horizontal` / `vertical` 与 `back_lands_at_end`，与 `editing.rs` 及其对齐测试双份维护；还要让 `View` 额外携带存储路径（`View` 的子节点不等于存储槽位）。等按键延迟真的成为瓶颈再考虑],
  [可编辑树整棵搬到前端（`MathData` 已是 serde 结构）], [要么把 `write_atom` / `view_atom` / 宏注册表一起搬到 Python（复制内核不变量），要么每次编辑回后端投影（省不下往返）。见取舍一],
  [渲染留在前端，只把缓存键换成后端给的 id], [前端仍要自己算批次、`context_end`、可见性与失败重试，第 1 节列的第 5、6 条差距不动，等于只修了一半],
)

= 构建最小可行案例
- 先确定接口规则：规则、动作表与控件数据已经在第 2 节的「接口规则」「动作表」「边界上的对象」三小节里，这一步是把它落成一份可执行的协议（JSON 行 + fixture）。
- *接口规则已落，尚未实现*：`protocol/schema.json`（机器可读：类型、动词、事件、枚举、fixture 约定、禁用名字）、`protocol/spec.md`（规则 R1–R8、十个动词、事件、能力表来源与 fixture 格式）、`protocol/fixtures/`（8 段对话 51 步合成数据）、`tools/check_protocol.py`（自检，退出码 0）。写 fixture 的过程改了两处设计：失效信息从"区间列表"改成"整条公式数据 + `id`/`view_version`"（否则前端还得自己算区间平移，正是 `incremental.py` 被删掉的那部分），并新增 `visible.retry`（滚动重复订阅不重编译，刻意动作才重试）。
- 只保留最基本的编辑功能，去掉如实时预览等辅助功能，跑通前后端：
  - 按接口规则写一个最小前端，用合成数据测试它能否正确通过接口收发消息，并与现有程序对比。
  - 然后，同样是按接口规则写一个最小后端。
  - 当上述两个步骤受阻时，优化接口规则，直至方案收敛。

第一个最小案例的边界（建议）：

+ *最小后端*：只实现 `hello` / `set_source` / `analyze` / `edit` / `navigate` / `command` / `visible` 七个动词，编辑意图只覆盖「输入字符、上下左右、Tab、退格、`\frac` 确认、增删行列」；结构、拼写、宏、显示树全部复用现有内核，砍掉的只是会话外壳与三个前端路由。
+ *最小前端*：一个窗口加一个 `FormulaControl` 控件（排版、绘制、命中、键盘到意图），会话（光标、选区、草稿、撤销）自己持有；渲染先只画 `/api/render` 回来的 SVG 与字形，不做位图 LRU 以外的缓存。
+ *对照两名对手*：同一份 fixture（请求与回复的 JSON 序列）喂真后端与假后端；同一串意图序列分别跑在现有程序与新前端上，逐步比对 `source` / `view` / `cursor` 三样。
+ *收敛判据*：fixture 全绿；把现有测试里与编辑行为相关的用例（`tests/caret_navigation.rs`、`tests/command_mode.rs`、`tests/lyx_traces.rs` 对应到前端那一侧的）按新接口重写后仍全绿；此时才进入正式迁移。

= 正式迁移
- 在最小可行案例跑通后，将全部测试套按功能重新分组到前端和后端。
