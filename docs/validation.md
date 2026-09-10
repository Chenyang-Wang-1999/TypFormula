# 验证记录 · 2026-09-09

## 原生 Qt 桌面端

后续桌面交互回归扩展至 32 项：增加源码补全弹窗 Enter 接受与撤销、真实公式命令投影补全、新公式源码自动触发、命令/字符串衬底、四向边界跳出、Raw 源区间移动仍复用 SVG、离开修改过的脚标槽时仅刷新基底 Raw、变量行高与源码行号，以及预览周期不请求 Raw、不重建编辑投影。均为离屏测试。

增量公式索引回归覆盖 80/60/200 公式文档：普通正文编辑使用 Typst `Source::edit` 返回的局部重解析范围，200 个公式时重建 0 个公式且不调用全量 `analyze`；单公式编辑仅重建局部少量公式；`let` 修改会重建后续受影响宏投影。未受影响的 Qt 文本块和公式对象 ID 也保持不变。测试机上，200 个公式的旧全量公式投影本身约 395ms；常驻 Equation AST 加空宏注册表快路径后首次投影约 210–220ms；增量后的完整编辑约 75–95ms，剩余主要是 Python 投影映射和 Qt 局部格式更新。

桌面离屏回归现为 44 项。新增检查相同 Raw 源码只产生一次原生请求并共享同一结果、跨公式的不同 Raw 合并为一次编译，以及同一个 SVG 连续绘制两次只创建/执行一次矢量渲染器，第二次使用 DPI 位图缓存。脚标修改仍会使对应共享 Raw 失效；未变化页面协议复用原 Qt 控件。

桌面窗口已移除实时预览 Dock 和自动页面编译。专项测试确认后台编辑周期不会请求 `/api/preview`，F5 路径调用 `/api/pdf`，返回值由 `typst-pdf` 生成且以 `%PDF` 开头，并通过系统默认阅读器 URL 打开。原生适配器回归现为 16 项。

原生预览回归 15 项通过，新增页面哈希协议测试。31 页、8.58MiB 的合成预览在 release 引擎中首次约 315–337ms、完整热输出约 233–242ms；带相同页面哈希的下一次请求约 1ms、3.7KiB，31 页均跳过 SVG 和源码映射导出。此前 debug 热输出约 1.13s。正式 `build-desktop.cmd` 已切换为 release 后端。

- `python -m unittest desktop.test_desktop -v`：21 项通过。使用离屏 Qt，不打开可见应用窗口；覆盖源码/UTF-16 映射、实际键盘输入、公式选区与撤销、双栏同步、宏预热、自然尺寸滚动、文件保存、真实 Tinymist 高亮、预览映射及多页导出。
- SVG 回归复现 Qt 5 拒绝 glyph symbol 的 `link … is undefined`，兼容转换后无警告，并验证字形位置和颜色像素。
- Rust 全量回归通过；新增 `tests/desktop.rs` 两项检查覆盖静态投影保留活动会话、不可展开宏保留源码和 AST 样式。
- 原生引擎 14 项测试通过；已有 Web/VS Code 前端 22 项测试通过。
- Rust 核心和原生渲染器构建完成，桌面入口为 `start-desktop.cmd`。运行与实际输入法体验交给用户。功能和当前限制见 [desktop.md](desktop.md)。

本次只构建和运行自动化测试，应用启动与浏览器实际体验由用户执行。

## 按 Raw 环境字号归一化

- 原生渲染回归 14 项通过，新增验证：12pt/36pt 归一化结果相同、两级上下标保留较小尺寸且基准字号不变、同一宏 Raw 的两次出现分别返回环境字号、Raw 内部局部放大不被错误消除。
- 前端回归 19 项通过，校验后端归一化字段、旧手填文章字号不再影响结果、逐出现位置和会话缓存隔离，以及编辑字号缩放。
- 字段约定：`base_font_size_pt = width / environment_font_size_pt`，高度和基线各有对应归一化字段；`_pt` 后缀是兼容命名，这些比值没有单位。
- 未自动启动 Web 应用；更新原生渲染器后需重启服务并刷新页面。SVG 提示文字显示环境字号和归一化宽度，方便体验时对照。

## Web 字号与文件选择

- 19 项 Node / WASM / jsdom 测试通过：新增编辑字号、文章字号、SVG 微调倍率计算，以及原生文件打开、保存和另存为句柄写入。
- Rust 核心 64 项通过；原生 Typst 渲染 10 项通过。stretch 对照改为文章默认 11pt，验证 Raw 映射仍保留原生排版宽度。
- Web 前端构建通过。未启动浏览器；系统文件菜单的外观、权限提示和 Ctrl+滚轮手感仍需实际浏览器体验。

## 公式视口修复 0.2.1

- 前端构建通过；17 项 Node / WASM / jsdom 回归通过。
- 新增覆盖：横纵双轴滚动露出槽位、可见槽位不扰动手动滚动、光标边界裁剪、预览改变编辑列宽度时更新公式视口上限。
- 本次只修改前端和扩展版本，原生程序复用 0.2.0 已验证产物。
- 产物为 `dist/visual-typst-0.2.1-win32-x64.vsix`。没有启动 VS Code；真实滚动条、嵌套公式字号和视觉基线仍需安装后体验。

## VS Code 扩展 0.2.0

- Rust 核心：64 项通过，3 项本机服务集成用例保持忽略。
- 原生适配器：10 项通过，新增整页尺寸、原文无注入、未保存 import 和日期编译测试。
- Node / 真实 WASM / jsdom：13 项通过。覆盖四方向目标/槽位选择、左右键真实 DOM 跳入、统一命令覆盖、快捷键修饰符、预览失败保留页面、异步回传丢弃、TextDocument 版本检查、连续输入确认和冲突处理。
- Webview 宿主消息路径通过模拟 VS Code 宿主测试；不启动 VS Code，不以该测试替代真实 Extension Host 集成验证。
- 前端、原生程序构建和 VSIX 打包通过。产物 `dist/visual-typst-0.2.0-win32-x64.vsix`，含两个原生程序、WASM、前端和字体，无仓库运行时依赖。

尚未执行：VSIX 实际安装、真实 VS Code 快捷键路由/输入法/像素布局、实际 Tinymist 语言提供器与联网包安装。用户自行启动体验。

## 独立版首次验证

| 检查 | 结果 |
| --- | --- |
| 根 crate Rust 回归 | 64 通过，3 个本机服务集成用例默认忽略 |
| 原生 Typst 适配器回归 | 9 通过；包含透明映射、stretch、上下标、错误恢复和子目录相对 import |
| Node / 真实 WASM / jsdom 集成 | 5 通过；包含单一 CodeMirror、唯一活动公式控件、全局撤销、文件初始加载、草稿与行内/行间插入 |
| WASM release 构建 | 通过；已更新 web/core.wasm |
| 前端 esbuild 打包 | 通过 |
| 本地服务与原生适配器最终构建 | 通过；产物分别位于 target/server、target/adapter |
| 正式版路径审计 | 20 份 Cargo manifest 的 24 个本地路径均存在且位于正式版内，无 prototype 依赖；正式版目录无软链接 |

尚未实测：真实浏览器中的视觉基线、输入法与滚动体验，实际 Tinymist 的诊断/补全/跳转/格式化，以及官方包联网安装。对应功能已经实现；上述项目不计入已验证结论。jsdom 验证 DOM 和编辑事务，不验证像素排版。

建议启动后先体验：中文正文中插入两个行内公式，再插入一个行间分式，来回切换并跨正文/公式撤销；随后检查 Tinymist 状态、搜索包并插入 import。运行入口和配置见根 README。
# 宏作用域与预热验证

- Rust 回归覆盖内外层遮蔽、作用域退出、函数参数遮蔽、定义时捕获，以及预热失败不改源码/版本/撤销状态。
- 原生服务集成覆盖无真实调用的 SVG 预热、显式空字符串、单个失败隔离、内容块的 23pt 环境、代码块内宏、`#amount` Raw 映射及重复缓存读取。
- Node / 真实 WASM / jsdom 共 22 项通过，包括无真实调用的宏定义显示预热 SVG、重复 Raw 区间隔离、旧响应丢弃、草稿期间延迟降级。
- 本轮不启动 Web 应用；真实浏览器的视觉效果仍由用户试用确认。

# 结构编辑健壮性、扩展宿主与桌面渲染修复 · 2026-09-10

## 已修复并回归

- **命令草稿不再写回源码。** 草稿期间的序列化只留在编辑会话里，Enter 提交一次、Esc 恢复原文；`tests/document.rs::a_command_draft_never_reaches_the_authoritative_source` 逐键断言源码不变。文档回写的判据也从"编辑器上次序列化"改成"与源码区间实际文本不同 + 树结构确实改变"，因此 `$ a/b $` 这类非规范拼写不会因为一次方向键被改写（`normal_source_is_lossless_even_when_incomplete`）。
- **宏定义变化统一走整格重解析。** 公式内确认 `#let` 后，调用按新定义重新分类；参数个数不再匹配的调用退化为 Raw，不再出现投影期越界 panic（`a_definition_confirmed_inside_a_formula_reclassifies_the_whole_cell`）。View 构建对参数个数与注册表索引都做了边界检查，不匹配的调用显示为"调用 + 参数"。
- **前端几何不再被当成光标来源。** `geometry` 输入只保留通过 `valid()` 的站点，垂直导航取点时把位置钳制到所在单元格；被拒绝的动作连同它的撤销步一起回滚（`lyx_traces.rs::stale_layout_stops_cannot_move_the_cursor_out_of_its_cell`）。跨巢 Shift 点击的选区也会钳制到单元格长度，不再产生越界光标（`structured_input.rs::shift_clicking_into_a_nest_clamps_the_selection_to_the_cell`）。
- **WASM 桥的所有权显式化。** `alloc` 记录地址与长度，`dispatch` 只接受与分配长度一致的调用；重复地址与伪造长度返回 `{"error": …}` 而不是二次释放或越界读。两项单测在宿主上运行（`cargo test --lib`）。
- **扩展产物同源。** `commands.json` 与 `package.json` 由 `node scripts/build-vscode.mjs` 重新生成后提交（33 条命令，package.json 34 个命令）；构建脚本导出清单供测试比对，新增三项 Node 测试：清单与 `config/editor-commands.json` 同源、`web/*.js` 绑定的元素都存在于 `web/index.html`、`extensions/vscode/media/` 与 `web/` 逐字节一致。`web/editor.js` 的工具栏绑定改为按 id 查找，缺元素只让一个按钮失效，不再让整个模块在求值阶段抛错。
- **扩展撤销改由宿主回放本文档历史。** VS Code 自带的 `undo`/`redo` 命令取 `getFocusedCodeEditor() || getActiveCodeEditor()` 并对该代码编辑器模型调用 `undo()`（见本机 VS Code 1.136.1 包内实现），Webview 自定义编辑器既到不了自己的 TextDocument，还可能误改另一个打开的文件，因此不再转发。宿主在 `extensions/vscode/src/history.cjs` 里保存本文档出现过的每一份文本（上限 200 步），用 `WorkspaceEdit` 回放，TextDocument、脏标记与版本仍是唯一权威；`tests/extension.test.mjs` 在模拟宿主下验证撤销/重做与"不调用工作台命令"。
- **`--stdio` 后端与 HTTP 一样并发。** `src/rpc.rs` 每个请求一个线程、整行加锁输出，`extensions/vscode/src/backend.cjs` 去掉客户端串行链。实测 1200 段文档的整页预览耗时 14.4s，期间 `/api/status` 在 8ms 返回；改动前该请求要排在预览之后。
- **前端资源与错误处理。** 旧编译快照的预览记录随会话释放并回收 Blob URL（不再随按键累积）、覆盖前释放旧 URL、Raw 附件结果表设上限、Ctrl 组合只拦截结构编辑器实现的那几个（Ctrl+F / Ctrl+P 等交还浏览器）、宏预热失败显示在状态徽标；文档同步的传输与大小错误不再伪装成"文档在另一处发生修改"（`web/document-sync.js` 新增失败通道）。

## 桌面端（LyX 式第一阶段）

- 公式 Box 按视图缓存：一次布局/绘制周期内 Qt 对同一公式的 `intrinsicSize` 与 `drawObject` 只排版一次，鼠标命中复用同一结果；Raw SVG、附件位置、编辑字号与 SVG 倍率变化时失效。
- 字体与度量按字号共享一份 `QFont`/`QFontMetricsF`。
- 增量合并改为写时复制：编辑点之前的公式与未移动的子树与上一份投影共享，不再每次按键深拷贝全部公式；`test_desktop.py` 断言未移动视图被共享、移动后的新节点不修改上一份分析。

67 节点公式的离屏实测（Python 3 + 离屏 Qt，debug 构建）：一次 `layout` 0.286ms，命中 Box 缓存 0.0012ms（约 240 倍）；单次 `QFont`+`QFontMetricsF` 构造 0.0009ms，按 67 节点计算约 0.059ms，共享后降到 0.018ms，相当于每次布局省约 14%。

## 本轮本机实测

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | 79 通过，4 个本机服务集成用例忽略 |
| `cargo check --offline --locked --features server` | 通过 |
| `cargo test --manifest-path native-adapter/Cargo.toml --target-dir target/adapter` | 16 通过 |
| `npm run build` + `node scripts/build-vscode.mjs` | 通过；`media/` 与 `web/` 四个产物逐字节一致 |
| `npm test` | 27 通过 |
| `python -m unittest desktop.test_desktop` | 44 通过（离屏，12.6s） |

尚未实测：真实 VS Code Extension Host 中的撤销/重做、快捷键路由与像素布局，以及 VSIX 安装；宿主历史回放只在模拟宿主下验证。`target/adapter/debug/incremental/visual_typst_layout-10leqeo7xetbb` 在本机文件系统上已损坏（`os error 1392`），普通删除与重启后删除计划都失败，会导致递归扫描该目录的工具报错；需 `chkdsk D: /f` 处理，与仓库内容无关。

# Rust 核心与桌面的热路径修复 · 2026-09-10

只涉及 Rust 核心与 `desktop/`。同一台机器、debug 构建、300 公式 / 14.2 KB 文档，改动前后对照（`tests/zz_measure.rs` 与一段临时桌面对照脚本测得，测完即删）：

| 调用 | 之前 | 之后 |
| --- | --- | --- |
| `Document::equations()` | 0.69 ms | 0.006 ms |
| `Document::equation_nodes()` | 0.69 ms | 0.012 ms |
| `Document::response()`（单公式） | 1.96 ms | 1.31 ms |
| `desktop::analyze_formula`（每按键重建一个公式） | 2.86 ms | 1.17 ms |
| `desktop::analyze`（第二次遍历同一文档） | 1130 ms | 420 ms |
| `load_raw` 可见性扫描（201 公式 × 201 对象） | 6.2 ms | 1.9 ms |
| `background` 附件扫描（每公式 × 每视图节点） | 10.0 ms | 0.08 ms |
| `apply_highlights`（2352 span，源码 dock 关闭） | 151 ms | 49.5 ms |

改动内容：

- **公式索引按语法树代次缓存。** `Document` 保留 `syntax_revision` 与一份 `Vec<(Equation, SyntaxNode)>`；`set_source`、`edit_source`、以及结构编辑写回活动区间这三处修改语法树的地方都会递增代次。新增单测 `document::tests::the_equation_index_follows_every_source_change` 覆盖三种路径，并把缓存结果与独立重走一遍语法树的结果逐一比较——这条测试立刻抓出了"写回时忘记递增代次"导致的 `active` 区间越界，已修复。
- **宏注册表缓存改为进程内共享、可容纳整份文档。** `MacroDefinition.template/context` 由 `Rc` 改为 `Arc`，缓存由 thread-local 单槽改为 `Mutex<Vec<(前缀, Arc<MacroRegistry>)>>`（8 MB 前缀预算 / 4096 条目，超出按最旧淘汰），选取 `previous` 时优先最长匹配前缀、否则回退最近使用的一份。这样 HTTP 与 `--stdio` 每请求一线程时不再每次重建（之前 thread-local 缓存等于每请求冷启动），桌面端第二次遍历同一文档由 1130 ms 降到 420 ms。`warmup` 失败表也改为进程级，`set_warmup_results` 仍在更新后清空注册表缓存。
- **`failed_previews` 加上限（256 条，超出整体清空）。** 它只决定哪些 Raw 在边界键上进入源码编辑，键里含整个定义前缀，长会话会持续增长。
- **`services.rs` 的 `url::Url::parse(..).unwrap().to_file_path().unwrap()` 改为返回错误**，不再在内部 URI 异常时 panic。
- **桌面端两处 O(公式 × 文档) 循环。** 附件请求改为按视图缓存（视图对象在写时复制投影下恰好存活到该公式之前的文本改变为止，因此定义前缀可与视图一起缓存），可见性判断改为先把各编辑器的对象按公式起点分组；A/B 对照如上表。
- **语义高亮。** 试验过"缓存已构建的 ExtraSelection、靠 QTextCursor 跟随编辑"的方案，实测无效——每次编辑都会清空 span 列表、LSP 回复再整体替换，签名必然变化、缓存永不命中，因此回退该方案，只保留两项安全的优化：每个视图（而非每个 span）解析一次颜色格式，且跳过隐藏的源码 dock（`visibilityChanged` 时立即重新着色）。新增测试 `test_semantic_highlights_follow_an_incremental_edit` 固定了 span 的单位约定（Python 源码下标 → 各视图的 Qt UTF-16 位置）以及"编辑后所有视图同步平移"的行为。
- **一条测试的断言方式**：`definition_edits_reuse_only_the_unchanged_prefix_and_rebuild_dependents` 原先用 `Arc::ptr_eq` 断言"前两个定义共享模板指针"。注册表缓存改为进程全局后，`previous` 由哪个条目提供不再由单个测试独占（测试线程并行、缓存共享），该断言会随机失败；改为比较模板与定义名/形参本身。复用规则本身是安全的：它按定义逐个比较该定义的源码文本。

本机实测：`cargo test --offline --locked` 80 通过 / 4 忽略；`python -m unittest desktop.test_desktop` 45 通过（离屏，12.9s）；`npm test` 27 通过（用重建后的 `web/core.wasm` 验证 `Arc`/`Mutex`/`OnceLock` 在 wasm32 上可用）；wasm / server / adapter 三个构建通过。

仍然存在、但属于设计问题而非本轮修复目标的一项：**首次 `analyze` 仍是 O(公式数 × 定义前缀)**。该文档 301 个不同前缀的注册表构建共 762 ms，占首次 1130 ms（debug）的 67%。

`analyze` 只有三个调用点，都在 `desktop/window.py`：`load()`（打开/新建文档）、`update_analysis()` 里 `analyze_formula` 返回 None 或合并结果找不到该起点时的兜底、以及 `background()` 收到宏预热结果且分类确实变化（`classification_changed`）时。也就是说它基本是**打开文档的开销**（外加一次预热分类变化），普通输入与单公式编辑都不会走它——测试里 `assertNotIn('analyze', calls)` 就是这条约束。按 release 构建（`build-desktop.cmd` 的实际产物）实测：

| 文档 | 一次 `analyze`（冷） | 再遍历一次（热） |
| --- | --- | --- |
| 300 公式 / 14.2 KB | 241 ms | 134 ms |
| 1500 公式 / 72.8 KB | ~8.0 s | ~8.0 s |

1500 公式那一行里，1501 次 `macro_registry` 合计 2.6 s（`analyze_macros` 约 1.7 ms/次，即每个公式都要重新解析并遍历"它之前的整段源码"），其余约 4.5 s 是每公式的 `activate_equation` + `response()` + JSON（约 3 ms/个）。这个文档的公式密度很高（约 48 字节一个公式），但结论是：打开时间随"公式数 × 前缀长度"增长，属于超线性。可行的方向是让注册表按"绑定上下文"（到最后一个 `let` 为止）缓存，并把尾随文本从 `def.source` 里拆出去——这需要动 Raw 映射语义，因此留待单独设计。本轮同时给缓存加了饱和判定：当前缀集合装不下预算时，缓存会**丢弃已存条目并停止写入**，避免"留着反而更慢"（实测 1500 公式热遍历曾比冷遍历慢约 1 s，源于保留 8 MB 字符串带来的分配压力）。

# 编辑器公式字体与 Raw 片段渲染 · 2026-09-10

只涉及 Rust 核心与 `desktop/`。两件事都来自同一次反馈：编辑器里的公式"有点丑"，打开大文档时"很多 Raw 片段"。

## 编辑器公式字体

症状是字体难看，根因有两层。

第一层，也是主因：`desktop/mathview.py` 向 `QFont` 请求 `"New Computer Modern Math"`，而随附的 `web/fonts/NewCMMath-Regular.otf` 在 Qt 中注册的家族名是 `NewComputerModern Math`（少一个空格）。Qt 对找不到的家族名**静默替换**，实测该请求被解析成 `宋体`（`QFontInfo(QFont("New Computer Modern Math")).family()`，`exactMatch()` 为 False）。于是同一个公式里，数字、括号、`+` 由宋体绘制，而宋体没有的 `U+1D44E`（数学斜体 a）、`≤`、`−`、`∑` 等再由 Qt 逐字回退到 Cambria Math 之类的系统字体——一个公式里混了三四套设计，这才是"丑"的来源。

第二层是个陷阱：随附数学字体的行度量是 TeX 尺寸。`NewComputerModern Math` 在 12pt 下 `QFontMetricsF.ascent()` 为 99、`height()` 为 185（它必须容纳四层高的定界符），而布局此前把 `metrics.height()` 当 em、`metrics.ascent()` 当基线。只把家族名改对，公式框会立刻变成几乎空白的巨框。因此把**结构行度量**与**字形绘制**分开：em、基线、下降部取编辑器正文字体（`font_family`，默认 Consolas，12pt 下 26/33），字形与前进宽度取数学字体。

改动：

- 新增 `desktop/mathfont.py`。`install()` 与 LyX 的 `FontLoader` 一样在启动时注册随附字体，并**读回 Qt 报告的真实家族名**而非假定文件名；`resolve()` 只在"Qt 真的拥有"的家族里按 `math_font` → 随附数学字体 → 系统数学字体 → 正文字体的顺序挑选，未安装的名字绝不交给 `QFont`；`glyph()` 把数学变量映射到 Unicode 数学斜体区间（`a→𝑎`、`h→ℎ`，U+1D455 未分配），文本单元与其它字符保持原样，所选家族没有该区间时退回普通字母。
- `Typesetter` 拆成 `line(factor)`（结构行度量）、`font(family, factor)`（绘制字形）、`run()` / `source_run()`（一次文本运行取哪个字体、画什么字、多宽）。
- 新设置项 `math_font`（默认 `NewComputerModern Math`）。它必须是有数学字形覆盖的字体：换成一款正文文本字体后，`≤`、`∑` 这类字符在该字体里没有字形，Qt 又会逐字回退，等于退回本次修复前的问题。

没有搬 LyX 的 BaKoMa 字体，原因是它们不适合 `drawText`：LyX 的 Qt 界面确实用 `lib/fonts/` 下那 12 个 Computer Modern TTF（`src/frontends/qt/GuiFontLoader.cpp:37` 把它们注册进 `QFontDatabase`），但它们是 TeX 编码——`cmsy10` 的 ≤ 在 TeX 槽位 0x14、`cmex10` 的 ∑text 在 `0x50`（即字符 `X`），cmap 里没有 U+2264/U+2211。Qt 只能按 Unicode 取字形，要用这些槽位就得改字体 cmap 或改用 `QRawFont.pathForGlyph` 按 GID 画路径（后者放弃 hinting，反而更糊）。而随附的 New Computer Modern Math 正是 Typst 自己用的 CM 复刻、覆盖齐全，编译结果也是它。因此选择修正家族名而不是搬运字体。要做"字面上就是 LyX 那套"，需要先决定改 cmap 还是走路径绘制，属于单独设计。

字形层面的对照证据（把新版本实际绘制的字形与 LyX 的 `cmmi10`/`cmr10`/`cmsy10` 逐字栅格化，96px、按墨迹包围盒 ±3px 取最佳对齐，比较墨迹 IoU）：

| 字符 | 对照 | IoU |
| --- | --- | --- |
| `𝑎 𝑏 𝑔 ℎ 𝑥 𝑦 𝑧 𝑓 𝑍 𝑀` | cmmi10 `a b g h x y z f Z M` | 0.901 – 0.949 |
| `0 1 7 + = ( ) , .` | cmr10 | 0.833 – 1.000 |
| `\|` | cmr10 / cmsy10 | 0.041 / 0.226 |
| `±` | cmr10 / cmsy10 | 0.286 / 0.158 |
| `𝑎` vs `𝑏`（对照组） | cmmi10 | 0.316 |

字母、数字、常用运算符与 LyX 的 Computer Modern 基本重合（对照组两个不同字母只有 0.316，说明该指标有分辨力）；`|` 与 `±` 的差异是设计变体——TeX 的数学竖线取自 `cmsy10`（更高），二进制 `±` 也是 `cmsy10` 的字形，而 NewCM Math 的 `U+007C`/`U+00B1` 是它自己的数学设计，不是错配。

## 大文档里的 Raw 片段不是编译失败

Raw 表示"合法但结构 IR 不建模的 Typst 片段"：`sum`、`integral`、`dif`、`quad`、`partial`、`lim`、`->`、`cases(...)`、`upright(...)` 都会落在这里，编译毫无问题；编辑器把它们交给 Typst 渲染成 SVG（Web 端和桌面端同一条通路）。真正的问题是**大多数 Raw 拿不到源码区间，于是永远画不出图**，只能以棕色源码文字显示，看起来像一堆失败片段。

根因在 `Document::annotate`：

- 它只在"公式的规范序列化与源码逐字节相同"时才运行。规范写法会补空格（`sum_(n=0)` → `sum_(n = 0)`），也会把 `a/b` 写成 `frac(a, b)`，所以任何按自己习惯书写的公式都被整体跳过。
- 即便运行，它也是"把原子替换成标记、序列化、在**规范文本**里找标记位置"，再用该位置去**源码**里切片校验。位置与校验不在同一个坐标系里，带上下标的片段（`sum_(n=0)^oo`）就会算错。

实测同一份 5.5 KB / 109 公式 / 220 个 Raw 节点的文档（公式密度很高，约 48 字节一个公式）：改动前只有 16 个区间、204 个节点没有 `render_id`；改动后 220/220 个节点都有区间（196 个不同区间），一次渲染请求返回 208 个 SVG。

改动：

- `annotate` 不再受规范写法限制，无条件运行。
- 新增 `Locator`：先把规范文本与源码**逐字符对齐**（只差空白时精确，这是最常见的情形），把标记位置换算成源码估算位置，再在同一个公式内取**最近的一次该片段文本出现**并校验；模板片段继续走 `warmup_range` 与"定义前缀 + origin"两条路径，同样校验。
- 校验的语义是"这个区间里必须恰好是这段文本"，因此估算再偏，后果只会是某个片段退回源码显示，不会指向别的文本。

以离屏真实窗口打开同一份文档（临时脚本：构造 `Window`、载入源码、跑完一次 Raw 请求后统计，测完即删）：220 个 Raw 节点中 208 个画出 Typst 图像、12 个显示源码、0 个仍在等待。剩下的 12 个全是 `quad`——它排版结果是空白，后端不会给出可见结果；这与 Web 前端一致（`web/app.js` 把这种结果标为 error 并说明"当前文档没有此 Raw 的可见排版结果"）。

## 无图片段的进入与修复

核心早就有这条规则：`move_horizontal` 在边界上遇到 Raw、且该片段在 `failed_previews` 里时，打开它的源码（`open_source`），并按方向把光标放在开头或结尾；Web 前端也一直在喂这个状态（每个片段渲染完就发 `preview_result`）。桌面端从来没有上报过，于是这条路径在桌面上永不触发——**没有图片的片段既点不到、也进不去，等于无法修**。

改动：

- 新增批量动作 `Action::PreviewResults { sources, definitions, display, failed }`（Web 继续用单条 `preview_result`，`record_preview` 是两者共用的实现）。
- 桌面端 `Window.report_raw_fragments` 在会话建立、渲染返回、宏预热、公式状态刷新后上报"有图"和"无图"两组片段；上报内容按签名去重，因此不改变状态的按键不产生额外往返；`activate_formula` 会重建 `Editor`（`failed_previews` 随之清空），所以建立会话时强制重报。没有源码区间的片段在 `load_raw` 里直接记为无图，避免永远停在"等待中"。
- 无图片段按 Web 的同款样式绘制：暖色底 `#fff2eb` + 红色虚线框 `#b3654e`，源码文字用编辑器正文字体（不再按数学斜体映射，`cases(1 & x > 0)` 不会被画成公式）。
- 进入之后要能落地：从 Raw 打开的草稿若按公式解析失败，此前 `close_command` 会拒绝提交，而 `insert_named` 恢复的是空选区，等于**丢掉该片段**。现在改为把编辑后的文本作为该片段的源码提交（`original` 存在即"以源码为准"），Esc 仍恢复原片段。

覆盖用例：`tests/document.rs` 的 `raw_ranges_do_not_depend_on_the_canonical_spelling`、`repeated_fragments_of_one_formula_take_their_own_occurrences`、`repairing_a_fragment_writes_the_edited_source_back`；`tests/failed_block.rs` 的 `one_batch_reports_every_fragment_of_a_render_pass`、`an_unparseable_repair_of_a_fragment_is_kept_as_source`；`desktop/test_desktop.py` 的字体解析、无图片段进入与标注、以及"默认数学字体必须覆盖核心能画出的每个字形"（缺少字形就会被别的字体接管，正是本次要修的 bug 类型）。

## 本轮实测

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | 86 通过，5 个本机服务集成用例忽略（`--ignored` 时 4 个服务用例通过） |
| `cargo check --offline --locked --features server` | 通过 |
| `cargo build --release --target wasm32-unknown-unknown --lib` + 复制到 `web/core.wasm` | 通过 |
| `cargo test --manifest-path native-adapter/Cargo.toml --target-dir target/adapter` | 16 通过 |
| `npm run build:vscode` | 通过；`media/` 与 `web/` 逐字节一致 |
| `npm test` | 27 通过 |
| `python -m unittest desktop.test_desktop` | 53 通过（离屏，17.2s） |
| 大文档 Raw 覆盖率（109 公式 / 220 节点） | 区间 16 → 196；出图 0 → 208；显示源码 12（全为 `quad`） |

尚未实测：真机肉眼确认字体观感（本轮只能给出字形栅格化比对数据），以及真实 VS Code Extension Host 中的相关行为。

# 取图路径与缓存形态调研 · 2026-09-10

针对反馈"`$ => psi(x+a) = beta^(a) psi(x) , beta in CC $` 是不是公式 AST 有问题，还是代码块找不到编译后的 SVG"。

## 样例结论：AST 与区间都没问题

该公式在三种上下文里都通过：单独一行、正文行内、以及带 `#set` / `#let` / 标题的文档中。`editable=true`，5 个片段（`=>`、`psi(x+a)`、`psi(x)`、`in`、`CC`）全部拿到正确区间，`/api/render` 返回 5/5 个 SVG。以离屏真实窗口打开也全部出图（含宏定义体里的 `partial`/`#f`/`#x`）。也就是说这个样例会渲染，问题不在 AST、也不在单个片段的定位。

## 真正会"整屏变源码"的原因：整文档编译是全有全无

Raw 片段的图像来自"把整份文档交给 Typst 编译、再从帧里取出带标签的那一组"。因此**文档里任何一处编译错误都会让整批取图失败**，测试文档只改一处即可复现（同一份含 5 个片段的文档）：

| 文档 | 请求耗时 | 返回片段 | 错误 |
| --- | --- | --- | --- |
| 干净 | 119 ms（含适配器进程冷启动） | 5 | — |
| 公式**之后**加 `#panic("坏了")` | 0.8 ms | 0 | `panicked with: 坏了` |
| 公式**之前**加 `#import "no-such-file.typ"` | 0.7 ms | 0 | `failed to load file (access denied)` |
| 公式之前加不存在的 `@preview` 包 | 1.4 ms | 0 | `failed to load file (access denied)` |

失败后 `load_raw` 会把这批片段标记为"无图"（画成暖色虚线框、可用左右键进入），但**旧实现里这个标记会一直留着**：目标筛选会跳过任何已缓存的键，于是这份文档在整个会话里都只能看到源码。这一段已修：失败记录绑定在当时的 `revision` 上，下一次编辑（通常正是修好文档的那次）就清掉并重新请求；同一 revision 内不重复请求，避免每次滚动都重试一个坏文档。用例：`desktop/test_desktop.py::test_a_failed_render_request_is_retried_after_the_next_edit`。

## 能不能"只编译一个公式"

Typst 的公开编译入口是文档级的（`typst::compile::<PagedDocument>(world)` 编译整个 `World`），没有"编译某个元素"的 API。但代码库里已经有两条路，实测都能用：

1. **整文档 + 标签抽取**（现 Raw 取图路径，`native-adapter/src/render.rs`）：把每个片段包成 `#[${片段}$<label>]` 插入源码，编译整篇，再从帧里取出带该标签的 group 做 SVG。上下文 100% 真实，代价是 O(整篇)，且与整篇的编译结果同生共死。
2. **只有前缀 + 本公式的合成文档**（`native-adapter/src/main.rs` 的 `resolve()` 已经用这条路做附件定位）：`前缀 + "$ 公式 $" + 前缀未闭合定界符的补全`。前缀就是编辑器里"公式之前的全部源码"，所以 `#set`、`#show`、`#let`、`#import`、以及**外层容器**里的局部 `#set` 都在其中。

实测（同一批片段、同一台机器、release 适配器）：

| 上下文 | 整文档取图 | 前缀 + 公式 | 前缀 + 公式 + 补齐定界符 |
| --- | --- | --- | --- |
| 顶层，`#set text(size: 11pt)` | 2 片段，环境字号 11.0 | ✅ SVG 逐字节相同，1.2 ms | ✅ 相同 |
| 公式位于 `#block[ … ]` 内，块内 `#set text(size: 20pt)` | 2 片段，环境字号 20.0 | ❌ `unclosed delimiter` | ✅ SVG 逐字节相同，环境字号 20.0，1.4 ms |
| 前缀含 `#show math.equation.where(block: true): set text(size: 18pt)` | 2 片段，环境字号 18.0 | ✅ 相同，1.2 ms | ✅ 相同 |

也就是说：**合成文档不仅更快，还能保住外层容器上下文**——只要把公式放在补全定界符之前（`前缀 + 公式 + 补全`），公式仍然处于原来的块里，字号与样式照旧；而"前缀 + 公式"这种简单拼法在容器未闭合时会直接编译失败。定界符补全的逻辑已经存在：`src/services.rs::closing_brackets`。

规模与时间：

| 场景 | 整文档取图 | 前缀 + 公式 |
| --- | --- | --- |
| 67 B 文档，2 片段 | 119 ms（含适配器冷启动） | 1.2 ms |
| 4.9 KB / 300 片段的文档，全部片段 | 32 ms（源码变化后）／15 ms（同源再取） | — |
| 同一份文档里的 1 个片段 | 15.5 ms（源码变化后）／1.2 ms（同源再取） | 约 1 ms，与文档大小无关 |

剩下的差别与边界：

- 前一路径的时间随文档规模与"文档是否刚被改过"变化（`typst::comemo` 会复用未变元素的排版，所以同源再取只要 1～15 ms）；后一路径只跟前缀 + 公式有关，实测约 1 ms。
- 前一路径的错误耦合是**全有全无**；后一路径只受公式之前的内容影响（那正是公式真实的上下文），公式之后的错误与它无关。
- 后一路径看不到"整篇查询"的结果：`counter(page)`、总页数、`#context` 里跨文档的引用只会看到前缀。正文里的公式若依赖这类查询，合成文档会与最终 PDF 不同。
- 两条路都会忽略"公式之后"的样式——Typst 本身也不回溯，所以这不构成差异。

结论：可以做，而且不需要改引擎。

## 已实现：取图上下文按"最后一个片段所在的节点"截断

落地时没有用"补齐定界符"那种字符级拼法（注释里的括号会让它数错，拼错就又是整批失败），而是用语法树给一个**永远合法**的截断点：把源码截到"包含最后一个请求片段的最外层顶层节点"的末尾。

- 前缀逐字节相同，所以**片段区间不用重映射**，`valid_range` 与标签回填照旧；
- 公式仍然处在原来的容器里，块内 `#set text(size: 20pt)` 这类上下文照旧生效；
- 截断之后的内容被丢掉，而 Typst 不回溯样式，因此它们不可能影响这些片段。

实现：`RenderRequest.context_end`（新增，缺省即整文档）+ `src/services.rs::context_source`；桌面端在 `load_raw` 里用"最后一个有新片段的公式的 end"填这个字段。整文档路径仍用于整页预览、PDF/SVG 导出与附件定位。

实测（4.9 KB / 118 公式，视口内 8 个片段，截断点 151 B）：

| 文档 | 整文档取图 | 按上下文截断 |
| --- | --- | --- |
| 干净 | 8 片段，1.4 ms（首个请求 131 ms，含适配器冷启动） | 8 片段，1.2 ms，**SVG 逐字节相同 8/8** |
| 末尾有 `#panic("坏了")` | **0 片段**，`panicked with: 坏了` | **8 片段，无错误** |

也就是说图像一模一样，而"文档别处出错"不再波及这个视口。用例：`desktop/test_desktop.py::test_fragments_render_even_when_the_document_has_a_later_error`（同一份文档先按上下文取到 8 张图，再按整文档取一次并断言报出那个 `#panic`），以及 `src/services.rs` 的 `a_fragment_context_ends_at_the_node_that_holds_it`。

## 更精简的 SVG 缓存：三种做法的实测

对 7 个片段共 17809 B 的 SVG 做解剖：

| 组成 | 字节 | 占比 |
| --- | --- | --- |
| `<defs>` 里的字形轮廓（10 个不同字形） | 14228 | 80% |
| 其余（SVG 头、`<use>` 引用、变换） | 3581（每片段约 512 B） | 20% |

- **"相同的文件头"不是重点**：每片段 `<defs>` 之外只有约 512 B，其中真正的 `<svg class="typst" …>` 头只有一两百字节，优化掉它省不到 2%。
- **重复的字形轮廓才是重点**：7 个片段只有 10 个不同字形，`<defs>` 被重复写了 14228 B；改成"每次会话发一份字形字典、片段只带 `<use>` 引用"是 8942 B 一次 + 3581 B，省约 **30%**。可行性的关键是 id 稳定性：实测字形 id 是内容哈希（`gA01783F…`），同一份源码两次编译完全一致，换文档时 9 个里仍有 8 个复用。
- **deflate 是最省事的一档**：整包压缩到 **38%**（level 6：压缩 0.36 ms、解压 0.04 ms），格式不变、消费端只需 `zlib`。适合"片段字符串在会话里长期驻留"的内存账，而不是管道带宽账（本地管道不是瓶颈）。
- **二进制矢量格式**：能把 XML 包装（约 20%）和 base64（+33%，若走文本通道）省掉，但**要避开的那次转换本来就不贵**——`QSvgRenderer` 解析实测 0.03–0.6 ms（6.5 KB 片段 0.13 ms），而栅格化 0.12–0.3 ms、Typst 编译约 1 ms 才是大头；栅格化是任何矢量格式都逃不掉的。Qt 侧消费二进制矢量很便宜（`QPainterPath` 走 `QDataStream`，或 `QPicture` 直接重放），成本全在 Rust 侧要新写编码器（现在输出的是 `typst_svg` 的 XML）。结论：收益与 deflate 同量级，工程量却大得多，不建议。
- 真正值得省的是**内存而不是字节**：一份片段在 4× 缩放下栅格缓存要 27.7 KiB（宽×高×4），桌面端现在靠 64 MiB LRU + 单边 4096 上限兜底；比换格式更有效的是"只保留可见片段的图"（`clear_actual_svg`/`invalidate_raw` 已有）+ 上面两条字符串级的压缩/共享。

## 缓存 SVG 还是缓存 PNG

桌面端现在交换的是 SVG，本地按 `(片段, 逻辑尺寸, 设备像素比)` 栅格化成 `QPixmap` 缓存（64 MiB LRU、单边 4096 上限）。实测单个片段（`psi(x+a)`，41.4×11.0 pt）：

| 形态 | 大小 | 首次解析/编码 | 光栅化/解码 | 之后每帧绘制 | 内存 |
| --- | --- | --- | --- | --- | --- |
| SVG @1× | 6466 B | 0.6 ms → 0.13 ms（缓存后） | 0.29 ms | 0.011 ms（重放 SVG） | 1.6 KiB（栅格） |
| SVG @4× | 6466 B（不变） | — | 0.23 ms | — | 27.7 KiB |
| PNG @1× | 462 B | — | 0.09 ms | 0.032 ms | 1.6 KiB |
| PNG @4× | 2311 B | 0.51 ms（编码） | 0.08 ms | — | 27.7 KiB |

- **传输**：SVG 是文本，桌面端走 JSON 管道，转义开销实测 1.9%（15187 → 15481 B），不必额外编码；PNG 走同一条管道要 base64，+33%。片段越小 SVG 越吃亏（简单片段 1.1～6.5 KB，PNG 只有 0.14～2.3 KB）。
- **缩放/DPI**：SVG 一份就够，任何字号、任何 `devicePixelRatio` 都能重新栅格化；PNG 必须在每个尺寸上重新编码、重新传输、单独缓存，缓存键数量随"缩放档位 × DPI"增长。
- **内存**：栅格内存 = 宽×高×4 字节，随缩放平方增长（4× 时 4.0 KiB → 27.7 KiB）；单边 4096 的上限意味着一份条目最坏 64 MiB，这正是 `BitmapCache` 要设上限和 LRU 的原因。缓存 SVG 时这部分由"当前可见尺寸"决定，而不是由"用户缩放过的每一个档位"决定。
- **CPU**：SVG 的解析 + 栅格化约 0.1～0.3 ms/片段，只在尺寸或内容变化时付一次；PNG 的解码约 0.02～0.1 ms，但**编码**在服务端要 0.03～0.5 ms（首次 6.5 ms，含编解码器初始化），而服务端本来就要跑一次 Typst——栅格化把它从"几何求解"变成"光栅求解"，放大的正是 Typst 最昂贵的部分（字形轮廓光栅化），还会丢掉向量在缩放下的清晰度。
- **结论**：交换与缓存都用 SVG 更划算，PNG 只在"要跨进程缓存大量已栅格化的小片段、且尺寸固定"时才有意义。桌面端当前形态（传 SVG、按需栅格化、位图 LRU）已经是这个结论下的正确取舍；要优化的话方向是**减少请求**（前缀合成文档把每次取图从整篇编译降到一个公式），而不是换图片格式。

# Raw 取图"整屏变源码"的第二个原因 · 2026-09-10

针对反馈"编辑器还是会某些字形无法识别，例如 `paper/slides` 里 ln.79 的 `=>` 和 ln.99 的 `det`"。

## 复现：不是字形，也不是那两行

以离屏真实窗口（`Window` + 真后端 + 真适配器）打开 `pygbz2d-details.typ`，跳到 ln.79 后触发一次 `load_raw`：缓存里 41 个片段**全部**是 `False`，编辑器把它们画成暖色虚线框 + Consolas 源码文字（`=>`、`det`、`det(E-h(beta))` 都一样）。也就是用户看到的不是"某两个字形画错"，而是**这个视口的所有片段都没有图**。上报的错误是：

```
REQUEST raw=41 context_end=6749 path=pygbz2d-details.typ
   error: expected function, found content
```

同一份 41 个区间的请求，无论带不带 `context_end` 都失败；把区间逐个单独请求，只有 `2820:2826`（`cal(A)`）会失败，且错误信息一样。文档本身没问题：同一份文档只请求它所在公式的 2 个片段时编译成功。

## 根因：片段被插进源码后与后面的 `(` 粘成了函数调用

`native-adapter/src/render.rs` 把每个片段替换成 `#[${片段}$<标签>]` 再编译整篇。`#[...]` 是**嵌入的代码表达式**（`typst-syntax/src/parser.rs` 里 math 的 `Hash` 分支走 `embedded_code_expr`），而代码解析器把**紧跟其后的** `(`、`[` 当作对它的调用：

```rust
// code_expr_prec
if p.directly_at(SyntaxKind::LeftParen) || p.directly_at(SyntaxKind::LeftBracket) {
    args(p);
    p.wrap(m, SyntaxKind::FuncCall);
```

文档原文是 `cal(A)(E)`：在 math 里这是并置（`cal(A)` 与括号组），所以文档本身合法；插入后变成 `#[…](E)`，于是被读成"调用 content" → `expected function, found content` → **整批取图全灭**。`directly_at` 的定义是"当前 token 匹配且前面没有 trivia"，所以修法是块尾补一个空格：`#[…] (E)`，代码解析器不再把它当调用。空格落在方括号之外，片段自己那个公式——也正是取出来的那张图——不受影响：`native-adapter/src/render.rs::the_trailing_space_of_a_splice_stays_outside_the_fragment` 断言同一片段的宽、高、基线与 SVG 在"后面紧跟 `(y)`"与"什么都不跟"两种写法下完全相同。

## 第二个改动：一个片段不再能带崩整批

上一条是具体写法，这一条是**这一类**问题：只要有一个片段自己编译不出来，整个视口就退回源码。后端现在在整批失败时**分半重试**，把能编译的子批的图都收回来，并在响应里给出 `failed`（单个片段仍然失败的 id 列表）；重试次数上限 `SALVAGE_BUDGET = 24`，避免一份满是坏片段的文档把一次按键变成编译风暴。**所有子批都失败时仍然返回错误**，因为那说明问题在共享上下文（正文真的错），用户需要看到原因而不是一批"无图"。

这不是假想：`$ #let z = 1; z + cancel(y) $` 是合法文档，但片段 `#let z = 1` 的区间不含后面的分号，单独包成 `$#let z = 1$` 必然报 `expected semicolon or line break`——以前它会让同一视口里的 `cancel(y)` 一起没图。用例：`native-adapter/src/render.rs::one_broken_fragment_does_not_blank_the_batch`（断言 `salvaged=true`、`failed=["0"]`、另一个片段仍有 SVG，以及"全坏 → 仍是 Err"）。

桌面端把 `failed` 当作与"整批失败"同一种判决：记在当前 `revision` 上，下一次编辑清掉并重新请求，见 `desktop/test_desktop.py::test_a_fragment_the_renderer_refused_is_retried_after_the_next_edit`。

## 缓存 SVG 一律按黑色栅格化

片段是从文档里切出来的图，文档可以给它上色（幻灯片主题常见 `#set text(fill: white)`，或 `#text(fill: red)[$…$]`），浅色编辑区上就会看不见。改动很小，所以做了：

- `desktop/svg.py::qt_svg(source, monochrome=False)`：Qt 兼容转换时把每个元素的 `fill`/`stroke` 改成 `#000000`（`none` 保留，否则只描边的形状会被填满）。
- `desktop/mathview.py::BitmapCache.draw` 用 `qt_svg(svg, True)`；导出 SVG/PDF 仍用原始 SVG（`window.py::export` 里 PDF 走 `qt_svg(page["svg"])`，那是**整页**，颜色照旧）。

实测：把一份 `fill="#ffffff"` 的片段画到白色 `QImage` 上，改前中心像素是 `#ffffff`（等于看不见），改后是 `#000000`；`qt_svg(svg)`（导出路径）仍保留 `#ffffff`。顺带量了本次这份文档：49 个片段**本来就全是** `#000000`，所以这条改动对本例没有可见变化，它防的是别的主题/写法。

## 本轮实测

| 检查 | 结果 |
| --- | --- |
| `paper/slides/pygbz2d-details.typ` 41 个片段的批量请求 | 改前 `expected function, found content`、0 张图；改后 49 个条目（含 `#pause` 拆出的多页重复）、0 错误 |
| 同一份文档在离屏真实窗口里的绘制操作 | 改前 `FAILED MARK + text 'det(E-h(beta))' (Consolas)`、`text '=>' (Consolas)`；改后 `svg image` + 邻接的 `=`、`0` 由 `NewComputerModern Math` 绘制 |
| 逐区间单独请求 | 改前只有 `cal(A)` 失败；改后 41/41 通过 |
| `cargo test --offline --locked` | 86 通过，5 个本机服务集成用例忽略 |
| `cargo test --manifest-path native-adapter/Cargo.toml --target-dir target/adapter` | 19 通过（新增 3） |
| `python -m unittest desktop.test_desktop` | 56 通过（离屏，21.2s，新增 3） |
| `npm test` | 27 通过（未改 Web 侧） |

新增用例：`native-adapter/src/render.rs` 的 `a_fragment_followed_by_a_parenthesis_is_not_read_as_a_call`（去掉那个空格即复现 `expected function, found content`）、`one_broken_fragment_does_not_blank_the_batch`、`the_trailing_space_of_a_splice_stays_outside_the_fragment`；`desktop/test_desktop.py` 的 `test_a_fragment_followed_by_a_parenthesis_still_gets_its_image`（真后端 + 真适配器的端到端）、`test_a_fragment_the_renderer_refused_is_retried_after_the_next_edit`、`test_a_cached_fragment_is_black_but_its_exported_svg_keeps_its_colour`。

# 普通输入的分隔符：只有数字连写 · 2026-09-10

针对反馈"依次输入 `-` 和 `>`，源码不是 `$- >$` 而是 `$->$`"。

## 结论：不是输入层加糖，是序列化不加分隔符

普通输入确实是"一个按键一个 `Char` 对象"（`tests/structured_input.rs::ordinary_input_keeps_char_nodes_and_only_maps_single_character_display` 一直在断言这一点）。变的是**写出的源码**：`typst::write_cell` 原来在"相邻两个运算符字符"之间也省略分隔符，于是 `-` `>` 被写成 `->`，而 Typst 把 `->` 读成**一个** token（`MathShorthand`），下一次解析（`analyze_formula`、重新进入公式）就把它变成一个 `Raw` 片段——也就是箭头图像。仅"数字"和"运算符"两类不加分隔符，字母本来就走默认分支，所以 `x` `y` 会写成 `x y`。

## 改动

`write_cell` 的分隔条件收窄为"只对数字连写"：

| 输入 | 改前源码 | 改后源码 | 改后重新解析 |
| --- | --- | --- | --- |
| `-` `>` | `$->$` | `$- >$` | `symbol(−) symbol(>)` |
| `=` `>` | `$=>$` | `$= >$` | `symbol(=) symbol(>)` |
| `-` `-` `>` | `$-->$` | `$- - >$` | 三个符号 |
| `<` `=` `>` | `$<=>$` | `$< = >$` | 三个符号 |
| `.` `.` `.` | `$...$`→`raw(…)` | `$. . .$` | 三个 `char(.)` |
| `\|` `\|` | `$\|\|$`→`raw(‖)` | `$\| \|$` | 两个 `raw(\|)` |
| `[` `\|` | `$[\|$`→`raw(⟦)` | `$[ \|$` | `char([) raw(\|)` |
| `:` `=` | `$:=$`→`raw(≔)` | `$: =$` | `raw(:) symbol(=)` |
| `<` `-` `>` | `$<->$`→`raw(↔)` | `$< - >$` | 三个符号 |
| `>` `=` | `$>=$`→`symbol(≥)` | `$> =$` | `symbol(>) symbol(=)` |
| `1` `.` `5` | `$1.5$` | `$1.5$` | 三个 `char` |
| `x` `y` | `$x y$` | `$x y$` | 两个 `char` |

`.` 的处理单独收窄：它原先与任何"数字类"字符相连，于是 `...` 仍会合并；现在 `.` 只与相邻数字相连（`1.5`、`.5` 保持一个数字），两个点之间必须分开。

## 为什么必须留分隔符（实测）

用自动页宽（`#set page(width: auto, height: auto, margin: 0pt)`）量 Typst 的排版宽度：

| 源码 | 结果 |
| --- | --- |
| `$ab$` / `$xy$` | 编译错误 `unknown variable: ab` / `xy`——连写的字母是一个变量名 |
| `$a b$` | 10.692 pt |
| `$12$` 与 `$1 2$` | 均为 11.000 pt（数字连写只影响可读性） |
| `$1.5$` 与 `$1 . 5$` | 均为 14.058 pt |
| `$->$` | 11.000 pt（一个箭头字形） |
| `$- >$` | 20.172 pt（减号与大于号两个关系符） |
| `$<=$` | 8.558 pt（`≤`） |
| `$< =$` | 17.116 pt（两个关系符） |

也就是说：分隔符改变的是**语义**，不是排版噪音——这正是本次修复的目的（按下的两个字符就是两个字符），同时也说明 `<=`、`>=`、`!=` 不再自动变成 `≤`、`≥`、`≠`。这类符号仍可由命令输入得到（`\>=` + Enter → `Symbol(name: ">=", glyph: "≥")`，`tests/structured_input.rs::compiler_owns_symbols_shorthands_and_escapes` 覆盖）。

## 本轮实测

| 检查 | 结果 |
| --- | --- |
| 离屏真实窗口逐个键入（16 组） | 全部按新规则写出：`- >`、`= >`、`- - >`、`< = >`、`. . .`、`: =`、`< <`、`> >`、`\| \|`、`[ \|`、`< - >`、`~ >`、`> =`；`1.5` 与 `x y` 保持原样 |
| `cargo test --offline --locked` | 87 通过（新增 1），5 个本机服务集成用例忽略 |
| `cargo build --release --target wasm32-unknown-unknown --lib` + `web/core.wasm` + `npm run build:vscode` | 通过；`extensions/vscode/media/` 与 `web/` 逐字节一致 |
| `npm test` | 27 通过 |
| `cargo test --manifest-path native-adapter/Cargo.toml --target-dir target/adapter` | 19 通过（未改适配器） |
| `python -m unittest desktop.test_desktop` | 56 通过（离屏，21.7s，重建 release 后端） |

改动文件：`src/typst.rs`（`write_cell`）、`tests/structured_input.rs`（两条旧断言 + 新增 `a_separator_keeps_typed_characters_from_becoming_one_shorthand`）、`tests/source_modes.rs`（一条断言），以及 `web/core.wasm`、`extensions/vscode/media/` 两个构建产物。

# 桌面端显示细项：命令字体、空槽、行间居中、上下标、源码栏对齐 · 2026-09-10

五项前端调整，全部只改 `desktop/`，核心与适配器未动。

## 1. 命令草稿用编辑器字体

`mathview.py` 的 `source_run`（Raw 源码用的"编辑器正文字体"路径）现在也覆盖草稿节点（`draft-text` / `draft-placeholder` / `draft-caret`）。实测键入 `\alph` 后画布的操作：

```
mode command  at 2.0,0.0 size 51.0x22.0
text 'a' at 6.0,16.0 font=Consolas    （改前是 NewComputerModern Math）
text 'l' … text 'p' … text 'h' … text '│' 全部 font=Consolas
```

草稿是正在输入的 Typst 源码，和编译出的字形不是一回事；字符串模式共用这条路径。

## 2. 空编辑块画成虚线方框

`empty-cell` 视图以前画的是 `□` 字符，现在是 `("slot", …)` 操作，画成虚线矩形（宽 ≈ .45 em、高一行），光标进去时竖线光标落在框内。实测 `\frac` + Enter（源码 `$frac("", "")$`）：

```
slot (dash) at 6.0,0.0  size 7.2x16.0     ← 分子
slot (dash) at 6.0,22.0 size 7.2x16.0     ← 分母
```

静态投影同样生效：`$frac("", "")$` 的文档视图里也是两个方框。矩阵空格、空公式本身同理。

**回归与修复**：第一版把 `empty-cell` 直接 `return` 了一个只有方框的 Box，把该槽自己的 `stop` 丢了——实测 `$ $` 的 `box.stops` 变成空表，`$frac("", "")$` 只剩公式外层的两个 stop（分子、分母的都没了），于是画布上光标画不出来，核心那边也拿不到位置：`MathCanvas.refresh` 把 stops 发回核心，而 `move_vertical`（`src/cursor.rs:648`）正是按这份几何找落点，找不到就只能停在 `original.pos`，也就是"光标移进空编辑框就失去位置"。现在方框会把子节点（stop）折进同一个 Box（`box.add(self.layout(child,…),0,0)`），实测：

| 源码 | 修复前 stops | 修复后 stops |
| --- | --- | --- |
| `$ $` | `[]` | `[(1, 0, active=True)]` |
| `$frac("", "")$` | `[(1, 12.9), (18.2, 12.9)]`（只有公式外层） | 外层两个 + 分子 `(7, 0)`、分母 `(7, 22)` |
| `$frac(a, "")$` | 外层 + 分子，**分母缺失** | 分母 `(9.4, 22)` 也在 |

用例 `desktop/test_desktop.py::test_the_caret_survives_inside_an_empty_slot`：`\frac` + Enter 后两个空槽各有 stop、当前光标画在所在槽内；点击下槽后光标移到分母且仍可见。（把折入子节点的那行去掉，该用例报 `0 != 2`。）

## 3. 行间公式整行居中

`Editor.apply_alignment`：**独占一行**的 `display` 公式所在段落设为 `AlignHCenter`（Typst 的行间公式是块级元素，自己居中）；与正文同行的 display 公式保持左对齐——Typst 会把它断到单独一行，这个投影不会，所以居中一行含正文的段落只会把正文也拉过去。实测段落对齐值（Qt 枚举）：

```
block 5 alignment=4 text='￼'                 ← $ frac(a, b) = sum_(i = 1)^(n) x_i $，居中
block 7 alignment=1 text='行内 ￼ 与 ￼ 混排'    ← 保持左对齐
```

## 4. 上下标位置：按编译器自己的移位

先从 Typst 编译出的 SVG 里量出脚标的真实基线差（11pt 文档，`translate(x y)` 的 y 就是基线，页面 y 向下）：

| 公式 | 基底基线 | 脚标基线 | 差值 |
| --- | --- | --- | --- |
| `$x^2$` | 7.513 | 3.520 | **−3.993 pt = −0.363 em**（上标） |
| `$x_1$` | 7.513 | 10.230 | **+2.717 pt = +0.247 em**（下标） |
| `$x_1^2$` | 7.513 | 3.4386 / 10.3114 | −0.370 em / +0.254 em |
| `$sum_(n = 1)^(oo)$`（limits） | 7.513 | 2.013 / 12.463 | −0.500 em / +0.450 em |

于是 `mathview.py` 的侧向脚标改成：上标 `max(.36 em, ascent − .4 em)`、下标 `max(.25 em, descent + .05 em)`，`ascent`/`descent` 只在基底是**组合对象**（分式、根式、定界符、片段图）时取盒子的真实几何——单个字符的盒子是行盒度量（Qt 的 ascent 含 leading，约为字号 .94 倍），用了反而把脚标推远，所以单字符基底走固定移位。改动前下标是按脚标自身的上伸部放的，落到基线下约 .6 em，视觉上就是"偏右下"；

清单里的下标移位实测（`font_size=12`，1 em = 16.67 px）：`$x_1$` 基线差 **4.17 px = .25 em**，`$x^2$` **6.00 px = .36 em**，与 Typst 的 .247 em / .363 em 一致（用例 `test_side_scripts_follow_the_shifts_the_compiler_uses`）。

已知差别：Typst 还会按字体的 MATH 表把脚标**向左**收（`math_kern` 的 bottom-right/top-right 斜体修正），对箭头、伸缩字形这类基底可达 −0.68 em；编辑器只看得到片段图的宽高，拿不到角部 kern，因此脚标仍从基底的右边缘开始。这是"对箭头类基底仍略偏右"的原因。

## 5. 源码栏与编辑器逐行对齐

改动前：源码栏是 `QPlainTextEdit`，文档默认字体是应用字体（正文括号里的 CJK 回退到另一款字），顶部偏移 1px，而编辑器样式表有 16px 内边距；更关键的是编辑器把每个公式换成一个占位字符，带高公式的行高 45px 而源码栏一律 20px。实测第 9 行差 **约 59px**。

改法（`Window.sync_source_lines` + `mirror_scroll`）：

- 源码栏与编辑器用同一个**文档默认字体**（`document().setDefaultFont`），于是纯文本行的高度一致；
- `documentMargin` 按"编辑器视口偏移 + 编辑器文档边距 − 源码栏视口偏移"算出来（实测 19 + 1 = 20 = 16 + 4），两栏顶行同高；
- 把编辑器**实测**的每行高度以 `MinimumHeight` 写给源码栏对应块（编辑器尚未布局时退回"公式盒高 + 6"的估算，离屏测试走这条）；
- 因为行高逐一相等，两栏滚动值可直接换算，滚动任意一栏另一栏跟随。

源码栏因此从 `QPlainTextEdit` 改为 `QTextEdit`：**QPlainTextDocumentLayout 忽略块行高**（实测写入 `lineHeight=45/type=MinimumHeight` 后 `blockBoundingRect` 仍是 20），换成 `QTextEdit` 后行高生效。

实测同一份文档十行的行顶（编辑器文档坐标 + 16px 内边距 vs 源码栏 1px 边框 + 19px 边距）：

| 行 | 编辑器（屏幕） | 源码栏（屏幕） |
| --- | --- | --- |
| 1（标题，行高 25） | 20 | 20 |
| 2 | 45 | 45 |
| 3 | 65 | 65 |
| 4 | 85 | 85 |
| 5（行间公式，行高 45） | 105 | 105 |
| 6 | 150 | 150 |
| 7（行内公式，行高 44） | 170 | 170 |
| 8 | 214 | 214 |
| 9 | 234 | 234 |
| 10 | 254 | 254 |

## 本轮实测

| 检查 | 结果 |
| --- | --- |
| `python -m unittest desktop.test_desktop` | 62 通过（离屏，22.9s，新增 6） |
| `cargo test --offline --locked` | 87 通过，5 个本机服务集成用例忽略（未改核心） |
| `npm test` | 27 通过（未改 Web 侧） |

新增用例：`test_a_command_draft_is_drawn_in_the_editor_font`、`test_an_empty_slot_is_a_dashed_box`、`test_the_caret_survives_inside_an_empty_slot`、`test_a_display_formula_gets_a_centred_line`、`test_side_scripts_follow_the_shifts_the_compiler_uses`、`test_the_source_dock_lines_up_with_the_editor`。

改动文件：`desktop/mathview.py`（草稿字体、空槽、脚标移位、`style_size`/`style_em`）、`desktop/editor.py`（`apply_alignment`、`SourceEditor` 改为 `QTextEdit`）、`desktop/window.py`（`sync_source_lines`、`mirror_scroll`、源码栏字体与边距）、`desktop/test_desktop.py`。

# 公式核心的 5 秒死线 · 2026-09-10

现象：同时开两个 desktop 时，第二个窗口弹出/提示「公式核心没有响应…Unknown error」，此后该窗口的公式服务一直唤不起来。

## 定位：不是两个实例互斥

`公式核心没有响应：` 全仓只出现在 `desktop/bridge.py:42`，条件是子进程**已经启动**却在 5 秒内没有吐出完整一行回复（启动失败会走 `waitForStarted` 的另一条报错）；`errorString()` 显示 `Unknown error`，说明那一刻进程还活着。

先排除"两个实例抢同一份资源"。每个窗口自带 `--desktop-core` 与两个 `--stdio` 子进程，取图引擎与 Tinymist 再由各自的 `--stdio` 惰性启动；没有端口、锁文件、命名互斥或固定临时路径（唯一共用的是文档、设置文件和 `%LOCALAPPDATA%/typst/packages`，都不在公式链路上）。实测两个完整实例（含用户文档 `pygbz2d-details.typ`，61 公式，两侧都走完 载入 → 进入公式 → 取图 → 附件 → LSP）：

| 观测 | A | B |
| --- | --- | --- |
| 核心 / 服务进程状态 | Running / Running | Running / Running |
| 载入后取图缓存 / 被拒片段 | 41 / 0 | 41 / 0 |
| `/api/attachments` | limits / limits | limits / limits |
| 进入公式（stop 数） | 25 | 25 |
| 状态栏报错、`raw_error` | 无 / None | 无 / None |

单进程两窗口（文件 → 新窗口）同样正常：两个核心 PID 不同，各自进入公式都是 25 个 stop。**第二个实例并没有抢占第一个的资源。**

## 测得的原因：最重的请求正好撞在统一的 5 秒死线上

载入文档时窗口同步发两次核心调用（`desktop/window.py:167-168`：`set_source` + `analyze`）。用 1500 公式（67 KB）实测：

| 场景 | `set_source` | `analyze` |
| --- | --- | --- |
| 单实例 | 0.03 s | 2.79 s |
| 两实例同时载入（各 1500 公式） | 0.03 s | 3.85 s / 3.74 s |

`analyze` 是超线性的（每个公式都要重扫它之前的整段源码，同一规模在本机更忙时曾量到约 7 s），而两个窗口同时载入还要叠加两边的整页编译、取图编译、两个 Tinymist，以及两个各自把系统字体全量载入的 Typst 引擎。第一个窗口载入时机器是空的，于是活了下来；第二个撞上这条对所有动作统一的 5 秒死线，`waitForReadyRead` 超时后 `Core.call` 直接 `kill()` 核心，而窗口没有任何重建路径——此后每次 `activate_formula` 都失败，表现就是"唤不起公式服务"。

## 改动（`desktop/bridge.py`）

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 预算 | 所有动作统一 5 s | `set_source`/`analyze`/`analyze_formula`/`activate_formula`/`macro_warmup`/`preview_results` 各 60 s（`BUDGETS`），其余 10 s，可按实例覆盖 |
| 超时 | `kill()` 子进程，该窗口此后再无公式服务 | 不杀进程；只有确实判定子进程退出时才报"已退出" |
| 恢复 | 无 | 退出或超时后重启核心、重放 `set_source`（含当时打开的公式会话），再重试该请求一次 |
| 报错 | `errorString()` ＝ `Unknown error` | 动作名 + 等待时长 / 退出码 + 核心 stderr 尾部（`Stderr` 只留最后 4 KB 中的三行） |
| `--stdio` 后端 | 退出即整场不可用，必须重开窗口 | 下一次请求自动重启该子进程 |

镜像之所以正确，是因为协议本身给了依据：每个会改动文档的回复都带完整 `source` 与 `active_range`（`src/document.rs::response`），`Core` 据此镜像源码与公式会话，所以重启后重放的是**当前**文档而不是载入时那一份；`analyze`/`scan` 不带 `active_range`，因此它们的回复不会把已打开的会话误判成"没有公式"。

## 本轮实测

| 检查 | 结果 |
| --- | --- |
| `python -m unittest desktop.test_desktop` | 67 通过（离屏，30.1s，新增 5） |
| 两实例各载入 1500 公式（真实 Rust 核心） | 都成功：2.99 s / 2.88 s，镜像源码一致 |
| 杀掉活窗口的核心后再调用（真实 Rust 核心） | 0.06 s 内重启并恢复：文档与公式会话都已重放（新 PID，`scan` 正常） |
| `BridgeTest` | 5 例；其中一例故意慢 6.0 s（＞ 旧死线 5 s），改前会报「没有响应」 |

新增用例（`desktop/test_desktop.py::BridgeTest`）：`test_a_slow_core_is_waited_for_instead_of_killed`、`test_a_core_that_died_mid_request_is_replaced_with_the_document_replayed`、`test_a_core_that_stopped_answering_is_replaced_instead_of_abandoned`、`test_a_core_that_keeps_failing_reports_the_request_and_its_stderr`、`test_a_backend_that_exited_is_replaced_for_the_next_request`。

改动文件：`desktop/bridge.py`、`desktop/test_desktop.py`、`docs/desktop.md`。Rust 核心未改动，无需重建后端。


