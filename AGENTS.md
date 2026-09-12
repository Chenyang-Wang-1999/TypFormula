# TypFormula --- Typst 可视化编辑器

## 开发原则
- 遇到意外情况马上上报，不要自行处理

## 项目结构

```
typformula/
├── src/                        外围（host crate `typformula`）：与外界打交道的一切
│   ├── lib.rs                  模块清单；说明"内核可以没有外围，外围不能没有内核"
│   ├── main.rs                 两个进程入口：`--desktop-core`（文档+公式会话）、`--stdio <目录>`（服务管道）
│   ├── document.rs             文档所有权：源码是唯一权威 + 一个活动公式会话；`Document::annotate` 给每个 Raw 算源码区间（`Locator`）
│   ├── desktop.rs              `--desktop-core` 的协议：`analyze`/`analyze_formula`/`scan`（含 `style_expressions`），一行一个 JSON 动作
│   ├── rpc.rs                  `--stdio` 的协议：`dispatch` 把请求路由到 services/lsp/packages/preview
│   ├── services.rs             Tinymist 会话（语言方法**与实时预览**共用）、公式/附件/字形适配器子进程、整页预览与 PDF；`ask_adapter` 是三条适配器请求的公共入口
│   ├── packages.rs             @preview 包索引检索、下载、解压到 Typst 缓存
│   └── workspace.rs            工作区内路径解析（拒绝越界）
├── crates/core/                编辑内核（crate `typformula-core`）：只依赖 `typst-syntax` 与 serde
│   ├── Cargo.toml
│   ├── build.rs                编译期把 config/*.json 生成成 `COMMANDS`/`SYMBOLS` 表（格式错误直接编译失败）
│   └── src/
│       ├── lib.rs
│       ├── math.rs             `MathData`/`MathAtom`/`Kind`：可编辑树本身；`command_shape()`/`is_macro()`
│       ├── slots.rs            声明表：每个 `Kind` 的槽位、形状名、拼写、对应的 Typst `MathKind`
│       ├── editing.rs          编辑模型：进入点、左右/上下怎么走、哪一格可达（按**形状名**取）
│       ├── typst.rs            源码 ⇄ 树：`parse_formula`/`write_atom`，宏注册表，`has_glyph_run`
│       ├── cursor.rs           `Editor` 与 `Action`：所有编辑动作（含命令草稿、选区、历史）
│       └── view.rs             `View`/`Response`：交给前端的显示树（`view_atom` 把形状合并成线名）；
│                               注册期存下的模板也是显示树（`ViewTemplate`，洞与边是它的**变体**）
├── desktop/                    Qt 前端（PyQt5，源码运行）
│   ├── __main__.py             入口：注册随附字体、装异常钩子、开窗口
│   ├── window.py               主窗口：源码/编辑区投影、公式会话驱动、Raw 取图调度、源码栏、大纲、预览、菜单
│   ├── editor.py               编辑区控件：自定义公式对象、投影与光标映射、行号、语法高亮
│   ├── mathview.py             **公式排版与绘制**：`Typesetter`（Box 布局）、`FormulaObject`（页面里的公式）、`MathCanvas`（公式编辑框）
│   ├── preview.py              实时预览的接线：QtWebEngine 的 import 顺序约束、从 Tinymist 回复里取页面地址（渲染归 Tinymist）
│   ├── mathfont.py             字体家族解析与字形映射（`glyph(..., substituted=True)` 是"引擎给的串原样画"的分界）
│   ├── model.py                UTF-8 / UTF-16 / Qt 位置映射（`to_byte`/`from_byte`/`u16`/`from_u16`）、`Projection`、设置读写
│   ├── bridge.py               子进程桥：`Core`（`--desktop-core`）、`Services`（`--stdio`）
│   ├── rawcache.py             Raw 片段的稳定身份（`raw_key`）与脚本形状摘要、失效判定
│   ├── incremental.py          增量合并：只重建受影响的公式视图
│   ├── svg.py                  Qt 5 SVG 兼容（Typst 的 glyph `<symbol>` 会报 link is undefined）
│   ├── test_desktop.py         离屏集成测试（需要先 build-desktop.cmd）
│   └── requirements.txt        PyQt5 等运行依赖
├── native-adapter/             独立 crate（`typformula-layout`）：唯一链接 Typst 编译器的地方
│   ├── Cargo.toml              自己的 workspace（与根 workspace 隔离）
│   ├── Cargo.lock
│   ├── engine-patches.json     vendor/typst 上打了哪些补丁（供重新 vendoring 时对照）
│   ├── layout-entry.rs         引擎补丁里那段入口代码
│   ├── src/main.rs             单次请求模式 + `--server`；三类请求：映射区间取 SVG / 附件 placement / `glyphs` 取替换后的字形
│   ├── src/render.rs           FontStore 与 World：把内存源码编译成 SVG/PDF
│   └── tests/fixtures/sub/     import 相关用例的夹具
├── config/                     编译期嵌入内核的配置（不需要运行时读取）
│   ├── commands.json           14 个命令名 → 形状（`fraction`/`decorated`/`style`/`grid` 等）
│   ├── symbols.json            40 项符号名 → 显示字形
│   ├── desktop-settings.json   桌面端默认设置（字号、字体、缩放）
│   └── README.md               字符显示映射的语义
├── tests/                      host 与内核的集成测试（`cargo test`）
│   ├── round_trip.rs           每个可存进树的原子都必须往返（回写是唯一没有安全网的义务）
│   ├── stored_kinds.rs         真正会被存进树的 `Kind` 恰好是哪 12 个
│   ├── command_mode.rs         命令草稿：`\frac`、`\frac()`、补全、确认与取消
│   ├── structured_input.rs     普通输入 / 字符串模式 / 符号简写的边界
│   ├── caret_navigation.rs     导航规则（与 editing.rs 的声明对应）
│   ├── lyx_traces.rs           LyX 行为对照回归
│   ├── macro_scope.rs          宏绑定与作用域
│   ├── source_modes.rs         可展/不可展宏的判定
│   ├── editing_model.rs        编辑模型的两条不变量（洞不上线、模板材料不可达）
│   ├── document.rs             annotate 的区间、失败块的重试
│   ├── failed_block.rs         取不到图的片段：进入修复与恢复
│   ├── attachments.rs          limits / stretch 的附件布局
│   ├── desktop.rs              `--desktop-core` 协议
│   ├── services.rs             服务路由；含对着真 Tinymist 钉住实时预览返回形状的用例
│   └── workspace.rs            路径解析
├── tools/                      用真实 release 二进制取证据的脚本
│   ├── kind_inventory.py       线上实测：每个 Kind 的 view JSON + 排布名双向对照（不一致则退出码 1）
│   └── engine_boxes.py         用真实适配器量公式盒子的宽高与基线
├── docs/
│   ├── architecture.md         分层、名字处理、两棵树、Kind/View 的判据（先读这篇）
│   ├── editing-model.md        三层职责的分工：为什么光标与可编辑性不属于 `Shape`
│   ├── kind-inventory.md       每个 Kind 的能力清单：存储字段、线上字段、引擎 item、缺口
│   ├── desktop.md              桌面端操作、字体、投影、构建运行与边界
│   ├── validation.md           按日期记录的实测与修复过程（历史，不改写）
│   ├── lyx-desktop-rendering-study.md  LyX 的渲染路径调研
│   ├── rust-for-cpp.md         写给 C++ 背景的 Rust 对照
│   ├── rust-book-walkthrough.md 用本项目代码讲 Rust 书
│   └── LYX-CREDITS             LyX 作者与许可
├── vendor/typst/               固定版本的 Typst 引擎（含本项目的数学 IR 标签桥接补丁，见 UPSTREAM.md）
├── fonts/                      随附数学字体（NewCM Math 与 NewCM10 Italic）与 NOTICE
├── workspace/                  运行时的默认工作目录（未跟踪）
├── Figures/                    文档插图
├── build-desktop.cmd           构建 release 后端 + 适配器，并检查 PyQt5（只构建，不启动）
├── start-desktop.cmd           启动窗口（`python -m desktop`）
├── Cargo.toml                  根 workspace（成员 `.` 与 `crates/core`；排除 vendor 与 native-adapter）
├── COPYING                     GPL-2.0-or-later
└── README.md                   特性、构建与验证入口
```

### 两条边界

- **内核不依赖外围**：`crates/core` 只用 `typst-syntax` + serde，反向依赖会被 cargo 拒绝。加功能时先问"这属于编辑模型还是属于周边"。
- **只有 `native-adapter` 链接 Typst 编译器**：内核与前端都不链接它；要引擎的数据（字形、附件位置、编译好的 SVG）一律走 `/api/*` 请求。

### 常用命令

```powershell
cargo test --offline --locked                                    # 内核 + host 测试
cargo build --offline --locked --release --bin typformula --target-dir target/server
cargo test --offline --locked --release --manifest-path native-adapter/Cargo.toml --target-dir target/adapter
$env:QT_QPA_PLATFORM='offscreen'; python -m unittest desktop.test_desktop
python tools/kind_inventory.py                                   # 前后端排布名双向对照
```
