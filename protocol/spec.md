# 文档协议（草案）

第 3 步的产物：**接口规则的可执行形态**。设计理由见 `TODO/重构前后端职责划分.typ` 第 2 节；这里只写协议本身。

- 机器可读的定义：[`schema.json`](schema.json)（动词、类型、事件、能力表、枚举、fixture 约定、禁用名字）。
- 合成数据：[`fixtures/`](fixtures/) 下的 JSONL，每个文件一段对话。
- 校验：`python tools/check_protocol.py`——检查 schema 自洽、fixture 合法、能力表覆盖 fixture 用到的每一个名字、以及旧协议的名字没有漏进来。**现在就能跑**（不需要构建后端）。
- 还没有实现：fixture 是*先写*的合成数据（第 3 步的"最小前端 + 合成数据"）。等最小后端出现后，同一种 fixture 由回放器逐条发送、逐条比对 `expect`（`$result…` 引用按实际回复解析），fixture 本身不改。

## 一、传输与消息

一条管道，UTF-8，一行一个 JSON 对象。三种行：

```text
请求   {"id": 1, "verb": "edit", "target": [0, 12], "intent": {...}, "session": {...}}
回复   {"id": 1, "result": {...}}   或   {"id": 1, "error": {"verb": "edit", "message": "...", "detail": "..."}}
事件   {"event": "render", "revision": 7, "images": {...}, "glyphs": {...}}
```

- `id` 由前端给，单调递增；回复按 `id` 匹配，**可以乱序**（后端在工作线程里作答）。
- 事件没有 `id`，带 `revision`；前端丢弃 `revision` 小于当前的事件。
- 上限：单条请求 8 MiB，单条回复 64 MiB（沿用今天 `desktop.rs` 与 `services.rs` 的量级）。

## 二、规则

| | 规则 | 落法 |
| --- | --- | --- |
| R1 | 无会话：后端不保存当前公式、光标、草稿、历史 | 光标与选区随请求传（`session`）；同一请求在同一源码上必得同一回复 |
| R2 | 请求自带上下文，回复自带新源码 | `edit` 的回复一定带 `source`（未变时 `source_changed: false`），以及**变化过的公式条目**（`changed`） |
| R3 | 只有 `edit` 会改变文档 | 其余九个动词都是纯查询，可以随时重发 |
| R4 | 位置一律 UTF-8 字节偏移 | Qt 的 UTF-16 由前端换算；LSP 的 UTF-16 由后端换算（`lsp` 的回复已经是字节） |
| R5 | 名字契约版本化 | `hello` 给出能力表；前端启动时对账，不认识的名字**报错**（今天只警告一次就按横排画） |
| R6 | 回复与事件同一条协议 | 取图与整页编译在工作线程完成后以事件回包，前端不阻塞等编译 |
| R7 | 失败是数据 | `images` 的值可以是错误对象，也可以带上引擎消息；`failed`、`batch_error` 与结果并列 |
| R8 | 错误带动作名与原因 | `error.verb` 就是失败的那个动作，`message` 沿用今天的措辞；前端显示在消息面板并保留源码 |

## 三、公式的身份：`id` 与 `view_version`

每条公式数据带两个字段，它们是**前端唯一需要的失效信息**：

- `id`：公式的身份，跨编辑稳定（正文里插入文字不会换，公式被删掉才消失）。今天这份身份由前端造（`window._object_id`）。
- `view_version`：视图内容变了才 +1。**区间平移不算变化**。

于是"正文编辑之后哪些投影要重建、哪些区间要平移"这件事只剩一句话：*按 `id` 对上，`view_version` 不同就重建，区间取新的*。今天这一整套在 `incremental.py::merge` 里（重解析范围、公式源码比较、`let` 绑定文本比较、`#[]{}` 保守失效），迁移 A 之后整段删除。

`edit` 的 `changed` 与 `removed` 因此是**公式条目**与**身份**，不是区间：区间会平移，身份不会。

## 四、动词

| 动词 | 请求 | 回复 | 语义与注意 |
| --- | --- | --- | --- |
| `hello` | `protocol` | 能力表 | 握手；版本不符即报错 |
| `set_source` | `source` | `revision`、`source` | 换文档（打开文件）。不带分析 |
| `analyze` | `revision?`、`ranges?` | `revision`、`source`、`formulas`、`styles`、`definition_blocks` | 整份公式表（每次都是完整的）。`formulas[i]` 带 `id`、`view_version`、`range`、`display`、`editable`、`reason?`、`context?`、`entry`、`view` |
| `edit` | `target`、`intent`、`session?` | `edit_result` | 唯一会改文档的动词。回复带 `source`、`source_changed`、`styles`、`definition_blocks`（今天的 `edit_source` 加 `scan` 合成一次往返）、`changed: [公式]`、`removed: [id]`、`cursor?`、`message` |
| `navigate` | `target`、`session`、`op` | `cursor`、`anchor`、`selected_source`、`left?` | 只动光标。上下左右的"走到哪一格"由后端按形状规则算；`left` 说明光标离开了公式、从哪一边出去（`before` / `after`）——今天这一步由前端比较前后光标得出 |
| `command` | `target`、`draft`、`op` | `candidates`、`message` | 草稿留在前端；这里只问候选。确认草稿走 `edit` 的 `insert_command` |
| `visible` | `revision`、`ranges`、`retry?` | `accepted`、`revision` | "这些公式在视口里"。后端合并成一次编译，结果以 `render` 事件回；`retry` 明说"这几个 id 请再问一次引擎"（滚动造成的重复订阅不该重编译） |
| `export` | `kind`、`overlays?` | `pages` 或 `pdf` | `svg` 给每页 SVG 与 hash，`pdf` 给 base64。落盘仍在前端 |
| `lsp` | `method`、`revision`、`position?`、`query?` | `lsp_result` | 诊断、补全、悬停、定义、格式化、语义 token；位置与区间都是字节 |
| `packages` | `action`、`query?`、`spec?` | 条目或安装结果 | 沿用今天的包管理 |

`edit` 的意图分两族（取值写在 `schema.json` 的 `enums` 里）：

- 文档级：`replace_source`（正文按键后的区间替换）、`insert_source`（导入文件内容，替代旧动作 `import`）
- 公式级（必须带 `session`）：`insert_text`、`backspace`、`delete`、`paste`、`insert_command`、`insert_symbol`、`grow_grid` / `shrink_grid`（`axis` 为 `row` / `column`）、`edit_raw`（双击片段改源码）、`set_display`、`select_all`

`navigate` 的 `op.kind`：`move`（`direction`、`word`）、`set_cursor`（点击或拖选，光标取自被点中的视图节点自己的 `cursor` 字段）、`home`、`end`、`cell_start`、`cell_end`、`entry`、`cell_index`、`select_all`。

## 五、事件

| 事件 | 载荷 | 语义 |
| --- | --- | --- |
| `render` | `revision`、`images`、`glyphs`、`placements`、`failed`、`batch_error?`、`dropped?` | 一次编译的全部结果。键就是视图节点上的 `image_id` / `glyph_id` / `attachment_id`——**前端不再自己发明键**（今天 `rawcache.raw_key` 干的就是这件事） |
| `diagnostics` | `revision`、`diagnostics` | 可选：语言服务把诊断推给你。今天前端是 400 ms 防抖后主动问，两种都允许 |

`images[id]` 要么是 `{"svg", "width", "height", "baseline", "base_font_*", "environment_font_size_pt"}`，要么是 `{"error": "unknown variable: undefinedname"}`——**画不出来也是一种答案**，前端据此画虚线源码框并把原因挂在那个片段上。`glyphs[id]` 是替换后的字形串（`bold(A)` 到 `𝑨`）；`placements[id]` 是 `{"placement": "limits" | "scripts"}`。

**id 的稳定性是契约的一部分**（今天由前端用 `SequenceMatcher` 维持）：`image_id` 是片段身份的函数——源码文本、定义出处、调用实例、所在脚本的摘要——所以只是把片段挪动位置的编辑不换 id，内容或上下文变了才换。后端按 id 缓存 SVG，前端按 id 缓存位图。

## 六、能力表（`hello` 的回复）

`protocol`、`app`、`verbs`、`document_ops`、`formula_ops`、`nav_ops`、`view_kinds`、`roles`、`markers`、`metrics`、`limits`。

- `view_kinds`：后端能发出的线名，**19 项**：`char`、`symbol`、`number`、`raw`、`text`、`absent`、`stop`、`cell`、`empty-cell`、`fraction`、`decorated`、`root`、`scripts`、`table`、`multiline`、`style`、`macro`、`macro-argument`、`raw_macro`。今天前端的白名单（`mathview.ARRANGEMENTS`）有 23 项，多出的 `unknown` 与 `draft-text` / `draft-placeholder` / `draft-caret` 是命令草稿的线名——草稿搬到前端之后它们由前端自造，所以不在后端能发的集合里（见第八节）。
- `roles`：`slots::Role::name` 的十个：`base`、`upper`、`lower`、`numerator`、`denominator`、`radicand`、`index`、`inner`、`cell`、`arg`。
- `markers`：按线名分组。`fraction`：`-`；`decorated`：`radical`、`delim`、`overline`、`underline`，加上 `config/commands.json` 里 `decoration` 形状的命令名（今天是 `hat`、`cancel`）。
- `metrics`：引擎量出来的常量——`script_up_em`（0.36）、`script_down_em`（0.25），以及按形状与角色给的 `slot_scale_permille`（分式 900、根指数 550、脚标 700；其余 1000，来自内核 `Slot::scale`——它今天声明了却没人读，迁移 D 开始下发）。
- `limits`：消息与投影上限（`projection_nodes` = 今天 `typst.rs` 的 4096）。

## 七、fixture 格式

`fixtures/*.jsonl`，一行一步。步骤键：`step`（文件内从 1 连续）、`note?`、`send?`、`expect?`、`paths?`、`recv?`、`expect_drop?`、`malformed?`。

```json
{"step": 1, "send": {"id": 1, "verb": "hello", "protocol": 1}, "expect": {"id": 1, "result": {"protocol": 1}}}
{"step": 2, "recv": {"event": "render", "revision": 7, "images": {}}, "expect_drop": true}
```

- `expect`：对上一次 `send` 的回复，**子集匹配**——只比对给出的键，对象递归，数组长度必须相等、元素逐个递归；值 `"?"` 表示"存在即可，不看值"（也可以作数组元素，于是 `["?"]` 就是"恰好一项，内容不管"）。
- `paths`：对上一次回复的深路径断言（`result.formulas[0].view.kind`），值是期望值（同样的子集规则）。校验器先按 schema 解析这个路径，路径不存在即失败——所以它顺带证明了"回复里真的有这个字段"。
- `$result…`：请求里的字符串若以它开头，表示引用上一次回复里的路径（`$result.changed[0].range`）。回放器按实际回复解析；fixture 因此不必去钉只有 `write_atom` 才能决定的数值，同时也记录了"前端用的是后端刚给它的东西"。
- `recv` / `expect_drop`：后端推的事件；`expect_drop` 要求它的 `revision` 比当前小（过期事件必须被丢弃）。
- `malformed`：这一步故意不合法（例如缺少 `session` 的公式级意图），跳过语义检查、只查形状。
- 校验器额外查：请求 `id` 单调、非丢弃事件的 `revision` 不比当前旧、`images` / `glyphs` / `placements` 的每个键都在更早的步骤里作为 `*_id` 出现过、`paths` 里断言的 `kind` / `role` / `marker` 都在能力表里、以及禁用名单（旧动词、旧路由、旧字段）没有出现。

## 八、还没定的事（迁移时再收口）

- `diagnostics` 事件是否取代前端 400 ms 防抖的主动询问（推送更省往返，但前端仍要防抖以避开每次按键）。
- `analyze` 的 `ranges` 区域分析是否第一期就做（现在只有全篇）。
- `export` 的 `pages` 与今天 `/api/preview` 的 `hash` 复用关系（今天前端用它做增量导出）。
- `unknown` 与三个 `draft-*` 线名搬到前端之后，是否要让前端把自造节点的名字也报备给后端（目前趋势是不报备：它们是纯显示节点，后端从不收）。
