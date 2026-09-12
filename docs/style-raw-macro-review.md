# style / raw_macro 实现审查

后续修复状态：以下八项均已处理。第 1、2 项及第 3 项的在途去重由“公式 View 复用与 style 空定义上下文”一轮完成；本轮完成失败缓存与局部重排、模板片段来源和调用实例匹配、参数光标保留、失败源码修复、活动调用内部取图，以及 UTF-8 诊断坐标。下文保留修复前的复现记录；当前行为见 [architecture.md](architecture.md)，后续实测见 [validation.md](validation.md)。下文的行号、缓存表和测试数量均属于修复前证据。

日期：2026-09-12。范围：审查当时的文档与源码、真实 release core / layout adapter，以及 Qt offscreen；未使用 computer use，未启动可见窗口。本次只梳理与复现，没有修改产品实现。

## 修复前的实现链路（历史）

这里的 `style` 是数学 View 的字体变体，不是 `analysis.styles` 中用于文档高亮的语法区间，也不是 Typst 的全部 set/show 规则。

两种 View 都可来自内核的 `Kind::MacroCall`。内核并没有两个独立的存储 Kind。

| 环节 | style | raw_macro |
| --- | --- | --- |
| 判定 | 配置命令是字体变体，且 `has_glyph_run` 判断主体为字符串 | 调用无法借用配置形状、无法结构展开、参数数量不匹配或展开超限等 |
| 示例 | `bold(x)`、`bold(upright(x))` | `bold(frac(a, b))`、`unknownfn(a)`、过大的用户宏 |
| View 内容 | 调用拼写 `text`、`style_name`、主体 children、可选 edit 光标 | 调用拼写 `text`、名称与参数槽 children、可选 edit 光标 |
| 引擎数据 | `/api/glyphs` → 适配器解析数学 IR → 字形串 | `Document::annotate` 定位 → `/api/render` → SVG |
| 缓存 | `(完整公式前缀, 调用拼写, display)` | `('raw', 调用拼写[, script 摘要])` |
| 未进入编辑 | 有字形串则画字形；没有则画名称和主体 | 有图则画图；没有则落入 children 排版 |
| 进入编辑 | `_active` 时画名称和主体 | `_active` 时画名称和参数槽 |
| 失败处理 | 请求失败移除缓存，没有明确失败状态；无取图区间，诊断不标记 | 有诊断 error 才走失败框；取图 False 没有同等分支 |

主要入口：`crates/core/src/math.rs::command_shape`、`typst.rs::has_glyph_run`、`view.rs::view_atom/view_configured/material`、`src/document.rs::annotate`、`desktop/window.py::load_glyphs/load_raw/stamp_draw`、`desktop/mathview.py::layout_node`。

## 已复现的问题

### 1. P1：字形请求使用不完整的文档前缀，合法 style 会被无关上下文阻断

位置：`desktop/window.py:616`、`native-adapter/src/main.rs:138–154`。

前端把公式前面的全部源码放进 `definitions`。适配器拼接 `definitions + 新公式`，先对整段文本 eval，再找最后一个 equation。这个前缀不一定是可独立执行的 Typst 文档，也不只是宏绑定。

真实适配器结果：

- `bold(x)`，空前缀：返回 `𝒙`。
- 同一表达式，前缀 `$unknownfn(a)$\n`：返回 `unknown variable: unknownfn`。
- 同一表达式，前缀 `#block[\n`：返回 `unclosed delimiter`。这正是完整文档 `#block[$bold(x)$]` 在公式起点截出的上下文形态。

因此“有时不解析”不等于 `has_glyph_run` 判断错。合法 style 会在后续取字形阶段失败。需要构造保留词法作用域且语法完整的求值上下文，或在真实文档中按节点获取结果，不能直接截断任意源码。

### 2. P2：普通正文进入 style 缓存 key，块外编辑必然重取

位置：`desktop/window.py:604–638`、`:666`。

复现 `Before $bold(x)$ After`：初次字形返回后，只把开头 `B` 改为 `b`。两次请求的表达式都为 `bold(x)`，前缀分别是 `Before `、`before `，但缓存 key 不同；新请求完成前 `_glyph=None`。

对照：在同一公式后面追加 `!`，新增字形请求数为 0。问题与编辑发生在公式之前有关，不是所有块外编辑都必然触发。

这同时导致多公式场景请求放大：前面的正文变化会影响后面每一个 style；相同表达式在不同前缀下也不能共用缓存。需要以实际依赖和环境身份缓存，而不是整段正文。有效绑定或 set/show 变化仍应按依赖失效，不能简单改成只按表达式缓存。

### 3. P2：style 没有真正的 pending / failed 状态，重复请求放大全局重排

位置：`desktop/window.py:624–635`、`desktop/bridge.py:229–236`。

`None` 被写作等待标记，但查询时 `get(key) is not None` 才跳过。因此等待同一结果时连续调用两次 `load_glyphs`，仍然发出两次相同 key 的 request。Services 只合并队列中的请求，不能消除已在执行的同 key 请求；正文变化产生的新 key 更不能合并。

失败会删除条目，下一次分析重试；没有按失败原因或依赖版本控制。每个回包无论成功失败都调用 `touch()` 和 `repaint_formulas()`，使所有公式的 Box 缓存失效。字形回调也没有文件代次检查，过期结果仍会触发重排。

需要区分未请求、等待、成功（包括合法空串）、失败；同 key 合并在途工作，过期回包不能影响当前文档的布局。

### 4. P1：宏模板中的 raw_macro 缺少来源，无法取图，text 还保留形参

位置：`crates/core/src/view.rs:520–535`、`:592–595`，`src/document.rs:241–255`。

复现：

```typst
#let wrap(x) = $bold(frac(#x, 2))$
$wrap(a)$
```

调用点投影产生 `raw_macro`，但 `text` 仍是 `bold(frac(#x, 2))`，`edit/source_range/origin/render_id` 全为空，`render.raw=[]`。

模板绑定只给 `raw` 写入定义来源，漏了 `raw_macro`。而在绑定后主体仍非字符串的路径中，计算出的 `spelled` 没有写回返回节点。`annotate` 既没有调用点光标也没有定义来源，无法定位，前端随后记为无图。

这与文档所述“宏内片段与普通片段共用定位取图机制”不一致。修复要同时解决来源与实例身份：不能把含形参的文本当成已绑定调用的源码，也不能假定不同实参共用同一张图。

### 5. P1：宏绑定重新生成 style 时丢失参数编辑光标

位置：`crates/core/src/view.rs:577–595`。

复现：

```typst
#let styled(x) = $bold(upright(#x))$
$styled(a)$
```

进入调用后按右键，core 的光标已经进入 `{atom:0, cell:0}`。但绑定后的两个 style 都没有 edit，排版只有根级两个 stop，活动 stop 数为 0。

原因是 `spelled` 将实参 View 转成文本，随后重新 parse，再通过 `view_cell(&data, None, ...)` 投影。原先携带参数编辑位置的 `macro-argument` 被丢掉了。它修好了“可以判定成 style”，却破坏了编辑位置。需要保留绑定实参的 View/光标身份，不能通过无会话文本重解析替换它们。

### 6. P2：raw_macro 取图失败未进入失败源码框

位置：`desktop/mathview.py:322–345`。

对 `$unknownfn(a)$` 真实 core 产生的 `raw_macro`，向图片缓存写入正式失败值 `False`（与 `/api/render` 失败回调一致），排版中没有 `failed` 绘制操作，而是名称 `unknownfn(`、数学实参和 `)`。

相邻的 `raw` 分支明确检查 `False` 并调用 `failed_box`；`raw_macro` 只尝试 image_box，没有失败分支。仅当另一条 LSP 路径给它写入 error 时才显示失败框。因此取图失败与 LSP 是否及时诊断，会造成同类错误的外观不同。

需先明确待渲染、编译失败、正在编辑三种状态的显示与修复入口。不能仅增加错误颜色后就认为源码修复完整：`report_raw_fragments` 和内核失败片段入口目前仅服务 `raw`。

### 7. P2：进入 raw_macro 后，内部 Raw 仍然无法请求自己的图片

位置：`src/document.rs:238–264`、`desktop/window.py:707–726`。

复现 `$bold(arrow.r)$`，进入参数槽后，内部 `arrow.r` 没有 render_id；`raw_fragments` 仍只返回 `bold(arrow.r)`。

后端 annotate 无条件跳过任意 raw_macro 内的定位；前端遍历也无条件设置 inside，不看活动状态。光标进入时绘制切成参数槽，但取图策略没有跟着切换。若相同内部文本恰好从别处已有缓存，就可能显示；否则一直显示源码。

文档写的是“进入前不单独取图”，实际实现却是始终不取图。应分离源码定位信息与本轮不重叠的取图选择：保持内部节点可定位，在外部整图和内部参数图之间按编辑状态选择，避免同时请求重叠范围。

### 8. P1：中文诊断区间混用字符与字节，失败回退可能直接中断

位置：`desktop/window.py:820–834`，`:1198–1201`。

`lsp_position` 已经返回 Python 字符索引；`mark_diagnostics` 却再传给 `from_byte`，把它解释为 UTF-8 字节。之后又拿结果与以字节存储的 render_id 比较。

复现 `中文 $unknownfn(a)$`，给 unknownfn 提供正确 LSP 诊断区间，直接抛出 `UnicodeDecodeError`。更长中文前缀同样复现。ASCII 测试会掩盖错误，因为字符索引与字节索引恰好相同。

应统一在文档字节坐标比较（对 `lsp_position` 的结果使用 `to_byte`），并补中文、非 BMP 字符、多行诊断用例。诊断目前仅写入 analysis View，活动会话 View 的一致性也需要纳入修复验证。

## 文档和测试的盲区

- `docs/desktop.md:54` 描述进入后可为内部片段取图，实际两端都始终跳过。
- `docs/editing-model.md:215` 描述 style 的实例侧绑定已经修复，但当前只保证显示形状，未保留参数光标。
- `native-adapter/src/main.rs` 的 collect_glyphs 注释声称拒绝后调用者回退整图，实际 style 请求失败没有转为 SVG 请求。
- 桌面字形测试大多通过同步 mock 返回字符，异步测试覆盖成功晚到，未覆盖正文变更、失败前缀、内容块上下文和在途去重。
- 诊断测试使用 ASCII 前缀；collapsed-call 测试覆盖外部整图和内部参数槽切换，未验证内部 Raw 能取图，也未验证失败框。

现有完整桌面测试仍然 **97 项通过（47.287 秒）**。这不否定以上缺陷，说明当前回归用例缺少这些交叉路径。

## 修复顺序建议

1. 修复诊断坐标异常，以及模板来源/参数光标丢失，先保证错误可见、编辑位置可靠。
2. 明确 style 字形请求的有效上下文与缓存身份，加入独立 pending/failed 状态，消除正文编辑导致的重取和无效回包重排。
3. 统一 raw_macro 失败回退，并在进入参数时切换内部取图集合。
4. 将上述最小复现变成回归测试，再更新架构说明中与实际不一致的承诺。

本次诊断脚本与机器结果保存在 `target/style_raw_audit.py`、`target/style_raw_audit.json`（构建目录，不作为正式测试提交）。其中图片失败以真实前端缓存状态注入，字形上下文失败调用真实 layout adapter，其余核心 View 由真实 release core 生成。
