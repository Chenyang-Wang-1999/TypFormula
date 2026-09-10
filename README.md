# Visual Typst

现已提供原生 Qt 桌面端：运行 `build-desktop.cmd` 构建，再由你运行 `start-desktop.cmd`。它直接复用 Rust 文档/公式核心，提供原生文件对话框、分栏、源码栏、大纲、Tinymist、公式编辑，以及按键编译并打开原生 Typst PDF；不需要启动 Web 服务。依赖、操作和当前边界见 [桌面端说明](docs/desktop.md)。

正式版从仓库根目录构建。完整 `.typ` 源码是唯一文档模型：一个 CodeMirror 编辑区负责代码输入、行号、高亮、搜索替换、选区和撤销；进入公式时，复用唯一一个 Rust/WASM 结构编辑会话。非活动公式只保留静态 DOM 投影，不创建独立编辑器。

## VS Code 扩展（0.2.1）

现在提供 **CustomTextEditorProvider** 扩展。安装 `dist/visual-typst-0.2.1-win32-x64.vsix`：VS Code 扩展面板右上角 `…` → **从 VSIX 安装**，然后右键 `.typ` → **打开方式 / Reopen Editor With → Visual Typst**。扩展自动管理私有 stdio 后端，不需要 `start.cmd` 或浏览器端口。

- 默认右侧显示整页实时预览，编译当前未保存源码和已打开的项目内依赖；错误时保留上次成功页面。预览遵循文档字号/页面设置，支持缩放、适应宽度和刷新。
- 每个编辑按钮都有独立的 **Visual Typst:** 命令。点击 **快捷键** 打开 VS Code 键盘快捷方式配置；可给所有按钮改键或增加键绑定。默认 Ctrl+Alt+I 插入行内公式、Ctrl+Alt+B 插入行间公式、Ctrl+Alt+P 切换预览，macOS 使用 Cmd。
- 在公式外按左右键，可从紧邻边界跳入；上下键按相邻视觉行和横向位置选择公式槽位。Shift+方向键及纯源码模式保持普通选区/导航。
- 公式框随内容自然撑开，达到编辑列宽度／高度上限后在框内滚动，不按框缩放内容。深层结构上下标有可读字号下限，滚动时保持光标和槽位导航坐标同步。
- VS Code 的 TextDocument 负责保存、脏标记和全局撤销。并发冲突时保留当前输入，显示处理按钮。文件树、Git 和普通源码编辑器仍由 VS Code 提供。
- 推荐安装 Tinymist 扩展，以桥接它的语言功能。原生整页预览随 VSIX 附带，支持系统字体及标准随附字体。

从源码生成扩展：`.\build-vscode.cmd`。该命令只构建并打包，不安装、不发布、不启动 VS Code。当前产物为 Windows x64；其他平台需在对应平台构建原生程序和 VSIX。更多说明见 [扩展 README](extensions/vscode/README.md)。下文仍保留独立版本的运行方式。

## 构建和启动

需要 Node.js/npm、Rust 和 `wasm32-unknown-unknown` 目标。Windows 下在仓库根目录执行：

```powershell
rustup target add wasm32-unknown-unknown
.\build.cmd
.\start.cmd
# 使用自己的项目目录：
.\start.cmd D:\documents\my-typst-project
```

浏览器访问 **http://127.0.0.1:4321**。默认项目是根目录下的 `workspace/`。启动脚本只检查并启动已有产物，不自动下载或重编；修改后运行 `build.cmd`，再重启服务。构建前请关闭正在使用旧 exe 的服务。

Tinymist 从 PATH、VS Code/Cursor 扩展目录或 `TINYMIST_BIN` 查找。没有 Tinymist 时仍能编辑和保存，结构命令有内置补全。普通代码的 LSP 功能需要安装 Tinymist。

## 编辑

- 正常输入直接编辑 Typst 源码，包含不完整或有语法错误的代码。不会将正文拆成多个输入框，也不会因输入 `$` 自动改变模式。
- **行内公式 / 行间公式**：在当前选区插入公式，选中文字作为公式内容，随后进入共享的结构编辑器。
- **编辑光标处公式**：在已有 `$…$` 内放置光标后点击。进入和离开公式不会仅因解析而重写其源码；实际结构修改只替换该公式区间。
- 公式内 `\` 输入命令，Enter 确认，Tab 切换槽位，`/` 创建分式，`^` / `_` 创建上下标。在最外层按 Esc 或点击 **完成公式** 返回代码输入。有命令草稿时先确认或取消。
- 行内公式使用与代码一致的 `--editor-size`（默认 16px），采用数学字体；边框和留白紧贴内容。所有 Raw SVG 按该字号缩放。
- **查看纯源码** 可展开全部公式；重新显示后保留已编辑公式的静态投影。点击静态投影复用同一个公式编辑会话。
- Ctrl+Z / Ctrl+Y 覆盖正文、结构公式和包导入。文件切换保留各文件的未保存源码和撤销历史。
- Ctrl+S 保存；已有文件被外部修改或新文件同名时拒绝覆盖。可下载 `.typ` 保留当前版本。子目录需预先存在。
- 在 Web 版编辑区内按 Ctrl+滚轮可按 1px 步进缩放编辑字号，工具栏的 A− / A+ / 还原也可操作。设置保存在浏览器本地。
- 每个 Raw SVG 由后端按其实际环境字号归一化。`base_font_size_pt = 原始宽度(pt) / 环境基准字号(pt)`，前端显示宽度为 `编辑字号 × base_font_size_pt × SVG 微调`；高度同理。虽然字段名称带 `_pt`，它保存的是无量纲比值。环境字号也以 `environment_font_size_pt` 返回，无需手动填写文章字号。
- Web 版“打开…”使用浏览器原生文件选择器，可选择项目目录外任意 `.typ` 文件；Ctrl+S 写回已授权的文件，“另存为…”选择任意新位置。浏览器基于安全规则不会把绝对路径字符串暴露给页面。若浏览器不支持文件句柄 API，则打开仍可用，保存退化为下载。

## Tinymist 和包

普通代码支持 Ctrl+Space 补全、自动诊断、悬停、F12 定义跳转、格式化。标准 LSP 使用真实文件 URI 和一个常驻会话进行 `didOpen` / `didChange` 全文同步；切换文件时重建会话。公式命令的临时投影继续使用独立补全查询，避免把草稿投影当作磁盘文件。

侧栏从官方 `packages.typst.org` 索引检索包版本，**安装并插入 import** 下载指定版本到标准 Typst 缓存（Windows：`%LOCALAPPDATA%/typst/packages`），然后通过全局编辑事务插入 `#import`。支持 `TYPST_PACKAGE_CACHE_PATH`。安装已缓存版本不访问网络；索引在本地服务进程中缓存。包管理器不隐式升级版本，也不提供卸载功能。

文件/包导入的诊断和补全由 Tinymist 处理；原生公式渲染支持读取项目文件及已安装包。尚未安装的传递依赖需先安装或由 Tinymist 获取。整页 SVG 预览已提供；尚无 PDF 导出按钮及预览到源码的双向定位。

## 独立性与许可

构建、资源和运行路径均位于正式版自己的 `src/`、`web/`、`config/`、`native-adapter/` 和 `vendor/typst/`。删除 `prototypes/` 不影响正式版构建；没有 `include!`、软链接或依赖路径指回原型（字符字典仍通过本地 build.rs 生成并 include）。Typst 固定源码和透明标签桥接已随仓库保存，构建不需要重新克隆引擎。

核心沿用 GPL-2.0-or-later，见 [COPYING](COPYING)、[LyX 作者](docs/LYX-CREDITS)。引擎来源与补丁说明见 [vendor/typst/UPSTREAM.md](vendor/typst/UPSTREAM.md)，字体许可见 [web/fonts/NOTICE](web/fonts/NOTICE)。CodeMirror 和前端依赖的许可随 npm 包及打包输出保留。

## 验证入口

```powershell
cargo test --offline --locked
cargo check --offline --locked --features server
cargo test --offline --locked --manifest-path native-adapter/Cargo.toml --target-dir target/adapter
npm run build
npm test
```

测试不启动 HTTP 应用。需要本机 Tinymist/原生进程的服务集成用例默认忽略。浏览器中的输入法、视觉对齐及实际 LSP/网络操作还需人工体验。架构见 [docs/architecture.md](docs/architecture.md)，本次结果见 [docs/validation.md](docs/validation.md)。
