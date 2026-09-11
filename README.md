# Visual Typst

原生 Qt 桌面编辑器：完整 `.typ` 源码是唯一文档模型，正文按普通文本编辑，进入 `$…$` 时用 LyX 式的结构公式编辑器，排版与 PDF 由随附的 Typst 引擎生成。

Rust 核心（`src/`）通过私有管道驱动窗口：`--desktop-core` 提供一个文档与一个活动公式会话，`--stdio <目录>` 提供 Tinymist 语言功能、公式取图和整页编译。没有 WebView、HTTP 端口或 WASM。依赖、操作与当前边界见 [桌面端说明](docs/desktop.md)。

## 构建和启动

Windows、Python 3.10+、PyQt5 5.15 与 Rust 工具链：

```powershell
python -m pip install -r desktop/requirements.txt   # 已安装 PyQt5 时无需重复安装
.\build-desktop.cmd
.\start-desktop.cmd
.\start-desktop.cmd "D:\documents\article.typ"
```

`build-desktop.cmd` 只构建，不启动窗口，尚未打包独立安装器。`VISUAL_TYPST_PYTHON` 可指定 Python 可执行文件，`VISUAL_TYPST_BIN` 可指定核心程序。

Tinymist 从 PATH、VS Code/Cursor 扩展目录或 `TINYMIST_BIN` 查找。没有 Tinymist 时仍能编辑、保存、结构编辑公式并出图，公式命令有内置补全；普通代码的补全、诊断、悬停、定义跳转和格式化需要它。

## 编辑

- 正常输入直接编辑 Typst 源码，包含不完整或有语法错误的代码。不会将正文拆成多个输入框，也不会因输入 `$` 自动改变模式。
- **行内公式**（Ctrl+Alt+I）/ **行间公式**（Ctrl+Alt+B）：在当前选区插入公式，选中文字作为公式内容，随后进入结构编辑器；**完成公式**（Ctrl+Alt+Return）或最外层 Esc 返回正文。
- 公式内 `\` 输入命令，Enter 确认，Tab 切换槽位，`/` 创建分式，`^` / `_` 创建上下标。有命令草稿时先确认或取消。数学菜单另有"矩阵增加行/列"与"刷新全部 SVG 缓存"。
- 公式里的普通输入**一个按键一个字符**，源码也一个字符一个空格地写出来：输入 `-` 再输入 `>` 得到 `$- >$`（而不是 `$->$`），输入 `x` `y` 得到 `$x y$`。原因是源码会被重新解析：`->`、`||`、`...`、`index` 这类连写会被 Typst 当成一个整体（箭头、`‖`、`…`、变量名），两个按键就会变成一个不可再拆的片段。只有数字连写（`12`、`1.5`）不加分隔符。`≤`、`≥`、`≠` 这类多字符符号改用命令输入（如 `\>=` 回车）或直接粘贴该字符。
- 行内公式使用设置里的编辑字号与数学字体；边框和留白紧贴内容，所有 Raw SVG 按该字号缩放。
- 公式里"合法但结构编辑器不建模"的片段（`sum`、`integral`、`dif` 等）保留为 Raw，由 Typst 渲染成图像，**不是编译失败**。是否建模看的是**名字在不在 `config/commands.json`**，不是写法：`cases(...)`、`cancel(...)`、`vec(...)` 都已进表，是可编辑的结构节点；有具名实参的（`lr(x, size: #100%)`）与未收录的名字才是 Raw。片段是插进源码后整篇编译、再按标签取帧的，所以拼接处不能与后文粘成别的语法（`cal(A)(E)` 这类写法要求块尾留一个空格）；某一个片段自己编译不出来时只放弃它、其余照常出图。排版结果为空白、或后端给不出可见结果的片段（如 `quad`）与"被放弃"的片段一样改为显示源码并标出暖色底 + 红色虚线框：左右键可进入该片段源码修复，Esc 恢复原内容，下一次编辑会自动重试。字体变体（`bold(x)`、`upright(A)`）画的是引擎替换后的字形，不取图。
- 片段图一律按黑色栅格化：片段是从文档里切出来的图，文档可能把数学排成白色或彩色，浅色编辑区里就看不见了。导出 PDF/SVG 仍用文档自身的颜色。
- 视图菜单可显示/隐藏源码栏、分栏（两栏共享同一份源码）、缩放编辑字号；源码栏与编辑器逐行对齐并双向同步滚动。左侧大纲按标题跳转。
- Ctrl+Z / Ctrl+Y 覆盖正文、结构公式和包导入。
- Ctrl+S 保存（原生绝对路径对话框，UTF-8 原子写入，保留 CRLF/LF 风格）；文件被外部修改时拒绝覆盖。文件菜单另有"另存为…"与"导入 Typst / 插入资源…"。
- Ctrl+H 查找/替换，作用在源码上。
- 每个 Raw SVG 由后端按其实际环境字号归一化。`base_font_size_pt = 原始宽度(pt) / 环境基准字号(pt)`，显示宽度为 `编辑字号 × base_font_size_pt × SVG 微调`；高度同理。虽然字段名称带 `_pt`，它保存的是无量纲比值。环境字号也以 `environment_font_size_pt` 返回，无需手动填写文章字号。

## Tinymist 和包

普通代码支持 Ctrl+Space 补全、诊断、悬停、定义跳转（工具菜单"格式化"为 Ctrl+Alt+F）。标准 LSP 使用真实文件 URI 和一个常驻会话进行 `didOpen` / `didChange` 全文同步；切换文件时重建会话。公式命令的临时投影继续使用独立补全查询，避免把草稿投影当作磁盘文件。

工具菜单"浏览 @local / @preview 包…"从官方 `packages.typst.org` 索引检索版本，**安装并插入 import** 下载指定版本到标准 Typst 缓存（Windows：`%LOCALAPPDATA%/typst/packages`），然后插入 `#import`。支持 `TYPST_PACKAGE_CACHE_PATH`。安装已缓存版本不访问网络，包管理器不隐式升级版本，也不提供卸载。

文件/包导入的诊断和补全由 Tinymist 处理；原生公式渲染支持读取项目文件及已安装包。尚未安装的传递依赖需先安装。F5 编译当前内存源码并交给系统默认阅读器打开原生 Typst PDF；预览到源码的双向定位尚未实现。

## 独立性与许可

构建、资源和运行路径均位于正式版自己的 `src/`、`desktop/`、`config/`、`fonts/`、`native-adapter/` 和 `vendor/typst/`。删除 `prototypes/` 不影响正式版构建；没有 `include!`、软链接或依赖路径指回原型（字符字典仍通过本地 build.rs 生成并 include）。Typst 固定源码和透明标签桥接已随仓库保存，构建不需要重新克隆引擎。

核心沿用 GPL-2.0-or-later，见 [COPYING](COPYING)、[LyX 作者](docs/LYX-CREDITS)。引擎来源与补丁说明见 [vendor/typst/UPSTREAM.md](vendor/typst/UPSTREAM.md)，字体许可见 [fonts/NOTICE](fonts/NOTICE)。`vendor/typst` 保留上游 Apache-2.0 许可，与本项目 GPL-2.0-**or-later** 的兼容路线是取 GPLv3（Apache-2.0 与 GPLv2-only 不兼容）；分发打包产物时需同时保留两份许可文本。

## 验证入口

```powershell
cargo test --offline --locked
cargo test --offline --locked --manifest-path native-adapter/Cargo.toml --target-dir target/adapter
python -m unittest desktop.test_desktop -v
```

桌面套件使用离屏 Qt（`QT_QPA_PLATFORM=offscreen`），不打开窗口，但需要先构建原生程序（`build-desktop.cmd`）。需要本机 Tinymist 的服务集成用例默认忽略。输入法、视觉对齐及真实鼠标操作仍需人工体验。架构见 [docs/architecture.md](docs/architecture.md)，各轮实测见 [docs/validation.md](docs/validation.md)。
