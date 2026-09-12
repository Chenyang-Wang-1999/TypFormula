# `Kind` 能力清单

这张表回答一个问题：**每个 `Kind` 现在能给前端提供什么信息，以及它对应的 Typst 引擎 item 还能提供什么而编辑器没有。**

三列信息来源不同，读的时候要分开：

- **存储 / 声明** 是代码事实，来自 `crates/core/src/math.rs` 的 `Kind` 与 `crates/core/src/slots.rs` 的 `Decl`（唯一一张表，穷尽 `match`）。
- **线上实测** 是从真实 release 后端取回来的，不是读代码推的：`tools/kind_inventory.py` 驱动 `typformula.exe --desktop-core`，按行发 `{"action":…}` JSON，`set_source` → `activate_formula` → 需要时再发 `input`，然后取 `state` 回来的 `view`。每个 Kind 至少一个能真正产生它的源文（宏那几项见下面的"不能从源码到达的两项"）。
- **引擎** 一栏来自 `vendor/typst/crates/typst-library/src/math/ir/item.rs`，是编译器自己的 item 形状；表里引用的盒子由 `tools/engine_boxes.py` 用真实适配器量出。

`class` 单独说：只有 `Fraction` 与 `Table` 在表里写死为 `7`，`Multiline` 与其余都是 `0`，而 `Char` 的类是**由字符本身决定**的（`slots::char_class`），表里那一格永远不会被读。

## 一、主表

「视图名」这一列是 **`Shape::view`，也就是*形状*名**——它是 `config/commands.json` 写的那个名字。**它不一定是线上的名字**：`view_atom` 可以把几个形状合并成一个线 kind，八处形状名与线名不同——`sqrt`/`delim`/`decoration`/`line` 都走 `decorated`（靠 `marker` 区分画法）：

| 形状名（`Shape::view`） | 线名（`view_atom`） |
| --- | --- |
| `sqrt`、`delim`、`decoration`、`line` | `decorated` |
| `script` | `scripts` |
| `grid` | `table` |
| `aligned` | `multiline` |
| `macro`（折叠时） | `raw_macro` |

**下表的每一行都是一个真的会被存进树的 `Kind`。** 形状比 `Kind` 多：`sqrt`/`root`/`delim`/`line`/`decoration`/`style` 这六个只有 `Shape` 而没有对应的 `Kind`——`sqrt(x)`、`hat(x)`、`bold(x)`、`abs(x)` 这些**调用形式**一律存成 `MacroCall`（形状由名字查出来），`√x`/`∛x` 也折进同一个 `MacroCall`。它们的槽位、导航、排布名与画法分别在 `Shape` 表和 `config/commands.json` 里（取图数据如 `abs` 的定界符对、`overline` 在上还是在下，就写在配置里）。

`Sqrt`/`Root`/`Accent`/`Line`/`Style` 五个变体**曾经留在 `Kind` 里**当"形状描述符"，现已被删除；判据见 `docs/architecture.md`：一个构造要是自己的 `Kind`，得带着名字与配置都给不出的**实例数据**（`Table.columns`、`Fenced.left/right`、`Multiline.row_lengths`、`Raw.source`……），或者要保住书写形式。`Fraction` 与 `Fenced` 都满足这条，所以它们真的被存下来（`frac(a, b)` 与语法写法 `a/b` 都存 `Kind::Fraction`）。

借形状的命令的线上实测，按形状列在这里：

| 命令 | 借的形状 | 线上排布 | marker |
| --- | --- | --- | --- |
| `frac` | `fraction` | `fraction` | `-` |
| `sqrt` | `sqrt` | `decorated` | `radical` |
| `root` | `root` | `root` | `radical` |
| `abs` / `norm` | `delim` | `decorated` | `delim` |
| `hat` / `cancel` | `decoration` | `decorated` | 命令名本身 |
| `overline` / `underline` | `line` | `decorated` | `overline` / `underline` |
| `bold` / `upright` | `style` | `style` | 无（用 `style_name`） |
| `mat` / `vec` / `cases` | `grid` | `table` | 无（见 `Table` 行） |

| `Kind` | 存储字段 | 槽位（role·scale·可空） | 入口（前进 → / 后退 ←） | 左右 | 上下 | 形状名 | 回写 | 线上实测 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `Char` | `text`（**一个字形簇**） | 无（叶子） | 边界 | 线性 | 无 | `char` | 自己的 `text` | `text`=字符；编辑器里打的 `-` 另带 `display_glyph`=`−`(U+2212) |
| `Symbol` | `name`, `glyph` | 无 | 边界 | 线性 | 无 | `symbol` | 自己的 `name` | `text`=`glyph`（如 `alpha` → `𝛼`） |
| `Number` | 无 | `inner`(100%) | 边界 | 线性 | 无 | `number` | 格内字符**不加分隔符**（`Write::Run`） | children 1×`cell@inner`，里面逐字是 `char`（`12.5` 是 4 个）；光标因此能停在数字之间 |
| `Raw` | `source` | 无 | 边界 | 线性 | 无 | `raw` | 自己的 `source` | `text`=源码片段 + **`edit`**（一个真实光标，前端据此给"打开源码"）；在宏模板里另带 `definitions`/`origin`/`source_range` |
| `Unknown` | `name`, `saved`, `caret`, `anchor`, `original` | 无 | 边界 | 线性 | 无 | `unknown` | 自己的 `name` | `text`=空串，内容全在 children：`draft-placeholder`/`draft-text`/`draft-caret`，**都不带 role** |
| `Parameter` | `index` | 无 | 边界 | 线性 | 无 | `parameter` | 占位 `#parameter{n}` | **不上线**（见第五节） |
| `Text` | 无 | `inner`(100%) | 边界 | 线性 | 无 | `text` | 带引号的字符串 | children 1×`cell@inner`；`text` 为空串 |
| `MacroCall` | `name`, `function` | `arg`(100%)，可重复 | 边界（首/末格） | 线性 | 无 | `macro` / `raw_macro` | 具名调用或裸名字 | `text`=宏名 + children 1×模板视图；子节点 `macro-argument` 的 `text`=形参名。折叠时 `text`=**调用本身的拼写**（用来取这一段的图），children=`symbol("名(")`、参数 `cell`、`symbol(")")` |
| `TemplateCall` | `definition` | `arg`，可重复 | 边界 | 线性 | 无 | `template-call` | 模板专用（写入时 `unreachable!`） | **不上线**（见第五节） |
| `Fraction` | 无 | `numerator`(90%)、`denominator`(90%) | 分子 / 分母 | **锁定** | 互换，落格首 | `fraction` | `frac({0}, {1})` | children 2×`cell@numerator`/`cell@denominator` |
| `Scripts` | 无 | `base`(100%)、`upper`(70%,可空)、`lower`(70%,可空) | `base` / `base` | **锁定** | 附件专用 | `script` | 自己拼 `^(…)`/`_(…)` | 线名是 **`scripts`**；**`attachment`**（仅顶层根分支）+ children 3：`cell@base`、`cell@upper`、`cell@lower`；缺席的脚标是 `absent`，**也带同一个 role** |
| `Fenced` | `left`, `right` | `inner`(100%) | 边界 | 线性 | 无 | `delim` | `abs()`/`norm()`/字面定界符 | 线上是 `decorated`/`marker="delim"`，字符仍在 `text`（`"左\n右"`）+ children 1×`cell@inner` |
| `Table` | `columns`, `row_lengths`, `name` | `cell`(100%)，可重复 | 中行首/末格 | 列内 | 列运算 | `grid` | `name(…)`：`mat` 行用 `;`，`vec`/`cases` 一个参数一行 | 线名是 **`table`**；`columns` + `row_lengths` + `N×cell@cell`，另带 `border`/`is_mat`（由 `name` 查配置得到） |
| `Multiline` | `columns`, `row_lengths` | `cell`，可重复 | 边界 | 列内 | 列运算 | `aligned` | 行用 `&`、`\` | 线名是 **`multiline`**；`columns` + `row_lengths` + N×`cell@cell`（`row_lengths` 两个 kind 都会上线，前端据此跳过补齐格） |

## 二、线上 `View` 的字段，谁填了什么

`View` 的字段（`crates/core/src/view.rs`）。**没有一个 Kind 填满过**，所以"某个 `Kind` 能提供什么"实际上是逐字段看：

| 字段 | 谁填 | 实测值示例 |
| --- | --- | --- |
| `kind` | 全部 | 视图名（**线名**，可以与*形状名*不同：`sqrt`/`delim`/`decoration`/`line` 都是 `decorated`，`script`/`grid`/`aligned` 分别是 `scripts`/`table`/`multiline`，`macro` 折叠时是 `raw_macro`） |
| `text` | 每个节点都有这个字段，但**带内容的只有** `Char`/`Symbol`/`Number`/`Raw`/`raw_macro`/`Fenced`/`decorated`/`style`；`Unknown` 与所有结构性 `Kind` 都是空串 | `hat`、`(\n)`、`x`、`12.5`、`layer15(a)`、`bold(upright(a))` |
| `role` | 父节点填给子节点（`Shape::role_at`）；根节点不填 | `numerator`、`radicand`、`inner` |
| `display_glyph` | **只有 `Char`**，且只有这个字符在 `config/symbols.json` 里时 | 打的 `-` → `−` |
| `children` | 除叶子外全部；`Scripts` 恒 3 个（缺席补 `absent`） | — |
| `cursor` | 每个 `stop` 节点（插入位），不在 Kind 节点上 | — |
| `active` / `selected` | 全部，由光标与选区决定 | — |
| `columns` | `Table`(列数)、`Multiline`(列数)、`TemplateCall`(定义序号)、`Parameter`(参数序号)、`macro-argument`(参数序号) | `2` |
| `row_lengths` | `Table` 与 `Multiline`：**补齐前**每行真正有几格。前端据此**跳过补齐格**，否则会画出源码里没有的空槽 | `mat(a, b; c)` → `[2, 1]` |
| `marker` | 画法靠名字而不是靠形状的节点：`decorated`（`radical`/`delim`/`hat`/`overline`/`underline`/`cancel`）与 `root` | `radical` |
| `style_name` | **只有 `style`**：命令名。前端画"光标进入后显示的名字"用它，而**取字用的是 `text`**（整段调用拼写），两者不是一回事 | `bold` |
| `border` | 表格的定界符，左+右或单边一个 | `mat`/`vec` → `()`；`cases` → `{ ` |
| `is_mat` | 表格按 `mat` 的方式切行（而不是一个参数一行） | `mat` true，`vec`/`cases` false |
| `edit` | **`Raw`、`raw_macro` 与 `style`**。前两者靠"源码 + 光标"在文档里定位；`style` **不取图**，它的 `edit` 只对前端有意义，因此**没有** `source_range` | `{slices:[], pos:0, occurrence:"root.a0.edit"}` |
| `attachment` | **只有顶层 `Scripts`**，值是它的 Typst 拼写 | `x^(2)` |
| `definitions` / `origin` / `source_range` | **只有位于宏模板里的 `Raw`** | 定义上下文 / 定义源码 / 定义中的区间 |

## 三、引擎侧：对应 item 与它的字段

| `Kind` | `MathKind` | 引擎 item 的字段 |
| --- | --- | --- |
| `Char` | `Glyph` | `GlyphItem`：`text`（**恰好一个字形簇**，构造函数里有 `assert`）、`class`、`stretch`、`mid_stretched`、`flac` |
| `Symbol` | `Glyph` | 同上 |
| `Number` | `Number` | `NumberItem`：`text`（数字串；引擎在 `resolve_text` 里按"全 ASCII 数字、至多一个点、至少一个数字"判定） |
| `Raw` | `Box`、`Mathml`、`External` | `BoxItem`：`elem`、`locator`；`MathmlItem`：`elem`、`body`；`ExternalItem`：`content`、`locator` |
| `Text` | `Text` | `TextItem`：`text`、`locator` |
| `Fraction` | `Fraction` | `FractionItem`：`numerator`、`denominator`、`line`、`padding` |
| `Sqrt` / `Root` | `Radical` | `RadicalItem`：`radicand`、`index: Option`、`sqrt`（根号字形本身） |
| `Scripts` | `Scripts` | `ScriptsItem`：`base`、`top`、`bottom`、`top_left`、`bottom_left`、`top_right`、`bottom_right` |
| `Fenced` | `Fenced` | `FencedItem`：`open: Option<MathItem>`、`close: Option<MathItem>`、`body`、`balanced` |
| `Table` | `Table` | `TableItem`：`cells`、`gap`、`augment`、`align`、`alternator` |
| `Multiline` | `Multiline` | `MultilineItem`：`rows: Vec<AlignedRow>`、`centered`；`AlignedRow` 是一行的各对齐列 |
| `Accent` | `Accent` | `AccentItem`：`base`、`accent: MathItem`、`position`、`dotless`、`exact_frame_width` |
| `Line` | `Line` | `LineItem`：`base`、`position`（没有记号，只有位置，所以编辑器也只存位置） |
| `Style` | `Glyph` | 同 `Char`：变体在 `resolve` 里就替换成码位了，引擎这一侧**没有"变体"这个 item**——`GlyphItem.text` 直接是 `𝐚`，适配器读回来的就是它 |
| `MacroCall`/`TemplateCall`/`Parameter`/`Unknown` | — | 编辑器专有，没有对应 item |

另外每个 item 还都挂着一份 `MathProperties`：`class`、`size`、`cramped`、`limits`、`lspace`/`rspace`、`ignorant`、`spaced`、`align_form_infix`、`editor_label`、`span`。编辑器这边只搬了 `class`（进 `Decl`）；`limits`（居中极限还是侧挂脚标）是**单独去问**的（`native-adapter` 的 attachments 服务），其余都没有对应物。

## 四、缺口：引擎有、编辑器没有

| `Kind` | 引擎有而我们没有 | 后果 |
| --- | --- | --- |
| `Accent` | `position` 由**记号字符自己**决定（`Accent::is_bottom` 用 ICU 的 `CanonicalCombiningClass::Below`），编辑器不存 | 编辑器只存命令名；今天接受的 `hat` 与 `cancel` 都把记号画在上面，所以这个偏差暂时看不出来 |
| `Accent` | 记号本身是**已解析的 item**，并带 `dotless`（有帽字母去点的替换）与 `exact_frame_width` | 前端按名字手画记号（`hat` 两条线、`cancel` 一条对角线），拿不到 `accent_base_height`、拉伸量、`accent_attach` 这些字体度量 |
| `Accent`（`cancel`） | `CancelItem` 的 `length`、`stroke`、`angle`、`inverted`、`cross` | 编辑器只存正文与命令名，前端固定画"内容框的上升对角线 + 0.3em"；`inverted`/`cross`/`angle` 表达不了。实测 `cancel(x)` 与 `x` 的盒子完全相同，所以记号是盖在正文上而不是把盒子撑高 |
| `Sqrt`/`Root` | 引擎是一个 `Radical`，`index: Option` | 编辑器分成两个 `Kind`（这是**有意保留**的：两者插槽不同，合并反而对前端不友好） |
| `Scripts` | 6 个附件字段（`top`/`bottom` 是居中极限，`top_right`/`bottom_right` 是侧挂脚标，另有左侧两个） | 编辑器只有 3 格；左侧附件 `native-adapter` 明确报错"暂不支持左侧附件的槽位映射" |
| `Fenced` | 定界符是 `Option<MathItem>`（`cases` 只有一个） | `Fenced` 存的是 `left`/`right` **两个字符串**，所以单边定界符（右边为空）表达得了；这也是 `Kind::Fenced` 唯一超出"名字加配置"的实例数据 |
| `Fraction` | `line`（是否有分数线）与 `padding` | 编辑器无法表达"无横线分式" |
| `Table` | `gap`、`augment`、`align`、`alternator` | 只有列数（`columns`/`row_lengths`）；表内对齐（`align`）与增广线（`augment`）都没有。行方式与定界符来自 `config/commands.json` 里那个名字，不是引擎给的 |
| `Multiline` | `centered` | 没有 |
| 全部 | `MathProperties` 里的 `cramped`、`lspace`/`rspace`、`spaced`、`ignorant` | 间距类信息只有 `class` |

## 五、不能从源码到达的两项

`Kind::Parameter` 与 `Kind::TemplateCall` 只存在于**宏模板的内部树**（`MacroDefinition::template`），线上到不了：

- 解析宏定义体时 `ParseContext { template: true }`，`#x` 变成 `Parameter`、嵌套调用变成 `TemplateCall`；
- 但显示调用时 `view.rs` 的 `bind_template_inner` 会把 `parameter` 换成实参视图、把 `template-call` 换成被调宏的展开结果。

实测：32 个用例的完整 `view` JSON 里，`"kind": "parameter"` 与 `"kind": "template-call"` 出现 **0 次**。它们仍必须有 `Decl`（`template_size` 要遍历、`Write::TemplateOnly` 要在写入时停下），但**前端永远看不到它们**。

## 六、探测中发现的十二件事

1. **`vec` 不是 accent。** `resolve_vec`（`resolve.rs:1018`）把每个参数变成一行再套定界符（`VecElem` 定义在 `matrix.rs`，默认 `delim: DelimiterPair::PAREN`）——它是**列向量**，和 `mat` 同族。`VecElem` 自己的文档就写着："To typeset a symbol that represents a vector, `math.accent[arrow]` and `bold` are commonly used"（`matrix.rs:23-25`）。编辑器把 `vec` 建成 `Accent`，而前端曾经按名字给它画一个箭头——**前端那行是凭名字猜的**（来自本仓库第二个提交 `d15f35d 桌面端`，Rust 侧从来没有 `arrow` 这个 Kind 或命令）。实测：`arrow(x)` 才是引擎的 `Accent`（13.728×17.328pt，宽度不变），`vec(x)` 是 29.5584×23.904pt（定界符被拉伸），两者不是一回事；`vec(x)` 与 `mat(x)` 的**映射 SVG 逐字节相同**，`vec(x, y)` 与 `mat(x; y)` 也相同。箭头已删除。**`vec` 现在进表了**：它是 `grid` 形状的第二个名字（`{"shape":"grid","rows":"each","border":"()"}`），行方式与定界符写在名字旁边，写回仍是 `vec(…)` 而不是 `mat(…; …)`；`cases` 同理，只是定界符是单边 `{ `。同一个形状下的第三个名字是 `mat`，它按分号切行。
7. **`mat` 的默认定界符是圆括号，前端曾经一律画方括号（已修正）。** `MatElem::delim` 默认 `DelimiterPair::PAREN`（`matrix.rs:103`）。实测 `(mat(x))` 比 `mat(x)` 宽出正好一对定界符（48.2304 − 29.5584 = 18.672pt，与 `(x)` − `x` 相同），说明 `mat` 自己那对确实画着圆括号。原先 `mathview.py` 的 `grid` 分支画的是两条竖线加四个短横（方括号），因为它拿不到定界符；现在 `View` 带 `border: "()"` 上线，前端按它画，两边一致。
8. **一条被证伪的怀疑。** 我一度以为映射会漏掉 `vec` 的定界符（因为 `vec(x)` 与 `mat(x)` 完全一样）。查下来不是：`mat` 的默认定界符本来就是圆括号，所以 `mat(x)` 与 `mat(x, delim: "(")` 是同一个东西，两者相同是必然的。当时留下的疑问——`Write::Matrix` 写出的 `mat(…)` 在前端是方括号、在文档里是圆括号——随 `border` 上线一并解决。
9. **数字串是一个"像 text 一样的容器"。** `Number` 有一个格，里面是逐字的 `Char`，所以光标**能停在数字之间**（`12|34` 插一个 `9` 得到 `12934`），这是叶子模型根本表达不了的。前端仍然只多一个排布名（`number`），通用"有子节点"分支会把那个格画出来；名字必须在 `ARRANGEMENTS` 白名单里，否则整篇带数字的公式都会报"不认识的排布"（`desktop/test_desktop.py::test_a_number_run_is_a_container_with_a_known_arrangement` 守着）。
10. **普通模式敲出的小数与读进来的小数不是同一棵树**（有意）：读 `12.5` 是一个 `Number`，逐键敲 `1` `2` `.` `5` 是 `Number(12) Char(.) Number(5)`，写出 `12 . 5`。实测四种写法两两渲染完全相同（见下），所以是纯表示差异；命令模式走解析器，得到的是一个 `Number`。
11. **一个"字符"是一个字形簇，不是一个 Unicode 标量。** 词法把 `e`+U+0301、`👍🏽`、ZWJ 家庭 emoji 各收成一个 `MathText` 节点，而 `GlyphItem.text` 也是一个字形簇；编辑器原先按标量拆成多个 `Char`，于是**回写会在字形簇中间插入分隔符**，敲一个键就把 `é` 变成 `e` + 空格 + 飘在后面的重音符（实测码位 `0x65 0x20 0x7a 0x20 0x301`）。对齐载荷为一个字形簇之后：`0x65 0x301 0x20 0x7a`，字形簇完好。这是"回写义务"那一类缺陷，会改坏文档内容。
12. **前端"有分支但后端到不了"的名字，一共九个，已全部删除。** 逐个对照命令表（`config/commands.json`）与 `Shape::view`：`decoration` 里的 `widehat`/`dot`/`ddot`/`dddot`/`arrow`/`underline`/`underbrace`/`underbracket`/`underparen` 都不可能出现在线上——后端只产生 `hat` 与 `cancel`（`Accent`，线上都是 `decorated`）以及 `overline`/`underline`（`Line`，线上也是 `decorated`），其余名字会落成 `Raw` 由引擎自己画。反向的检查也做了：`multiline` 的左右交替对齐、`scripts` 的 `_placement`（`limits`/`scripts`）、`unknown` 的 `_string_mode` 都是真的到得了的；`ARRANGEMENTS` 白名单里多出的 `draft-*`/`absent`/`stop`/`cell`/`macro-argument` 是前端自造或后端合成的节点，不在 `Shape` 里，属于白名单该有的成员。后来 View 词汇整体重划（`sqrt`/`delim`/`decoration`/`line` 合并成 `decorated`，`grid`/`aligned`/`script`/`macro-collapsed` 改名），这张白名单也随之换过一遍；`decorated` 内部改用 `marker` 分派，同样只收后端真能产生的记号。

    这次靠人眼逐个对照，**所以后来把它变成了机器检查**：`tools/kind_inventory.py` 现在把 32 个用例里**实际发出的线名**与 `mathview.py` 的 `ARRANGEMENTS` 做双向对照，任一边多出来就打印并**以非零码退出**。实测两个方向都对齐——23 个真发出的线名 + `parameter`/`template-call`（第五节：只存在于宏模板里，上线前就被 `bind_template_inner` 换掉，但必须有画法与 `Decl`）+ `absent`/`symbol`（前端自造）= 全部 25 个。检查本身也验过有牙：往 `ARRANGEMENTS` 里塞一个 `bogus-arm` 立刻报「后端发不出来的排布名」，退出码 1。

    顺带在两处踩到"看着该有却没有"：`draft-text` 需要草稿里**有名字**（只打一个 `\` 只有占位符与光标），`empty-cell` 需要**真的有空格子**——而 `frac(a, )` 的尾逗号**不产生实参**，所以要用带显式空档的 `mat(, ; , )`。两个用例因此补进了 `CASES`。

| 写法对 | 实测（24pt，真实适配器） |
| --- | --- |
| `1.5` / `1 . 5` | 30.672 × 16.512（相同） |
| `.5` / `. 5` | 18.672 × 16.512（相同） |
| `12` / `1 2` | 24.0 × 15.984（相同） |
| `98456` / `98 456` | 60.0 × 16.776（相同） |
2. **单字母名字在 Typst 里不是标识符。** `lexer.rs:742-753`：只占一个字形簇的名字词法成 `MathText`，不是 `MathIdent`，因此 `f(x)` **本来就不是函数调用**（渲染成并排），宏调用要求名字 ≥2 个字形。实测 `#let f(a) = $ #a $` + `$ f(x) $` 不展开，而 `twice`/`foo`/`f2` 都会展开成 `macro`。这不是编辑器的缺陷。
3. **`Kind::Symbol` 只覆盖 `config/symbols.json` 的 40 条**：20 个希腊字母（15 小写 `alpha`…`omega` + 5 大写 `Delta`/`Gamma`/`Omega`/`Sigma`/`Theta`），其余 20 条是关系与算术符号（`<=`、`>=`、`!=`、`+-`、`-+`、`minus.plus`、`plus.minus`、`times`、`dot`、`div`，以及 `+ - * < = >` 和 `\/`、`\\`、`slash`、`backslash`）。`arrow.r`、`dif`、`sum`、`oo` 都**不在**表里 → 落成 `Raw`（由编译器出图，这本身是对的）。
4. **`raw_macro` 的那句文案已经删掉了。** 原先 `view.rs` 用 `definition.is_some()` 在"参数个数与定义不符"和"展开较大"两段文案里二选一，而两者都说不准：实测 16 层 `twice(twice(…))`（参数个数**是**对的，只是展开规模超限）报的是"参数个数与定义不符"——三个状态配两个标签，其中一个必然说错。查读者时发现**前端一处都没有**（`layout_node` 里 `raw_macro` 走通用分支，只排 children，从不读 `text`），所以这个字段一直是线上死数据。现在它装的是**调用本身的拼写**，前端拿它去取这一段的图，两种画法（光标在外画图 / 在内画名字与参数槽）就都成立了。
5. **`display_glyph` 只服务于"编辑器里打进去的字符"。** 从源码解析出来的 `-` 是 `MathShorthand` → `Symbol{name:"-", glyph:"−"}`（`text` 本身就是字形），只有编辑器 `input` 插入的 `Char{'-'}` 才需要 `text='-'` + `display_glyph='−'` 这一对。
6. **`Fenced` 的 `text` 是两个定界符用换行拼起来的**（`"(\n)"`、`"|\n|"`、`"‖\n‖"`），`abs`/`norm` 走的是同一个视图，前端靠 `text` 反推该画 `|` 还是 `abs()`——回写时也是这么反推的（`write_atom` 里比对 `left == "|" && right == "|"`）。

## 七、这张表怎么重做

两个探针都在 `tools/` 下，不需要 Qt，只驱动刚构建出的可执行文件：

```powershell
cargo build --offline --locked --release --bin typformula --target-dir target/server
cargo build --offline --locked --release --manifest-path native-adapter/Cargo.toml --target-dir target/adapter
python tools/kind_inventory.py            # 第一、二节的线上实测（可加 --json 存全量）
python tools/engine_boxes.py              # 第三、六节的引擎盒子（可传自己的公式）
```

`tools/kind_inventory.py` 里每个 `Kind` 对应一个能真正产生它的源文；只有 `Unknown` 需要额外发一次 `{"action":"input","text":"\\"}`（命令草稿）。两处坑写在该文件里：

- **公式要取 `state` 返回的 `equations` 的最后一个**再 `activate_formula`。用 `source.index("$")` 会命中定义里的 `$…$`，整张表会安静地把定义体当成公式（实测踩过）。
- 输出要 `reconfigure(encoding="utf-8")`：Windows 控制台的默认代码页印不出 `Symbol` 节点带的 `𝛼`。

`tools/engine_boxes.py` 默认那组公式就是本文档比较用的：一个基准、几种记号、以及"数字串之间的空格算不算数"的那几对。它走真实适配器：`target/adapter/release/typformula-layout.exe --server`，请求体是 `{"path","source","raw":[{id,start,end}]}`，返回的 `items` 里有 `width`/`height`/`baseline`（pt）。
