# 正式版架构

## 状态所有权

| 层 | 保存什么 | 不保存什么 |
| --- | --- | --- |
| CodeMirror EditorState | 完整源码、选区、每文件撤销历史 | 每公式 Editor |
| Rust Document | 完整源码镜像、活动公式字节区间、唯一一个 Editor | contexts / Vec<Editor> |
| Rust Editor | 活动公式的 MathData、槽位光标、草稿和测量 | 其他公式的状态 |
| Decoration / Widget | 区间和静态显示；活动控件复用同一个 DOM host | 文档权威状态 |
| 本地 Services | Tinymist 文档会话、独立公式补全、常驻 Typst 渲染器 | 正文编辑状态 |

正常代码输入由 CodeMirror 事务提交，WASM `set_source` 无损保存源码并用 Typst AST 扫描公式区间。扫描忽略注释、字符串和 Raw 中的 `$`。源码有错也允许保留。按钮调用 `activate_formula(start)`，只解析活动范围并读取前缀中的宏定义。

结构修改通过唯一 Editor 执行；若序列化结果实际改变，Document 仅替换活动范围。JS 将这次替换提交为 CodeMirror 事务，标记为公式来源，避免再次 `set_source` 清空活动树。源码进入和退出不做隐式格式化。撤销和重做统一回到 CodeMirror，随后重建源码镜像。隐藏公式只保存 DOM 投影，不保存单独的光标和撤销栈。

Rust / Typst 字节区间使用 UTF-8；CodeMirror 字符位置与 LSP character 使用 UTF-16。所有跨层转换集中于 `web/source.js` 和 `src/services.rs`。LSP 返回替换范围后检查边界与重叠；异步请求返回时核对源码和文件身份，避免过期结果写入当前文档。

## 后端

本地 HTTP 只绑定回环地址，并检查 Host、Origin 和 POST JSON 类型。静态资源固定白名单。项目目录由 `VISUAL_TYPST_WORKSPACE` 指定，默认 `workspace/`；文件接口使用规范化路径限制项目边界，保存时对照打开时的磁盘内容，避免静默覆盖外部修改。

`/api/lsp` 提供 completion / hover / definition / formatting / diagnostics。一个文档使用一个常驻 Tinymist 进程，版本递增并全文同步；换文件或协议失败时重建。诊断来自 publishDiagnostics，前端防抖、版本检查和过期结果丢弃。公式补全仍使用隔离的临时源码投影，以保留原型已验证的命令行为。

`/api/packages` 从官方索引查版本，安装精确版本到标准缓存。下载有超时与大小限制；包解压仅接收普通文件和目录，拒绝链接、越界和过大归档。先解压到临时目录并校验 manifest 存在，再重命名发布缓存，避免半安装状态。

`/api/render` 和 `/api/attachments` 使用正式版自己的 native-adapter。数学 IR 标签桥接保存在 `vendor/typst`；构建不再准备原型引擎或修改其他目录。源码、资源与缓存失败不影响代码区继续输入。

## 当前边界

- 公式静态投影只在按钮进入并编辑后缓存；源码输入 `$` 不自动变成控件。
- 宏的结构展开仍沿用受限静态分析，任意 Typst 求值由引擎处理。
- Raw SVG / 附件位置沿用按需刷新策略；不保证任何源码修改后都自动刷新全部数学投影。
- 代码高亮为轻量 Typst token 高亮，诊断和语义查询由 Tinymist 提供。
- F12 目前可打开项目内目标；项目外依赖目标显示位置提示。
- 暂无整页预览/PDF 导出、目录创建/重命名、包卸载或插件系统。
