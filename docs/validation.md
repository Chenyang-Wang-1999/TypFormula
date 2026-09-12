# 验证记录 · 2026-09-09

> **改名说明（2026-09-12）**：项目原名 Visual Typst，现名 **TypFormula**。本文按日期保留当时的实测记录，因此**下面正文里出现的 `visual-typst` / `visual_typst` / `VISUAL_TYPST_*` 一律是当日的名字**，不是当前可用名。当前名字对照：crate `visual-typst`→`typformula`、`visual-typst-core`→`typformula-core`、`visual-typst-layout`→`typformula-layout`，二进制 `visual-typst.exe`→`typformula.exe`，环境变量 `VISUAL_TYPST_*`→`TYPFORMULA_*`，协议标签 `visual-typst-raw-`/`visual-typst-origin-v1`→`typformula-raw-`/`typformula-origin-v1`。改名的当轮实测见文末 2026-09-12 一节。

> **当前状态（2026-09-10 之后）**：仓库只维护原生桌面编辑器。Web 前端（`web/`）、VS Code 扩展（`extensions/`）、VSIX 产物（`dist/`）、HTTP 模式（`start.cmd`）、WASM 桥（`src/wasm.rs`）和 npm 工具链（`package.json`、`scripts/`、`tests/*.test.mjs`）已删除，随附字体从 `web/fonts/` 移到 `fonts/`。下面按时间顺序保留当时的实测记录：其中 Web/VSIX 相关的构建、`npm test`、`build.cmd`/`start.cmd`、`web/core.wasm` 等条目属于历史证据，不再是可执行的验证入口；当前可用的入口见 [README「验证入口」](../README.md)。

## 原生 Qt 桌面端

后续桌面交互回归扩展至 32 项：增加源码补全弹窗 Enter 接受与撤销、真实公式命令投影补全、新公式源码自动触发、命令/字符串衬底、四向边界跳出、Raw 源区间移动仍复用 SVG、离开修改过的脚标槽时仅刷新基底 Raw、变量行高与源码行号，以及预览周期不请求 Raw、不重建编辑投影。均为离屏测试。

增量公式索引回归覆盖 80/60/200 公式文档：普通正文编辑使用 Typst `Source::edit` 返回的局部重解析范围，200 个公式时重建 0 个公式且不调用全量 `analyze`；单公式编辑仅重建局部少量公式；`let` 修改会重建后续受影响宏投影。未受影响的 Qt 文本块和公式对象 ID 也保持不变。测试机上，200 个公式的旧全量公式投影本身约 395ms；常驻 Equation AST 加空宏注册表快路径后首次投影约 210–220ms；增量后的完整编辑约 75–95ms，剩余主要是 Python 投影映射和 Qt 局部格式更新。

桌面离屏回归现为 44 项。新增检查相同 Raw 源码只产生一次原生请求并共享同一结果、跨公式的不同 Raw 合并为一次编译，以及同一个 SVG 连续绘制两次只创建/执行一次矢量渲染器，第二次使用 DPI 位图缓存。脚标修改仍会使对应共享 Raw 失效；未变化页面协议复用原 Qt 控件。

桌面窗口已移除实时预览 Dock 和自动页面编译。专项测试确认后台编辑周期不会请求 `/api/preview`，F5 路径调用 `/api/pdf`，返回值由 `typst-pdf` 生成且以 `%PDF` 开头，并通过系统默认阅读器 URL 打开。原生适配器回归现为 16 项。

> **后记（2026-09-12）**：实时预览后来**以另一条路加回来了**——走 Tinymist 自己的预览（`/api/preview/live` + web view），默认关闭、开启才渲染。上面这句"已移除"描述的是**当时**的状态与当时那条路（自己的整页编译）；`后台编辑周期不会请求 /api/preview` 这一条**至今仍然成立**（预览走的是另一个路由，且只在开启时发）。详见本文末"实时预览回来了"一节。

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

# 宏内片段改走普通取图路径 · 2026-09-10

可展宏原先有一套私有渲染机制：给每个可展定义生成一份**空串实例**投影（定义之后接 `#name("", …)`），编译后按 `warmup_key`（其实"到定义末尾的文档前缀"）+ 源区间缓存 SVG，并由"预热是否失败"决定这个宏还能不能展开（失败则降级为普通调用）。这一轮整套删掉，宏内片段与正文片段走同一条路。

## 为什么不再需要

`Document::annotate` 早就给了宏内片段一个**文档区间**（`definition_raw_ranges` 在"到定义末尾的前缀"里按序数取第 n 次出现），并把它写进 `render.raw` —— 也就是普通片段请求里的那种区间。此前不走这条路只有两个原因：预热先把图塞进了 `typesetter.cache`（`load_raw` 见到缓存就跳过），而那份图来自空串实例。现在请求照常发出，文档里对该宏的**调用**就是编译它并产出那一帧的地方。

实测（`workspace/main.typ`，其中 `#let pd(f, x) = $ frac(partial #f, partial #x) $` 与 `$ pd(f, x) $`）：

| 片段 | 位置 | 改前 | 改后 |
| --- | --- | --- | --- |
| 定义体 `partial` / `#f` / `partial` / `#x` | 384:391 / 392:394 / 396:403 / 404:406 | 空串实例出图 | 普通区间出图 |
| 调用点 `partial` ×2 | 同上两个 `partial` 区间 | 空串实例出图 | 与定义体**同一张图**（`('raw', 文本)` 共享） |
| 该文档缓存 | — | 9 项，**3 项被拒** | 7 项，**0 项被拒** |

## 截断：定义片段必须看到调用

`context_end` 是"只编译到最后一个需要出图的公式"的优化。片段在定义里、排版结果产生在调用处，而调用可能在文档更后面——实测同一份文件里，只把定义体公式算作可见时截断点落在 409（定义结尾），请求回来的 `items` 是**空的**（定义从未被实例化）；不截断则四个片段全部返回：

| 请求 | items |
| --- | --- |
| `context_end = 409`（定义结尾） | `[]` |
| 不截断（全文） | `384:391:0`、`392:394:0`、`396:403:0`、`404:406:0` |

因此只要本次请求含定义内的片段，窗口就把 `context_end` 取全文（`window.py::load_raw` 用 `styles` 里的 `let` 区间判断）。代价是含可展宏的文档失去"后文错误不带走本屏图"这一层保护，由既有的分半重试（`salvaged`/`failed`）兜底；定义从未被调用时片段没有图，按"暂无图像"显示。

## 删除面

| 层 | 删除 |
| --- | --- |
| Rust | `src/prewarm.rs`（只剩取区间两个函数，移入 `typst.rs` 为 `definition_raw_ranges`/`raw_ranges`）、`src/warmup_service.rs`、`Services::prewarm` 与 `Services.warmups`、`/api/prewarm` 与 `/api/cache/clear` 路由、`macro_warmup` 动作、`reset_warmups`、`FAILED_WARMUPS`/`warmup_failed`/`set_warmup_results`/`clear_warmup_results`/`warmup_within_limit`、`MacroCache::clear`、`equation_at`；`warmup_key` 改名 `definition_prefix`（保留，它就是取区间要用的前缀）；视图字段 `warmup_key` 删除、`warmup_range` 改名 `source_range` |
| 可展性 | 只由结构检查决定（体是公式、位置参数唯一、参数不落在 Raw 里、每个参数都有槽位），"空串实例预热失败→按普通调用渲染"这条降级路径取消 |
| Python | `/api/prewarm` 与 `warmup_status`、`macro_warmup` 调用、`reset_warmups`、`load_raw` 里的 `warmup_key` 跳过分支、`classification_changed` 重新 `analyze`、`Typesetter.raw` 的 `(warmup_key, range)` 查表、`rawcache` 身份里的 `warmup_key`、`incremental` 的 `warmup_range` 平移、`BUDGETS` 的 `macro_warmup` |

## 本轮实测

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | 86 通过 / 5 忽略（`macro_scope` 6 通过，其中 3 条为改写后的新用例） |
| `cargo test --test macro_scope -- --ignored`（真实渲染器） | 通过：调用点片段由普通请求出图，`items` 一条、含 `<svg>`、`start` 等于定义内偏移 |
| `python -m unittest desktop.test_desktop` | **68 通过**（离屏，31.7s） |
| 真实窗口 `workspace/main.typ` | 7 公式全部可编辑；定义体与调用点片段均出图（7 项缓存，0 被拒）；进入宏体公式 8 个 stop、324 个墨点 |

新增/改写用例：`tests/macro_scope.rs::expandability_is_decided_by_structure_alone`、`a_template_fragment_is_asked_for_at_its_own_place_in_the_definition`、`a_call_site_fragment_carries_the_definition_range_it_is_rendered_from`、`duplicate_fragments_in_one_definition_keep_distinct_ranges`、`a_call_site_fragment_renders_from_the_document_its_own_call`；`desktop/test_desktop.py::test_a_macro_fragment_is_rendered_from_its_call_site_like_any_other`、`test_a_definition_fragment_keeps_the_call_that_renders_it`。

改动文件：`src/typst.rs`、`src/view.rs`、`src/document.rs`、`src/services.rs`、`src/rpc.rs`、`src/lib.rs`（+ 删除 `src/prewarm.rs`、`src/warmup_service.rs`）、`tests/macro_scope.rs`、`desktop/window.py`、`desktop/mathview.py`、`desktop/rawcache.py`、`desktop/incremental.py`、`desktop/bridge.py`、`desktop/test_desktop.py`、`README.md`、`docs/desktop.md`、`docs/architecture.md`、`docs/lyx-desktop-rendering-study.md`。

# 只复用"不含函数调用"的片段：实验开关 · 2026-09-10

## 开关做了什么

`VISUAL_TYPST_RAW_CACHE=plain` 只改一件事：`load_raw` 的命中判定（`window.py`）对**含函数调用**的片段不再算命中，于是它每一轮都进请求批次。含调用与否由 `rawcache.contains_call` 从片段**文本**判断（`标识符` 紧跟 `(`），不是解析：`cases(1 & x > 0, 2 & y < 0)`、`cancel(a)`、`#pd(f, x)` 算，`partial`、`#f`、`x_1` 不算。

两点刻意保持：

- 上一张图**留在缓存里继续绘制**，所以实验期画面不会退回源码，也不省下栅格化——多出来的编译是唯一差别；
- 被拒（`False`）的片段照旧跳过，只在下一次编辑后重试，否则一份坏文档会被每 160 ms 滚动重编一次。

## 冒烟实测（真实后端 + 真实适配器）

文档 `正文 $ cases(1 & x > 0, 2 & y < 0) $ 与 $ partial + 1 $。`（两个片段：`cases(…)` 含调用、`partial` 不含），每轮 `load_raw` 后等事件循环跑完：

| 轮次 | 模式 | `/api/render` 请求数 | 请求片段 | 出图 | 缓存项 |
| --- | --- | --- | --- | --- | --- |
| 冷启动 | `all` | 1 | `(9,36)`、`(45,52)` | 2/2 | 2 |
| 第 2、3 轮 | `all` | 0 / 0 | — | 2/2 | 2 |
| 第 1、2、3 轮 | `plain` | 1 / 1 / 1 | `(9,36)` | 2/2 | 2 |

即：默认模式下第二、三轮**零请求**，`plain` 下每轮重编一次含调用的片段；三次都仍然出图 2/2（没有退回源码）。这个文档极小、后端是热的长驻 release 引擎，单次请求 < 0.5 ms，所以延迟差看不出来——量级要在大文档上测，这也是这枚开关留着的原因。

## 本轮实测

| 检查 | 结果 |
| --- | --- |
| `python -m unittest desktop.test_desktop` | **69 通过**（离屏，30.5s，新增 1） |

新增用例：`desktop/test_desktop.py::test_the_experiment_switch_reuses_only_call_free_fragments`（默认模式两轮零请求；`plain` 下只重问 `cancel(a)`、`partial` 不再问、两个片段都还在出图；`False` 在同一个 revision 内不重问）。

改动文件：`desktop/rawcache.py`、`desktop/window.py`、`desktop/test_desktop.py`、`docs/desktop.md`、`docs/validation.md`。Rust 与前端构建未改动。

# 编辑器被源码栏光标拽回文档开头 · 2026-09-10

症状：把编辑器滚到文档中段后，**任何一次编辑**（含公式编辑）都会跳回文档开头。

## 定位

120 行文档（每行一个 `$ x_{i} + {i} $`），真窗口 + 真后端，编辑前把编辑器滚到 `2594`，并在滚动条的 `valueChanged` 上挂一个打印调用栈的探针：

```
after load                         scroll=     0 max=  5188 dock=     0/  5252
scrolled to the middle             scroll=  2594 max=  5188 dock=  2594/  5252
   valueChanged(27) via mirror_scroll:201 <- <lambda>:102 <- project:182 <- replace:296
after plain replace                scroll=    27 max=  5188 dock=    27/  5252
   valueChanged(27) via decorate_incremental:176 <- incremental_project:122 <- project:177 <- math_action:346
after math input                   scroll=    27 max=  5211 dock=    27/  5252
```

`project:182` 是 `Window.project` 里那行 `self.source_view.setTextCursor(cursor)`（同步源码栏光标，早于 `f54049d` 就存在）。`QTextEdit.setTextCursor` 会把该控件滚到光标处：源码栏的光标一直停在文档开头（这个面板默认隐藏，没人点过它），而它的滚动值此前是被 `mirror_scroll` 从编辑器镜像过来的 `2594`，于是这一步把源码栏拉回 `27`，`valueChanged(27)` 顺着 `<lambda>:102`（源码栏滚动条 → `mirror_scroll`）反过来把**编辑器**设成 `27`。也就是说 `f54049d` 新增的 `mirror_scroll` 让"镜像"变成了双向环：跟随者（源码栏）被程序改动后会去改写主人（编辑器）。公式编辑与普通编辑都会触发，因为两者都走 `Window.project`。

## 改法

| 位置 | 改动 |
| --- | --- |
| `Window.mirror_scroll` | `self.loading` 期间直接返回：重投影重建两栏、各放回自己的滚动值，那不是"要跟随的滚动" |
| `Window.project` | 设置源码栏光标前记下它的滚动值，设完立刻放回（`loading` 期间镜像已被抑制，放回不会回流） |
| `Window.dock_visibility_changed` | 打开源码栏时的重新着色 + 重新定行高也包在 `loading` 里，行高变化可能挪动它的滚动条，同样不该被跟随 |

## 实测

| 场景 | 改前 | 改后 |
| --- | --- | --- |
| 中段普通编辑（编辑器滚动值，文档 120 行） | 2594 → **27** | 2594 → 2594 |
| 中段公式编辑（`activate` + `input`） | 2594 → **27** | 2594 → 2594 |
| 两栏跟随仍有效（`test_the_source_dock_lines_up_with_the_editor`） | 通过 | 通过 |

新增用例 `desktop/test_desktop.py::test_an_edit_keeps_the_view_where_the_reader_scrolled_it`（离屏 `show()` 后取真实滚动范围，先做普通编辑再做公式编辑，两次都要求滚动值不变）。把 `desktop/window.py` 暂存回改前版本后该用例失败：`AssertionError: 21 != 1272 : an edit must not move the view`。

| 检查 | 结果 |
| --- | --- |
| `python -m unittest desktop.test_desktop` | **70 通过**（离屏，31.4s，新增 1） |

改动文件：`desktop/window.py`、`desktop/test_desktop.py`、`docs/desktop.md`、`docs/architecture.md`、`docs/validation.md`。Rust 未改动。

# 片段的图跟随谁：attach 用 script 形状，其余只用自己的源码 · 2026-09-10

起因是用户报告 `$stretch(->)^x$` 与 `$stretch(->)^(…) $` 里的 `stretch(->)` 显示同一张图（应不同），随后讨论到"内容变动时到底该重渲染哪些片段"。

## 后端没问题，是客户端按"文本"复用

片段是被**插回原位置**编译的（`#[$ 片段 $<label>] `），所以它的盒子和周围内容有关。真后端 + 真适配器直接请求 `/api/render`：

| 请求 | 片段区间 | 返回 |
| --- | --- | --- |
| `$ stretch(arrow.r)^("a") $` | `2:18` | **11.000 pt** |
| `$ stretch(arrow.r)^("a much longer label") $` | `30:46` | **66.305 pt** |
| 两个区间同一批 | — | 两条 item：`2:18:0` 11.000 / `30:46:0` 66.305 |

适配器里本来就有这条断言（`native-adapter/src/render.rs::stretch_document_probe`）。客户端两处把它抹平：`load_raw` 用 `('raw', 文本)` 判"已有图就不再请求"（第二个区间**连请求都没发**），`Typesetter.cache`/`Typesetter.raw` 也只用文本做键。

顺带排掉一个假线索：`$…^(xxxxxx)$` 会让整个请求报 `unknown variable: xxxxxx`，那是 `xxxxxx` 在 Typst 里本来就不是已知符号，与拼接无关。

## 实测：哪些兄弟改动会改变片段自己的盒子

同一片段放在两侧不同环境里，只比较它自己的 width（真后端 + 真适配器）：

| 情形 | 短 → 长 | 变化 |
| --- | --- | --- |
| `stretch(arrow.r)` 作为 **attach 的基底**，脚标短/长 | 11.000 → 30.669 pt | **DIFFERS** |
| `cal(A)` 作为基底，脚标短/长 | 8.778 → 8.778 | 不变 |
| `cal(A)` 在分式里，另一格变宽/变高 | 8.778 → 8.778 | 不变 |
| `cases(1 & x > 0, 2 & y < 0)` 旁的兄弟变长 | 40.211 → 40.211 | 不变 |
| `mat(1, 2; 3, 4)` 旁的行变长 | 32.692 → 32.692 | 不变 |
| `cal(A)` 在 `abs(...)`/`sqrt(...)` 里，兄弟变长 | 8.778 → 8.778 | 不变 |
| `cal(A)` 作为 `sum` 的极限，极限变宽 | 6.514 → 6.514 | 不变 |
| `stretch(arrow.r)` 在矩阵格里，行变长 | 11.000 → 11.000 | 不变 |

样本里**只有"attach 的基底"会跟着脚标变**——这正是 Typst `stretch` 的文档用法（基底拉伸到附件的宽度）。行宽、分式另一格、矩阵邻格都不影响片段自己的盒子。

## 视图里到底有什么（`$frac(x^(2+), cal(A))$`）

```
cell
  stop(root.p0)
  fraction
    cell                     ← 分子
      stop
      script
        cell                 ← 基底
          stop  char('x')  stop
        cell                 ← 上标
          stop  char('2')  stop  symbol('+')  stop
        absent
      stop
    cell                     ← 分母
      stop  raw('cal(A)' render_id=14:20)  stop
  stop(root.p1)
```

两点和"顺着 AST 追溯"的直觉不同：`+` 是 **symbol 结点**（不是 Raw），`frac`/`x^…` 是编辑器自己排版的结构结点（没有 `render_id`）；这条公式里唯一的 Raw 是分母的 `cal(A)`。于是"只重渲染路径上的 Raw"在这条公式里等于**什么都不重渲染**（光标在分子里时路径上一个 Raw 也没有）；而在 `$stretch(->)^x$` 里，跟着脚标变的基底是**兄弟分支**，同样不在路径上——所以判据不能是"路径"，只能是"这次变动能影响到谁的盒子"。

## 规则与实现

| 位置 | 规则 |
| --- | --- |
| 不在 `script` 里的片段 | 键 = `('raw', 源码)`：一个文本一张图，任何兄弟改动都不重取 |
| 在 `script` 里的片段 | 键 = `('raw', 源码, script 形状摘要)`：基底、脚标、上下标方向变了就重取 |
| 宏定义体内的片段 | 上下文只在**定义所在的公式视图**里建立（只索引"该公式自己定义"的区间，按 `render_id` 是否落在公式内判断），定义视图与各调用点视图因此同键 |

实现（客户端，Rust 未动）：

| 位置 | 改动 |
| --- | --- |
| `rawcache.raw_key(node)` | 有 `_context` → 三元键，否则二元键 |
| `rawcache.signature_digest(value)` | 视图子树的短摘要 |
| `Window.context_index` / `index_fragment_contexts` | 每个公式走一遍视图，记下每个片段所处的最近 `script`，上下文 = `signature_digest(signature(script))`（`signature` 跳过 stop/draft 结点，所以移动光标不改键） |
| `Window.stamp_contexts` | `prepare_view` 与每次设置 `math_state` 时按 `render_id` 区间查表写 `_context` |
| `Window.drop_dead_contexts` | 每轮 `load_raw` 清掉已无脚本使用的上下文，避免打字积压 |

## 实测（真后端 + 真窗口）

| 场景 | 结果 |
| --- | --- |
| `$frac(x^(2+), cal(A))$`，在分子里打字 | 请求**空**（`[]`）；`cal(A)` 仍用 `('raw','cal(A)')` 缓存的图（旧行为：会重取 `cal(A)`） |
| 两条公式的 `stretch(arrow.r)`，脚标 `("a")` / `("a much longer label")` | 一批两个区间：**11.000 pt** / **66.305 pt**，各画各的 |
| 同一公式里把脚标改宽 | 重新请求基底 `stretch(->)` |
| 宏定义体 + 调用点的 `cancel(a)^2` | 两视图同键 `('raw','cancel(a)','96a8ac09c87153d5')`、同一张图 |
| 在 `stretch(arrow.r) + x` 公式里连打 4 个字符 | 缓存稳定 **2 项**，无积压 |
| 重取期间 | 该片段先按"无图"画源码 **172 ms**（160 ms 去抖 + 一次编译），随后回到图 |

## 测试

改写 `test_a_base_that_an_attachment_stretches_is_asked_for_again`（两条公式各要各的区间；同一公式里改宽脚标会重新请求基底）；`test_raw_keeps_svg_when_adjacent_text_moves_its_source_range` 恢复原来的 `cancel(a)` 用例（"编辑相邻字符不重取"对不在 attach 里的片段成立）；直接读写 `typesetter.cache` 的 4 处用例改用 `raw_key(node)`。

| 检查 | 结果 |
| --- | --- |
| `python -m unittest desktop.test_desktop` | **71 通过**（离屏，31.6s） |

改动文件：`desktop/rawcache.py`、`desktop/mathview.py`、`desktop/window.py`、`desktop/test_desktop.py`、`docs/desktop.md`、`docs/architecture.md`、`docs/validation.md`。Rust 未改动，无需重建后端。

## 后端槽位改成声明式：一张表 + 两条测试 · 2026-09-11

动机：`Kind` 的槽位含义与导航规则此前散在 5 个文件的 190 处 `Kind::` 引用里。其中 `math.rs` 的 `entry_cell`（`_ =>` 兜底）、`math_class`（`_ => 0`）、`idx_horizontal`（三处 `matches!` 特例）和 `cursor.rs` 的 `vertical`（`_ => {}`）各带静默分支——加一个 `Kind` 时编译器只强制 2 项（`view_atom` 的视图投影、`write_atom` 的源码回写），其余静默走默认值。本次把它收敛成一张穷尽声明的表。

### 步骤 0：先建安全网（回写往返）

`write_atom` 是唯一**没有编译器兜底**的义务：它的结果会被 `Document` 写回权威源码，写错就是静默损坏文档。`tests/round_trip.rs` 的判据是"写出去、读回来、必须等于原树"，14 个用例覆盖 13 个可往返 Kind。

三个刻意的排除，各自有理由而非遗漏：`TemplateCall`（`write_atom` 直接 `unreachable!`）与 `Parameter`（`#parameter0` 不是合法 Typst）只存在于宏模板、不属于可编辑源码树；`Unknown` 承载半打完的命令草稿，`fra` 不是一个公式。

**证明这条测试有牙齿**：故意把 `typst.rs` 里 `root` 的两个参数写反。

| 变更 | 结果 |
| --- | --- |
| `root({c(1)}, {c(0)})` → `root({c(0)}, {c(1)})` | `root_atoms_round_trip` 失败：`源码 "root(3, x)" 写成 "$ root(x, 3) $" 后读回了不同的树`，并列出两侧格子 |
| `git checkout -- src/typst.rs` 还原 | 14 通过 |

**意外结论**：这 14 个用例**在改动前就全部通过**。`write_atom` 本来就正确（含 `root` 的参数倒序、多原子基底的加括号、对齐公式的 `row_lengths`），这条测试是把不变量钉住，不是修 bug。

### 步骤 1：`src/slots.rs`

一个节点声明 6 项：`slots`（角色 + 相对字号 + 是否可空）、`arity`（定长 / 重复）、`entry`（光标首次进入哪一格）、`horizontal`、`vertical`、`class`。`Kind::decl()` 是覆盖全部 16 个 `Kind` 的穷尽 `match`。

| 义务 | 改动前 | 改动后 |
| --- | --- | --- |
| 视图投影 `view_atom` | 编译强制 | 编译强制（不变） |
| 源码回写 `write_atom` | 编译强制 | 编译强制 + 往返测试 |
| 光标进入 `entry_cell` | `_` 兜底 | **声明驱动** |
| 左右移动 `idx_horizontal` | 三处 `matches!` 特例 | **声明驱动** |
| 上下移动 `cursor::vertical` | `_ => {}` | **声明驱动** |
| 数学间距类 `math_class` | `_ => 0` | **声明驱动** |

`entry_cell` / `idx_horizontal` / `vertical` 里的 `match Kind` 全部消失，改为读声明的通用算法。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **115 通过 / 5 忽略**（基线 86 + 往返 14 + 导航 9 + 表自洽 6） |
| `python -m unittest desktop.test_desktop` | **71 通过**（离屏，34.1s） |
| 表自洽性（`src/slots.rs` 内 6 个单元测试） | 命名角色都在表里、定长图式格数 == 真实格子数、入口与左右邻居不越界、字符类正确 |

对真实 release 二进制（`--desktop-core` 管道）逐条探针，不经过 Qt：

| 探针 | 结果 | 钉住的声明 |
| --- | --- | --- |
| `root(3, x)` 向右进入 | `[c1]` | `Entry::Role{forward: Index}`——**参数倒序感知**的入口 |
| 根式内下 / 上 | `[c0]` / `[c1] pos=1` | `Swap` 互换、`end_up` 落格尾 |
| 再向右 | `[c0]` | `Horiz::Pair` 折回 |
| `mat(1, 2; 3, 4)` 向右进入 | `[c0]` | `Entry::GridMiddle` |
| 矩阵下 / 上 | `[c2]` / `[c0]` | `Vertical::Column` |
| 矩阵列内连按向右 | `c0 → c1 → 离开` | `Horiz::Column` 列边界阻挡 |
| `frac(x^2, sqrt(y))` | 分子/分母互换；进入 `Script` 落在基底 | `Frac` 的 `Swap`、`Script` 的 `Attach` |

### 表自身的保护强度：变异检查

对 `src/slots.rs` 逐条改错一个声明，再跑全套，看是否有测试失败：

| 变异 | 结果 |
| --- | --- |
| `Frac` `end_up` false → true | 1 个套件失败 |
| `Root` `end_up` true → false | 1 个套件失败 |
| `Root` `entry` Index ↔ Radicand | 1 个套件失败 |
| `Root` `horizontal` Pair → Linear | 1 个套件失败 |
| `Grid` `entry` GridMiddle → Edge | 1 个套件失败 |
| `Aligned` `entry` Edge → GridMiddle | 1 个套件失败 |
| `Grid` `horizontal` Column → Linear | 1 个套件失败 |
| `Script` `entry` Base → Upper/Lower | 1 个套件失败 |

**过程里踩到的坑，值得记下**：第一遍变异检查用 `Copy-Item` 还原备份，而 `Copy-Item` **保留被复制文件的 `LastWriteTime`**。还原后 `slots.rs` 的时间戳比已编译产物更旧，cargo 判定"无需重建"，于是后续测试跑的是**变异后的旧产物**。这一遍得出的"某两条声明无测试覆盖"是**错的**（哈希显示文件本身已正确还原，坏的是构建）。正确做法：写回后把 `LastWriteTime` 置为当前时间。修正后重跑，8 条变异全部被抓住。

**顺带发现的真实缺口**：在补 `tests/caret_navigation.rs` 之前，`Frac` 的 `end_up` 与 `Grid` 的 `Entry::GridMiddle` 确实**没有任何测试覆盖**——把任一条改错，全套仍然全绿。新的导航用例把它们连同 `Root` 的 `end_up`、`Aligned` 的 `Entry::Edge`、`Grid` 的列边界一起钉在真实光标位置上。用例全部先在真实二进制上量过再写下来。

### 附带修正：`char_class` 里的空格

`char_class` 的右括号集原写作 `")] }"`，中间那个空格让**空格字符**落进"右括号"类。本次改成 `")]}"`。

它不是死代码：`interpret_char` 的文本格分支排在 `' '` 分支**之前**，所以在行内字符串里打字真的会插入 `Char(' ')`。唯一读者是 `move_word`（Ctrl+方向键），它按类聚合一段连续字符：

| 文本格 `"a b c"`，Ctrl+→ 从格首 | 空格类 | 结果 |
| --- | --- | --- |
| 改动前 | 5（右括号） | 逐个字符走：pos 0 → 1 → 2 → 3（0 与 5 交替，每组只含一个字符） |
| 改动后 | 0（普通） | 一次走到整段末尾：pos 0 → 5 |

即 Ctrl+→ 在行内字符串里由"逐字符"变成"按类走整段"，与公式内普通字符的处理一致。`tests/caret_navigation.rs::a_text_run_is_one_class_so_ctrl_arrow_moves_by_run` 把这个结果钉住（先量后写）。

### 未做（需要先确认）

- 协议变更：`view` 携带 `role`、前端 `elif kind ==` 链改成按角色分派。

改动文件：`src/slots.rs`（新增）、`src/math.rs`、`src/cursor.rs`、`src/lib.rs`、`tests/round_trip.rs`（新增）、`tests/caret_navigation.rs`（新增）、`docs/architecture.md`、`docs/validation.md`。

## 视图名与回写模板进声明表 · 2026-09-11

`Kind::decl()` 原先声明 6 项（槽位、形态、入口、左右、上下、间距类）；本次把**视图名**与**回写模板**也放进去，共 8 项，加一个 `Kind` 时一次说清。

### `view`

`Decl::view` 是前端排布策略名。`view_atom` 的 16 个字面量全部删掉，改读 `atom.decl().view`，视图名不再有两份；`Frac | Sqrt | Root` 三个 arm 合并成一个。`MacroCall` 有第二个形态（展开不了时显示调用与参数），具名为 `slots::view::MACRO_COLLAPSED`。

### `Write`

`Decl::write` 是一个 10 变体的枚举，覆盖 16 个 `Kind` 的拼写形状，`write_atom` 改为按它分派。收益最直接的是根式：

```rust
write: Write::Template("root({1}, {0})"),
```

"内部是 `[被开方式, 根指数]`、Typst 是 `root(指数, 被开方式)`"这个反转，过去分散在 `parse_atom` 的 `args.swap(0, 1)` 与 `write_atom` 的 `c(1), c(0)`，相隔 60 行；现在是一行自解释的声明。

**模板必须单遍展开。** 一个格子的源码里可能有花括号（`Text` 写成带引号的字符串），先后替换会让文本里的 `{1}` 被当成占位符：

| `frac("x{1}y", b)` | 写法 | 结果 |
| --- | --- | --- |
| 先后替换 `{0}` 再 `{1}` | 错 | `frac("xby", b)`——文本被改写 |
| 单遍扫描（现行） | 对 | `frac("x{1}y", b)` |

`tests/round_trip.rs::a_template_does_not_re_expand_braces_that_came_from_a_cell` **直接断言写出的字符串**，因为"先后替换"也可能往返成*另一棵*树。把展开器改成双遍，该用例失败（14 通过 1 失败）；还原后 15 全绿。

`write_atom` 里声明与 `Kind` 不符的分支做成**响亮的 `unreachable!`**，不是静默空串——它写回权威源码，写错是静默损坏文档。两个新单元测试让它可证明不可达：`every_declaration_agrees_with_the_kind_it_describes`、`a_template_only_names_placeholders_that_exist`。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **119 通过 / 5 忽略** |
| `python -m unittest desktop.test_desktop` | **71 通过**（离屏，33.0s） |
| 视图名逐一比对 | 九个公式（`x^2`/`frac`/`sqrt`/`root`/`mat`/`hat`/`abs`/`dif`/`aligned`）的 kind 与改动前**逐字相同** |
| 双遍展开变异 | 被花括号用例捕获 |

## `Script` 存储整形为固定三槽 · 2026-09-11

原先 `Kind::Script { cell_1_is_up: bool }` 配**变长** `cells`：`[base]`、`[base, up]`、`[base, down]`、`[base, up, down]` 四种长度，`cells[1]` 的含义取决于标志位。现在固定为 `[base, upper, lower]`**三格**，空格子表示"没有这个脚标"，标志位删除。新增 `math::script_cell(up) -> usize` 作为唯一的格索引来源。

| 位置 | 原先 | 现在 |
| --- | --- | --- |
| `script_idx(up)` | 按 `cells.len()` 分三种情况 + 比标志 | 常量格索引 + 判空 |
| `ensure_script(up)` | 处理 1→2、2→3 的插入，方向不符时用 `mem::take` 搬格子 | **删除**，调用方直接用 `script_cell(up)` |
| `remove_script(idx)` | 3→2 时回头改标志 | 清空该格 |
| `parse_atom` 的 `MathAttach` | 按需 push，长度随源文变化 | 恒定三格，按 `script_cell` 落位 |
| `view_atom` 的 `Script` | 合成 `absent` 补齐 | **无需改动**——`script_idx` 的判空让原式产出不变 |

### 可观察变化（实测，两处，均为预期）

真机 release 二进制（`--desktop-core` 管道）逐条探针：

| 探针 | 改动前 | 改动后 |
| --- | --- | --- |
| `$ x_1 $` 右、右、下 | `c0p0 → c0p1 →` **`c1`**`p1` | `c0p0 → c0p1 →` **`c2`**`p1` |
| `$ x^2 $` 右、右、上 | `c0p0 → c0p1 → c1p1` | **不变** |
| `$ x^2 $` 右、Tab×3 | `c0 → c1 → c0` | `c0 →` **`c1 → c2`** `→ c0` |

**视图结构一字未变**（这是前后端协议，必须不变）：

| 源文 | 视图 |
| --- | --- |
| `$ x $` | `cell stop char stop` |
| `$ x^2 $` | `script [cell(x), cell(2), absent]` |
| `$ x_1 $` | `script [cell(x), absent, cell(1)]` |
| `$ x_1^2 $` | `script [cell(x), cell(2), cell(1)]` |

两处变化都是预期的：下标固定在第三格；Tab 多停在一个空脚标格上（按 Tab 进空格再打字即可补脚标）。`tests/caret_navigation.rs::an_attachment_lives_in_a_fixed_cell_and_tab_visits_both` 把两者钉住。

### 声明表与存储从此一致

自洽性测试 `the_schema_covers_exactly_the_cells_a_kind_stores` **原先按名字排除 `Script`** 并注明"整形时会被逼着回来删掉这个例外"。本次整形后该例外已删除——`Script` 现在和其余 15 个 `Kind` 一样，槽位数必须等于真实格子数。

## `view` 携带 role、前端按角色分派 · 2026-09-11

### 后端

`View` 新增 `role: Option<&'static str>`（带 `skip_serializing_if`，所以是可选的纯增量字段）。值来自父节点的 `Decl::role_at(cell_index)`，经 `Role::name()` 映射为 `"numerator"`/`"denominator"`/`"base"`/`"upper"`/`"lower"`/`"radicand"`/`"index"`/`"inner"`/`"cell"`/`"arg"`。

线上实测形状（真实 release 二进制）：

| 源文 | 视图 |
| --- | --- |
| `frac(a, b)` | `fraction` → `cell[numerator]`, `cell[denominator]` |
| `root(3, x)` | `root` → `cell[radicand]`, `cell[index]` |
| `x_1` | `script` → `cell[base]`, `absent[upper]`, `cell[lower]` |
| `mat(1, 2; 3, 4)` | `grid` → 4× `cell[cell]` |
| `hat(x)` | `decoration` → `cell[inner]` |

`x_1` 那条值得注意：**空附件也是一个槽**，所以合成出来的 `absent` 也带 role——否则前端按角色查会查不到它。

### 前端

`mathview.py` 三处按角色取子节点：`fraction`（numerator/denominator）、`sqrt`/`root`（radicand/index）、`script`（base/upper/lower）。`Typesetter.slot(children, role, index)` 先按 role 找，找不到才回退到位置——回退让**旧内核也能驱动新前端**。`grid`/`aligned` 的 role 全是 `cell`，位置就是唯一信息，保持不变。

另外加了 `Typesetter.ARRANGEMENTS`：本前端实现的 25 个排布名的白名单。`kind` 不在其中时**报告一次**（经 `Typesetter.warn`，由 `window.py` 接到状态栏），而不是静默按横排画——将来加新排布时能立刻看出来，而不是画错了没人知道。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **119 通过 / 5 忽略** |
| `python -m unittest desktop.test_desktop` | **72 通过**（71 + 新增 1） |
| 线上 role 形状 | 5 个公式逐一比对，见上表 |

### 新增用例的"牙齿"是补出来的

`test_a_fraction_places_its_slots_by_role_not_by_position` 把 role 分派钉住：反转 `fraction` 的 children 数组，几何必须不变（按位置就会换掉分子分母）。

**第一版没有牙齿。** 它用 `frac(a+b, c)`，而 `a+b` 与 `c` 都是单行、盒子度量完全相同，所以"调换分子分母"在几何上看不出来——把角色查找禁用后它**仍然通过**。换成 `frac(frac(a, b), c)`（两槽高度不同）后，禁用角色查找立即失败：`AssertionError: 46.4 != 32.2275`，正是基线被调换的差值；还原后通过。用例里保留了这个前提的注释：**两个槽的盒子必须不同，这个测试才成立**。

改动文件（本轮三节合计）：`src/slots.rs`（新增）、`src/math.rs`、`src/cursor.rs`、`src/typst.rs`、`src/view.rs`、`src/lib.rs`、`desktop/mathview.py`、`desktop/window.py`、`desktop/test_desktop.py`、`tests/round_trip.rs`（新增）、`tests/caret_navigation.rs`（新增）、`docs/validation.md`。

## `Kind` 变体名向 Typst `MathKind` 对齐（第一步：纯改名 + 记下对应关系） · 2026-09-11

### 改了什么

`Kind` 的五个变体改名，让它们与 Typst `MathKind`（`vendor/typst/crates/typst-library/src/math/ir/item.rs`）同名：

| 原名 | 新名 | Typst 对应 |
| --- | --- | --- |
| `Kind::Frac` | `Kind::Fraction` | `MathKind::Fraction` |
| `Kind::Script` | `Kind::Scripts` | `MathKind::Scripts` |
| `Kind::Delim` | `Kind::Fenced` | `MathKind::Fenced` |
| `Kind::Grid` | `Kind::Table` | `MathKind::Table` |
| `Kind::Aligned` | `Kind::Multiline` | `MathKind::Multiline` |

`Write::{Grid, Aligned}` 同步改为 `Write::{Matrix, Rows}`：它们命名的**不是** `Kind`，而是"`mat(…)` 拼法"与"用 `&`/`\` 分行"两种写法，跟着 `Kind` 一起叫 Grid/Aligned 只会让人以为它们是一回事。

**没有动 `view` 名。** 线上前端看到的排布名仍是 `fraction`/`script`/`delim`/`grid`/`aligned`——排布名是给前端的绘图契约，只在画法变化时才需要改；变体名是给读代码的人的。这两件事本来就该解耦，本轮正是靠这一点做到了"改名不动协议"。

`Decl` 新增第 9 项 `typst: &'static [&'static str]`，逐条记下这个 `Kind` 认领哪些 `MathKind`：0 个（编辑器专有 `MacroCall`/`TemplateCall`/`Parameter`/`Unknown`）、1 个、或几个（`Raw` → `Box`/`Mathml`/`External`；`Sqrt` 与 `Root` 都 → `Radical`；`Decoration` → `Accent`+`Line`；`Char` → `Glyph`+`Number`）。

### 怎么让"对齐"可查而不是靠印象

核心 crate 只依赖 `typst-syntax`，**不依赖编译器**（这是刻意的：编辑器核心不链接 Typst 编译器），所以两套词汇表不能靠类型系统绑定。代替它的两条测试：

1. `the_math_kinds_are_read_correctly_from_the_vendored_source` 用 `include_str!` 读 vendor 里那份 `item.rs`，从 `pub enum MathKind<'a> {` 扫出变体名，断言**恰好 18 个**并逐个点名核对。Typst 增删改名一个变体会让它失败——而且它是下面那条的**自检**：扫描坏掉若返回空表，下面那条会"什么都没说"地通过。
2. `the_typst_vocabulary_is_covered_exactly_once` 断言每个 `Kind` 声明的名字都是真实存在的变体，并且**没被任何 `Kind` 认领的变体恰好等于** `slots::UNMODELLED`。

于是对齐的剩余工作量成了一个常量，写在 `src/slots.rs`：

```rust
pub const UNMODELLED: &[&str] = &["Cancel", "Group", "Primes", "SkewedFraction"];
```

四项性质不同，注释里写清了：`Group` **不是缺口**（编辑器的一个格子就是一个 group，不需要哪个 `Kind` 去代表它）；`SkewedFraction`(`a/b`)、`Cancel`(`cancel(x)`/`strike(x)`)、`Primes`(`x'`) 是**真缺口**，目前都退化成 `Raw` 源码片段。认领掉一项就必须改这个常量——改的时候正是写下"其余几项为什么还没做"的地方。同一条测试还钉住"被两个 `Kind` 共同认领"的只有 `Glyph`（`Char` 与 `Symbol`）和 `Radical`（`Sqrt` 与 `Root`），因为共享是一次决定而不是巧合。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **121 通过 / 5 忽略**（119 + 新增 2） |
| `python -m unittest desktop.test_desktop` | **72 通过** |
| 线上视图逐字节比对 | **21 个公式全部 identical**（见下） |

线上比对用的是真实 release 二进制：改名前先把 21 个公式的 `view` JSON 全量导出（`x^2`/`x_1`/`x_1^2`/`frac(a,b)`/`frac(frac(a,b),c)`/`a/b`/`sqrt(x)`/`root(3,x)`/`abs(x)`/`norm(x)`/`mat(1,2;3,4)`/`a &= 1 \ b &= 2`/`hat(x)`/`overline(x)`/`underline(x)`/`vec(x)`/`"text"`/`dif x`/`(a+b)`/`unknown_thing(x)`/`frac(x^(2+), cal(A))`），改名后重新导出并**按 JSON 全等**比较：`raw identical: True`，形状差异 0 处。

### 新测试的牙齿（四次变异）

| 变异 | 结果 |
| --- | --- |
| `Decoration` 的 `typst` 改成 `K_NONE` | 失败：未认领多出 `Accent`、`Line` |
| 扫描器 `.take(17)` 少读一个变体 | 两条都失败：自检报 `left: 17, right: 18` |
| `K_TEXT` 拼成 `"Tex"` | 失败：`声明对应 MathKind::Tex，但 vendored 枚举里没有这个变体` |
| `UNMODELLED` 删掉 `Primes` | 失败：`未建模的 MathKind 清单变了` |

四次都还原后全绿。变异一律用文件工具写回（会刷新 mtime），不用 `Copy-Item` 还原——上一轮就是 `Copy-Item` 保留旧时间戳、cargo 判定"无需重建"，测的是变异后的旧产物，得出了错误结论。

### 顺带修掉的两处失配

`slots.rs` 的模块注释还写着"视图名与回写模板**尚未**进声明表"，并列出理由——但上一轮已经把它们放进去了，是上一轮改完没删的陈旧注释。另外两份教学文档引用的 `Kind::Frac`/`Kind::Delim`/`Kind::Grid` 代码片段同步更新（`docs/rust-for-cpp.md` §4.8、`docs/rust-book-walkthrough.md` 第 6 章）。

`docs/validation.md` 里旧条目中的 `Kind::Script { cell_1_is_up: bool }` 等**保留原样**：那是当日实测记录，不是当前代码说明。

### 本轮发现、需要决定的三件事

这一步只做"改名 + 记账"，因为下面三件的**代价不是改名字**，写在下一轮之前先报：

1. **`Sqrt` + `Root` → `Radical`（合并）**：Typst 是一个 `Radical{radicand, index: Option}`。合并后 `sqrt(x)` 就是"根指数格为空"的根式，但两处会变：线上 `sqrt`/`root` 两个排布名要合成一个（前端要画"没有指数的钩子"）；更实际的是**光标**——今天 `sqrt(x)` 只有一格，合并后多出一个空的指数格，Tab/上下键会走到它，这是可观察的行为变化。另外 LyX 自己是分开的（`InsetMathSqrt` 与 `InsetMathRoot` 是两个 inset），合并会与"对照 LyX"这条线分叉。
2. **`Decoration` → `Accent` + `Line`（拆分）**：本身是纯词汇改动（两者的槽位与拼写都一样，`view` 可继续共用 `decoration`，前端不用动）。真正的收益在它能给 `Cancel` 一个家：`cancel(x)`/`strike(x)` 今天退化成 `Raw`，Typst 的 `CancelItem` 还有 `angle`/`inverted` 两个参数。
3. **`SkewedFraction`（新增）**：`$ a/b $` 今天和 `frac(a, b)` **合成同一个 `Kind::Fraction`**，回写时被规范化成 `frac(a, b)`——这是既有行为，`src/document.rs` 的注释里就写着"`$ a/b $` 序列化为 `frac(a, b)`"，`tests/structured_input.rs::fraction_slash_uses_typst_precedence_and_keeps_nested_fallbacks` 还把 `write_cell(&e.root) == "frac(x, 2)"` 钉成了预期。要保真就得新增 `SkewedFraction` 并改那条既有预期，同时前端要有"斜杠分式"的排布。

改动文件：`src/slots.rs`、`src/math.rs`、`src/cursor.rs`、`src/typst.rs`、`src/view.rs`、`tests/command_mode.rs`、`tests/structured_input.rs`、`docs/architecture.md`、`docs/rust-for-cpp.md`、`docs/rust-book-walkthrough.md`、`docs/validation.md`。

## 分层：内核与外围 · 2026-09-11

`Kind` 与 Typst 词汇对齐之后要动的是引擎侧的数据通道；在那之前先把代码分层，因为后面的每一步都要先知道"这一改动归内核还是外围"。

### 改了什么

拆成两个 crate，五个内核模块搬进 `crates/core/`：

| 层 | crate | 内容 |
| --- | --- | --- |
| 内核 | `visual-typst-core`（`crates/core/`） | `math`、`slots`、`typst`、`cursor`、`view` |
| 外围 | `visual-typst`（仓库根，lib + `visual-typst` 二进制） | `document`、`desktop`、`rpc`、`services`、`packages`、`workspace` |

搬动之前先把 `use crate::` 全梳了一遍，结论是**内核本来就已经闭合**：`math ↔ slots`、`typst → math/slots`、`cursor → math/slots/typst`、`view → cursor/math/slots/typst`，五个模块谁也不引用 `document`/`services` 等任何一个。所以这一轮只差一道边界，没有解环工作。

### 边界是编译器守的，不是文档写的

在 `crates/core/src/lib.rs` 里加一行 `use visual_typst::document as _;` 实测：

```
error[E0432]: unresolved import `visual_typst`
  --> crates\core\src\lib.rs:29:5
   |     ^^^^^^^^^^^^ use of unresolved module or unlinked crate `visual_typst`
```

还原后通过。这就是把内核做成独立 crate（而不是一个模块）的全部理由——`pub(crate)` 拦不住内核文件伸手去够外围文件。

### 两个 cargo 陷阱，都是实测撞上的

1. **工作区会把路径依赖自动收成成员。** 加了 `[workspace] members = ["crates/core"]` 之后构建直接失败：`vendor/typst/crates/typst-syntax` 被拽进我们的工作区，于是它 `edition = { workspace = true }` 去继承**我们的** `[workspace.package]`——那里没有 `edition`。而 vendored 的 Typst 是自带工作区的（`vendor/typst/Cargo.toml` 里就写着 `[workspace.package] edition = "2024"`）。加 `exclude = ["vendor/typst", "native-adapter"]` 后正常。
2. **裸 `cargo test` 只测根包。** 分层后总数从 121 掉到 111——差的那 10 个正是内核自己的 `slots` 单元测试，它们静默地不跑了。加 `default-members = [".", "crates/core"]` 后总数回到 **121**（分层前 121）。这条写进根 `Cargo.toml` 的注释里，因为它只会在"测试变少了但没人注意"的时候咬人。

### 顺带修掉的路径

- `crates/core/build.rs` 原来按相对路径读 `config/symbols.json`（构建脚本的 CWD 是包根，现在深了一层），改成从 `CARGO_MANIFEST_DIR` 定位；`cargo:rerun-if-changed` 同步。
- `crates/core/src/slots.rs` 里那条"从 vendored 源码扫出 `MathKind` 变体名"的测试用 `include_str!` + `CARGO_MANIFEST_DIR`，路径要写成 `../../vendor/...`。**它没被漏掉，正是因为它用绝对路径拼接**——如果当初写的是相对路径，这里会静默指向不存在的文件。
- 测试里跨两个 crate 的引用要分开（`tests/desktop.rs`、`tests/document.rs`、`tests/macro_scope.rs`、`tests/services.rs`、`tests/workspace.rs`）：内核项来自 `visual_typst_core`，外围项来自 `visual_typst`。其余 8 个测试文件只引用内核，一个字没改。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **121 通过 / 5 忽略**（与分层前同数；两个 lib 的单元测试都在跑） |
| `python -m unittest desktop.test_desktop` | **72 通过** |
| 线上形状 | `python tools/kind_inventory.py` 复跑，18 个用例的视图与 `docs/kind-inventory.md` 一致（`parameter`/`template-call` 仍为 0 次） |
| 内核→外围 | 变异检查：加 `use visual_typst::…` 编译失败，还原后通过 |
| 交付路径 | `target/server/release/visual-typst.exe` 路径与二进制名未变，`build-desktop.cmd` 与 `desktop/bridge.py` 无需改 |

**桌面套件需要更宽的文件权限**：在 `workspace-write` 沙箱下 72 个用例里 66 个报 `QProcess: CreateFile failed. (拒绝访问。)`——是沙箱拦了 QProcess 的命名管道，核心进程根本没起来（`window.py:169` 的 `set_source` 因此失败）。这不是代码问题，用 `danger-full-access` 跑同一条命令即 72 全过。

### 已确认、下一步要做的一件事

**`Number` 只允许至多一个点号**，两条独立规则各自保证：词法上从数字开头只试探性吃一个点、且点后必须还有数字（`lexer.rs:789-807`），解算上 `resolve_text` 要求 `decimal_count <= 1` 且至少一个数字（`resolve.rs:302-308`）。两处不一致的地方也查清了：`MathTextKind::get` 用的是 `is_numeric()`（含 `²`、阿拉伯数字等），而 `resolve_text` 用的是 `is_ascii_digit()`——所以要 1:1 对齐 `MathKind::Number`，解析端必须照**解算**那条，否则 `²3` 会被我们当成 Number、被引擎当成 Text。

据此刻定的行为：普通模式 `.` 走普通字符（`123<光标>456` 输入 `.` → `123 . 456`），数字并入相邻 `Number` 是无条件的（`123.456<光标>` 输入 `7` → `123.4567`），命令模式走解析器、解析出什么就是什么（`<\>123.456<回车>` → `123.456`）。

改动文件：新增 `crates/core/`（`Cargo.toml`、`build.rs`、`src/lib.rs`，以及搬入的 `math.rs`/`slots.rs`/`typst.rs`/`cursor.rs`/`view.rs`）、`tools/kind_inventory.py`、`tools/engine_boxes.py`、`docs/kind-inventory.md`；改 `Cargo.toml`、`Cargo.lock`、`src/lib.rs`、`src/main.rs`、`src/document.rs`、`src/desktop.rs`、`src/services.rs`、`tests/{desktop,document,macro_scope,services,workspace}.rs`、`docs/{architecture,kind-inventory,rust-book-walkthrough}.md`。

## `Kind::Number`：数字串成为一个原子 · 2026-09-11

### 先确认的事实

**一个 `Number` 至多一个点号**，两条独立规则各自保证：

- 词法（`lexer.rs:789-807`）从数字开头只**试探性**吃一个点，且点后必须还有数字才留下，所以 `1.2.3` 词法上是三个节点；
- 解算（`resolve.rs:302-308`）要求 `decimal_count <= 1`、至少一个数字、且每个字符都是 ASCII 数字或点。

两处对"数字"的定义**并不一致**，这一点决定了实现：`MathTextKind::get`（`ast.rs:917-926`）用 `char::is_numeric()`，`resolve_text` 用 `is_ascii_digit()`。反例 `²3`：词法收成一个 token 且 `MathTextKind` 说是 Number，引擎却把它做成 `Text`。所以 `Kind::Number` 的判定照**解算**那条写（`math::is_number`），才能与 `MathKind::Number` 保持 1:1。

### 改了什么

| 位置 | 改动 |
| --- | --- |
| `math.rs` | 新增 `Kind::Number { text }`、`MathAtom::number`、`is_number`（引擎规则） |
| `slots.rs` | 表里加一条声明：叶子、`view: "number"`、`typst: ["Number"]`、`Write::OwnText`、class 0；`Char` 不再认领 `Number`（现在它只认领 `Glyph`） |
| `typst.rs` | 解析：`MathText` 节点先试 `is_number`，命中则整串一个原子；写回：`write_cell` 的 `previous_digit`/`previous_dot` 状态机**整块删除**，只剩"每两个原子之间一个分隔符" |
| `cursor.rs` | 新增 `insert_digit`：数字并入**左边**的 `Number`，没有则新建一个单数字的 `Number` |
| `view.rs` | `Number` 节点把整串放在 `text` |
| `desktop/mathview.py` | `ARRANGEMENTS` 加 `number`（通用叶子分支已经会把它画成一个文本 run，无需新分支） |

按定好的行为：普通模式 `.` 走普通字符，所以 `123<光标>456` 输入 `.` 得到源码 `123 . 456`；数字并入左边的串是无条件的，所以 `123.456<光标>` 输入 `7` 得到 `123.4567`；命令模式走解析器，`\123.456` 回车就是一个 `Number`。

### 一行偏离原话的实现（已按你的方案改成容器）

第一版把数字串做成了**叶子**（一个原子带一段文本），于是"并入右边"做不到：原子内部没有光标位置，把 `9` 并到右边 `456` 的前面得到 `Number(9456)`，光标却停在原子之前，下一个 `8` 会继续往前插，`9` `8` 敲出 `89456`。当时只能"只并左边 + 新建原子"，并把偏离写进报告。

**你给出的方案是正解**：数字用和 `Text` 一样的光标处理（一个格，里面逐字是 `Char`），但不继承 text 困住左右键的那部分。改成容器后：

- `12|34` 插入 `9` 得到 `12934`——叶子模型下**根本做不到**（光标进不去串内部）；
- `|456` 输入 `9` `8` 得到 `98456`，光标停在 `9` 与 `4` 之间，"并入右边"自然成立；
- 左右键在串的两端**走出去**（text 在两端是 `return`，见 `cursor.rs` 的按键分支）。

### 字符规则与按键规则分别对齐

| | 字符（`interpret_char`） | 按键（`key`） |
| --- | --- | --- |
| `Text` | 一律插成 `Char`，不走 `/`、`^`、`_`、`\` | 两端**吞掉**左右键，`ArrowUp/Down`/`Tab` 也吞 |
| `Number` | 数字插成 `Char`；**其它字符先在光标处把串断成两半**，再按普通字符在该处处理 | **不吞**，两端走出去（`pop`），`Tab` 也照常离开 |

"非数字断串"是为了保住不变式：串里只能有数字，否则写回时不加分隔符的 `12x` 会被词法读成一个标识符。断串同时保住了光标位置，并且让 `/`、`^`、`"` 在断点处保持原意（`12|34` 打 `/` 得到 `frac(12, "") 34`，和任意两个原子之间打 `/` 完全一致）。

两条删除路径也顺手对齐了：删掉最后一个数字时，空串会被**移除**而不是写成空原子（空串写出来是一个孤立的分隔符，读回就是另一棵树）；而在串**开头**按退格走的是 LyX 的 pullArg 规则（容器解散、内容留下），数字因此变成散字符——渲染完全相同（`1 2 3` 实测与 `123` 同宽），下次解析又成串。

### 验证（容器模型，真实后端 + 源码层）

| 操作 | 源码 |
| --- | --- |
| `123<光标>456` 输入 `.` | `$123 . 456$` |
| `123<光标>456` 输入 `9` | `$1239 456$` |
| `<光标>456` 输入 `9` `8` | `$98456$` |
| `12<光标>34`（串内）输入 `9` | `$12934$` |
| `12<光标>34`（串内）输入 `x` | `$12 x 34$` |
| `123.456<光标>` 输入 `7` | `$123.4567$` |
| `\123.456` 回车 | `$123.456 x$` |

### 有意保留的不对称

读进来的 `12.5` 是一个 `Number`（格内 `1`、`2`、`.`、`5`）；逐键敲出的 `12.5` 是 `Number(12) Char(.) Number(5)`，写出 `12 . 5`。四种写法两两实测渲染完全相同：

| 写法对 | 实测（24pt，真实适配器） |
| --- | --- |
| `1.5` / `1 . 5` | 30.672 × 16.512 |
| `.5` / `. 5` | 18.672 × 16.512 |
| `12` / `1 2` | 24.0 × 15.984 |
| `98456` / `98 456` | 60.0 × 16.776 |

旧的"数字是连写例外"这条规则本来只影响 token 身份、不影响渲染，现在连同它一起消失：`write_cell` 没有例外分支了，`docs/architecture.md` 里那段说明也重写了。`tests/structured_input.rs::a_separator_keeps_typed_characters_from_becoming_one_shorthand` 按新行为更新（`("12.5","12 . 5")`、`(".5",". 5")`、`("1..2","1 . . 2")`），并且多断言一条更强的不变式：**写出的串必须读回成写出它的那棵树**。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **125 通过 / 5 忽略** |
| `python -m unittest desktop.test_desktop` | **73 通过** |
| `python tools/kind_inventory.py` | `Number`：`number` → `cell@inner` → 逐字 `char` |
| `python tools/engine_boxes.py` | 上表四对写法 |

### 容器化连带改到的三条导航用例（都是预期的）

数字串多了一层，光标要多走一步，三条钉住导航的用例按实际更新并写了原因：`caret_navigation.rs` 的根式与矩阵两条（`root(3, x)` 的度数、`mat(1, 2; 3, 4)` 的格内容都会先进串再出来）、`lyx_traces.rs::root_cell_zero_is_nucleus_and_index_is_one`（从根指数回到被开方式要多一次右移）。这不是回归，是容器模型的可观察结果。

### 新用例的牙齿（五次变异）

| 变异 | 结果 |
| --- | --- |
| 去掉 `is_number` 的 `dots <= 1` | 失败：`1.2.3 不应当是 Number` |
| `is_ascii_digit` 改成 `is_numeric` | 失败：`²3 不应当是 Number` |
| 关掉 `insert_digit` 的并入分支 | 失败：`a_typed_digit_joins_the_number_run_beside_it` |
| 前端 `ARRANGEMENTS` 去掉 `number` | 失败：报出未认领排布 |
| `dissolve_empty_run` 去掉 | 失败：`the_caret_goes_inside_a_number_run`（删掉最后一个数字后写出 `""`） |

**第一次变异检查是"假通过"的**：去掉 `dots <= 1` 之后全套仍然绿。原因是往返自洽——写回把串原样写出、解析用同一条（错的）规则读回。真正的判据是**与引擎一致**，而引擎规则里"至多一个点"那一半从解析路径**根本到不了**（词法不会给出含两个点的 token），所以它只能被**直接**钉住：`math.rs` 新增单元测试 `only_ascii_digits_with_at_most_one_dot_are_a_number`，把规则的两半都写死。这件事值得记下来：**当一个规则来自外部（引擎）而不是自己的约定时，往返测试看不见它。**

改动文件：`crates/core/src/{math,slots,typst,cursor,view}.rs`、`tests/{structured_input,round_trip,caret_navigation,lyx_traces}.rs`、`desktop/{mathview,test_desktop}.py`、`tools/kind_inventory.py`、`docs/{architecture,kind-inventory,validation}.md`。

## `Decoration` 拆成 `Accent`/`Line`，`Char` 载荷改成一个字形簇 · 2026-09-11

### 一、`Decoration` → `Accent` + `Line`

引擎侧本来就是两个 item、字段也不同：`AccentItem { base, accent: MathItem, position, dotless, exact_frame_width }` 与 `LineItem { base, position }`。一个 `name: String` 同时装这两样东西，既说不出"位置是引擎决定的"，又逼前端从名字反推上下。

| | 旧 | 新 |
| --- | --- | --- |
| 存储 | `Decoration { name }` | `Accent { name }`（`hat`/`vec`）与 `Line { above: bool }`（`overline`/`underline`） |
| `typst` | 认领 `["Accent","Line"]` | 各认领一个 |
| 视图名 | `decoration`（两者共用） | `Accent` → `decoration`（`text`=名字）；`Line` → **`line`**（`text`=`above`/`below`） |
| 回写 | `"{name}({0})"` | `Accent` 照旧；`Line` 走新的 `Write::Positioned { above, below }`，由**存的位置**选模板 |

`Line` 只存位置，因为 `LineItem` 只有位置——写回用哪个命令是从位置推出来的，不是另存一个名字。前端新增一个 `line` 排布（按 `text` 画基线上方或下方的横线），`decoration` 只留记号；顺带删掉了 `decoration` 里那串**永远不会到达**的名字分支（`underbrace`/`underbracket`/`underparen` 等：引擎把它们解成**带拉伸记号的 `Accent`**，不是 `Line`，而且它们本来就不在编辑器的命令表里，会走 `Raw`）。`vec` 仍是 `Accent`（也就是仍然画箭头）——它是引擎侧的**缺陷**（`vec` 是列向量 `Fenced(Table)`），修它要改行为，等你定，见 `docs/kind-inventory.md` 第六节第 1 条。

### 二、`Char` 的载荷：一个 `char` → 一个字形簇

原先的解析按 **Unicode 标量**拆：`e`+U+0301、`👍🏽`、ZWJ 家庭 emoji 各被拆成 2/2/5 个 `Char`，而词法本来把它们各收成**一个**节点、`GlyphItem.text` 也装**一个**字形簇。后果是**回写义务**上的缺陷——分隔符被插进字形簇中间：

| 字形簇 | 修前（敲一个键之后的码位） | 修后 |
| --- | --- | --- |
| `e`+U+0301 | `65 20 7a 20 301` → `é` 变成 `e` + 空格 + 飘在 `z` 后面的重音符 | `65 301 20 7a` |
| `👍🏽` | `1f44d 20 7a 20 1f3fd` → 肤色修饰符被拆开 | `1f44d 1f3fd 20 7a` |
| ZWJ 家庭 | 被空格切成五个独立 emoji | 整个簇加空格加 `z` |
| NFC 的 `é`（单标量） | 正常 | 正常 |

改法：`Kind::Char { value: char }` → `Kind::Char { text: String }`（一个字形簇），解析改用 `graphemes(true)`（内核新增直接依赖 `unicode-segmentation`，它本来就在依赖树里，构建里没有新代码）；构造函数带 `debug_assert_eq!(count, 1)`，与 `GlyphItem::create` 的 `assert` 对应。`char_class` 取簇的**首个**标量，也与 `GlyphItem` 读 `default_math_class` 的方式一致。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **126 通过 / 5 忽略** |
| `python -m unittest desktop.test_desktop` | **73 通过** |
| `python tools/kind_inventory.py` | `Accent` → `decoration`；`Line` → `line`（`cell@inner`）；`Char` 不变 |
| 字形簇 | 上表四行，真实后端 + 源码码位对照 |

### 新用例的牙齿（三次变异）

| 变异 | 结果 |
| --- | --- |
| 解析改回按标量拆（`chars()`） | 失败：`one_character_is_one_grapheme_cluster`（`left: 2, right: 1` 个节点） |
| 前端 `ARRANGEMENTS` 去掉 `line` | 失败：`test_a_line_draws_its_rule_on_the_side_it_stores`（报出未认领排布） |
| 前端 `line` 分支按名字而非按位置画 | 由同一条用例的几何断言覆盖：上/下两条的 `height` 与 `baseline` 走向必须不同 |

第二条检查**第一次是假失败**：新用例先写成 `$\overline{x}$`，而 Typst 的数学命令**不带反斜杠**（`\o` 是转义），于是文档里解析成 `\o` + `verline` + `{x}`，`line` 节点根本不存在，断言在"找不到节点"上失败——看着像回归，其实是用例写错了。改用 `$overline(x)$` 后通过，并把这条写进用例注释。

`Accent`/`Line` 的往返由 `tests/round_trip.rs` 的 `accent_atoms_round_trip` 与新增的 `line_atoms_round_trip` 守着——`Line` 的判据是"两个命令必须回到它们拼出来的那个位置"，而不是某个别处的标志位。

改动文件：`crates/core/{Cargo.toml,src/{math,slots,typst,cursor,view}.rs}`、`Cargo.lock`、`tests/{structured_input,round_trip}.rs`、`desktop/{mathview,test_desktop}.py`、`tools/kind_inventory.py`、`docs/{architecture,kind-inventory,validation}.md`。

## `vec` 不再画箭头 + 前端名字分支排查 · 2026-09-11

### 箭头是哪来的：Python，不是 Rust

`desktop/mathview.py` 的 `decoration` 分支里有一行 `if name in ("arrow","vec")` 给记号补了两笔箭羽。**Rust 侧从来没有 `arrow` 这个 Kind 或命令**：`crates/core/src/cursor.rs` 与 `typst.rs` 只认 `hat`/`vec` 两个 `Accent`，`math.rs` 的 `COMMANDS` 里也没有 `arrow`。追到引入点：`d15f35d 桌面端` 这个提交（本仓库第二个提交）里就已经有这一行，当时后端还是原型，`decoration` 是"按名字画"的形态。所以它是一次**凭名字猜的绘图**，不是从 LyX 或 Typst 搬来的。

实测确认这个猜法错得干净：`arrow(x)` 是引擎的 `Accent`（13.728×17.328pt，宽度不变），`vec(x)` 是列向量 `Fenced(Table)`（29.5584×23.904pt，宽度 2.15 倍），两者根本不是一回事。改法：`vec` 走通用分支（基线上方画一条横线），箭头分支整段删除。

### 同类问题排查（"前端有分支、后端到不了"）

把所有前端名字分支逐个对照后端能产生的东西：

| 位置 | 结论 |
| --- | --- |
| `decoration` 的 `widehat`/`dot`/`ddot`/`dddot` | **到不了**：后端命令表只有 `hat`/`vec`（`cursor.rs`），这四个会落成 `Raw`，由引擎自己画。已删 |
| `decoration` 的 `arrow` | 同上，已删 |
| `decoration` 的 `underline`/`underbrace`/`underbracket`/`underparen` | **到不了**：`overline`/`underline` 现在是独立的 `Line`（`view: "line"`），另三个会被引擎解成带拉伸记号的 `Accent`，而它们又不在命令表里 → `Raw`。上一轮拆分时已删 |
| `line` 的 `above`/`below` | 到得了：正是 `Line { above }` 的两个值 |
| `script` 的 `_placement`（`limits`/`scripts`） | 到得了：由适配器按 Typst 的 `Limits` 算出来，实测 `sum_1^2` 显示模式是 `limits`、行内是 `scripts` |
| `unknown` 的 `_string_mode` | 到得了：由 `MathCanvas.refresh` 自己盖上，给命令草稿选底色 |
| 排布名白名单 `ARRANGEMENTS` | 与后端 `Decl::view` 逐一对照：后端现在声明 18 个排布名，白名单里多出的 `draft-*`/`absent`/`stop`/`cell` 等是前端自造或后端合成的节点，不是 `Decl` 里的 |

结论：**除已修的两处，没有其他"到不了的分支"**；`mathview.py` 里那串名字表已经和后端对齐。

### 光标所在框的四角高亮

新增 `mark_active_path(view, cursor.slices)`：按光标的 `slices` 链在视图里走一遍（一个 slice 是 `{atom, cell}`，视图的一个 cell 里 stop 与原子交替排列，所以"`stop` 的 `cursor.pos` 等于 `atom`"就是那个原子的位置），给走到的节点盖上 `_active`；`layout` 对带 `_active` 的框在四角各画两笔短括号（不是整框，否则嵌套公式会看着像表格）。

实测 `frac(a, b)`、光标在分子时：**2 个框被标记**——根格与分子格。分式的节点本身也在路径上（`fraction._active` 为真），但它的框就是根格那个框的绘制结果，所以画面上是 2 个。分母**没有**标记，这是这条用例的对照项。下键移到分母后标记跟着走。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **127 通过 / 5 忽略** |
| `python -m unittest desktop.test_desktop` | **75 通过**（73 + 角标 1 + 上一轮 `line` 1） |
| `python tools/kind_inventory.py` | `vec` 仍是 `decoration`（`vec` 节点），画法变了、形状没变 |

### 新用例的牙齿（两次变异）

| 变异 | 结果 |
| --- | --- |
| `layout` 里去掉角标插入 | 失败：`0 != 2` 个框 |
| `mark_active_path` 取错兄弟节点（`index+2`） | 失败：`1 != 2` 个框 |

写这条用例时也踩了两次自己的坑，都记在用例注释里：第一次用 `core.call("click")` 直接点，核心答"请先进入公式"（点击不是进入公式的方式，要先 `window.activate`）；第二次断言"只有一个框"是我对标记语义的预期错——**光标路径上的每个框都算数**，正确的是两个。

改动文件：`desktop/{mathview,test_desktop}.py`、`docs/{validation,kind-inventory}.md`。

## 三张命令表收敛，命令名进配置文件 · 2026-09-11

### 问题

同一个命令名原先写在**三个地方**：`math.rs` 的 `COMMANDS`（"这是不是命令"）、`cursor.rs` 的 `factory`（"打字时建什么"）、`typst.rs` 的 `(name, args.len())` match（"读源码时建什么"）。三份能漂开，漂开的后果是**同一个名字打字进去和从文档读出来得到两种树**；实测 `vec` 就是这样：三处都按名字猜它是 accent，而引擎说它是列向量。

### 收敛结果

| 原来 | 现在 |
| --- | --- |
| `math.rs::COMMANDS` | 删除 |
| `cursor.rs::factory`（10 个分支） | 删除，命令交给 `parse_command_invocation` |
| `typst.rs` 的 `(name, args.len())` match | 删除，改问 `slots::command_kind(name)` |
| `Decl::commands`（内联常量 `C_*`） | 移到 `config/commands.json`，`build.rs` 生成 `COMMANDS` 表 |

```json
{ "frac": "fraction", "sqrt": "sqrt", "root": "root", "mat": "grid",
  "abs": "delim", "norm": "delim",
  "overline": "line", "underline": "line",
  "hat": "decoration", "vec": "decoration" }
```

值是 **`Kind` 的视图名**而不是变体名：视图名是给前端的契约，比 `Kind` 稳定（`Frac` → `Fraction` 那次改名不需要动这个文件）。`build.rs` 同时生成符号表，两张表进同一个 `OUT_DIR/config.rs`。

### 参数个数不在这份配置里

这点是讨论出来的关键：**配置只说"编辑器有没有这个名字的结构"，不说它要几个参数**。参数个数从**拼写**里数出来（`fill_command_cells`）：`frac({0}, {1})` 两格、`overline({0})` 一格、矩阵是行列默认值、定界符是中间那个体。所以没有第二份 arity 数据——这也正是"由 parser 给出参数列表"做不到的地方：实测 `typst-syntax` 看 `frac()` 和 `foo()` 完全一样（都是 `MathCall` + 空 args），arity 是 `typst-eval` 求值期才知道的，而内核刻意不依赖求值。

### 实测：改一行配置就多一条命令

给配置加 `"cancel": "line"` 并重建，**不动任何 Rust**：

| | 加之前 | 加之后 |
| --- | --- | --- |
| `\cancel(x)` 回车 | `Raw{cancel(x)}` | `line` 节点（写成 `underline(x)`——因为 `cancel` 的记号引擎侧是 `CancelItem`，配成 `line` 只是演示配置生效） |

还原后回到 `Raw`。这条演示说明配置真的在驱动行为，而不是文档。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **129 通过 / 5 忽略**（127 + 配置表 2 条） |
| `python -m unittest desktop.test_desktop` | **75 通过** |
| 十条命令逐条比对 | 键入与从源码读得到的 `Kind` 完全一致（上一轮的表继续成立） |

### 新用例的牙齿（两次变异）

| 变异 | 结果 |
| --- | --- |
| `"sqrt": "radical"`（不存在的视图名） | 失败：`config/commands.json 里的 sqrt 指向未知视图名 radical` |
| `"sqrt": "grid"`（视图名存在但是另一个 Kind） | 失败：`sqrt 是命令能建的 Kind，却没有命令名` |

第二条是关键：只检查"视图名存在"是不够的，配置完全可以把 `sqrt` 指到 `grid` 而两边都合法——反向检查（每个命令能建的 `Kind` 都必须有名字）才抓得住。

### 仍未修的一处

`\frac()` 回车会把 `frac({0}, {1})`（模板原文）写进文档：草稿 `frac()` 不等于 `frac`，`bare_name` 为假，于是走了没有方程包装的那条 `parse_command`。这与 `\overline` 那次同类，是**数据损坏**，等你定方案后再修。

改动文件：`config/commands.json`（新增）、`crates/core/{build.rs,src/{math,slots,cursor,typst}.rs}`、`docs/{architecture,validation}.md`。

## `frac()` 的模板占位符泄漏 · 2026-09-11

### 现象与更正

上一节我写的是"`\frac()` 把 `frac({0}, {1})` 写进文档"。**这条描述对 release 不准确**：`write_atom` 在模板占位符找不到格子时执行的是 `debug_assert!(false, …)`，debug 构建 panic，release 构建**跳过断言继续执行**，写进文档的是**字面的 `{0}`**。我是在 debug 测试里看到 panic 就下了结论，没有区分构建类型——危害性质（数据损坏）判断对了，描述错了。

### 根因

`fill_command_cells`（补格子）只在 `bare_name` 为真时执行，而 `bare_name` 的判据是"草稿去掉空白后恰好等于命令名"：

| 草稿 | `bare_name` | 补格子 | 结果 |
| --- | --- | --- | --- |
| `frac` | 真 | 执行 | `frac("", "")` |
| `frac()` | **假** | **不执行** | 解析出 `Fraction` 但 **0 格** → 写出时泄漏 `{0}` |
| `frac(1,2)` | 假 | 不执行（也不需要） | `frac(1, 2)` |

问题在于**"要不要补格子"和"要不要补调用语法"被同一个布尔量决定了**，而它们是两件事：前者取决于节点缺不缺格子，后者取决于作者写没写参数。

`frac()` 这个草稿本身**不是系统产生的**：实测补全表给的是不带括号的裸名（`\fr` 的候选是 `['frac']`），`Complete` 只把草稿变成 `frac`。它是**读者手打的退化输入**，但仍然必须结果是合法的——因为它会写进文档。

### 改法

把补格子从布尔量里拿出来，对**每一份**解析成功的草稿都执行：

```rust
for atom in &mut data { fill_command_cells(atom); }
```

`fill_command_cells` 只对**没有格子**的节点动手（`if !atom.cells.is_empty() { return; }`），所以 `frac(1,2)` 不受影响；而 `frac`、`frac()` 两种写法因此得到同一棵树。

### 实测（真实 release 二进制）

| 草稿 | 改前 | 改后 |
| --- | --- | --- |
| `\frac` | `frac("", "")` | `frac("", "")` |
| `\frac()` | **`frac({0}, {1})`** | `frac("", "")` |
| `\frac(1,2)` | `frac(1, 2)` | `frac(1, 2)` |
| `\hat()` `\overline()` `\abs()` `\mat()` `\norm()` `\underline()` `\sqrt()` `\vec()` | 同类泄漏 | 与各自的裸名写法一致 |

13 种输入逐个验过，写出结果里**没有 `{`**。

### 新用例的牙齿

`tests/command_mode.rs::a_command_written_with_empty_parentheses_is_writable` 对**每一条命令**比对"裸名写法"和"空括号写法"：两者的写出必须相同、都不含 `{`、且能读回同一棵树。

变异检查：把补格子改回"只在 `bare_name` 时执行"，用例立即失败：

```
thread '…is_writable' panicked at crates\core\src\typst.rs:730:27:
模板占位符 {0} 没有对应的格子
```

还原后全绿。这条用例补上了此前缺失的一类输入——`tests/command_mode.rs` 原本覆盖了 `\frac(a, b)` 和裸 `\frac`，唯独没有退化括号，而新增一条命令时这类输入是自动被覆盖的。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **130 通过 / 5 忽略** |
| `python -m unittest desktop.test_desktop` | **75 通过** |

改动文件：`crates/core/src/cursor.rs`、`crates/core/src/slots.rs`（`command_views` 补 `#[cfg(test)]`）、`tests/command_mode.rs`、`docs/validation.md`。

## 命令模式敲出 `bold(x)` 后公式框不画变体 · 2026-09-11

### 现象

读者的原话："进入命令模式后，输入 `bold(x)` 再回车，此时是不渲染的。"

复现（离屏窗口 + 假服务，**答案故意延后**）：`activate` → `input('\')` → `input('bold(x)')` → `key('Enter')`，公式框里画出来的文字运行是 **`['', '𝑥']`**——变体那一格是**空串**，其余正常。也就是说不是"整条公式没渲染"，而是 `style` 节点画了一个空运行（`Box` 宽度退到下限 2px，等于看不见）。

### 根因：答案晚到，而盖章发生在建视图那一刻

字形不是前端算的，是 `/api/glyphs` 问回来的（见 [architecture.md](architecture.md) 的 `Style` 一节），**必然晚于**布局：

1. 回车提交草稿 → 源码变化 → `update_analysis` 推出新视图 → `load_glyphs` 发出请求（缓存里先记 `None` = "问了还没答"）；
2. `math_action` 随即 `stamp_view(state['view'])` —— 此刻缓存里只有 `None`，于是**什么都没盖上**；
3. 答案到达时，回调只对 `self.analysis` 的公式补盖并重画**页面**。公式框画的是 `activate_formula` 返回的**另一个视图对象**（`state['view']`），补盖够不着它。

所以"进入公式框就不显示"和"刚敲完命令不显示"是同一件事的两面：`_glyph` 是**建视图那一刻**抄进节点的一份**值**，而值是晚到的。我上一轮的修法（在 `activate`/`math_action` 里补一次盖章）只覆盖了"答案已经在缓存里"的情况，正好漏掉"刚产生的新拼写"，而这两条路径的差别只在**时序**上——手工调用的测试全走的是前者。

### 改法：不在建视图时盖章，在**布局那一刻**读缓存

把"盖章"从每个投影各自的义务变成两个入口各做一次的事：

| 位置 | 做什么 |
| --- | --- |
| `Window.stamp_draw(view, definitions, display)` | 从两个缓存读：变体的字形串、附件的 placement。**没有答案也写 `None`**，否则改了定义（键变了）旧字形会留着 |
| `FormulaObject.box(formula)` | 页面的唯一入口：`stamp_formula(formula)` → `typesetter.layout(view)`，布局结果仍按 `typesetter.version` 记忆 |
| `MathCanvas.refresh(state)` | 公式框的唯一入口：`prepare_view(...)` → `layout` |
| 答案回调（`arrived`/`attached`） | 只做 `touch()`（作废记忆的盒子）+ `repaint_formulas()`，不再盖任何东西 |
| `prepare_view(view, …)` | = `stamp_contexts` + `stamp_draw`。`_context` 留在这一侧：它是**渲染轮记录 `False` 时用的键**（`raw_key`），记录方和查表方必须同时写，不能交给画的那一刻 |

顺带删掉三处上一轮的临时机制：`Window.stamp_view`、`Window.stamp_glyphs`、`Window.style_nodes`，以及 `load_raw` 里那一遍补盖和 `activate`/`math_action` 里的两次调用。

**画法也从四种收敛成两种**：有字形画字形，否则画这个调用（名字 + 主体 + 右括）。第二种不只是"光标在里面"那一种——答案在路上、或请求失败时，画调用正是源码说的东西，画空串则什么都不说。`cal(A)` 那类"取了字但字不对"的异常态因此也不需要单独分支。

### 实测

| 时刻 | `style` 节点画的运行 |
| --- | --- |
| 回车之后、答案到达之前 | `bold(` + 主体 + `)` |
| 答案到达并重画之后 | `𝐱`（U+1D499） |

### 新用例的牙齿（两条，各钉一个入口）

- `test_committing_a_variant_command_draws_it_once_the_answer_arrives`：命令模式整条路径，答案延后发放，断言公式框画的是 `𝐱`。删掉 `FormulaObject.box` 里的盖章 → 失败；删掉 `MathCanvas.refresh` 里的 `prepare_view` → 失败。
- `test_the_page_renders_a_variant_whose_glyphs_arrive_late`：只钉页面那一侧（答案晚到后**不重建视图**，只有缓存变了）。删掉 `FormulaObject.box` 里的盖章 → 失败（画成 `bold(` `)`）。这条是必要的，因为"只写公式框能过"的测试**盖不住**页面：公式框每次 `refresh` 都会重新盖章。

两条都用**延后发放**的假服务：此前的假服务是**同步**回话的，答案在 `stamp` 之前就已经在缓存里，于是整类时序缺陷在测试里根本不存在——这正是上一轮"手工调用全绿"的原因。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **136 通过 / 5 忽略** |
| `python -m unittest desktop.test_desktop` | **84 通过**（新增 2 条） |
| `python tools/kind_inventory.py` | 退出码 0，32 个用例、23 个线名两个方向都对上 |

改动文件：`desktop/window.py`、`desktop/mathview.py`、`desktop/test_desktop.py`、`docs/architecture.md`、`docs/desktop.md`、`docs/kind-inventory.md`、`docs/validation.md`。

## `upright` 画成了斜体：引擎给的码位又被映射了一遍 · 2026-09-11

### 现象

读者："现在 upright 不显示直体了。"

复现（离屏窗口，`$upright(A) + bold(A)$`）：画出来的三个运行是 **`['𝐴', '+', '𝐀']`**——`bold(A)` 是对的，`upright(A)` 是**斜体** `𝐴`。

### 根因：同一个映射做了两遍

编辑器画"数学变量"时把 ASCII 字母映射到 Unicode 数学斜体区（`a→𝑎`），因为那是 Typst 的默认。而**字体变体是引擎已经做过的替换**：`/api/glyphs` 给的 `upright(A)` 就是普通 `A`，`bold(A)` 就是 `𝐀`。于是 `mathfont.glyph` 又映射一次：

| 节点 | 引擎给的 | 编辑器又映射成 | 结果 |
| --- | --- | --- | --- |
| `bold(A)` | `𝐀`（不是 ASCII） | 原样 | 对 |
| `upright(A)` | `A`（ASCII） | `𝐴` | **直体被改回斜体** |

`bold` 一直是好的，只是因为 `𝐀` 不是 ASCII、恰好躲过那个映射——这就是缺陷能藏住的原因。

改法：`mathfont.glyph(..., substituted=True)` 这一个开关，`mathview` 只在画 `style` 节点的字形串时传它。引擎的替换结果原样画，别的路径（变量、文本格、符号）照旧映射。

### 顺带查清的一处"特殊处理"：`h`

`mathfont.glyph` 里那一行 `"ℎ" if text == "h"` 曾被删掉。它不是给字体打的补丁：**U+1D455（mathematical italic small h）在 Unicode 里未分配**（`unicodedata.category == 'Cn'`），没有任何字体能画出它；引擎把默认斜体 h 排成 **Planck 常数 `ℎ` U+210E**。用真适配器把 52 个字母逐个问过：

| 公式 | 引擎（`/api/glyphs`） |
| --- | --- |
| `h`（默认斜体） | U+210E `ℎ` |
| `italic(h)` | U+210E |
| `upright(h)` | U+0068（直体 h） |
| `bold(h)` | U+1D489 |
| `bold(upright(h))` | U+1D421 |

52 个字母里**只有 `h` 一处**与"朴素映射"不同。删掉那一行的后果是编辑器向 Qt 要 U+1D455 → 字体没有 → Qt 逐字回退到别的家族（正是本项目一直在防的"一个公式里混进多种设计"），而且对**任何公式里的默认斜体 `h`** 都会发生。已加回。

### 新用例的牙齿（两边各一条）

- `native-adapter`：`the_italic_default_has_one_hole_and_it_is_h` —— 断言 `h`/`italic(h)`/`upright(h)`/`bold(upright(h))` 四种写法的字形串，并断言相邻的 `g`/`i` 是普通码位（说明这是 Unicode 的洞，不是映射规则）。
- `desktop`：`test_the_math_font_covers_every_glyph_the_core_can_draw` 改成**从 `mathfont.glyph()` 取那 52 个字母**再查字体覆盖。此前它手写字母表，所以删掉 `h` 的特例它**照样绿**——测的是我写的那条路径，不是用户走的那条。现在删掉特例会红，报 `NewComputerModern Math cannot draw ['\U0001D455']`。
- `desktop`：`test_upright_draws_the_plain_letter_the_engine_asked_for` —— 断言同一份公式里 `A`（直体）与 `𝐀`（加粗）同时在，且**不能**出现 `𝐴`。把 `substituted=True` 去掉即失败。

### 一次误判，记下来

我先把 `mathfont.py` 里那两行的缺失当成了"工具/编辑器意外删除"，其实**它在本人编辑这个文件之前就已经不在**（字体用例在我动手之前就是红的），是读者有意删掉的；我看 `git diff` 时把三处改动混成一个 hunk，才读成"工具删了我的行"。教训是：**diff 的 hunk 不是因果**——判断"谁改的"要看**改动发生的时间点**（用例何时变红、文件何时被写），不是看改动挨不挨着。

### 验证（本轮收尾时实测）

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **136 通过 / 5 忽略** |
| `cargo test … --manifest-path native-adapter/Cargo.toml` | **21 通过**（新增 1 条） |
| `python -m unittest desktop.test_desktop` | **85 通过**（新增 1 条，另 1 条改成从映射取字母） |
| `python tools/kind_inventory.py` | 退出码 0 |

改动文件：`desktop/mathfont.py`、`desktop/mathview.py`、`desktop/test_desktop.py`、`native-adapter/src/main.rs`、`docs/validation.md`。

## 过期注释与文档的全面排查 · 2026-09-11

重构（Kind/View 重划、`Style`、`Number` 容器、命令表收敛）之后，代码里与文档里都留了一批"当时对、现在不对"的说法。这一轮把它们当**缺陷**清了一遍：注释错了会误导下一个改这段代码的人，而文档错了会让"判据"本身失真。

方法不是通读，而是**逐条验证**：凡出现"文件/函数/字段/测试名/行号/条目数"的说法，就去 grep 或 glob 一次，对不上才改。两个并行审计（文档一份、源码注释一份）各自带证据回报。

### 明确错误的（已改）

| 位置 | 原说法 | 实际 |
| --- | --- | --- |
| `crates/core/src/view.rs`（`Style` 分支） | 注释说 `edit` 是给 `annotate` 定位用的，且"字形未到就用**调用的图**" | `style` 从不取图（`annotate` 只看 `raw`/`raw_macro`），回退画的是**调用本身**；`edit` 只对前端有意义，因此没有 `source_range` |
| `crates/core/src/slots.rs`（`MACRO_COLLAPSED`） | "`raw_macro` 画成*源码*而非编译图" | **反了**：光标在外时画的正是整段调用的**图**，进去才画名字与槽位 |
| `crates/core/src/math.rs`（`Table`） | "`Fraction` 是形状描述符、不存储" | `Fraction` **就是**存的（`a/b` 语法）；不存的五个是 `Sqrt`/`Root`/`Accent`/`Line`/`Style` |
| `crates/core/src/cursor.rs` | "见 `slots::Decl::grow_empty`" | 全仓没有这个符号，机制是 `fill_command_cells` |
| `desktop/window.py`（`self.contexts`） | 记的是"每个公式的 `(start,end,context)` 排序表" | 实际是 `{render_id 前缀 → 脚本形状摘要}` |
| `desktop/window.py`（`invalidate_raw`） | "持有调用的片段**按公式**各存一份图" | 按**源码文本**共享（`raw_key`），跨公式只有一份 |
| `src/services.rs`（用户可见文案） | 取字超时"先按整段调用的**图**显示" | 现在画的是调用本身（名字与主体），不取图 |
| `README.md` | `cases(...)` 还是"保留为 Raw"的例子 | `cases`/`cancel`/`vec` 都已进 `commands.json` |
| `docs/architecture.md` | `SourceDock` 类；`lsp_position` 在 `model.py`；`commands.json（9 项）`；`bb(A)` 是 `Raw`；`Style{text, style_name}`；`lr(...)` 64 处 | 分别是 `Window.source_dock`（`QDockWidget`）、`Window.lsp_position`、14 项、未知名字的调用（`raw_macro`）、`Kind::Style{name}`（`style_name`/`text` 是线上字段）、实测 14 个文件 80 处 |
| `docs/architecture.md` | `cancel`/`vec` 与 `RR` 一样"只剩 `Raw` 一条退路" | 前两个在命令表里、是可编辑结构；只有 `RR` 是那一类 |
| `docs/kind-inventory.md` | `text`/`edit` 的"只有…"枚举漏 `style`；"30 个用例"/"26 个用例…24 个" | 已补 `style`；实测 `CASES` **32**、`ARRANGEMENTS` **25**（本轮真发出的线名 23） |
| `docs/desktop.md` | 未设置环境变量时用 `target/server/debug/...` | `bridge.py` 是 **release 优先**，debug 只是回退 |

行号引用（`typst.rs:541`、`view.rs:117`、`typst.rs:776` 等）整体漂移，已按当前代码重新核对。**没有**去修 `docs/rust-book-walkthrough.md`、`docs/rust-for-cpp.md` 里的行号：它们是教学材料，指代的是概念不是位置，逐行维护没有收益。

### 顺带补上的一条测试

`tests/stored_kinds.rs` 守着"哪些 `Kind` 真的会进树"，而 `tests/round_trip.rs` 守着"每个会进树的原子都能往返"——但 `Style` 之前**没有往返用例**，于是"可往返的 Kind 共 15 个"这句话本身就是错的（漏了一个）。新增 `style_atoms_round_trip`：`bold(x)`、`upright(A)`、`bold(upright(a))`、`bold(x + 1)` 断言借到 `style` 形状并往返，`upright(alphabets)`、`bold(frac(a, b))` 断言**落回普通调用**（主体不是一行字形，`has_glyph_run` 为假）。写这条时踩了一次：最初把 `bold(x + 1, 2)` 也放进"普通调用"那一组，它当场报出**多余实参被静默丢弃**（`bold(x + 1, 2)` 写成 `bold(x + 1)`）——那不是 `Style` 的问题，而是一个本文件不负责的既有缺陷，于是把它从用例里撤掉，没有顺手改行为。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **137 通过 / 5 忽略**（新增 1 条） |
| `cargo test … --manifest-path native-adapter/Cargo.toml` | **21 通过** |
| `python -m unittest desktop.test_desktop` | **85 通过** |
| `python tools/kind_inventory.py` | 退出码 0 |

另外记下一次环境坑：桌面套件在受限沙箱下会**整片报错**（`QProcess: CreateFile failed（拒绝访问）`，79/85 个用例 ERROR）。那不是回归，是 `QProcess` 管道被沙箱拒绝；放开 IPC 后全绿。**看起来像"改注释改崩了"的整片失败，先看错误类型再怀疑自己。**

### 两个审计各自的完整版里、第一轮漏掉的两条

- `docs/kind-inventory.md` 那句「`Fraction`/`Sqrt`/`Root`/`Accent`/`Line` 这五个只作为形状描述符」**自己跟自己打架**：`Fraction` 是存的（`a/b` 语法与 `frac(a, b)` 都存），下一句「只有 `a/b` 还真的存 `Kind::Fraction`」正是承认它。我第一轮只补了漏掉的 `Style`，没纠正 `Fraction` 的错位——已改。
- `config/README.md` 说值「是希望显示的 Unicode 字符（**也可使用字符串**）」：`build.rs` 按 `BTreeMap<String, String>` 解析，非字符串直接编译失败，括号里那半句是旧格式残留。已改，并补一句说明同一目录的 `commands.json` 管的是**结构**而不是显示（此前这篇只讲 `symbols.json`，容易让人以为 `commands.json` 也在这里描述）。

### 验证（本轮补完后重跑）

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **137 通过 / 5 忽略** |
| `cargo test … --manifest-path native-adapter/Cargo.toml` | **21 通过** |
| `python -m unittest desktop.test_desktop` | **85 通过** |
| `python tools/kind_inventory.py` | 退出码 0 |


改动文件：`crates/core/src/{math,slots,typst,view,cursor}.rs`、`desktop/window.py`、`src/services.rs`、`tests/round_trip.rs`、`README.md`、`AGENTS.md`、`config/README.md`、`docs/{architecture,kind-inventory,desktop,validation}.md`。

## 实时预览回来了：用 Tinymist 的预览，不做自己的渲染 · 2026-09-12

### 先澄清一件事：预览不是"被删掉的代码"

读者的描述是"这个前端之前把实时预览机制去掉了"。查证结果是：**git 里从来没有过那个 Dock**。`desktop/` 的**第一个提交**（`d15f35d 桌面端`）一进来就带着 `# Kept off-window only for explicit SVG export; there is no live preview pane.`，同时 `docs/desktop.md` / `docs/validation.md` 是**新增文件**、开头就写着"已移除实时预览 Dock"。所以那是**在提交之前**于工作区里摘掉的，无从恢复，只能重做。

残留物倒是留了一地：`Page` 类、`preview_container`/`preview_scroll`（建好但**从未挂进窗口**）、`compile()`（唯一 `/api/preview` 调用者，只剩导出 SVG 在用）、`fit_preview`（无调用者）、`reveal_preview`（无调用者，且第 914 行引用了一个**从未存在过**的 `preview_dock`——一个必然 AttributeError 的死代码）。这些正是这次清理的对象。

### 选型：为什么不复用 `/api/preview`

两条路都试算过，最后**没走**"把 `/api/preview` 挂到空闲计时器"这条，理由不是"不符合指示"，而是它**会打破文档里已经写着的承诺**：

| | Tinymist 预览（采用） | 复用 `/api/preview`（未采用） |
| --- | --- | --- |
| 实时性 | Tinymist 推送**增量**帧 | 自己 debounce |
| 与片段取图抢锁 | **不抢**（Tinymist 自己的进程） | **抢**：`/api/render`、`/api/preview`、`/api/pdf` 共用同一个 `render_adapter` 互斥量 |
| 新依赖 | `PyQtWebEngine` | 无 |
| 编译器份数 | 2（Tinymist + 适配器） | 1 |

`docs/architecture.md:76` 与 `docs/validation.md` 里都写着"整页编译不会把公式取图排在后面"（实测 1200 段文档整页预览 14.4 s，期间 `/api/status` 8 ms）。那句在**进程/路由**层面仍然成立（`rpc.rs` 每请求一线程），但 `render_adapter` 的锁会让每 550 ms 一次的整页编译把视口取图整段挡住。Tinymist 预览不碰那把锁，所以那句承诺**继续成立**——这是选它的硬理由。

### 实测：`doStartPreview` 到底返回什么

返回形状是从 Tinymist 的 VS Code 客户端代码里读来的，不是实测的，所以先量：

```
workspace/executeCommand "tinymist.doStartPreview"
  arguments: [["--task-id","probe","--data-plane-host","127.0.0.1:0", <绝对路径>]]
→ {"dataPlanePort":11451,"isPrimary":true,
   "staticServerAddr":"127.0.0.1:11451","staticServerPort":11451}
静态页: HTTP 200 · text/html · 2,046,807 字节 · 确认是预览应用
doKillPreview → {"result": null}，端口随即关闭
```

三点出乎预期：**①** 还有一个 VS Code 客户端不读的字段 `staticServerAddr`（自带 host，比只给端口更好用）；**②** 静态服务与 data plane **默认同端口**，所以一个 URL 就够（页面和它开的 WebSocket 同源）；**③** `doKillPreview` 是真的优雅关闭（日志 `Preview server joined` / `Data plane server shutdown` / `killed`），不是只摘任务。

**探针第一次挂死，是我自己的 bug**：`stderr=subprocess.PIPE` 却不排空，Tinymist 的日志写满管道后**阻塞**，于是 `initialize` 永远不回。这不是 Tinymist 的问题，也不是"服务器不回话"——是经典管道死锁。**对 Rust 宿主也是真实风险**：`src/services.rs` 给 LSP 设的是 `stderr(Stdio::null())`，恰好绕过了；任何改成管道的人都必须持续排空。

### 改法

| 层 | 做什么 |
| --- | --- |
| `src/services.rs` | 抽出 `DocumentLsp::ensure`——语言方法与预览**共用同一个 LSP 会话**（预览就托管在 Tinymist 自己进程里，没有第二个进程要起）；新增 `Services::preview` 转发 `doStartPreview` / `doKillPreview` |
| `src/rpc.rs` | 新路由 `/api/preview/live` |
| `desktop/preview.py`（新） | QtWebEngine 的 import 顺序约束、从回复取页面 URL、缺 wheel 时"不可用"而不是崩 |
| `desktop/__main__.py` | 在**建 `QApplication` 之前**调 `preview.prepare()`（import + `AA_ShareOpenGLContexts`，顺序反了是硬错误） |
| `desktop/window.py` | "实时预览" dock（默认隐藏）、视图菜单"显示 / 隐藏实时预览"、`preview_widget`/`set_preview`/`start_preview`/`stop_preview`；关窗停预览；删掉 `reveal_preview` 那段死代码 |
| `desktop/requirements.txt` | 加 `PyQtWebEngine>=5.15.4,<5.16`（**独立 wheel**，不是 PyQt5 的附带品） |

"只有开启时才渲染"落在实现上就是：**开启**才发 `doStartPreview`，**关闭**立刻 `doKillPreview` 并把页面清成 `about:blank`——开着的预览就是一个在跑的编译器，不能让它留在后台。

### 两个测试坑

1. **QtWebEngine 在 `QT_QPA_PLATFORM=offscreen` 下无法构造**：不是抛异常，是**访问违例**（退出码 `-1073741819`），整个测试进程当场死。所以桌面套件用替身 web view 跑启停逻辑，`Window.preview_widget` 就是为可替换而拆出来的。第一次跑套件时正是这个原因让"预览"两条用例失败、随后整片崩掉——**我起初以为是断言写错，实际是平台限制**。
2. 真窗口的行为测试**测不到**，于是另外跑了一次真平台探针（已删）：真实窗口 + 真 Tinymist + 真 web view，结果 `loadFinished ok=True`、`url=http://127.0.0.1:4151/`、`page title='untitled.typ'`、页面 HTML 2,047,014 字节且含 `typst`/`svg`/`canvas`，关闭后 `started=False`、`dock=False`、`url=about:blank`，**且 4151 端口确实关闭**（`TcpClient` 连接被拒）。也就是说"页面真的跑起来了"和"关掉真的停了"都有实测，不是靠断言推的。

### 新增用例的牙齿

- `tests/services.rs::tinymist_serves_the_live_preview_on_the_ports_it_reports`（对着**真** Tinymist，`--ignored` 类）：断言两个端口为真、`isPrimary`、静态与 data plane 同端口，**真去连一次**确认返回 200 且页面是预览应用，`kill` 之后**再连必须失败**。
- `test_desktop.py::test_the_live_preview_starts_only_when_it_is_switched_on`：开启才发 `start`、按回复里的端口加载、关闭发 `kill` 且把页面清空。
- `test_desktop.py::test_closing_the_window_stops_a_running_preview`：关窗必须停预览。
- 改掉三处钉"没有预览"的断言：`test_background_services_...`（现在断言后台周期**不碰** `/api/preview/live`）、`preview_revision == -1` 那句的措辞、以及原来 `assertFalse(hasattr(window,'preview_dock'))` 反过来断言 dock 存在且默认关闭。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **137 通过 / 5 忽略** |
| `cargo test --test services -- --ignored` | **5 通过**（含新增的预览契约） |
| `python -m unittest desktop.test_desktop` | **87 通过**（新增 2 条） |
| 真窗口探针 | 页面加载成功、关闭后端口确实关闭 |

改动文件：`src/{services,rpc}.rs`、`desktop/{preview,window,__main__,requirements.txt,test_desktop.py}`、`tests/services.rs`、`README.md`、`AGENTS.md`、`docs/{architecture,desktop,validation}.md`。

## 梳理宏展开机制时发现：单字母宏名不展开 · 2026-09-12

### 现象

对着真实 release 二进制逐条实测宏展开，发现**单字母名字的宏参数调用不展开**：

| 定义 + 调用 | 画出来的顶层节点 |
| --- | --- |
| `#let a(x) = $#x + 1$` · `$a(y)$` | `char "a"` + `decorated "(y)"`（**字面量**） |
| `#let ab(x) = $#x + 1$` · `$ab(y)$` | `macro "ab"`（展开成 `y + 1`） |

名字长度逐个试过：`a` `b` `x` `f` `q` `A` 全部**不展开**，`ab` `xy` `aa` `dbl` `abc` `a1` `AA` 全部展开。判据干净得只有一条：**标识符长度**。

### 根因不在本项目，是 Typst 数学词法器的规则

用 `typst-syntax` 直接打印语法树（`Source::detached`）：

```
$ a(y) $     → Math → [MathText "a", MathDelimited "(y)"]         ← 没有 MathCall
$ ab(y) $    → Math → [MathCall → MathIdent "ab", MathArgs "(y)"]
```

`MathTextKind::get` 只把**多字符**的数学文本识别成 `MathIdent`，单字母是 `MathText`。于是 `a(y)` 在引擎眼里就是"字母 a 后面跟一个括号组"，与 `(a)(y)` 同类，**根本没有可绑定的调用节点**。

同一轮还确认了注册表这一侧是**对的**：`macro_registry("#let a(x) = …")` 给出 `bound = true`、`expandable = Some(true)`——单字母定义的识别、分类、模板构建全部正常，问题纯粹在**更早一步拿不到 `MathCall`**。

### 处理：只记录，不改代码

已按读者指示**不动代码**。修它要动解析器去把 `MathText`+`MathDelimited` 也认成调用，而那会改变渲染语义：`a(y)` 的引擎真实排版就是"a 后跟括号组"，认成宏展开会让编辑器画的东西和引擎不一致（本项目一直按"引擎说什么就是什么"办）。单字母宏名在真实文档里也极少见。

### 顺带确认下来、值得记的几条机制

这一轮梳理同时把展开机制的关键规则逐条实测钉住了（都在真实二进制上跑）：

| 规则 | 实测 |
| --- | --- |
| 模板拼接 | `$dbl(y)$` 展开后 `macro` 节点里，**模板的 `+ 1` 与实参格 `y` 在同一条 cell 线上**——参数位置被实参替换，其余模板内容原样内联 |
| 实参个数必须相等 | `$dbl(y, z)$`（2 实参 vs 1 参数）**不展开**，画成 `raw_macro`（`raw` 文本 `dbl(y, z)`） |
| 嵌套模板递归绑定 | `outer` 的模板里调 `inner`，展开是**递归内联**的：`macro "outer"` → 里面直接是 `scripts`（`inner` 的 `#a^2`），`absent` 占位于没有的上下标格 |
| 不可展宏的公式整体锁死 | `#let q(x) = $lr(#x, size: #100%)$`（命名参数）→ 该定义体的公式 `editable=false`，理由是"位于不可展开的 let 定义中，按设计保留源码模式"；而同文档里 `$q(a)$` 仍可编辑 |

最后一条与 `src/desktop.rs::scan_syntax` / `analyze_formula` 里那段 `blocked` 判定对应：公式落在**不可展开**的 `#let` 里时按源码保留，`opaque()` 会沿语法树找出这个位置。

**这一轮没有代码改动**（探针已删，工作区只剩上一轮预览的改动）。

## 改名：Visual Typst → TypFormula · 2026-09-12

### 改了什么

| 类别 | 旧 | 新 | 处数 |
| --- | --- | --- | --- |
| 包名 | `visual-typst` / `-core` / `-layout` | `typformula` / `typformula-core` / `typformula-layout` | 3 个 `Cargo.toml` + 2 个 lock（用 `cargo update` 重生成，未手改） |
| Rust 模块路径 | `visual_typst::` / `visual_typst_core::` | `typformula::` / `typformula_core::` | 25 个文件 |
| 二进制 | `visual-typst.exe` / `visual-typst-layout.exe` | `typformula.exe` / `typformula-layout.exe` | `bridge.py`、`services.rs`、两个 `.cmd`、两个 `tools/*.py` |
| 环境变量 | `VISUAL_TYPST_{BIN,ADAPTER,CONFIG,RAW_CACHE,WORKSPACE,PYTHON}` | `TYPFORMULA_*` | 6 个变量、5 个文件 |
| 设置目录 | `%APPDATA%/VisualTypst/` | `%APPDATA%/TypFormula/` | `model.py`、`window.py`（临时 PDF 目录） |
| 界面标题 | `Visual Typst` | `TypFormula` | `window.py`、`__main__.py` |
| 文档 | `Visual Typst` | `TypFormula` | 7 份文档的正文 |

### 最需要小心的一处：vendor 里的协议串

`vendor/typst/` 里的三个字符串**不是名字，是适配器与引擎之间的协议**：

| 字符串 | 谁发 | 谁认 |
| --- | --- | --- |
| `visual-typst-raw-<i>` | `native-adapter/src/render.rs` | `typst-library/ir/resolve.rs`、`typst-realize/lib.rs` |
| `visual-typst-origin-v1` | `typst-eval/src/math.rs`（哈希键） | 同上 |
| `visual-typst-formula-<i>` | adapter | adapter 自己读回 |

**只改一边就等于协议断了**，而且不会编译失败——引擎只是再也认不出 Raw 片段，界面会静默退回源码。所以四处**同时**改成 `typformula-raw-` / `typformula-origin-v1`，并把 `native-adapter/engine-patches.json`（补丁对照表）里同一批字符串一起改，否则那份表就不再反映 `vendor/` 的实际内容。改完用**真适配器**实测映射仍然有效：

```
frac(a, b)  {"width": 15.216, "height": 25.3128, "baseline": 16.8648}
x           {"width": 13.728, "height": 10.872, "baseline": 10.608}
```

### 一个把我误导了一阵的坑：旧二进制留在 target 里

桌面套件第一次跑 **81 个 ERROR**，报"公式核心已退出（退出码 0）；请求 set_source 在重建公式核心后仍未成功"。我先后怀疑了两件事，都是错的：

1. **怀疑 `bridge.py` 的路径**——查过 `git show HEAD:desktop/bridge.py`，改名前的路径解析就该是那个样子，逐字对得上；
2. **怀疑 `target/adapter/debug/` 里的旧名二进制**——`debug` 构建确实会去找 `typformula-layout.exe` 而那里只有旧名，于是补了 debug 构建、删了旧名产物。**但重跑仍是 81 ERROR。**

真正的原因是**第三条**：`QProcess: CreateFile failed.（拒绝访问。）`——**沙箱拦了 QProcess 的命名管道**。这正是本文档**已经记过两次**的那件事（第 1162 行、第 1645 行），原文就写着"那不是回归，是 `QProcess` 管道被沙箱拒绝；放开 IPC 后全绿"。放开后 **87 个用例一次全过**。

教训和第 1643 行那句是同一个，但这次我自己踩了：**整片失败的报错，先看错误文本里的系统级线索（`CreateFile failed`），再怀疑自己的改动**。我花在"逐个排查旧名产物"上的时间，本可以用第一行 stderr 省掉。

### 保留旧名的地方（故意的）

`docs/validation.md` 正文里仍留着 `visual-typst`/`visual_typst`/`VISUAL_TYPST_*` 共 12 处，**这是历史记录，不改写**：它们分别是当时的 VSIX 产物名、一段真实的编译错误输出（`unresolved import visual_typst`）、以及描述当日代码结构的表述。已在本文件开头加了一段"改名说明"，声明这些一律是当日的名字并给出新旧对照。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **137 通过 / 0 失败** |
| `cargo test … --manifest-path native-adapter/Cargo.toml` | **21 通过** |
| `python -m unittest desktop.test_desktop` | **87 通过**（需放开沙箱 IPC） |
| `python tools/kind_inventory.py` | 退出码 0，两个方向都对齐 |
| 真适配器映射 | `frac(a, b)` 与 `x` 的宽高基线正常（证明改名后的协议串仍然有效） |

改动文件：`Cargo.toml`、`crates/core/Cargo.toml`、`native-adapter/Cargo.toml`、`Cargo.lock`、`native-adapter/Cargo.lock`、`src/{lib,main,document,desktop,services}.rs`、`native-adapter/src/render.rs`、`native-adapter/engine-patches.json`、`vendor/typst/crates/{typst-eval/src/math,typst-layout/src/lib,typst-layout/src/math/mod,typst-library/src/math/ir/resolve,typst-realize/src/lib}.rs`、`desktop/{__init__,__main__,bridge,model,rawcache,window,test_desktop}.py`、`tools/{engine_boxes,kind_inventory}.py`、`tests/*.rs`（13 个）、`build-desktop.cmd`、`start-desktop.cmd`、`AGENTS.md`、`README.md`、`docs/{architecture,desktop,kind-inventory,lyx-desktop-rendering-study,rust-book-walkthrough,validation}.md`。

## 拆 `Decl`：`Grammar`（语法）+ `Shape`（排布与编辑），并让 root 按书写顺序存 · 2026-09-12

### 为什么要拆

`Decl` 一个结构体装了**三家的活**，九个字段的消费者数出来是这样的：

| 字段 | 谁读 | 归属 |
| --- | --- | --- |
| `view` | `view_atom`（选排布） | 渲染 |
| `entry` / `horizontal` / `vertical` / `class` | `entry_cell` / `idx_horizontal` / `cursor::vertical` / `math_class` | **编辑** |
| `slots` / `arity` | `view_atom`（盖 role）+ 上面几条 | 语法↔渲染的**接口** |
| `write` | `write_atom`、`fill_command_cells` | **语法** |
| `typst` | 只有那条词表对账测试 | 对账标签 |

后果不是"代码长"，而是**同一件事存在两处、必须手工同步**。最清楚的一处是 root：

```rust
// 解析期（typst.rs）
if let slots::Write::Template(template) = shape.decl().write {
    if placeholder_indices(template).first() == Some(&1) && args.len() == 2 {
        args.swap(0, 1);        // ← "怎么写"决定了"树里存什么"
    }
}
```

存储顺序是 `[被开方式, 根指数]`（与 Typst 的 `root(index, radicand)` 相反），靠写模板 `root({1}, {0})` 反回来，解析期再 swap 配合。同一个事实写在两处，还牵动另外四处：入口角色、`Vertical::Swap{end_up}`、`Horiz::Pair` 的硬编码下标、`∛x` 的 `MathRoot` 构造。

### 拆成什么

| 表 | 取用键 | 声明 | 读它的地方 |
| --- | --- | --- | --- |
| `Grammar` | **`Kind`** | `write` | `write_atom` |
| `Shape` | **形状名** | `view`、`typst`、`slots`、`arity`、`entry`、`horizontal`、`vertical`、`class` | `entry_cell`、`math_class`、`idx_horizontal`、`cursor::vertical`、`view_atom` |

分界是**"这个节点是什么"**与**"它长什么样、光标怎么走"**。所以：

- **拼写不跟着借来的形状走**。`frac(a, b)` 存成 `MacroCall`，拼写就是 `MacroCall` 自己的 `Write::Named`。这正是`Write::Delimited` 与 `Write::Positioned` 里那两条 `Kind::MacroCall` 分支消失的原因——它们存在的唯一理由就是拼写曾被借走。
- **槽位与导航按形状名取**，配置命令借的就是它。

### 顺带删掉的

| 删掉 | 为什么 |
| --- | --- |
| `Horiz::Pair` 整个变体 | 它硬编码的下标编码的是"反着存"这件事；按书写顺序存之后，它与 `Horiz::Linear` **完全等价** |
| 解析期 `args.swap(0, 1)` | 不再需要 |
| `Write::Template("root({1}, {0})")` | 改成 `root({0}, {1})`，读作书写顺序 |
| `Write::Delimited` / `Write::Positioned` 里的 `Kind::MacroCall` 分支 | 拼写不再借形状 |
| `fill_command_cells` 对 `Write` 的依赖 | 格子数改问 `Shape::slots`（`Arity::Exact`）或矩阵默认值（`Repeat`），回写与"敲几个格子"彻底分开 |
| `MathAtom::decl()` | 换成 `shape()` + `grammar()` |

`configured_kind` 保留，但**职责缩小到"取图数据"**：`abs` 是哪对定界符、`hat` 是哪个记号、`overline` 在上还是在下。拼写、槽位、导航三样都不再经过它。它会在配置自己带 view 字段那一轮消失。

### root 的实测（真二进制 + 真前端）

存储改序后，线上视图的角色顺序跟着变，**前端不用改**——`mathview.slot()` 按 role 找孩子，位置只作回退：

```
$ root(3, x + 1) $
  [root] marker=radical
    [cell] role=index      ← 3
    [cell] role=radicand   ← x + 1
  PAGE ops: '𝑥', '+', '1', '3', line×4      ← 被开方式与根指数都画对
```

### 测试改了三条（都是预期的，不是回归）

| 测试 | 原断言 | 新断言 | 为什么 |
| --- | --- | --- | --- |
| `a_radical_enters_its_degree_which_is_not_typsts_first_argument` | `([1], 0)` | `([0], 0)` | 根指数现在是第 0 格；**语义没变**（向前仍落在根指数），改名去掉"not typst's first argument" |
| `a_radical_walks_its_two_cells_and_then_leaves` | `([1],…)`/`([0],…)` | 对调 | 同上 |
| `a_radical_is_entered_backward_at_the_end_of_its_radicand` | `([0], 3)` | `([1], 3)` | 被开方式是第 1 格 |
| `root_cell_zero_is_nucleus_and_index_is_one` | cell 1 → cell 0 | 改名 `root_cell_zero_is_the_index_and_one_is_the_nucleus`，cell 0 → cell 1 | 同上 |

**行为本身一条都没变**：向前进入仍落在根指数、向后仍落在被开方式且落格尾。变的只是格子的下标，而这正是这次改动的**目的**——下标不再需要被记住。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **137 通过 / 0 失败** |
| `cargo test … --manifest-path native-adapter/Cargo.toml` | **21 通过** |
| `python -m unittest desktop.test_desktop` | **87 通过** |
| `python tools/kind_inventory.py` | 退出码 0，两个方向都对齐 |
| root 的线上视图与绘制 | role 顺序 `index`→`radicand`，页面画出 `x + 1` 与 `3`（真二进制 + 真前端实测） |

## 清理不必要的 Kind：五个描述符变体出栈 · 2026-09-12

### 删掉了什么

| 删掉 | 类型 | 为什么它已经不需要存在 |
| --- | --- | --- |
| `Sqrt` / `Root` / `Accent` / `Line` / `Style` | `Kind` 变体 | **从来没有任何源码能把它们存进树**（`tests/stored_kinds.rs` 一直在守这条）。它们唯一的作用是给 `configured_kind` 一个可以借的载体 |
| `configured_kind` | 函数 | 它的"取图数据"搬进了 `config/commands.json` |
| `Write::Positioned` | 枚举变体 | 只有 `Line` 声明它，而 `Line` 没了 |
| `Horiz::Pair` | 枚举变体 | 上一轮让 root 按书写顺序存之后，它与 `Linear` 完全等价 |
| `placeholder_indices` | 函数 | 两个调用者都没了：`fill_command_cells` 改问 `Shape::slots`，解析期的 `args.swap` 上一轮删掉 |
| `fill_template` 的 `{name}` 分支 | 代码 | `Write::Template` 只剩 `frac({0}, {1})`，没有名字可替换 |

`Kind` 现在只剩**会被存进树的变体**（14 个 = 12 个进树 + `Parameter`/`TemplateCall` 这两个模板专用），"这个变体会不会被存"不再是一个需要回答的问题。

### 五个描述符携带的东西去了哪

这是本轮的关键：删得掉是因为**每一样都已经有别的地方放了**。

| 原来由描述符携带 | 现在在哪 |
| --- | --- |
| 槽位、导航、排布名 | `Shape`（本来就是按形状名取的） |
| 回写模板 | `Grammar`——而一个调用永远是 `Write::Named`，不需要模板 |
| 取图数据：`abs` 是哪对定界符、`overline` 在上还是在下 | **`config/commands.json` 的 `text` / `above`** |
| 记号名（`hat`）、变体名（`bold`） | **就是命令行本身**，所以配置里不用写 |

最后一条值得单说：`hat` 的记号叫 `hat`、`bold` 的变体叫 `bold`，所以那两个字段根本不需要存在——`Draw::Mark` 与 `Draw::Variant` 都不带数据，画的时候直接读调用的名字。

配置因此长这样（不填的字段取默认值）：

```json
"abs":      { "shape": "delim", "text": "|\n|" },
"overline": { "shape": "line",  "above": true  },
"underline":{ "shape": "line",  "above": false },
"hat": "decoration",
"bold": "style",
```

`build.rs` 为这几个字段加了交叉校验，写错形状直接**编译失败**：`text` 只能配 `delim`、`above` 只能配 `line`、`text` 必须是"左\n右"两段且都非空。

### 借形状的入口只剩两个

`view_atom` 原来要先把一个 `Kind` 求出来（`shape.as_ref().unwrap_or(&atom.kind)`）再走那张大表。现在是两条清楚的路：

- `MathAtom::command_shape()` → `Option<Shape>`（拿槽位与导航，并替字体变体把关字形串）
- `slots::configured_draw(name)` → `Option<Draw>`（拿画法）

新增的 `View::view_configured` 只做一件旧事：把 `Draw` 翻成线上节点。**它的注释里专门写了"线名要写死，不能拿 `Shape::view`"**，因为 `sqrt`/`delim`/`decoration`/`line` 四个形状名与线名不同（都走 `decorated`）。

### 一个我自己写出来又被测出来的缺陷

第一版 `view_configured` 里，我把分派条件写成了 `configured_draw(name)` 有值就画配置形状，**漏掉了 `command_shape()` 那道关**。后果是可观察的：`$bold(frac(a, b))$` 的主体不是字形行，本该退回 `raw_macro` 画成一张图，却建出了一个永远填不上字形的 `style` 节点。

`test_a_variant_without_a_glyph_run_is_drawn_as_the_call` 当场报错：

```
AssertionError: True is not false : $bold(frac(a, b))$ 的结构化主体不该建样式节点
```

修法是把关重新放回 `command_shape()`——它才是那个会因"主体不是字形串"而返回 `None` 的查询。这条用例是上一轮为了别的事写的，这次恰好逮住了反向的错误，说明它钉的位置是对的。

### 两条测试的覆盖变化（不是回归）

| 用例 | 变化 |
| --- | --- |
| `four_kinds_are_shape_descriptors_that_are_never_stored` | **删掉**——它守的五个变体不存在了，问题本身消失 |
| `a_named_command_stores_a_call_and_borrows_its_shape` | **新增**，覆盖十个借形状的命令：断言存成 `MacroCall`、借到的形状名、线上排布、`marker`、`style_name` |
| `the_typst_vocabulary_is_covered_exactly_once` | **改为按形状对账**。`Radical`/`Accent`/`Cancel`/`Line` 现在只由**借来的形状**认领，五个描述符删掉后形状表是唯一认领它们的地方 |
| `named_roles_exist_…` / `the_schema_covers_exactly_…` | 改为遍历 `every_shape()`：**存进树的 Kind 的形状 + 借来的形状**。只查 Kind 会漏掉 `frac`/`sqrt`/`hat` 的槽位——而它们恰好是借来的那一半 |

词汇表那条测试第一次改完仍然失败，报 `["Fenced", "Fraction", "Glyph", "Radical"]`。原因不是语义而是**列表重复**：`every_shape()` 里 `fraction`/`delim`/`grid` 会各出现两次（一次经由 Kind、一次经由可借形状），重复项被读成了"两个形状共同认领"。加了 `distinct_shapes()` 去重后是 `["Glyph", "Radical"]`，与删除前一致。

### 验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **137 通过 / 0 失败** |
| `cargo test … --manifest-path native-adapter/Cargo.toml` | **21 通过** |
| `python -m unittest desktop.test_desktop` | **87 通过** |
| `python tools/kind_inventory.py` | 退出码 0，两个方向都对齐 |
| 死代码扫描 | `cargo clippy --all-targets` 无 `dead_code`/`never used` 警告；`slots.rs` 的常量逐个查过引用 |

改动文件：`config/commands.json`、`crates/core/{build.rs,src/{slots,math,typst,view,cursor}.rs}`、`tests/{stored_kinds,round_trip}.rs`、`docs/{architecture,kind-inventory,rust-for-cpp,validation}.md`。

`docs/rust-for-cpp.md` 里的代码片段早已落后于代码（它举的 `match (name, args.len())` 分发几轮前就删了，`Kind::Decoration` 更是早就拆开），本轮把变体名换成存在的、并在开头加了一段"片段可能跑在前面，以点名的文件为准"的说明——教学文档逐行维护没有收益，但让人照着抄到不存在的类型上是另一回事。

改动文件：`crates/core/src/{slots,math,typst,cursor,view}.rs`、`crates/core/build.rs`、`tests/{caret_navigation,lyx_traces,command_mode,failed_block,round_trip,stored_kinds,structured_input}.rs`、`docs/{architecture,kind-inventory,validation}.md`。

## 编辑模型独立出来 + 模板改存视图树 · 2026-09-12

按 [editing-model.md](editing-model.md) 实施。两件事一起做，因为它们互相是对方的前提：编辑层要能说"哪一格是洞"，而"洞"只有在模板那条路上才真的吃紧（同一形状的同一实例里，分子可达、分母不可达）。

### 一、`Shape` 缩到 box，编辑规则搬到 `editing.rs`

新增 `crates/core/src/editing.rs`。`Shape` 从 8 个字段减到 5 个（`view`/`typst`/`slots`/`arity`/`class`），`entry`/`horizontal`/`vertical` 搬去一张**按形状名取**的规则表：

```rust
const RULES: &[(&str, Rules)] = &[ ("fraction", FRACTION_RULES), ("root", ROOT_RULES), … ];
pub fn rules(shape: &str) -> Rules
```

`MathAtom::entry_cell` / `idx_horizontal` 也从 `math.rs` 搬走（`math.rs` 是模型，`cursor.rs` 是编辑层），`Editor::vertical` 里那段 `match` 抽成纯函数 `editing::vertical(atom, index, up, at_cell_end) -> Option<Landing>`，光标自己那套几何（`target_x`、`geometry`）留在原地——那是测量，不是规则。

顺带把一条**没有名字的判断**变成了规则：**从左右走到相邻格时，进入的那一格光标落在末尾**。它原来写作

```rust
let radical = owner.command_shape().is_some_and(|shape| shape.is_radical());
let root_back = !forward && (radical || matches!(owner.kind, Kind::Table { .. } | Kind::Multiline { .. }));
```

——一条规则靠一个词汇表字段加一个 `Kind` 匹配拼出来，`Shape::is_radical()` 也随之删掉。现在是 `Rules::back_lands_at_end`，`sqrt`/`root`/`grid`/`aligned` 为真。

规则表按**名字**查，编译器管不到它，所以加了两条对账测试（都在 `editing.rs`）：

- `every_shape_declares_its_editing_rules`——**两个方向**：每个形状都有规则、每条规则都指着一个真实形状。没有它，漏一条的形状会静默按 `PLAIN` 导航。
- `named_roles_exist_in_the_schema_that_names_them`——`slots.rs` 那条闸跨两表继续生效：规则里写的 `Role` 必须在那个形状的 `slots` 里存在。

两条都从 `slots::fixtures::every_shape()` 取样本（`#[cfg(test)] pub(crate) mod fixtures`），**同一个清单**是"跨两表"能成立的前提。另有 `vertical_landings_stay_inside_the_node`（新）与恢复的 `entry_cells_stay_inside_the_cells_they_describe`、`horizontal_neighbours_stay_inside_the_cells_too`。

`Slot.optional` 删除，`Slot::blank` 并入 `Slot::scaled`（见 editing-model.md 第四节：零读者）。

`class` **没有**跟着搬，理由写在 editing-model.md 第五节：它的数据来源是 Typst 的数学间距类，前端既不读也不上线，只是恰好被 `move_word` 用了。

### 二、模板改存视图树

`MacroDefinition.template` 从 `Arc<MathData>`（原子树）改成 `Arc<ViewTemplate>`（**显示树**）。定义体只解析一次，投影之后原子树丢掉——两份表示会漂移，而显示树才是每个读者要的那份。

```rust
pub enum ViewTemplate {
    Node(ViewNode),                                        // 一个显示节点
    Hole { index: usize },                                 // 洞：绑定时换成实参视图
    Edge { definition: usize, cells: Vec<ViewTemplate> },  // 边：绑定时递归展开
}
```

三处关键：

1. **注册期的投影没有会话。** `Projector { registry, session: None }` —— 注册期没有光标、没有选区、没有历史、没有几何，**没有东西可传**。所以"存下来的模板不带光标"是调用事实，不是要记住的纪律（`ViewTemplate::of` 里那句 `debug_assert!` 是防止这条悄悄失效）。
2. **绑定是全函数。** 原来的 `bind_template_inner` 按 `view.kind == "parameter"` / `== "template-call"` 两个**名字**判断；现在 `Projector::material` 按**变体**分派，洞一定变成实参视图、边一定变成被调宏的展开。嵌套时实参先在**调用方**语境里绑定（`docs/editing-model.md` §8 拒绝的"来源旁挂表"因此不需要）。
3. **`View` 拆出一个 `ViewNode`。** 前者是上线节点（19 字段 = 结构 + 会话 + 归属），后者是模板能存的那一半（11 字段）。会话字段（`cursor`/`active`/`selected`/`edit`/`attachment`）和归属字段（`definitions`/`origin`/`source_range`，由绑定者按定义写出）都不在模板树里。

`Projector` 的字段也从 `definitions: &str` 改成 `registry: &MacroRegistry`：注册期没有前缀字符串可传，而 `view_atom` 本来每次都要 `macro_registry(&self.definitions)` 查一遍——现在整次投影只查一次。

### 三、`#parameter0`：先看到它红，再看到它绿

这一条**先于实现写**（上一轮就以 `#[ignore]` 落进 `tests/editing_model.rs`），当时实测红：

```
#let mathbf(x) = $bold(upright(#x))$   配   $mathbf(u)$
  [raw_macro] text='bold(upright(#parameter0))'      ← 同一处漏了两层
```

现在绿，而**真正修好它的只有一件事**：`Kind::Parameter` 带上参数在定义里的**真名**（`{ index, name }`），`Write::Marker` 拼 `#x` 而不是 `#parameter{index}`。被删掉的是**构造 `#parameter0` 的那行代码**，而且新拼写是可定位的真源码——`raw_macro.text` 现在是 `bold(upright(#x))`，`definition_raw_ranges` 也真能在定义里找到它。`tests/editing_model.rs::template_material_spells_its_hole_the_way_the_definition_writes_it` 把这条直接钉住（构造模板里的那个节点，断言它写出来是定义自己的名字）。

**一处需要更正的说法。** 我起初把"模板改存视图树"也算成 6.2 变绿的原因之一，写成"靠两件独立的事"。**那是不准确的。** 漏点的**起点**是 `write_atom` 的拼写，起点换了，这条路无论模板怎么存都不会再漏出那个字符串。至于"显示树里没有洞这种节点"，它是**控制流**保证、不是类型保证：`View.kind` 仍然是 `String`，`view_atom` 仍然建得出一个 `kind == "parameter"` 的 `View`；拦住它的是"文档树里不可能有 `Kind::Parameter`"（解析期事实，重构前就成立）加上"模板投影的结果总被 `ViewTemplate::of` 消费"（控制流事实）。模板存视图树的价值在别处，见 editing-model.md 第九节。

另一条不变量 `template_material_is_unreachable_from_the_caret` 一直是绿的（构造），本轮未动。

### 四、前端与工具：例外清单消失

`parameter`/`template-call` 是模板内部树的节点，上线前就被换掉，所以 `mathview.py` 那两行画法是死代码，`tools/kind_inventory.py` 还得为它们维护一份 `NEVER_ON_THE_WIRE` 例外清单。改存视图树之后它们在显示树里连节点都不是：

- `mathview.py` 的 `ARRANGEMENTS` 删掉这两个名字（真漏出来会变成"不认识的排布"，报一次）；
- `kind_inventory.py` 的例外清单整个删掉，双向对照变成**确切**的：23 个真发出的线名 = 23 个声明 − 2 个前端自造的（`symbol`/`absent`）。

### 五、恢复的测试与改写过的断言

| 用例 | 处理 |
| --- | --- |
| `source_modes.rs::macro_edits_reclassify_…` | 恢复。原来用 `first_view_kind` **按画法**回答"这个名字还绑着可展宏吗"；改成 `expands(&e, "ratio")` 直接问注册表——画法是显示层的事，这个问题是模型的 |
| `macro_scope.rs::lexical_blocks_shadow_…` / `function_parameters_and_later_bindings_…` | 恢复。原来 `write_cell(&def.template)` 读原子树（改存视图树后**编译都过不了**），改成走视图树的小助手 `material()` |
| `source_modes.rs::dependency_graphs_…` | `d.template.len() <= 3` 改成"根节点的孩子 ≤ 3"——同样材料的同一个计数，读的是注册表真存的那棵树 |

### 六、验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **142 通过 / 0 失败 / 6 忽略**。6 条忽略都需要本机环境、与改动无关（5 条要真 Tinymist、1 条要构建好的原生渲染器）。用例数 148，比上一轮多 9：`editing.rs` 新增 5 条、后补的拼写断言 1 条，另 3 条从 `cfg(any())` 摘除状态恢复 |
| `cargo test … --manifest-path native-adapter/Cargo.toml` | **21 通过** |
| `python -m unittest desktop.test_desktop` | **87 通过** |
| `python tools/kind_inventory.py` | 退出码 0；23 对 23，`parameter`/`template-call` 上线次数 0 |
| `tests/editing_model.rs` | 两条不变量全绿（第二条从红转绿） |

行为零变化的部分：`$ dbl(y) $`（`#let dbl(x) = $#x + 1$`）的视图逐字段与改前相同；`frac(a, b)`、`mat`、`x^2`、`a/b` 等 32 个盘点用例的输出未变。

### 七、分开记账、本轮不解决的
- **`Style` 的重新定形**：`bold(upright(#x))` 里那两层 `raw_macro` 仍在——`has_glyph_run` 在**绑定之前**问，那时洞还没填，`bold` 借不到 `style` 形状。介入点是"绑定之后要有一次重新定形"，与编辑层拆分无关（editing-model.md §9）。
- **两个命令模式缺陷**（上一轮已记）：`\bold`/`\mat` + Enter 建出 4 格而不是 1 格——`command_shape()` 对空主体答 `None`，退回 `MacroCall` 的重复模式。测试缺口在 `command_mode.rs` 的"空括号可写"清单里没有这两个名字。
- **`Style` 的重新定形**：已在下一节修好。
- 光标越界 panic（`cursor.rs` 的 `valid()` 不覆盖 `Action::Key`）与多格溶解丢内容，均未动。

改动文件：`crates/core/src/{editing.rs（新）,slots,math,typst,cursor,view,lib}.rs`、`desktop/mathview.py`、`tools/kind_inventory.py`、`tests/{editing_model,source_modes,macro_scope,stored_kinds}.rs`、`docs/{architecture,editing-model,kind-inventory,validation}.md`、`AGENTS.md`。

## 字体变体的定形挪到实例侧 · 2026-09-12

修的是上一次记下的那条待办（editing-model.md §9）。症状：

```
#let mathbf(x) = $bold(upright(#x))$   配   $mathbf(u)$
  [macro] mathbf
    [raw_macro] text='bold(upright(#x))'      ← 一整段调用的图，而不是字体变体
```

### 一、先量清楚：判据没错，只是问得太早

把这几个主体**绑定之后**再喂给 `has_glyph_run`，答案全部正确：

| 绑定后的主体 | `has_glyph_run` | 借到的形状 | |
| --- | --- | --- | --- |
| `bold(upright(u))` | `true` | `style` | ✅ |
| `bold(a + 1)`（`#x` → `a`） | `true` | `style` | ✅ |
| `bold(frac(a, b))` | `false` | — | ✅ 仍退图 |
| `bold(x^2)` | `false` | — | ✅ 仍退图 |

而**直接写**的 `$bold(upright(a))$` 一直是好的——所以缺陷的边界不是"嵌套"，是"**主体里有洞**":注册期问这个问题时，主体是 `Parameter`，而洞不是字符。

### 二、修法：注册期不下结论，调用点重投影

被否掉的第一版给 `ViewTemplate` 加了 `Deferred` 变体。**设计评审时指出它不必要**，而且对：折叠节点自带的 `text` 就是那句**带洞的拼写**，所以"未定"本来就有地方放。

最终改动只用四处，`ViewTemplate` 一个字段都没动：

| 改动 | 做什么 |
| --- | --- |
| `MathAtom::command_shape_ignoring_body()` | 只按**名字**借形状，不看主体。模板里那个变体调用因此有形状可借 |
| `MathAtom::shaped_by_body()` | 把原来内联在 `command_shape` 里的主体判定提成有名字的查询 |
| `Shape::needs_binding()` | 哪种形状的适用性依赖主体——只有 `style` |
| `Projector::material` 的折叠分支 | 绑完拼写、重解析、**用普通的 `view_atom` 再投影一次** |

最后一条是全部要点：**判据一个字符都没改**，只是终于拿到了主体。`spelled()` 把模板里那句带洞的拼写按实参填好（洞的位置来自 `ViewTemplate::Hole`，实参的拼写来自实参视图），`parse_document` 把它读回原子，然后走的就是画文档里任何节点的那条路。

### 三、实测（真后端）

| 输入 | 视图 |
| --- | --- |
| `#let mathbf(x) = $bold(upright(#x))$` + `$mathbf(u)$` | `style{text:"bold(upright(u))", style_name:"bold"}` ✅ |
| `#let plus(x) = $bold(#x + 1)$` + `$plus(a)$` | `style{text:"bold(a + 1)"}` ✅ |
| `#let fracx(x) = $bold(frac(#x, 2))$` + `$fracx(a)$` | `raw_macro{text:"bold(frac(#x, 2))"}` ✅ 不降级 |
| `$bold(frac(a, b))$` 直接写 | `raw_macro{text:"bold(frac(a, b))"}` ✅ 不变 |
| 未知名字的调用（宏内） | 折叠不变 ✅ |

**字形请求的键跟着变成绑完之后的拼写**（`bold(upright(u))`）——它本来就是唯一能被引擎编译的表达式，原先那把带 `#x` 的钥匙打不开门，正是整块退成图的原因。前端一行没改。

### 四、新用例与变异检查

`tests/editing_model.rs::a_variant_a_template_built_around_a_hole_is_shaped_once_it_is_bound`，三条断言：形态是 `style`、命令名是 `bold`、**`text` 是绑完之后的拼写**（最后这条是字形请求的键，最要紧），外加一条反例（分式主体仍该退图）。

变异检查：把 `settled_by_binding` 那道关短路成"永远不重投影"，用例立刻红在第一条断言上——**它有牙**。

### 五、验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **143 通过 / 0 失败 / 6 忽略**（+1 新用例） |
| `cargo test … --manifest-path native-adapter/Cargo.toml` | **21 通过** |
| `python -m unittest desktop.test_desktop` | **87 通过** |
| `python tools/kind_inventory.py` | 退出码 0 |

改动文件：`crates/core/src/{math,slots,view}.rs`、`tests/editing_model.rs`、`docs/{editing-model,validation}.md`。

**仍然不解决的（分开记账）**：`bold(frac(a, b))` 这类结构体**仍然是整段调用的图**——这是设计决定，不是遗留缺陷（本地排版画不出引擎的分数线与根号钩子）。要让结构体也换成字形，得走当时讨论过的 (C) 路：让引擎逐叶子回答字形，结构仍由引擎的图或本地排版负责。本次做的是 (A)，即"变体套普通字符"这一类的定形。

## `#let` 体一律保留源码 + 公式接上 LSP 诊断 · 2026-09-12

### 一、`#let` 体里的 `$$` 不再是公式框

原来只有**不可展**定义体被 blocked：

```
#let dbl(x) = $#x + 1$\n$dbl(y)$
  formula 14..22  editable=true     ← 定义体自己，一个公式框
  let      1..22
```

`#let dbl(x) = $#x + 1$` 是**可展**的，于是它整个区间不 blocked，`$$` 就拿到了 `editable=true`。

**改动**：`scan_syntax` 里任何 `LetBinding` 的区间都 blocked，理由是两层的——不可展定义体是任意 Typst（`$$` 不是能投影的公式），可展定义体更强：它是**宏模板**（`Kind::Parameter`/`TemplateCall` 的树），而那不是文档公式可以写回的树。把模板体投影成公式框，等于把模板专用节点放在离文档一步的位置，正是 `docs/editing-model.md` §6 要防的事。`blocked` 因此带上 reason，两种定义给两句话。

**不损失任何东西**：定义体里的片段在**调用点**渲染，而调用点的视图本来就带着它、外加它在定义里的区间（上一轮 `Projector` 写 `definitions`/`origin`/`source_range` 正是为此）。实测：

```
#let fixed(x) = $#x + lr(a, size: #100%)$\n$fixed(y)$
  formula 16..41 editable=false                       ← 定义体，保留源码
  formula 42..52 editable=true                        ← 调用点
    raw text="lr(a, size: #100%)" render_id="22:40:0:0" source_range=[22,40]
      render.raw id="22:40" 22..40                    ← 要的仍是定义里的那段
```

### 二、公式接上 LSP 诊断

**LSP 管线早就在跑**（`services.language` 的 `diagnostics` 支持 push 诊断的等待与版本校验），缺的是接线：诊断回来了，没有任何东西把它变成视图节点。

- `Window.request_diagnostics` 在 `project()` 之后由单发的 `diagnostic_timer`（400ms）触发，一次停顿一个请求；
- `Window.mark_diagnostics` 把 LSP 位置（行 + UTF-16 字符）**经 `lsp_position` → `from_byte`** 换成文档字节，再与每个节点的 `render_id`(`start:end`) 比对——两套坐标因此对齐；
- 命中的节点盖 `error`，没命中的清掉。

**只有带 `render_id` 的节点会被标**，而那就是引擎必须求值的那些（内核画不出、退成图的调用）。**可编辑槽位永远不会被标**，所以打字打到一半不会变红——这是"只拦结构公式的展开结果"这条要求的落地方式。

### 三、画法：一个函数，一个状态

`mathview.py` 新增 `failed_box`，被两处调用：图的请求被拒绝的 `raw`，和语言服务拒绝的节点。两者都是"编辑器在这里没有东西可以排"，所以用一种样子——源码、暖底、虚线框，加进 `box.raws` 因而还能双击进源码修。**两处共用一个函数是刻意的**：读者不该为一个状态学两个信号。

### 四、三条既有用例的语义变化（不是回归）

| 用例 | 处理 |
| --- | --- |
| `tests/desktop.rs` 的投影用例 | 断言从 `formulas[1].editable == true` 改为 `false`（**这就是需求**），并断言 reason 提到"宏模板"；用例改名以说明它现在守什么 |
| `test_a_macro_fragment_is_rendered_from_its_call_site_like_any_other` | 从**调用点**的视图取 `raw` 节点（定义体不再是公式）。断言本身不变：要的区间仍是定义里的那段 |
| `test_a_definition_fragment_keeps_the_call_that_renders_it` | 前提（"只有定义体在屏幕上"）消失了。改成"调用点在屏幕上、片段的区间在定义里"，`context_end` 仍是整篇——意图保留 |
| `test_let_edit_rebuilds_affected_following_projections` | 顺带发现它**一直是假通过**：fixture 用单字母名 `f`，而单字母名被词法读成 `MathText`，`$f(a)$` 根本不是调用，所以"后续投影变了"其实是被**定义体自己的视图**变出来的。改用 `dbl` 之后它守的才是它声称的事 |

### 五、验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --offline --locked` | **144 通过 / 0 失败 / 6 忽略**（+1 新用例） |
| `cargo test … --manifest-path native-adapter/Cargo.toml` | **21 通过** |
| `python -m unittest desktop.test_desktop` | **88 通过**（+1 新用例） |
| `python tools/kind_inventory.py` | 退出码 0 |

改动文件：`src/desktop.rs`、`desktop/{window,mathview,test_desktop}.py`、`tests/desktop.rs`、`docs/validation.md`。










## 2026-09-12：矩阵补齐、会话恢复、实时预览和宏定义块

- 矩阵短行补为空块，新增输入后写回及退出重进回归；普通对齐公式保持原行为。
- 后端重启重放当前公式已确认操作，实测保留光标、选区、命令草稿与撤销历史。
- 实时预览与源码同步共用语言管道，新增服务重建、换文件与过期回包测试。
- 相邻 #let 显示为紧凑内嵌块；四个方向键进入，Enter 确认退出，Shift+Enter 换行，Esc 取消。草稿编辑不发送核心请求，提交产生一个文档撤销步。
- Rust 默认测试全部通过（5 项环境依赖测试默认忽略）；另行执行真实 Tinymist 预览及同步测试通过。Qt 离屏全套 95 项通过；最终 release 二进制上的 7 项新增回归全部通过。
- 已构建 target/server/release/typformula.exe；没有使用 computer use，没有打开可见窗口或浏览器。native-adapter 未修改。

### 2026-09-12：宏定义块直接展示高亮源码

未进入编辑时，宏定义块改为白底细框内显示完整定义源码，复用编辑区字体、语义颜色和引擎颜色；长行按可用宽度换行。文本布局按源码、字体、宽度和高亮结果缓存，高亮回包到达时重绘。没有新增后端请求，草稿确认与键盘行为不变。

仅使用 Qt 离屏验证：完整测试 97 项通过，覆盖源码完整性、UTF-16 高亮范围、颜色更新、长行换行及原有方向键进入/确认退出；另生成离屏图像检查框内排版。未使用 computer use，未打开可见窗口。
