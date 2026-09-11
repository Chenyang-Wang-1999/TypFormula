# 正式版架构

## 原生桌面前端

`desktop/` 是 Qt Widgets / PyQt5 原生前端，通过 `--desktop-core` 管道使用 Rust Document，通过独立 `--stdio` 管道使用编译/包服务与 Tinymist。没有 WebView、HTTP 端口或 WASM；两个后端进程都是窗口自己启动的私有管道。完整源码及文档撤销由窗口持有，QTextDocument 只是带自定义公式对象的投影；位置映射显式转换 Python Unicode、Rust UTF-8 和 Qt/LSP UTF-16。一个窗口只保留一个活动公式会话和绘图控件，分栏共享源码。

Rust Document 常驻 `typst_syntax::Source`。源码编辑调用 `Source::edit`，使用 Typst 增量解析器返回的实际重解析范围；桌面公式索引只扫描更新后的轻量语法节点，平移范围外已有投影，只为重解析范围相交的新建/改变公式构造 View。`let` 变化会使其后的宏投影失效。合并时只重建真正移动的节点：编辑点之前的公式与未移动的子树与上一份投影共享，因此每次按键不再深拷贝全部公式。载入文件和 `analyze_formula` 不可用时的回退仍走一次全量 `analyze`，即完整投影；上面描述的增量只覆盖编辑路径。此策略沿用 Typst 保持远处 span 稳定的增量解析边界，而不是按输入字符猜测影响范围。参见 [Typst 编译器架构](https://github.com/typst/typst/blob/main/docs/dev/architecture.md) 与 [comemo](https://github.com/typst/comemo)。

窗口与后端的连接由 `desktop/bridge.py` 管理：`Core` 持有一个文档与一个活动公式会话，按动作给等待预算（`set_source`/`analyze` 这类要整篇解析的请求另计），核心退出或超时后重启、按镜像到的源码重放 `set_source` 并恢复当时打开的公式会话，再重试该请求一次；`Services` 持有 `--stdio` 通道，子进程退出后由下一次请求重启。两条通道的失败信息都带动作名、退出码与后端 stderr 尾部。细节见 [desktop.md](desktop.md)。

公式排版结果按视图缓存：`FormulaObject.box` 为每个视图保留一份 Box，Qt 在一次布局/绘制周期内对同一公式的 `intrinsicSize` 与 `drawObject` 调用因此只排版一次，鼠标命中判定复用同一结果。Raw SVG 或附件位置变化（`Typesetter.touch`）以及编辑字号、SVG 倍率、`math_font` 变化时缓存失效。片段位图一律按**黑色**栅格化（`desktop/svg.py` 的单色转换只在编辑器缓存这条路上启用）：片段是从文档里切出来的图，文档可能把数学排成白色，浅色编辑区上就看不见了；导出路径仍用原始 SVG。

编辑器自己画的公式有三个约定，都与编译结果对齐：**脚标移位**取 Typst 的 `.36 em / .25 em`（从编译 SVG 的基线差量出），基底是组合对象时再按它的真实升降部让开；**空槽**（`empty-cell`）画成虚线方框而不是 `□` 字形，`\frac` + Enter 之后分子分母各一个，方框会把该槽的 stop 一起折进 Box，否则光标画不出来、核心的 `move_vertical` 也找不到落点；**独占一行的行间公式**整行居中（`Editor.apply_alignment` 设块的 `AlignHCenter`），与正文同行的保持左对齐，因为 Typst 会把块级公式断到单独一行而投影不会。命令草稿与 Raw 源码共用 `source_run`，都走编辑器正文字体：它们是待编译的源码，不是编译出的字形。

源码栏（`Window.source_dock`，一个 `QDockWidget`，里面放 `SourceEditor`）与编辑器**逐行对齐**：同一个文档默认字体、同一个顶部偏移，并把编辑器实测的每行高度以 `MinimumHeight` 写进源码栏的块（带高公式的行比纯文本行高）。行高逐一相等之后两栏的滚动值可以直接对应，`Window.mirror_scroll` 双向跟随。跟随只发生于**用户滚动**：重投影会重建两栏并各放回自己的滚动值，源码栏的光标同步也会让源码栏滚到自己的光标处，这两种都不算"要跟随的滚动"（`Window.loading` 期间 `mirror_scroll` 直接返回，`Window.project` 设完光标把源码栏滚回原位），否则停在一处的源码栏光标会把编辑器一起拽走。源码栏是 `QTextEdit` 而非 `QPlainTextEdit`：后者的文档布局忽略块行高。

公式字体分两件事：**结构行度量**（em、基线、下降部）取编辑器正文字体，**字形**取数学字体。`desktop/mathfont.py` 注册随附字体并读回 Qt 报告的真实家族名，只在已安装的家族里解析设置里的 `math_font`；名字不存在时退回随附数学字体而不是交给 `QFont`，因为 Qt 的替换是静默的，随后逐字回退又会把多个设计混进同一个公式。数学字体本身不能提供行度量：`NewComputerModern Math` 必须容纳四层高的定界符，12pt 下报告 `ascent=99`、`height=185`，直接当行高会得到几乎空白的巨框。数学变量的字形来自 Unicode 数学斜体区间（`a→𝑎`、`h→ℎ`），与编译结果同一套字形；文本单元保持直立。这里有一个必须分清的分界：**那个映射只适用于编辑器自己认出来的变量**。字体变体（`style` 节点）画的是**引擎已经替换好的**串，再映射一遍就把 `upright(A)` 的直体 `A` 改回斜体 `𝐴`——所以 `mathfont.glyph(..., substituted=True)` 让引擎给的串原样画（`bold` 只是因为 `𝐀` 不是 ASCII 才一直没露馅）。`h` 是这条映射唯一的例外，而且例外来自 **Unicode 的洞**而不是映射规则：U+1D455（mathematical italic small h）**未分配**，引擎把默认斜体 h 排成 Planck 常数 `ℎ` U+210E；52 个字母逐个问过真适配器，只有它一处不同。随附字体位于 `fonts/`，`native-adapter` 也把数学字体编进自己的二进制。

每个 Raw 片段的源码区间由 `Document::annotate` 给出，它是"片段图像"与"源码"之间唯一的连接：窗口用 `render.raw` 的区间请求 Typst。规范序列化只用于估算位置——它补空格、把 `a/b` 写成 `frac(a, b)`——最终以"该区间里恰好是这段文本"为准，所以按自己习惯书写的公式同样出图。区间无法确定的片段退回源码显示并标出，核心允许左右键进入它的源码（`failed_previews` + `Action::PreviewResults`），这是它唯一的修复入口。

规范序列化（`typst::write_cell`）在**每两个原子之间**写一个分隔符，没有例外。原因是写出的源码会被重新解析，而 Typst 会把连写的字符当成一个整体：`xy` 是变量名（实测 `$ab$` 直接报 `unknown variable: ab`），`->` 是箭头、`||` 是 `‖`、`[|` 是 `⟦`、`...` 是 `…`。少一个分隔符，用户按下的两个字符就会在离开公式后变成一个不可再拆的 Raw 片段；`tests/structured_input.rs::a_separator_keeps_typed_characters_from_becoming_one_shorthand` 守着这条规则。代价是两个按键连写出的 `<=`、`>=`、`!=` 不再合并成 `≤`、`≥`、`≠`：这类符号改用命令输入（`\>=` 回车）得到。

数字曾经是唯一例外（`12`、`1.5`、`.5` 不加分隔符），因为那时一个数字串是**每个数字一个 `Char`**，必须靠这条规则保持被词法读成一个数字。现在数字串是一个 `Kind::Number`：**一个格，里面逐字是 `Char`**，与 `Text` 同形，回写走 `Write::Run`（把格内字符连成一个记号，格内**不**加分隔符）。这样光标能停在数字之间（`12|34` 插入 `9` 得 `12934`），而叶子模型下这是做不到的；串内没有两个原子，普通的分隔符规则也不会插进串里。

两条光标规则与 `Text` 有同有异：**字符**处理与 text 格相同（直接插成 `Char`，不走 `/`、`^`、`_`、`\` 的解释，但只接受数字——其它字符先在光标处把串断成两半，再按普通字符在该处处理）；**按键**处理则与 text 相反，不困住左右键（`cursor.rs` 里 text 在格两端会吞掉 `ArrowLeft`/`ArrowRight`，数字串不吞），所以光标能自由进出。`|456` 输入 `9` 因此得到 `9456` 而不是把光标留在 `9` 前面。

由此带来一处**有意的**不对称：从源码读到的 `12.5` 是一个 `Number`（格内 `1`、`2`、`.`、`5`），而普通模式逐键敲出的 `12.5` 是 `Number(12) Char(.) Number(5)`，写出 `12 . 5`——`.` 在普通模式里就是普通字符。两者渲染完全相同（实测 24pt 下 `1.5` 与 `1 . 5` 都是 30.672×16.512pt，`.5` 与 `. 5` 都是 18.672×16.512pt），所以都不算损失；命令模式（`\12.5` 回车）走解析器，得到的就是一个 `Number`。

派生数据同样按视图缓存，避免每个编辑周期重算整篇：附件请求表 `Window.attachment_nodes` 记住每个视图的「定义前缀 + 表达式 + 显示模式」，`Window.visible_formula_starts` 先把各编辑器的公式对象按公式起点分组，再做可见性判断，因此 `load_raw` 是"对象数 + 公式数"而不是两者相乘。语义高亮的 span 每次回复都会整体替换，所以那一侧不缓存，改为每个视图复用同一 `QTextCharFormat`，并且只为可见视图构建（源码 dock 默认隐藏，展开时经 `visibilityChanged` 立即着色）。

窗口不包含实时页面预览。F5 通过原生 `typst-pdf` 编译当前内存源码并交给系统默认阅读器；公式 Raw 和附件位置继续使用独立后台服务，不依赖 PDF 编译。详情及范围见 [desktop.md](desktop.md)。

## 状态所有权

| 层 | 保存什么 | 不保存什么 |
| --- | --- | --- |
| QTextDocument 投影 | 源码文本、公式对象位置、选区 | 公式内部结构、撤销栈 |
| 窗口 | 完整源码（唯一的文档权威）、撤销/重做栈、脏标记 | 逐公式 Editor |
| Rust Document | 完整源码镜像、活动公式字节区间、全局 Editor | 其他公式的状态 |
| Rust Editor | 活动公式的 `MathData`、槽位光标、草稿和测量 | 其他公式的状态 |
| 自定义文本对象 | 区间与静态显示；活动公式复用同一个 `MathCanvas` | 文档状态 |
| 本地 Services | Tinymist 文档会话、独立公式补全、常驻 Typst 渲染器 | 正文编辑状态 |

正文输入由窗口写进自己的源码字符串，再经 `edit_source` 让 Rust 增量重解析；一次按键是一次 `Source::edit`，不是 `set_source`。扫描忽略注释、字符串和 Raw 中的 `$`，源码有错也允许保留。进入公式调用 `activate_formula(start)`，只解析活动范围并读取前缀中的宏定义。公式区间表按语法树代次缓存：每次编辑（含结构编辑写回活动区间）重建一次，同一代次内后续的每次 `response()`、`analyze_formula()` 都复用它，不再逐次遍历整篇源码。命令草稿不会写回源码，也不产生撤销步。

宏绑定以 Typst 语法树中的作用域边界为依据：只收集活动公式祖先作用域中已经出现的 `let`，跳过已经结束的兄弟内容块/代码块；函数自身和形参遮蔽外层绑定。绑定表按顺序更新，模板依赖边仍引用不可变的定义版本，因此后续同名定义不会改变早先宏的捕获。当前仅支持 `let` 的静态结构展开，不尝试执行任意导入、循环或动态函数来推导展开结构。宏注册表按定义前缀缓存并**进程内共享**，保留多份条目：优先复用最长匹配前缀（同文档增长），否则回退到最近使用的一份并逐定义比较源码文本。`--stdio` 通道是每请求一线程，线程内缓存等于每请求重建，因此这里必须是跨线程缓存。

**宏内片段没有独立渲染机制。** 定义体里画不出来的片段（Raw）和正文里的片段走同一条路：`Document::annotate` 为它算出它**在定义文本中的文档区间**，`render.raw` 用这个区间请求编译，文档里对该宏的调用就是编译它并产出那一帧的地方。所以同一个片段在定义体和各个调用点共用一张图，缓存命名空间是 `('raw', 源码文本)`。**唯一的例外是 attach 里的片段**：片段被插回原位置编译，`stretch(->)^x` 的基底拉伸到脚标 `x` 的宽度，同一段源码在不同 `script` 下是两张图，因此这类片段的键是 `('raw', 源码, script 形状摘要)`（`rawcache.signature(script)` 的摘要，见 [desktop.md](desktop.md)）；摘要只在**定义所在的公式视图**里建立，所以定义视图与调用点视图仍然同键。唯一额外的约束是**取图截断**：片段在定义里，它的排版结果产生在调用处，调用可能在文档更后面，所以只要这次请求里含定义内的片段，窗口就不截断源码（`context_end` 取全文），否则截断会把调用一起切掉、片段必然没有图。定义从未被调用时它自然没有图，和任何画不出来的片段一样显示源码 + 暖色标记。

结构修改通过唯一 Editor 执行；若序列化结果实际改变，Document 仅替换活动范围，窗口把这次替换并入自己的撤销栈并重建投影，不会再次 `set_source` 清空活动树。源码进入和退出不做隐式格式化。隐藏公式只保存静态投影，不保存单独的光标和撤销栈。

Rust / Typst 字节区间使用 UTF-8；Qt 字符位置与 LSP character 使用 UTF-16。所有跨层转换集中于 `desktop/model.py`（`to_byte`/`from_byte`/`u16`/`from_u16`）与 `Window.lsp_position`（`desktop/window.py`，唯一的 UTF-16 → LSP character 转换点）和 `src/services.rs`。LSP 返回替换范围后检查边界与重叠；异步请求返回时核对源码版本和文件身份，避免过期结果写入当前文档。

## 分层：内核与外围

代码分两个 crate，边界由 **cargo** 强制，不靠约定——`pub(crate)` 拦不住"内核文件伸手去够外围文件"，crate 边界可以。

| 层 | crate | 内容 | 依赖 |
| --- | --- | --- | --- |
| 内核 | `visual-typst-core`（`crates/core/`） | `math`（可编辑树）、`slots`（每个 `Kind` 的唯一一张声明表）、`typst`（解析与回写）、`cursor`（`Editor` 与全部编辑动作）、`view`（交给前端的形状） | 只有 `typst-syntax` + serde |
| 外围 | `visual-typst`（仓库根） | `document`（源码即权威）、`desktop`/`rpc`（两条私有管道）、`services`/`packages`/`workspace`（进程、网络、路径）、`visual-typst` 二进制 | 内核 + std/网络/压缩 |

方向是单向的：外围可以依赖内核，内核**不能**依赖外围。这条不是纸面规则：在 `crates/core/src/lib.rs` 里写 `use visual_typst::…` 会编译失败（`unresolved import`，实测）。所以"只改外围、不动内核"是编译器保证的——加一个新前端、新传输或新文件功能时，内核的五个模块不需要打开。

内核里**不许出现**的东西（出现就说明该往上挪）：`std::process`、`std::fs`、网络、任何"传输/协议"形状的类型。给它定量身标准会更清楚：内核只回答两件事——**这棵树是什么**，以及**它该怎么写成 Typst**。

两处容易踩的坑，写在根 `Cargo.toml` 里：

- cargo 会把工作区目录内**所有路径依赖**自动收成成员，所以 `vendor/typst`（自带 `[workspace.package]`，它的 crate 靠继承）和 `native-adapter`（自己就是工作区）必须 `exclude`，否则会被重新认亲、丢掉它们继承的字段。
- 不写 `default-members = [".", "crates/core"]` 的话，根目录下裸跑 `cargo test` 只测根包，**内核自己的单元测试会静默不跑**。实测加上它以后总数与分层前一致（当时 121 通过 / 5 忽略；数字随后续工作增长，写文档时是 129 通过 / 5 忽略）。

## 后端

后端没有网络端口：`visual-typst --stdio <目录>` 由窗口启动，请求按 id 匹配回包，`src/rpc.rs` 每请求分发一个线程，`Services` 通过互斥量共享，因此整页编译不会把公式取图和语言请求排在它后面。项目目录由启动参数指定，窗口把当前文档所在目录作为编译根；`src/workspace.rs` 用规范化路径限制项目边界，任何越界或含父级步骤的路径都被拒绝。

`/api/lsp` 提供 completion / hover / definition / formatting / diagnostics / semanticTokens。一个文档使用一个常驻 Tinymist 进程，版本递增并全文同步；换文件或协议失败时重建。诊断来自 publishDiagnostics，窗口防抖、版本检查和过期结果丢弃。公式补全仍使用隔离的临时源码投影，以保留原型已验证的命令行为。

`/api/packages` 从官方索引查版本，安装精确版本到标准缓存。下载有超时与大小限制；包解压仅接收普通文件和目录，拒绝链接、越界和过大归档。先解压到临时目录并校验 manifest 存在，再重命名发布缓存，避免半安装状态。

`/api/render` 和 `/api/attachments` 使用正式版自己的 native-adapter。数学 IR 标签桥接保存在 `vendor/typst`；构建不再准备原型引擎或修改其他目录。源码、资源与缓存失败不影响代码区继续输入。

`/api/glyphs` 走同一个适配器，回答"这段调用被引擎替换成了哪些字符"（下一节的 `Style`）：请求体带 `definitions` 与 `display`，适配器把这段拼写编译成数学 IR 再把字形串读回来。同一个片段可以既取图又取字，两者互不影响。

## 一个公式怎么变成可编辑的树

这一节把"哪一层知道什么"写成一条链，因为分界线不明显，读代码时很容易把 `parse_atom` 的分支结构误当成 Typst 的分层结构。**一条公式里能成为可编辑节点的构造，只有两个来源：Typst 的语法节点，或者 `config/commands.json` 里的名字。**

### 来源一：语法节点（parser 直接给出，与名字无关）

`typst-syntax` 的 parser 里，mathematical operator 带优先级，会被折成**专门的节点**（`parser.rs:404` 的 `math_op`）：

| 源码 | 节点 | `typst.rs` 的分支 |
| --- | --- | --- |
| `1/2` | `MathFrac` | `:545` |
| `x_1`、`x^2`、`x'` | `MathAttach` | `:546` |
| `∛x`、`∜x` | `MathRoot` | `:559` |
| `(a + b)`、`[x]` | `MathDelimited` | `:575` |
| `&`、`\` | `MathAlignPoint`、`Linebreak` | `:498`（`parse_cell` 预处理） |

`math_op` 把 `/` 映射到 `MathFrac`、`_`/`^` 映射到 `MathAttach`，**按算符形态判定，完全不查名字**。所以 `1/2` 是分式这件事，parser 自己就知道。

### 来源二：名字表（parser 给不出，只能查配置）

`parser.rs:281-284` 是另一条路：

```rust
if MATH_FUNC_PREC >= min_prec && p.directly_at(SyntaxKind::LeftParen) {
    math_args(p);
    p.wrap(m, SyntaxKind::MathCall);
}
```

规则只有一句：**一个 `MathIdent` 后面紧跟 `(`（`directly_at`，中间不许有空格），就包成 `MathCall`**。不查名字、不查作用域。所以 `frac(...)`、`vec(...)`、`cancel(...)`、`foo(...)` 在这一层**型别完全相同**——`MathCall` 只断言"有个标识符被调用了"，不回答"它是什么"。

那"它是什么"在哪回答？在 **`typst-eval`**：把 callee 求值成作用域里的 `Func`，实参求值成 `Content`，得到 `Content::Elem(FracElem{…})`。**`MathKind` 还要更晚**：它是布局期的中间表示，`resolve_equation` 的调用点全在 `typst-layout`（`math/mod.rs:68`、`:123`）和 `typst-html`（`rules.rs:824`），`typst-library/src/math/ir/mod.rs:23` 只提供函数本身。到那里才由 `resolve.rs:197` 起的一长串 `to_packed::<FracElem>()` / `to_packed::<CancelElem>()`（`:222`）按**元素类型**分派。

**所以内核拿不到它。** `visual-typst-core` 只依赖 `typst-syntax`，`typst-eval`/`typst-library`/`typst-layout` 只有 `native-adapter` 那一侧才链接。名字→结构这件事必须由内核自己回答，`config/commands.json` 就是那个答案：

```
"frac(1, 2)"
   ├─ typst-syntax : MathCall{callee:"frac", args:[1, 2]}      ← 调用结构与参数范围
   └─ parse_atom   : 表里有 frac → 存成 MacroCall{name:"frac"}，两个参数各自 parse_cell
                        ↑ 名字表只回答"这名字认不认"，形状（分式的两格）是后面查出来的
```

### 由此得到的三条判断

1. **`commands.json` 不是识别机制，是识别机制的补充。** 它只管"写成调用形式的构造"。`frac(1,2)` 不在表里会退成 `Raw`，但 `1/2` 仍然可编辑——同一种排版有两条入口，只有一条依赖这张表。
2. **表里删一项的后果，`tests/round_trip.rs` 测不出来。** 删掉 `"frac"` 后 `frac(1,2)` 变 `Raw`，而 `Raw` 保留原文、往返照样成立。这类改动只有直接断言 `Kind` 的用例才有牙（对照 `math::is_number` 的单元测试：外部来源的规则，往返测试看不见）。
3. **要拿真实 `MathKind` 就绕不开跑一遍 layout。** 这不是选型问题，是 `resolve_equation` 的位置决定的——`native-adapter` 与 `tools/engine_boxes.py` 都因此必然在布局路径上。

### 三张表的边界（当前的已知缺口）

| 名字 | 引擎里有对应元素 | 表里有名字 | 结果 |
| --- | --- | --- | --- |
| `frac`、`sqrt`、`root`、`hat`、`overline`、`underline`、`abs`、`norm`、`cancel` | `FracElem`/`RootElem`/`AccentElem`/`CancelElem` 等 | 是 | **存成 `MacroCall`，形状查表得到**（见下节） |
| `mat`、`vec`、`cases` | `MatElem`/`VecElem`/`CasesElem` | 是 | `Kind::Table`，但**行方式与定界符来自配置**（见下节） |
| `a/b`、`√x`、`(a+b)`、`x^2` | `Fraction`/`Radical`/`Fenced`/`Scripts` | 不适用 | 语法节点，与名字无关 |
| `bb(A)`、`lr(x, size: #100%)` | `TextElem` 变体 / `LrElem` | **否** | `bb` 不在表里、`lr` 有具名实参——建的是未知名字的 `MacroCall`（画成 `raw_macro`，参数可编辑），**不是** `Raw` |
| `bold(x)`、`upright(A)` | 变体在引擎里是**码位替换** | 是 | **存成 `MacroCall`，形状查表得到**；字形由 `/api/glyphs` 问引擎，见下节 |

`vec` 曾经是这里"引擎认得、表里没有"的例子，现在进表了。**注意 `VecElem` 一直是 define 过的**（`math/mod.rs:68`），所以引擎从来就把它解析成列向量；内核当初不认它只是因为表里没有——两者是两回事。

`resolve_vec`（`resolve.rs:1018`）把**每个参数各包成一行**（`map(|child| vec![child])`）再交给 `resolve_cells`，所以 `vec(1,2,3)` 与 `mat(1;2;3)` 排版逐字节相同。这是元素自己的固定行为，与逗号/分号的分列语义无关——前一版文档若把它解释成"逗号分隔却要换行"，那是错的。

### 表格的行方式与定界符在配置里

`vec`/`cases`/`mat` 是**同一个 `grid` 形状的三个名字**，差别只有两件，都写在名字旁边：

```json
"mat":   { "shape": "grid", "rows": "mat",  "border": "()" },
"vec":   { "shape": "grid", "rows": "each", "border": "()" },
"cases": { "shape": "grid", "rows": "each", "border": "{ " }
```

`rows` 说参数列表怎么变成行（`mat` 按分号，其余一个参数一行），`border` 说画什么定界符（左+右，或单边一个字符）。实测三条路都成立，而且**写回的是各自的名字**：

| 源码 | `columns` | `border` | `is_mat` | 写回 |
| --- | --- | --- | --- | --- |
| `mat(1, 2; 3, 4)` | 2 | `()` | true | `mat(1, 2; 3, 4)` |
| `vec(1, 2, 3)` | 1 | `()` | false | `vec(1, 2, 3)`（**不是** `mat(1; 2; 3)`） |
| `cases(1, 2)` | 1 | `{ ` | false | `cases(1, 2)` |
| `mat(a, b; c)` | 2 | `()` | true | `mat(a, b; c)`（**行宽不等**，见下） |

这就是为什么 `Kind::Table` 存了一个 `name`：**形状不蕴含拼写**。`vec(a, b)` 与 `mat(a; b)` 排出同一张表，节点不记住名字就写不回原样。

配置值因此有两种写法：只写形状名（`"frac": "fraction"`），或者写带参数的对象。对象形式是这一次加的——`grid` 这个名字单独一个字符串说不清"这个表怎么切行、外面画什么"。

### 行宽可以不等——旧的那条拒绝是错的

`Kind::Table` 里还有 `row_lengths`，和 `Multiline` 一模一样。它存在的直接原因是**编辑器原先拒绝建这个表**：

> 「只有各行等宽的矩阵才是矩阵：`mat(a, b; c)` 不是 Typst 能排的表，所以留作源码。」

**这句话是错的。** 实测 24pt：`mat(a, b; c)` 高 `43.008pt`，与 `mat(1, 2; 3, 4)` **完全相同**——引擎照排，短行按缺格处理。所以那是一条**既错误又没人守**的规则：`round_trip.rs`、`structured_input.rs`、`desktop` 里都没有 `mat(a, b; c)` 的用例，删掉它时**一个测试都没红**。现在它有用例了（`a_matrix_may_have_rows_of_different_widths`）。

`row_lengths` 记的是**补齐前**每行真正有几格。两个地方靠它：

* **写回**要按它把补齐的格子去掉，否则 `mat(a, b; c)` 会被写成 `mat(a, b; c, )`；
* **不能改用"删掉行尾空格子"这个更简单的办法**：`mat(a, ; c, d)`（两列、第二格是**有意留空**）会被改写成 `mat(a; c, d)`——那变成一行两格加一行一格，意思完全不同。这正是 `Multiline` 早就有 `row_lengths` 的原因。

行的补齐是**逐行**做再摊平的（`Multiline` 也是），不是最后一次性补齐：否则 `mat(a; b, c)`（行宽 `[1, 2]`）会摊成 `[a, b, c, _]`，被 `chunks(2)` 读回成 `[a, b]` 与 `[c, _]` 两行。这个是**新加的用例抓出来的**。

还有一个空格的坑：空格子写回时**什么都不写**（`write_cell` 对空格子回答的是 `""`，那是文本单元里空串的拼法，而 `mat(a, ; c, d)` 不是 `mat(a, "", c, d)`），**唯一的例外是最后一行的最后一格**——它后面没有分隔符了，裸写会留下一个尾逗号而没有实参，于是 `mat(, ; , )` 会读回"两格 + 一格"。所以那一格写成 `""`，而解析器会把空文本还原成空格子。这条也是**旧用例抓出来的**（`a_command_written_with_empty_parentheses_is_writable`）。

`Kind::Table` 现在与 `Kind::Multiline` 的数据形状完全一致（`columns` + `row_lengths` + 扁平格子），差别只在写回（`; ` vs `&`/`\`）与视图（有无定界符、是否居中）。**没有拆成"稠密/稀疏"两个视图**：按"一条独立的编辑或显示逻辑"这条判据，两者的画法与编辑完全相同，只是**实例**参差与否，所以是 `row_lengths` 这个数据，不是第二个视图 kind。

### 配置好的名字存成 `MacroCall`，形状是查出来的

`config/commands.json` 里的名字**不再各自有一个 `Kind`**：节点存的是**调用**（`MacroCall { name, cells }`），而**槽位、视图、拼写**都在画或写的那一刻由名字查表得到（`math::MathAtom::command_shape` → `slots::configured_kind`）。所以 `Kind::Fraction`/`Sqrt`/`Root`/`Accent`/`Line`/`Style` 这些变体**仍然存在，但不再是树里的节点**——它们是**形状描述符**，被 `configured_kind` 按名字返回。`Kind::Fenced` 与 `Kind::Table` 例外，它们仍然真的被存下来（前者是 `(a+b)` 的语法节点，后者见上表）。

这样做换来一件事：**配置文件决定什么被结构化，而且改了文件不会留下持有旧形状的节点**。代价是两类**按 `Kind` 写死的规则会静默失效**，实测在两处发生过，都记在这里：

| 规则 | 原写法 | 为什么失效 | 修法 |
| --- | --- | --- | --- |
| 退格在格首"只拉出当前实参" | `matches!(owner.kind, Kind::MacroCall { .. })` | 借了形状的 `frac(a, b)` 也是 `MacroCall`，于是它继承了**宏实参**的语义，退格再也拉不出实参（`lyx_traces.rs` 抓到） | `MathAtom::is_macro()`：`MacroCall` **且**没有配置形状，即"实参是 `#let` 的形参" |
| 向左跨格进入根式时落在格尾 | `matches!(owner.kind, Kind::Root)` | `root(...)` 存成 `MacroCall`，这个判断恒为假，光标落到了格首 | 改问 `owner.command_shape()`（`caret_navigation.rs` 新增用例抓到） |

第二条尤其值得记：**当时没有任何测试覆盖它**，是翻完之后逐条审 `Kind::` 判断才发现的，而补的第一版用例又走的是 `entry_cell` 而不是 `move_horizontal` 那条路，加了变异检查才发现它照样通过。同一个理由让 `is_macro()` 必须存在：`Kind::MacroCall` 现在是一个**过载**的标记，既表示"宏调用"也表示"借了形状的命令"，凡是按它分派的规则都要重新问一遍。

### 哪些 `Kind` 必须留下

一个构造要是自己的 `Kind`，只有两个理由：

1. **它带着名字和配置都给不出的实例数据**——`Table{columns}`、`Fenced{left,right}`、`Multiline{columns,row_lengths}`、`Raw{source}`、`MacroCall{name}`、`TemplateCall{definition}`、`Parameter{index}`、`Unknown{…}`、`Char{text}`、`Symbol{name,glyph}`；
2. **它要保住书写形式**——`SkewedFraction`（若采纳）记的是"源码写的是 `a/b`"，这是出处而不是数据，但同样只有节点能记住。

按这条，`Fraction`/`Sqrt`/`Root`/`Accent`/`Line` 都**不该是节点**：它们没有自己的数据，名字加上配置已经说全了。`√x`/`∛x` 也因此与 `sqrt(x)`/`root(3, x)` 折成同一个 `MacroCall`——radical 的形状不带数据，`√x` 该记的只有"它是 sqrt"这一个名字。

**`Table` 则必须留下，这是原计划里唯一算错的一项。** `mat` 的列数既不在名字里也不在配置里，只在参数列表的分号里：`mat(a, b; c, d)` 存成一个扁平格子表之后就再也分不出行界，写不回 `mat(a, b; c, d)`。所以 `mat` 不能变成普通 `MacroCall`——除非 `MacroCall` 自己长出一个列数字段，而那只是把同一个问题换了个地方放。

这条判据**有测试守着**：`tests/stored_kinds.rs` 解析一份覆盖"命令写法 + 语法写法"的语料，收集**真正进过树**的 `Kind`，断言集合恰好是 `Char`/`Symbol`/`Number`/`Raw`/`MacroCall`/`Text`/`Fraction`/`Scripts`/`Fenced`/`Table`/`Multiline`/`Unknown` 十二个。多一个就说明有人把某个**形状描述符**重新变成了会存的节点——那种回归能编译、能往返，此前没人会说。实测把 radical 那一支改回 `Kind::Sqrt`，它会报「多出来的：["Sqrt"]」。

`Sqrt`/`Root`/`Accent`/`Line`/`Style` 五个变体因此**只在形状表里活着**：`configured_kind` 拿它们当形状返回、`Decl` 描述它们的槽位、`view_atom` 按它们画，但没有源码能存它们。这不是"该删没删"，而是**一个变体身兼两职**——`command_shape()` 与 `is_macro()` 之所以必须存在，就是因为 `Kind::MacroCall` 同时也是"借了形状的命令"。

### `Style` 的替换表在适配器那一侧，不在前端也不在内核

`Kind::Style` 的载荷是 `{name}`（命令名），**线上**才叫 `style_name`；`text` 是线上 View 的字段，装的是整段调用拼写。`Style{name}`（`bold`/`upright`…）的**机制**很便宜：形状不带数据（`name` 就是命令名，和 `Accent` 一样），所以只要一个形状描述符 + 配置里几行。**贵的是那一步"替换"**，也就是"让前端负责渲染"实际要求什么。查证结果：

| 事实 | 证据 |
| --- | --- |
| 变体是**码位替换**，不是字体特性 | `resolve.rs:311` 把每个字符过一遍 `to_style(c, MathStyle::select(c, variant, bold, italic))`，替换后的文本才去整形 |
| 但那张表**不在 vendor 里** | `resolve.rs:4` 是 `use codex::styling::{MathStyle, to_style}`。`codex` 是 **Typst 自己的符号数据库**（`github.com/typst/codex`，Apache-2.0，作者是 The Typst Project Developers），`vendor/typst` 里的 `typst-library/src/symbols.rs:8,14` 用的就是它的 `ROOT`/`SYM`——它不在 `vendor/` 下，作为普通依赖随 `typst-library`/`typst-layout` 进构建（编辑器内核只依赖 `typst-syntax`，所以内核与前端都碰不到它） |
| 它**不是偏移表**：一个字符可能变成**两个**（基字 + 变体选择符） | `to_style('Q', Chancery) == "𝒬\u{fe00}"`（`codex/src/styling.rs:362`）；`mathfont.glyph` 目前只处理**单字符** |
| 它覆盖**非拉丁**字母表 | 同一份文档的例子：`ظ → 𞺚`、`ذ → 𞺸`（阿拉伯数学字母） |
| `cal` 与 `scr` 是**两种**变体，Unicode 只有一套 script 区 | 实测 24pt：`cal(A)` 19.152 ≠ `scr(A)` 20.52 |

字体本身没问题，这一点也量过了：随附的 `NewComputerModern Math` **覆盖全部变体区**，各区的"缺口"正是 Unicode 自己的设计（script 大写 18/26、fraktur 21/26、double-struck 19/26），而 Letterlike 那几个替代码位（`ℂℍℕℙℚℝℤ`）**全部存在**。

所以这不是"把 `codex/src/styling.rs` 那张表转写进 Python"还是"先不做"的选择，而是**不要在前端复述那张表**：`to_style` 的输出早已在数学 IR 里（`resolve.rs:309-317` 把 `styled_text` 交给 `TextItem::create`），适配器只是把它**读回来**——`/api/glyphs` 带 `glyphs:true` 编译这段拼写，递归取 `Glyph`/`Text`/`Number` 的 `text`。前端因此不依赖 codex，"表抄错了就画错"这个风险不存在：编辑器画的字形与引擎排的字形是同一份数据。

键是 `(definitions, 调用拼写, display)`，**按层**各问一次：`bold(upright(a))` 的引擎结果是 `𝐚`，但 `upright(a)` 这一层自己的字形是 `a`，光标进去要看到的正是后者。

内核只在主体**真的有字形串**时才建 `Style` 节点（`has_glyph_run`：`Char`/`Symbol`/`Number`/`Text`，以及形状是 `style` 的嵌套调用；`Raw`、分数、重音都不算）。`bold(frac(a, b))`、`bold(hat(a))`、`upright(a/b)` 因此走 `raw_macro` 那条老路（一张图，光标进入画名字与参数槽），不需要字形，也就不存在"取不到字"的状态。

前端只有两种画法：**有字形画字形，否则画这个调用**（名字 + 主体 + 右括）。第二种不只是"光标在里面"那一种——变体套一个字形时，画出来与那个字形一模一样，宏名是唯一说明在编辑什么的东西；而字形是**异步**到的，答案到达之前画调用也正是源码说的东西，画空串则什么都不说。

**取字与画分离**，这一条是踩过一次才定下来的：盖章发生在**布局那一刻**（页面的入口是 `FormulaObject.box`，公式框的入口是 `MathCanvas.refresh`），从缓存里读，所以答案回调只需要 `touch()` + 重画。此前盖章写在"视图建好时"，于是答案晚到时要么补盖、要么不盖——而一个公式恰好有**两个投影**（页面一份、公式框一份），"补盖"就变成每个投影各自的义务，漏掉一个的表现正是一个空白变体。

未做：`bb`/`cal`/`frak`/`scr` 还没进 `config/commands.json`，所以 `bb(A)` 建的是**未知名字的调用**（`raw_macro`：光标在外画一张图，进去画名字与参数槽，参数可编辑）——不是 `Raw`。加它们是配置里几行——表不再是障碍。已知限制：`to_style` 可能返回两个码位（`𝒬\u{fe00}`），而 `mathfont.glyph` 是按字符映射的，`cal`/`scr` 这类变体要单独确认 Qt 画变体选择符的行为。

### `MathIdent` 查的是两张表，不是一张

上面讲的 `commands.json` 只接住**调用**（后面有 `(`）。一个**裸标识符**走的是另一条路，而且中间还要先过宏作用域：

```
$RR$  →  MathIdent("RR")
   ├─ ① parse_atom:532   ctx.is_bound("RR")?        ← 宏/定义作用域（不是配置）
   │       否 → 继续
   ├─ ② node.cast::<ast::Expr>()  → MathIdent 不 cast 成任何 Expr → 落到 :643 的 `_`
   └─ ③ :645   symbol("RR")?                        ← config/symbols.json
           ├─ 命中 → Kind::Symbol { name, glyph }
           └─ 未命中 → MathAtom::from_source → 又查一次 symbol() → Kind::Raw
```

所以"名字→结构"一共有**三处**入口，容易混成一处：

| 入口 | 表 | 命中后 | 未命中后 |
| --- | --- | --- | --- |
| 裸标识符，先 | 宏作用域（`definitions` 里的 `#let`） | `MacroCall`（可展）或 `Raw`（不可展） | 继续查符号表 |
| 裸标识符，后 | `config/symbols.json`（40 项） | `Kind::Symbol` | `Kind::Raw` |
| 调用 `name(...)` | `config/commands.json`（14 项） | 借形状的调用（`MacroCall`），或表里没有但实参全是位置实参的 `Kind::Raw`（画成 `raw_macro`） | `Kind::Raw` |

**`Symbol` 不是 `Char`**，这一点常被含混过去：`Kind::Symbol` 的载荷是 `{name, glyph}`，`name` 保留源名、`glyph` 只用于显示（`view.rs`把它放进 `display_glyph`），回写走 `Write::OwnText` 写的是 **`name`** ——所以 `alpha` 存盘回来还是 `alpha`，不会变成 `𝛼`。真正"拆成字符"的是单字母的 `MathText`：`$R$` 是 `Char{text:"R"}`，而 `$RR$` 是 `MathIdent` 整体落 `Raw`（多字母标识符是一个节点，**不是**两个 `MathText`，所以那条 `graphemes` 拆分不会碰它）。

| 输入 | 节点 | Kind | 回写 |
| --- | --- | --- | --- |
| `$R$` | `MathText` | `Char{text:"R"}` | `R` |
| `$alpha$` | `MathIdent` | `Symbol{name:"alpha", glyph:"𝛼"}` | `alpha` |
| `$RR$` | `MathIdent` | `Raw{source:"RR"}` | `RR` |
| `$RRR$` | `MathIdent` | `Raw`（引擎侧 `unknown variable: RRR`） | `RRR` |

`$RR$` 落在最后一行**不是错误**：`RR` 在引擎里是一个符号（黑板粗体 ℝ，`style.rs:150` 的文档自己写着 `bb(N) = NN`），实测 24pt 下 `RR` 与 `bb(R)` 盒子逐字节相同（都是 17.328），而单个斜体 `R` 是 18.792、`R R` 是 37.584。它成 `Raw` 只是因为 `symbols.json` 这 40 项没收它，而这**不是** `cancel`/`vec` 那种缺口：那两个早就在 `commands.json` 里（`decoration` 与 `grid` 形状），建的是借形状的调用，可以编辑；`RR` 才是"符号表没收、只能保留原文由引擎自己画"的那一类。

## 两棵树：可编辑树与显示树

宏调用最容易被误解的一点，是"显示时展开、回写时不展开"看起来需要一个判断。**实际上没有那个判断**——展开产物从一开始就不在被回写的那棵树里。

| | 可编辑树 `MathData` | 显示树 `View` |
| --- | --- | --- |
| `MacroCall` 是什么 | `{name, cells}`——只有名字和实参 | 展开后的模板 |
| 模板从哪来 | **不存** | 每次 `response()` 从注册表现取 |
| 谁读它回写 | `write_atom` → `Write::Named` | 没人读 |
| 生命周期 | 作者编辑、撤销、存盘 | 一次回复 |

```
MathData:  MacroCall { name: "twice", cells: [x] }        ← 唯一权威
                │  view_atom (view.rs:117) 现算
                ▼
View:      macro
             ├─ template = view_cell(&def.template)      ← 从注册表取，不在节点里
             └─ bind_template: 把 Parameter{index} 换成实参 View 的克隆
```

四步机制，缺一不可：

1. **回写只认名字和实参**（`typst.rs:776`）：`Write::Named` 拼 `format!("{name}({})", joined(&atom.cells))`；公式常量那种 `function: false` 连括号都不写，直接 `name`。**模板不在节点里**，所以"要不要写出来"这个问题不存在。
2. **展开是读取时现算**（`view.rs:117`）：`registry.get(name)` 拿定义，节点存名字、注册表存定义——与 `Symbol{name,glyph}`、`Accent{name}` 同一个模式：节点存"指向什么"，内容存在别处。
3. **绑定就是替换占位符**（`view.rs:226`）：模板里的 `Parameter{index}` 换成调用点实参 View 的 `cloned()`；**克隆一份给显示，原实参仍在 `cells[0]`**。
4. **嵌套调用递归替换**（`view.rs:239`）：`TemplateCall{definition}` 按**定义版本号**取模板再整体替换，所以后面的同名定义不会改变早先宏的捕获。

三个让这件事安全的细节：`def.template` 是 `Arc<MathData>`（`typst.rs:39`），模板只有一份、N 个调用点共享，`View` 那一侧才克隆；**参数个数不符就不展开**（`view.rs:124` 要求 `def.params.len() == atom.cells.len()`），定义中途变了就退化成 `macro-collapsed` 显示原文而不是错位展开；**展开有 4096 的上限**（`PROJECTION_LIMIT`），递归宏退回同样的折叠显示。

**由此看清三类构造处在三种状态**：

| | 底层存什么 | 显示 | 谁展开 |
| --- | --- | --- | --- |
| 可展宏 `twice(x)` | `MacroCall{name, cells}` | 模板 + 实参 | 内核（`view.rs:117`） |
| 不可展宏 `foo(x)` | `Raw{"foo(x)"}` | 原文，`edit` 进源码 | 无 |
| 结构命令 `frac(x,y)` | `Fraction{cells}` | 分数排布 | 不适用（形状直接建出来） |

"底层保留函数名、显示时展开"这个统一设想，第一类已经是；第三类不是展开而是**直接建形状**；第二类今天**做不到**，因为 `Raw` 只有一个 `source: String`、**没有名字字段**——要把 `foo` 从 `"foo(x)"` 里切出来只能重新解析那段字符串。这正是 `Raw`（不建模的不透明片段）与 `MacroCall`（有名字有实参）作为两种数据形状的分界。

## 槽位模型

`math::MathAtom` 只保存实例数据（几列、有哪个脚标）；一个节点的**格子含义、视图名、Typst 拼写、所对应的 Typst 构造与导航规则**集中在 `crates/core/src/slots.rs` 的唯一一张表里，由穷尽 `match` 的 `Kind::decl()` 声明 9 项：`view`、`typst`、`slots`、`arity`、`entry`、`horizontal`、`vertical`、`class`、`write`。`entry_cell` / `math_class` / `idx_horizontal` / `cursor::vertical` / `view_atom` / `write_atom` 全部读这张表，不再各自 `match Kind`——加一个 `Kind` 时编译器会要求把这几件事一次说清。

`typst` 那一项是**与 Typst 词汇表的对应关系**：`Kind` 的变体名照着 Typst 的 `MathKind`（`vendor/typst/crates/typst-library/src/math/ir/item.rs`）取，一个 `decl` 可以认领 0 个（编辑器专有：`MacroCall`/`TemplateCall`/`Parameter`/`Unknown`）、1 个或多个（`Raw` 认领 `Box`/`Mathml`/`External`；`Sqrt` 与 `Root` 都认领 `Radical`；`Char` 与 `Symbol` 都认领 `Glyph`）。核心 crate 不依赖编译器，所以两边不能靠类型系统绑定；代替它的是两个测试：一个从 vendor 源码里扫出 `MathKind` 的变体名（`MathKind` 增删改名会让它失败），另一个断言"没被任何 `Kind` 认领的变体"恰好等于 `slots::UNMODELLED`——现在只剩 `Group`、`Primes`、`SkewedFraction`。因此对齐与否是可查的：认领掉一项就必然要改那张表，并在那里写下为什么其余几项还没做。这三项与上一节的"名字表"是**两件事**：`VecElem` 连名字都没进表，而 `Group` 是"编辑器的一个格子就是一个 group"、根本不需要谁去代表它。`cancel` 则是第三类——它靠 `commands.json` 里的一行把 `Accent` 的形状借过来用，`MathKind::Cancel` 因此由 `Kind::Accent` 一并认领（见下节）。`view` 名与变体名**故意不同**（`Kind::Fenced` 的排布名仍是 `delim`）：排布名是给前端的绘图契约，只在画法变化时才需要改。

`Char` 的载荷是**一个字形簇**（`String`，不是一个 `char`），因为"字符"与"Unicode 标量"不是一回事：`é` 可能是一个标量也可能是两个，emoji 常是好几个。词法本来就把一个字形簇收进一个节点，`GlyphItem` 也装一个簇——按标量拆会让回写在簇中间插入分隔符，把 `é` 写成 `e ́`。`Kind::Number` 同理是"一个格"，串内字符由 `Write::Run` 连成一个记号。

**命令名在 `config/commands.json` 里**，`crates/core/build.rs` 把它和 `config/symbols.json` 一起生成成 `COMMANDS`/`SYMBOLS` 两张表。文件里的值是 **`Kind` 的 `view` 名**（`frac` → `fraction`、`mat` → `grid`、`hat` → `decoration`），因为视图名是给前端的契约、比 `Kind` 变体名稳定（`Frac` 改名成 `Fraction` 不影响这个文件的意思）。`slots::kind_for_view` 把视图名翻译回 `Kind`。

这条配置只回答一个问题：**"这个名字，编辑器有没有对应的结构"**。它不回答参数个数（那是 `Decl::write` 的拼写里数出来的：`frac({0}, {1})` 是两格）、不回答拼写（也是 `write`）。所以它不是第二张表，而是"编辑器认识哪些命令"这**一个**事实的存放处——`slots.rs` 的两条单元测试把它钉在这里：每个名字必须指向一个真实存在、且视图名与声明一致的 `Kind`，反之每个"命令能建的 `Kind`"也必须有名字。

一处不能从配置到达：**`grid` 不在 `kind_for_view` 的可达视图里。** 表格的 `columns` 来自它的参数列表（几格一行），而这件事只有 `parse_atom` 里那条建表的**分支**知道，所以 `grid` 是**按命令名**走的，不是按形状名。区别有实测意义：按形状名放行时 `"cases": "grid"` 能通过检查，而那时这个名字既查不到、也没有解析分支，写回时会带着一个从未被填过的 `columns` 走到 `chunks()` 上。检查因此按名字做（`slots.rs` 的 `source_built_commands` 白名单，现在是 **`mat`、`vec`、`cases`** —— 三个名字共用 `grid` 形状，差别只有"怎么切行"和"外面画什么"，两件都写在名字旁边）。

「实测改一行配置（`"cancel": "line"`）就能让 `cancel(x)` 从 `Raw` 变成 `line` 节点」这句要连同上面第三节一起读：**只有调用写法**会被这条路接住，而且把一个名字指向 `line` 是类型上合法、语义上错误的——它会得到一条位置取自 `name == "overline"` 判定的规则线。这正是"配置能改什么"的边界。

这条例外值得说明：`config/` 是仓库的，crate 是 `crates/core/`，所以构建脚本用 `CARGO_MANIFEST_DIR` 定位 `../../config/`，并且把 `cargo:rerun-if-changed` 指到真实文件上，改配置会触发重建。

四处值得单独记：

- `Entry::Role` 按**角色**指定光标首次进入的格子，因此根式把"根指数"排在"被开方式"前面。
- `Vertical::Swap { end_up }` 区分上下键换格后落在格首还是格尾：分式落格首、根式落格尾。
- `Write::Template("root({1}, {0})")` 把"内部 `[被开方式, 根指数]` 与 Typst 的 `root(index, radicand)` 相反"写成一行声明，取代过去分散在解析（`args.swap`）与回写（`c(1), c(0)`）两处的隐式约定。模板必须单遍展开，否则格子源码里的花括号会被当成占位符。
- `Kind::Scripts` 的存储固定为 `[base, upper, lower]` 三格（`math::script_cell` 是唯一的格索引来源），空格子表示没有该脚标；视图因此不再需要合成缺格。Typst 的 `ScriptsItem` 有六个附件字段，因为它区分"居中极限"与"侧挂脚标"、并且保留左侧附件；一个格子属于哪一种由编译器决定、单独去问（`native-adapter`），不存在这里。

视图节点还带一个 `role`：父节点声明的**槽位角色**（`numerator`/`denominator`/`base`/`upper`/`lower`/`radicand`/`index`/`inner`/`cell`/`arg`）。前端 `mathview.py` 按角色取子节点，位置只作回退，所以一个复用已有排布与角色的新 `Kind` 不需要改前端。

### 两个名字：形状名与线名

`Decl::view` 是**形状名**，不是线名，两者可以不同：

* 形状名要**每个 `Kind` 唯一且稳定**，因为 `config/commands.json` 写的是它（`slots.rs` 的测试就钉着这条），所以 `Kind` 变体改名不该动它（`Frac` → `Fraction` 之后仍是 `fraction`）。
* 线名由 `view_atom` 算出来，因此**可以把几个形状合并到一个线上 kind**，而它就合并了：`Sqrt`、`Fenced`、`Accent`、`Line` 一律以 `decorated` 上线，靠 `marker` 区分画法。

这不是审美问题，是四者的**编辑声明逐项相同**（1 格、`Arity::Exact`、`Entry::Edge`、`Horiz::Linear`、`Vertical::None`、`class` 0），只有 `write` 与画法不同。线名按"一条独立的编辑或显示逻辑"划，形状名按"配置能指向什么"划，两者各自成立。

| 形状名（`Decl::view`） | 线名（`view_atom`） | 额外字段 |
| --- | --- | --- |
| `fraction` | `fraction` | `marker: "-"` |
| `sqrt` | **`decorated`** | `marker: "radical"` |
| `delim` | **`decorated`** | `marker: "delim"`（字符仍在 `text`，左、换行、右） |
| `decoration` | **`decorated`** | `marker`: 重音名，如 `"hat"` |
| `line` | **`decorated`** | `marker: "overline"` / `"underline"` |
| `root` | `root` | `marker: "radical"` |
| `script` | **`scripts`** | |
| `grid` | **`table`** | `border: "()"`、`is_mat: true` |
| `aligned` | **`multiline`** | |
| `macro`（折叠时） | **`raw_macro`** | |

`marker` 的词汇表是**与前端约定的封闭集**（`mathview.py` 的 `MARKERS`）：前端按它选画法，遇到不认识的值报告一次而不是静默画错——与 `ARRANGEMENTS` 同一套约定。`border` 同理：`mat` 在引擎里的默认定界符是一对圆括号，而前端过去**给每张表都画方括号**，正是因为线上一律不带定界符；现在带上了，两边才对得上。

### `raw_macro` 覆盖什么：**所有非结构调用**

判据只有一条，而且它是语法层面的：**一个调用的实参是否**全部是位置实参**。

| 情况 | 节点 | 例子 |
| --- | --- | --- |
| 名字在配置里 | `MacroCall`，**借用那个形状** | `frac(a, b)`、`sqrt(x)`、`vec(1, 2, 3)` |
| 名字**不在**配置里，实参全是位置实参 | `MacroCall`，**形状未知 → `raw_macro`** | `bb(A)`、`binom(n, k)`、`text("hello")` |
| 有具名实参 / 展开 / 尾分号 | `Raw`（一个格子表拼不回原样） | `lr(x, size: #100%)`、`mat(x, delim: #none)` |
| 名字是个绑定宏 | 宏的那条路（可展 → `macro`，否则 `raw_macro`） | `#let f(a) = …` 之后 `f(x)` |

最后一项是**放宽之后的行为变化**：从前"实参落在 `Raw` 里"是让宏不可展的判据（`#f` 在 `Raw` 里 ⇒ 没有槽位可给参数），而未知调用**就是** `Raw`，所以一个内部有未知调用的宏整体不可展。现在那个内部调用是 `MacroCall`（画成 `raw_macro`、参数仍可编辑），参数进了格子，于是**展开是安全的**——`source_modes.rs` 里 `jac`（内部调未定义的 `pd`）从"不可展"变成"可展，展开结果里 `pd(…)` 是 `raw_macro`"。

这一条也是为什么 `Raw` 仍然存在：**它不是"没有形状的调用"，而是"格子表表达不了的源码"**。`lr(x, size: #100%)` 永远是 `Raw`，因此仓库里"一个没有图像的片段"的范例从 `undefinedfunc(α)`（现在可解析、成了结构节点）换成了**裸标识符** `undefinedname`——标识符永远不会变成调用，所以它是最稳的那个范例。

`raw_macro` 本来只覆盖"投影超限"，见 `tools/kind_inventory.py` 的 `RawMacro` 用例（15 层翻倍链）；放宽之后那条路径仍在，只是不再是它唯一的来源。

### `raw_macro` 有两种画法，取决于光标在哪

它是唯一一个**画法随光标位置变**的节点，而这是它可以做到的原因：两种画法来自**同一棵树**，区别只在用不用孩子。

| 光标位置 | 画法 | 用什么 |
| --- | --- | --- |
| 在节点**外** | 调用**本身的一张图**（就是文档里那一块） | `text` = 调用的拼写，交给引擎编译 |
| 在节点**内** | 宏名 + 若干个参数槽 | 视图里的 children（`symbol` 与各实参格） |

后端为此做两件事：`view.rs` 把 `text` 从**文案**换成**调用的拼写**，并挂上 `edit` 光标；`document.rs` 的 `annotate` 因此把它和 `raw` 一样对待，算出它在文档里的区间并给一个 `render_id`。实测：

```
raw_macro: text="layer15(a)"  render_id="684:694:0:0"  edit={slices:[],pos:0}
render.raw: [{start:684, end:694}]
```

那句被换掉的文案（`"展开较大，显示调用与参数"` / `"参数个数与定义不符"`）**此前没有任何读者**——前端只排 children，从不读 `text`，所以这个字段本来就是空的，正好让给源码。

前端一侧有两处配套：`mathview.image_box` 抽出来给两种 kind 共用，`raw_macro` 在**未激活**时直接返回那张图；`window.raw_fragments` 做一次**带祖先的**遍历，**跳过塌缩调用内部的片段**——整段调用是一张图，它的实参槽里的片段在光标进入之前不单独取图（否则会白编译一遍，而且和那张整图重复）。

`_active` 不是新机制：`MathCanvas.refresh` 早就把光标路径上的节点标出来了，所以"光标在不在里面"这个问题前端本来就有答案。

### `cancel` 只加了一行配置，代价全在夹具上

`cancel(x)` 的形状就是 `hat(x)` 的形状——一个正文格 + 一个画在它上面的记号——所以它**不需要新的 `Kind`**，只需要在 `config/commands.json` 里加一行 `"cancel": "decoration"`（`Kind::Accent` 已经会把命令名存进 `name`，拼写也已经是 `{name}({0})`）。实测：`cancel(x)` 变成 `decorated`/`marker="cancel"`、一格、写回 `cancel(x)`，往返成立。

引擎侧量到的两件事决定了前端怎么画：默认记号是**内容框的上升对角线**（`CancelItem` 的 `length` = 对角线 + 0.3em），而且 **`cancel(x)` 与 `x` 的盒子完全相同**（24pt 下都是 13.728×10.872），而 `hat(x)` 是 16.92、`overline(x)` 是 16.056——所以记号是**盖在**正文上、盒子不长高，这与 `hat`/`overline` 那两条"抬起身子腾地方"的分支相反。

**真正的工作量在测试夹具上，而且不在配置一侧。** `cancel` 曾经是仓库里"编辑器不建模的片段"的**规范示例**，同时承担两种用途，落地时红了 **21 个测试**（13 个 Rust + 8 个桌面）：

| 用途 | 曾经的写法 |
| --- | --- |
| Raw/SVG 管道的片段示例（取图、缓存、`_raw_key`、`definition_raw_ranges`、源码区间） | `document.rs`、`macro_scope.rs`、`desktop.rs`、`test_desktop.py` |
| **让宏变成不可展的惯用写法**：把形参放进 `Raw` | `#let f(x) = $cancel(#x)$` → `参数引用位于 Raw 中，无法在原位编辑` |

第二种最要紧：`cancel(#x)` 是"造一个不可展宏"的手段，它结构化之后那些用例就失去了构造方式。替代范例换成 **`lr(x, size: #100%)`**（按 `git grep -F 'lr('` 数：14 个文件、80 处）：`lr` 带具名实参，而解析器对**非位置实参一律退回原文**，所以它保持 `Raw` 是由语法保证的，不只是"暂时没进配置"。`rawcache.contains_call` 也仍然认它（`标识符` 紧跟 `(`）。

两类容易漏的坑：一是**硬编码的长度**——`test_desktop.py` 里 `definition+9` 是照着 `cancel(a)` 的 9 个字符写的，换范例后静默变成错的偏移（现在改为从字符串本身取长度）。二是**不会变红而是静默失去被测对象**：`round_trip.rs` 与 `structured_input.rs` 的 Raw 清单里也躺着 `cancel(...)`，它们会继续通过，但已经不在测 Raw 了。所以 `tests/round_trip.rs` 另加了一条直接断言——`cancel` 之所以是结构节点**只因为配置文件写了它**，而往返测试看不见这件事（删掉配置项后它变成 `Raw`，照样逐字节往返），与 `math::is_number` 是同一类盲区。

前端另有一张 `ARRANGEMENTS` 白名单：遇到不认识的排布**报告一次**（经 `Typesetter.warn` 到状态栏），而不是静默按横排画错。

回写仍是最需要兜底的一环，因为它的结果会写回权威源码。`tests/round_trip.rs` 对 **15** 个可往返 Kind 逐一验证"写出去、读回来、必须等于原树"（`TemplateCall`/`Parameter` 只存在于宏模板，`Unknown` 是命令草稿，三者没有 Typst 拼写）。`tests/caret_navigation.rs` 把表里每一条导航声明钉在真实光标位置上；`slots.rs` 的单元测试保证每一条声明的形状与它自己的 `Kind` 相符。

每个 `Kind` 实际给前端提供了什么、对应的引擎 item 又有什么、两者差在哪，逐条列在 `docs/kind-inventory.md`（从真实后端与真实适配器取回，不是读代码推的）。

## 整页预览

原生 `RenderRequest.preview` 为 true 时，直接编译原文，不插入结构编辑器的 24pt 字号或映射标签。FontStore 加载随附字体及系统字体；日期由系统提供。已打开的项目内文档与未保存的 import/include 依赖作为 overlays 传入。页面尺寸、字体和正文样式由原文决定。窗口按需请求整页 SVG、保留上次成功页面并显示错误，页面尺寸按编辑区宽度缩放。

## 当前边界

- 公式静态投影在进入公式后缓存；源码输入 `$` 不自动变成控件。
- 宏的结构展开仍沿用受限静态分析，任意 Typst 求值由引擎处理。
- Raw SVG / 附件位置沿用按需刷新策略；不保证任何源码修改后都自动刷新全部数学投影。
- 代码高亮为轻量 Typst token 高亮，诊断和语义查询由 Tinymist 提供。
- 当前预览没有到源码的双向定位，也没有包卸载；文件管理使用原生对话框，不提供目录创建/重命名。
