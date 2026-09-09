# Visual Typst

正式版从仓库根目录构建。完整 `.typ` 源码是唯一文档模型：一个 CodeMirror 编辑区负责代码输入、行号、高亮、搜索替换、选区和撤销；进入公式时，复用唯一一个 Rust/WASM 结构编辑会话。非活动公式只保留静态 DOM 投影，不创建独立编辑器。

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

## Tinymist 和包

普通代码支持 Ctrl+Space 补全、自动诊断、悬停、F12 定义跳转、格式化。标准 LSP 使用真实文件 URI 和一个常驻会话进行 `didOpen` / `didChange` 全文同步；切换文件时重建会话。公式命令的临时投影继续使用独立补全查询，避免把草稿投影当作磁盘文件。

侧栏从官方 `packages.typst.org` 索引检索包版本，**安装并插入 import** 下载指定版本到标准 Typst 缓存（Windows：`%LOCALAPPDATA%/typst/packages`），然后通过全局编辑事务插入 `#import`。支持 `TYPST_PACKAGE_CACHE_PATH`。安装已缓存版本不访问网络；索引在本地服务进程中缓存。包管理器不隐式升级版本，也不提供卸载功能。

文件/包导入的诊断和补全由 Tinymist 处理；原生公式渲染支持读取项目文件及已安装包。尚未安装的传递依赖需先安装或由 Tinymist 获取。当前 UI 重点是代码与结构公式编辑，尚未提供整页预览和 PDF 导出。

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
