# `Kind` 能力清单

本文区分编辑树的存储 `Kind`、配置和编辑规则使用的形状名，以及前端收到的 View kind。当前事实以 `crates/core/src/math.rs`、`slots.rs`、`editing.rs`、`view.rs` 与 `src/document.rs` 为准；`tools/kind_inventory.py` 用真实后端核对线上排布，第六节保留早期引擎测量及其后续修正。

## 一、存储 Kind 与借用形状

`Kind` 枚举共有 **14 个变体**：12 个可进入编辑树（含命令草稿 Unknown），另 2 个仅用于注册期模板。正常源码往返覆盖其中 11 个；模板专用项与命令草稿不作为正常源码树写回。

| 存储 Kind | 实例字段（子格统一在 MathAtom.cells） | 形状 / 线上 View | 拼写与编辑要点 |
| --- | --- | --- | --- |
| Char | text：一个字形簇 | char / char | 保留字符源码；可有 display_glyph 显示覆盖 |
| Symbol | name、glyph | symbol / symbol | 按 name 回写，View.text 是 glyph |
| Number | 无 | number / number | 一个 inner 格，内部逐字 Char；回写连成数字串 |
| Raw | source | raw / raw | 保留原文；edit 光标用于失败后打开源码 |
| Text | 无 | text / text | 一个 inner 格，按带引号字符串回写 |
| MacroCall | name、function | 配置形状或 macro / 配置 View、macro、raw_macro | 函数写 name(args)，内容值写裸名字；实参按绑定或配置确定 |
| Fraction | 无 | fraction / fraction | 来自语法 a/b；当前规范回写 frac(a, b) |
| Scripts | 无 | script / scripts | 固定 base、upper、lower 三格；空脚标不写出 |
| Fenced | left、right | delim / decorated | 保留实际定界符；marker 为 delim |
| Table | columns、row_lengths、name | grid / table | mat/vec/cases；矩阵逐行补空块至齐行，补齐格可编辑并写回 |
| Multiline | columns、row_lengths | aligned / multiline | 对齐列用 &、行用反斜杠；保留各行原列数 |
| Unknown | name、saved、caret、anchor、original | unknown / unknown | 命令或源码草稿；显示为 draft-* 子节点，不是已提交源码 |
| Parameter（模板专用） | index、name | parameter（仅注册期） | 参数身份为 index，临时拼写为定义中的真实 #name；不上线 |
| TemplateCall（模板专用） | definition | template-call（仅注册期） | 先前宏版本的模板引用；不能交给正常源码写回，上线前展开 |

`sqrt`、`root`、`decoration`、`line`、`style` 都是形状，没有同名的存储 Kind。配置调用 `frac(a, b)` 同样存为 MacroCall，而不是 Fraction；`√x` / `∛x` 分别折入 sqrt/root 调用。`delim` 既能由配置调用借用，也能由 Fenced 使用。

| 配置命令 | 借用形状 | 线上 View / 绘制字段 |
| --- | --- | --- |
| frac | fraction | fraction，marker="-" |
| sqrt | sqrt | decorated，marker="radical" |
| root | root | root，marker="radical" |
| abs / norm | delim | decorated，marker="delim"，text 为定界符对 |
| hat / cancel | decoration | decorated，marker 为命令名 |
| overline / underline | line | decorated，marker 为命令名 |
| bold / upright | style（主体可表示为字形串时） | style，style_name 为命令名；否则 raw_macro |
| mat / vec / cases | grid | table，border / is_mat 按配置填写 |

此表以名字未被文档绑定遮蔽、实参形式受支持为前提。未知的位置调用如 `bb(A)` 存为 MacroCall，投影为 raw_macro；具名实参、不可静态展开的已绑定宏、不匹配的绑定实参等在解析期回退 Raw。已存在的 MacroCall 在投影超限或绑定条件变化时也可退为 raw_macro。

## 二、槽位、导航与空槽

Shape 声明 role、scale、arity、view 与 class/引擎对应信息；导航从 `editing::Rules` 取，实例可达性由是否携带调用点的 stop 决定。`Slot.optional` 已删除。scale 是内核声明，当前 View 不传它，前端排布分支仍独立写出相应缩放比例。

| 形状 | 格子角色（声明比例） | 导航要点 |
| --- | --- | --- |
| fraction | numerator / denominator（90%） | 前进进分子、后退进分母；左右锁定，上下换格落格首 |
| root | index（55%）/ radicand（100%） | 按书写顺序存放；上下换格落格尾 |
| script | base（100%）/ upper / lower（70%） | 默认从 base 进入，上下按附件规则导航 |
| grid | 重复 cell（100%） | 列内移动，上下按列换行 |
| aligned | 重复 cell（100%） | 使用行列导航，未使用的补齐格不显示 |
| text / number | inner（100%） | Text 两端保留字符串导航；Number 可用左右键进出 |
| macro | 重复 arg（100%） | 未借配置形状时按实参顺序进入；展开后仅实参可达 |

Scripts 恒有三个 View 子节点：有内容的脚标正常投影，未使用的空脚标为带 role 的 absent；光标正在其中编辑时保留 empty-cell 与 stop。因此 `a` 后输入 `^` 或 `_` 立即显示空槽和光标，而另一侧不会凭空显示。

`mat(a, b; c)` 的 columns=2、row_lengths=[2,2]，规范回写 `mat(a, b; c, "")`。Multiline 的 row_lengths 则保留各行原宽度，前端只跳过它未使用的补齐格。进入公式本身不会立即规范化原文。

## 三、线上 View 字段及 host 附加字段

| 字段 | 含义与填写者 |
| --- | --- |
| kind | 线上排布名；与 Shape.view 不必同名 |
| text | 字符/字形、宏名、调用拼写、装饰记号或草稿文本等；Number/Text 自身为空串，内容在 children |
| children / role | 子节点及父形状为它填写的槽位角色 |
| display_glyph | Char 的显示覆盖；Symbol 的 glyph 直接放在 text |
| cursor / active / selected | stop 的真实编辑位置，以及当前光标与选区状态 |
| columns | 表格/对齐列数或 macro-argument 的参数序号；模板中间节点也暂用该字段，但不上线 |
| row_lengths | Table 为补齐后的行宽，Multiline 为原行宽 |
| marker / border / is_mat | 装饰类型、表格定界符与行切分方式 |
| style_name | style 调用名；字形请求使用完整 text |
| edit | 可达的 raw/raw_macro/style 及 macro 包装节点的源码编辑位置；模板材料没有独立 edit |
| attachment | 受支持顶层 Scripts 的 Typst 拼写，范围穿过 Multiline；基底非空且不含草稿才发 |
| definitions / origin / source_range | 模板 raw/raw_macro 的定义前缀、定义原文及字节区间；绑定后转为 style 时可保留 |
| source_text | 模板片段的定位拼写，或 macro 包装节点的实际调用拼写；区别于绑定后的 text |
| render_id / render_request | host 为已定位的 raw/raw_macro 附加 id/start/end；没有独立 edit 的模板 raw_macro 另带 call 与 occurrence |

style 不进入 render.raw；即使带有模板来源区间，也不等于有取图请求。host 为外层 raw_macro 内的片段保留定位信息，但批次只选择当前显示所需的非重叠区间。

style 使用 `/api/glyphs`，definitions 固定为空；一次请求带上文档里所有还没答案的拼写（`{"expressions":[…]}`，逐项回包），缓存键为 `("", text, display)`，在途、成功（含空串）和失败分别处理。raw/raw_macro 使用 `/api/render`；带 call 的模板 raw_macro 按调用实例缓存，普通 Raw（包括模板中的 Raw）仍按源码和脚本摘要共享。公式的图像与 attachment 的 placement 都以该公式**上下文的摘要**为身份（`desktop.rs::project` 里的 `context`，内核 `context::digest`）：正文里与它无关的一次编辑不再让它们作废。具体失效规则见 [architecture.md](architecture.md) 与 [desktop.md](desktop.md)。

## 四、引擎对应关系与当前缺口

引擎 item 定义位于 `vendor/typst/crates/typst-library/src/math/ir/item.rs`；内核只有 Shape 中的名字对应，不链接编译器。

| 编辑器节点或形状 | MathKind / 引擎信息 | 当前边界 |
| --- | --- | --- |
| Char / Symbol / style | Glyph：字形簇、class、stretch 等 | style 读取引擎替换后的码位；未同步所有字形属性 |
| Number / Text | Number / Text：text 等 | 采用可编辑字符容器 |
| Raw | Box / Mathml / External | 片段保留源码，通过适配器取图 |
| fraction | Fraction：numerator、denominator、line、padding | 前端固定画有横线分式，不支持全部 line/padding 设置 |
| sqrt / root | Radical：radicand、index、sqrt | 两个借用形状、不同槽位；不是两个存储 Kind |
| Scripts | Scripts：base 与六个附件字段 | 编辑树只有 base/upper/lower；不支持左侧附件映射 |
| delim / Fenced | Fenced：open、close、body、balanced | 定界符用字符串表示；复杂伸缩可能简化 |
| grid / Table | Table：cells、gap、augment、align、alternator | 行方式与定界符来自配置，未表示全部引擎选项 |
| Multiline | Multiline：rows、centered | 前端按行列对齐，未携带 centered 字段 |
| decoration（hat） | Accent：accent、position、dotless、exact_frame_width | 前端手画 hat，未获取全部重音字体度量 |
| decoration（cancel） | Cancel：length、stroke、angle、inverted、cross | 前端固定画对角线，不表示全部取消线参数 |
| line | Line：base、position | 位置按命令配置 above 决定 |
| macro / 模板 / 草稿 | 无 | 编辑器专有，不对应独立引擎 item |

附件服务只为公式顶层及 Multiline 对齐单元格顶层获取 limits/scripts 判定，分式、脚标内部等更深层尚未覆盖。请求携带完整公式上下文，适配器在保留封闭作用域的内存源码中求值；前端仍自行计算侧挂移位与盒子布局，并非直接复制引擎的全部坐标。

MathProperties 中的 cramped、lspace/rspace、spaced、ignorant 等尚未完整传给前端；class 是内核声明，只有按类跳词使用，不在 View 中。Shape 对账的 UNMODELLED 当前为 Group、Primes、SkewedFraction。

## 五、模板专用项不上线

Parameter 与 TemplateCall 仅存在于注册期解析的原子树。定义体投影一次后保存为 `Arc<ViewTemplate>`，原子树丢弃；洞与边存为 `ViewTemplate::Hole` / `Edge`，绑定时替换成实参 View 和被调宏的展开。因此正常线上树没有 parameter/template-call 节点。

`tools/kind_inventory.py` 对真实后端输出与前端 ARRANGEMENTS 做双向核对；symbol/absent 也可由前端合成，并非只由前端产生。新增形状或 View 时要同步核对配置、规则表、投影与前端画法。

## 六、早期探测记录与后续修正

1. **`vec` 不是 accent。** `resolve_vec`（`resolve.rs:1018`）把每个参数变成一行再套定界符（`VecElem` 定义在 `matrix.rs`，默认 `delim: DelimiterPair::PAREN`）——它是**列向量**，和 `mat` 同族。`VecElem` 自己的文档就写着："To typeset a symbol that represents a vector, `math.accent[arrow]` and `bold` are commonly used"（`matrix.rs:23-25`）。编辑器曾把 `vec` 建成 `Accent`，而前端曾经按名字给它画一个箭头——**前端那行是凭名字猜的**（来自本仓库第二个提交 `d15f35d 桌面端`，Rust 侧从来没有 `arrow` 这个 Kind 或命令）。实测：`arrow(x)` 才是引擎的 `Accent`（13.728×17.328pt，宽度不变），`vec(x)` 是 29.5584×23.904pt（定界符被拉伸），两者不是一回事；`vec(x)` 与 `mat(x)` 的**映射 SVG 逐字节相同**，`vec(x, y)` 与 `mat(x; y)` 也相同。箭头已删除。**`vec` 现在进表了**：它是 `grid` 形状的第二个名字（`{"shape":"grid","rows":"each","border":"()"}`），行方式与定界符写在名字旁边，写回仍是 `vec(…)` 而不是 `mat(…; …)`；`cases` 同理，只是定界符是单边 `{ `。同一个形状下的第三个名字是 `mat`，它按分号切行。
7. **`mat` 的默认定界符是圆括号，前端曾经一律画方括号（已修正）。** `MatElem::delim` 默认 `DelimiterPair::PAREN`（`matrix.rs:103`）。实测 `(mat(x))` 比 `mat(x)` 宽出正好一对定界符（48.2304 − 29.5584 = 18.672pt，与 `(x)` − `x` 相同），说明 `mat` 自己那对确实画着圆括号。原先 `mathview.py` 的 `grid` 分支画的是两条竖线加四个短横（方括号），因为它拿不到定界符；现在 `View` 带 `border: "()"` 上线，前端按它画，两边一致。
8. **一条被证伪的怀疑。** 我一度以为映射会漏掉 `vec` 的定界符（因为 `vec(x)` 与 `mat(x)` 完全一样）。查下来不是：`mat` 的默认定界符本来就是圆括号，所以 `mat(x)` 与 `mat(x, delim: "(")` 是同一个东西，两者相同是必然的。当时留下的疑问——`Write::Matrix` 写出的 `mat(…)` 在前端是方括号、在文档里是圆括号——随 `border` 上线一并解决。
9. **数字串是一个"像 text 一样的容器"。** `Number` 有一个格，里面是逐字的 `Char`，所以光标**能停在数字之间**（`12|34` 插一个 `9` 得到 `12934`），这是叶子模型根本表达不了的。前端仍然只多一个排布名（`number`），通用"有子节点"分支会把那个格画出来；名字必须在 `ARRANGEMENTS` 白名单里，否则整篇带数字的公式都会报"不认识的排布"（`desktop/test_desktop.py::test_a_number_run_is_a_container_with_a_known_arrangement` 守着）。
10. **普通模式敲出的小数与读进来的小数不是同一棵树**（有意）：读 `12.5` 是一个 `Number`，逐键敲 `1` `2` `.` `5` 是 `Number(12) Char(.) Number(5)`，写出 `12 . 5`。实测四种写法两两渲染完全相同（见下），所以是纯表示差异；命令模式走解析器，得到的是一个 `Number`。
11. **一个"字符"是一个字形簇，不是一个 Unicode 标量。** 词法把 `e`+U+0301、`👍🏽`、ZWJ 家庭 emoji 各收成一个 `MathText` 节点，而 `GlyphItem.text` 也是一个字形簇；编辑器原先按标量拆成多个 `Char`，于是**回写会在字形簇中间插入分隔符**，敲一个键就把 `é` 变成 `e` + 空格 + 飘在后面的重音符（实测码位 `0x65 0x20 0x7a 0x20 0x301`）。对齐载荷为一个字形簇之后：`0x65 0x301 0x20 0x7a`，字形簇完好。这是"回写义务"那一类缺陷，会改坏文档内容。
12. **前端"有分支但后端到不了"的名字，一共九个，已全部删除。** 逐个对照命令表（`config/commands.json`）与 `Shape::view`：`decoration` 里的 `widehat`/`dot`/`ddot`/`dddot`/`arrow`/`underline`/`underbrace`/`underbracket`/`underparen` 都不可能出现在线上——后端只产生 `hat` 与 `cancel`（`Accent`，线上都是 `decorated`）以及 `overline`/`underline`（`Line`，线上也是 `decorated`），其余未配置的位置调用现在成为 `raw_macro`，由引擎取图；具名实参等仍保留 Raw。反向的检查也做了：`multiline` 的左右交替对齐、`scripts` 的 `_placement`（`limits`/`scripts`）、`unknown` 的 `_string_mode` 都是真的到得了的；`ARRANGEMENTS` 白名单里多出的 `draft-*`/`absent`/`stop`/`cell`/`macro-argument` 是前端自造或后端合成的节点，不在 `Shape` 里，属于白名单该有的成员。后来 View 词汇整体重划（`sqrt`/`delim`/`decoration`/`line` 合并成 `decorated`，`grid`/`aligned`/`script`/`macro-collapsed` 改名），这张白名单也随之换过一遍；`decorated` 内部改用 `marker` 分派，同样只收后端真能产生的记号。

    这次靠人眼逐个对照，**所以后来把它变成了机器检查**：`tools/kind_inventory.py` 现在把 32 个用例里**实际发出的线名**与 `mathview.py` 的 `ARRANGEMENTS` 做双向对照，任一边多出来就打印并**以非零码退出**。检查本身也验过有牙：往 `ARRANGEMENTS` 里塞一个 `bogus-arm` 立刻报「后端发不出来的排布名」，退出码 1。对照曾经需要一份例外清单——`parameter`/`template-call`（第五节：只存在于宏模板里，上线前就被换掉，但必须有画法）+ `absent`/`symbol`（前端自造）。**现在清单只剩前端自造的那两个**：模板改存视图树之后，前两个在显示树里连节点都不是，前端那两行画法已删，于是"每个线名都有画法、每个画法都有线名"是**确切**的 23 对 23。

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

`tools/kind_inventory.py` 的用例同时覆盖存储 Kind、借用形状与特殊 View 状态，标签中的 Sqrt/Style 等是形状标签，不代表当前存在同名 Kind；只有 `Unknown` 需要额外发一次 `{"action":"input","text":"\\"}`（命令草稿）。两处坑写在该文件里：

- **公式要取 `state` 返回的 `equations` 的最后一个**再 `activate_formula`。用 `source.index("$")` 会命中定义里的 `$…$`，整张表会安静地把定义体当成公式（实测踩过）。
- 输出要 `reconfigure(encoding="utf-8")`：Windows 控制台的默认代码页印不出 `Symbol` 节点带的 `𝛼`。

`tools/engine_boxes.py` 默认那组公式就是本文档比较用的：一个基准、几种记号、以及"数字串之间的空格算不算数"的那几对。它走真实适配器：`target/adapter/release/typformula-layout.exe --server`，请求体是 `{"path","source","raw":[{id,start,end}]}`，返回的 `items` 里有 `width`/`height`/`baseline`（pt）。
