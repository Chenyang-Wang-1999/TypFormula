# Visual Typst：文档与结构公式编辑原型

当前实现允许在 Typst 上下文之间插入多个行内／行间公式，每个公式独立进行结构编辑。Rust/WASM 管理编辑状态，浏览器绘制结构，原生 Typst 引擎负责文档编译和 Raw SVG 提取。文件仍只保存为 Typst 源码。

## 模块总览

| 模块 | 入口 | 主要功能 |
|---|---|---|
| 文档协调 | [src/document.rs](src/document.rs) | 组合上下文与多个公式；插入、删除、激活公式；同步词法前缀；文档级撤销／重做；生成完整源码和 Raw 区间映射 |
| 数学数据模型 | [src/math.rs](src/math.rs) | 定义 MathData、MathAtom、结构槽和树形光标；统一字符、Raw、宏调用、分式、根式、上下标等对象的存储 |
| 公式编辑器 | [src/cursor.rs](src/cursor.rs) | 执行单公式输入、命令草稿、导航、选区、删除、粘贴和槽位操作，维护编辑器快照 |
| Typst 语法与宏 | [src/typst.rs](src/typst.rs) | 将 Typst 语法转换为编辑树并序列化；分析宏是否可展；登记定义版本与共享模板，保留调用源码 |
| 视图投影 | [src/view.rs](src/view.rs) | 从编辑树生成浏览器视图，展开宏显示，绑定重复参数到同一实参槽，输出光标位置与附件查询源码 |
| WASM 接口 | [src/wasm.rs](src/wasm.rs)、[src/lib.rs](src/lib.rs) | 用 alloc／dispatch／output_len 交换 JSON；浏览器同步调用 Document，原生服务依赖不进入 WASM |
| 前端交互与绘制 | [web/app.js](web/app.js)、[web/index.html](web/index.html) | 绘制上下文和公式，转发键盘、鼠标、输入法事件，测量光标坐标，管理宏列表、异步请求及 SVG 缓存 |
| 样式与字体 | [web/style.css](web/style.css)、[web/math-font.js](web/math-font.js) | 结构槽布局、围绕中心 bbox 放置上下标、数学斜体显示；显示变化不改写源码 |
| HTTP 服务 | [src/main.rs](src/main.rs) | 提供静态页面、WASM 和本地 API，分发补全、文档渲染与附件位置请求 |
| 原生服务桥接 | [src/services.rs](src/services.rs) | 管理 Tinymist LSP 和原生适配器进程，处理请求协议、超时、错误与进程复用 |
| Typst 原生适配器 | [native-adapter/src/main.rs](native-adapter/src/main.rs) | 提供内存 World；常驻模式接收文档渲染请求；单次模式从数学 IR 读取上下标位置 |
| 文档编译与 SVG 提取 | [native-adapter/src/render.rs](native-adapter/src/render.rs) | 校验源码区间，添加透明映射标签，编译整份文档，从对应 frame 提取 SVG、尺寸和基线 |
| 引擎桥接补丁 | [native-adapter/prepare.ps1](native-adapter/prepare.ps1)、[engine-patches.json](native-adapter/engine-patches.json)、[layout-entry.rs](native-adapter/layout-entry.rs) | 准备固定版本引擎，让 Raw 标签保留原生数学语义，在最终排版片段上保存映射；不使用隔离 box 或 stretch 专用生成路径 |
| 字符配置 | [config/symbols.json](config/symbols.json)、[build.rs](build.rs) | 构建时校验并嵌入“源码 → 显示字符”字典，供解析和字符显示使用 |
| 回归验证 | [tests/](tests/)、适配器内部测试 | 覆盖编辑、宏、多公式、源码映射、缓存刷新、字体和原生排版结果 |

详细的状态归属、模块调用关系、接口字段与修改入口见 [架构与数据流](docs/architecture.md)。键盘操作及宏示例见 [编辑与宏使用指南](docs/editing.md)。

## 三条主要数据流

```text
结构编辑：浏览器事件 → WASM / Document → Editor → View → DOM 与光标测量
Raw SVG：完整源码与 Raw 区间 → /api/render → 常驻 Typst 编译 → 映射 frame → SVG 缓存
辅助查询：命令草稿 → /api/completion → Tinymist
          上下标分支 → /api/attachments → Typst 数学 IR → 槽位位置
```

Raw 的 SVG 是从当前文档排版结果中提取的，包含上下文和附件对它的影响。透明标签允许 `stretch` 自然参与原生排版；上下标位置查询只返回位置，不生成中心 SVG。

## 当前行为与边界

- 用按钮插入行内／行间公式，没有模式切换按钮。上下文中直接输入的公式保持源码形式；显式导入时才转换文档的顶层公式。
- 结构公式保存编辑树，可展宏保存调用和一份实参，仅在视图中展开。无法结构化的表达式保留为 Raw。
- 成功的 Raw SVG 按源码文本缓存，同文共享。普通输入、上下文变化不自动使已有 SVG 失效；允许编辑器预览暂时与实际排版不同。
- “更新全部 SVG”刷新所有 Raw 和附件位置缓存。编辑上下标后离开附件区域或输入失焦，会刷新中心项内的 Raw；仅移动光标不会触发。
- 浏览器负责结构布局，整式不要求与 Typst 最终页面像素一致。原生适配器目前仅提供内存主文档及随附数学字体，不提供文件导入、包加载或日期。
- 附件位置适配目前针对视图提供查询源码的顶层 Script；嵌套分支保留默认编辑布局。详见 [附件布局适配](docs/operator-limits.md)。

## 运行与构建

在 `prototypes/lyx-rust` 目录执行：

```powershell
.\start.cmd
```

浏览器打开 http://localhost:4320 。启动脚本会在 WASM 缺失时构建它，然后准备并构建原生适配器，再启动本地服务。已有 WASM 不会因为核心源码变化而自动重编。

| 修改内容 | 操作 |
|---|---|
| Rust 编辑核心、解析器、视图或字符字典 | `.\build.cmd` 更新 WASM，再刷新页面 |
| 前端 JS／HTML／CSS | 刷新页面，必要时 Ctrl+F5 |
| 原生适配器、引擎补丁或 HTTP 服务 | 停止旧服务，重新运行 `.\start.cmd` |
| 全套 release 产物 | `.\build-release.cmd`，成功后 `.\start-release.cmd` 启动 |

构建需要 Rust 工具链、`wasm32-unknown-unknown` 目标和 Git；首次准备依赖及固定引擎需要网络。共享引擎放在 `prototypes/typst-engine`，固定 revision 为 `59b5999da8e74e74583069408d2564fc1f9bc973`。

Tinymist 仅用于补全，自动从 PATH 或 VS Code／Cursor 扩展中查找，也可通过 `TINYMIST_BIN` 指定。找不到时保留内置命令补全；Raw 渲染使用独立的原生适配器。

## 文档与验证入口

- [架构与数据流](docs/architecture.md)：各模块如何合作、状态放在哪里、修改功能应看哪些文件。
- [编辑与宏使用指南](docs/editing.md)：键盘、导入导出、宏管理、缓存与刷新规则。
- [附件布局适配](docs/operator-limits.md)：limits／scripts、中心 bbox、透明映射及 stretch 回归。
- [字符显示配置](config/README.md)：修改符号字典。
- [历史会议纪要](会议纪要.md)：早期单公式阶段的决策记录，当前实现以本文和架构说明为准。

常用检查如下；具体覆盖范围与本机依赖见架构说明中的测试表。这些命令是维护入口，不代表每次文档更新都执行了全部测试。

```powershell
cargo test --offline --locked
cargo test --offline --locked --manifest-path native-adapter/Cargo.toml --target-dir target/adapter
node tests/wasm-command.mjs
node tests/preview-cache.mjs
node tests/math-font.mjs
node --check web/app.js
```

字体文件和许可见 `web/fonts/NOTICE`；LyX 源码快照、作者和项目许可见 [upstream/](upstream/) 与 [COPYING](COPYING)。
