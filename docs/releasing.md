# Windows 便携版打包

`build-release.cmd` 生成包含 Python、Qt、原生后端、字体和配置的目录版程序，并压缩为 ZIP。默认还包含 Tinymist；最终用户解压整个目录后双击 `TypFormula.exe` 即可，无需安装开发工具。

脚本不上传 GitHub，不签名，不启动可见窗口。ZIP 生成前必须通过冻结程序自身的离屏自检；实际窗口和 WebEngine 页面显示仍需发布前人工验收。

## 准备独立打包环境

在 Windows x64 上使用 x64 Python，先准备 README 中的 Rust/MSVC 构建环境。建议使用独立虚拟环境，避免将 Anaconda 或其他项目的 Qt 包混入产物：

```powershell
python -m venv target/release-venv
.\target\release-venv\Scripts\python.exe -m pip install -r tools/requirements-release.txt
$env:TYPFORMULA_PYTHON = (Resolve-Path .\target\release-venv\Scripts\python.exe).Path
```

打包依赖固定 PyInstaller 与 hooks 版本，应用 Qt 依赖范围沿用 `desktop/requirements.txt`。实际安装版本写入每个发布包的 `release-manifest.json`。

## 生成完整便携版

```powershell
.\build-release.cmd --fetch --version 0.1.0-alpha.1
```

`--fetch` 下载 Cargo.lock 中的依赖；已缓存时可省略。Rust 编译输出位于 `target/package-build/`，与运行中的开发编辑器使用的 `target/server/`、`target/adapter/` 分开。

Tinymist 按显式 `--tinymist`、`TINYMIST_BIN`、PATH、VS Code/Cursor 扩展目录的顺序查找。脚本不会自动下载或更新 Tinymist，找不到时会报错。可指定已经验证的版本：

```powershell
.\build-release.cmd --version 0.1.0-alpha.1 --tinymist "C:\tools\tinymist\tinymist.exe" --tinymist-license "C:\tools\tinymist\LICENSE.txt"
```

许可文件默认在 Tinymist 程序目录及上一层寻找；提供独立程序时应一起提供同一发行版的许可文件。打包脚本也收集项目、引擎、字体许可及可发现的 Python/Qt 分发元数据与许可文件。

## 使用已有 release 后端

只验证打包、无需重新编译 Rust 时：

```powershell
.\build-release.cmd --skip-build --version 0.1.0-alpha.1
```

默认读取 `target/server/release/typformula.exe` 与 `target/adapter/release/typformula-layout.exe`。也可同时传 `--backend PATH` / `--adapter PATH` 选择已有程序。脚本验证 PE 架构必须是 x64，并记录文件 SHA256；`--skip-build` 不保证这些程序来自当前源码，正式发布应使用默认编译流程。

基础版必须显式选择：

```powershell
.\build-release.cmd --skip-build --without-tinymist --version 0.1.0-alpha.1
```

它保留原生公式编辑和 PDF/SVG 导出，但不随包提供语言服务；文件名带 `-basic`，启动说明也会标明区别。QtWebEngine 仍随包包含，用户以后配置 Tinymist 后可使用实时预览。

## 产物与自检

完整版本默认输出：

```text
dist/
  TypFormula-0.1.0-alpha.1-windows-x64/
    TypFormula.exe
    _internal/                  Python/Qt、bin/ 后端、字体、默认配置
    docs/                       教程与文档
    licenses/                   许可与第三方元数据
    START-HERE.txt
    release-manifest.json
  TypFormula-0.1.0-alpha.1-windows-x64.zip
  TypFormula-0.1.0-alpha.1-windows-x64.zip.sha256
```

使用 `--output PATH` 改输出父目录。相同输出已存在时脚本拒绝覆盖；用新版本号或新输出目录重试。中间文件和失败的自检报告保留在 `target/packaging/<本轮标识>/`，便于排查，不会递归清理已有目录。

自检从独立的构建工作目录（不以仓库根目录为 cwd）运行冻结后的 `TypFormula.exe --self-test REPORT.json`，隔离用户设置、开发工具 PATH 和扩展目录，并检查：

- Qt Widgets/SVG/WebEngine 模块及 Windows/offscreen 插件、WebEngine 资源。
- 随附字体、默认配置、新文档的可写工作目录。
- 核心文档解析、公式激活、前端排版。
- 中文正文中确认 `dots` 命令后的后续公式重排，捕获 Qt 回调中的异常。
- 真实适配器的字形查询、整页 SVG 与 PDF 编译。
- 完整版随附 Tinymist 的 LSP 请求。

自检只导入 WebEngine，不创建 WebEngine 页面；也不调用系统 PDF 阅读器。通过后才生成 ZIP 与 SHA256。运行时不依赖 Rust 源码、Cargo 缓存或本机 Python，但这不代替干净 Windows 环境中的人工验收。

发布版资源从 PyInstaller 资源目录读取，辅助程序位于 `_internal/bin/`；新文档工作目录为 `%LOCALAPPDATA%/TypFormula/workspace`，设置仍位于 `%APPDATA%/TypFormula/settings.json`。打开已有文件时以文件所在目录为编译根。`TYPFORMULA_BIN`、`TYPFORMULA_ADAPTER`、`TINYMIST_BIN` 仍可显式覆盖。

## 发布前

检查 `release-manifest.json` 中的提交、工作区修改标记、依赖版本和自检结果；在没有开发工具的 Windows 环境人工确认实际窗口、实时预览、文件打开与保存。将验证后的 ZIP 与 SHA256 文件作为 GitHub Release 附件，并保留对应源码版本和随附许可材料。

打包实现依据：[PyInstaller spec 文件](https://pyinstaller.org/en/stable/spec-files.html)、[运行时资源路径](https://pyinstaller.org/en/stable/runtime-information.html)。
