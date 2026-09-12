# 编辑模型（editing model）

**状态：已实现。** 三层职责已落到代码里：`slots::Grammar` / `slots::Shape` / `crate::editing`。实测记录见 [validation.md](validation.md) 的对应一节。

先读 [architecture.md](architecture.md) 的「槽位模型」一节。本文接手它，但**改变它的结论**：那里曾说 `Decl` 拆成 `Grammar`（语法）+ `Shape`（渲染与编辑），本文论证"编辑"不该在 `Shape` 里——现在已经拆开。

## 一、三层

| 层 | 回答什么 | 取用键 | 与另一层的关系 |
| --- | --- | --- | --- |
| **`Grammar`**（`slots.rs`） | 怎么写回源码 | `Kind` | 独立 |
| **`Shape`**（`slots.rs`） | **这个 box 有哪些 item、怎么排、怎么嵌套** | 形状名（可被命令借） | 声明 item 的 `role` 名 |
| **editing-model**（`editing.rs`） | 光标怎么走、`stop` 怎么发、**哪一段能编辑** | 静态部分按形状，动态部分按**实例** | **引用** `Shape` 的 `role` 名 |

一句话：**`Shape` 只管字面意义上的"形状"（box 与排布），光标、停靠点、可编辑性全部归 editing-model。**

## 二、为什么 `Shape` 要缩到 box

### 2.1 拆分前后

拆分前 `Shape` 有八个字段。逐字段实测读者之后，其中三个搬去了 `editing::Rules`，一个留在原地：

| 字段 | 读者 | 现在在哪 |
| --- | --- | --- |
| `view` | `view_atom`（选前端排布） | **`Shape`** |
| `slots` | `view_atom`（盖 `role`）、`fill_command_cells`、`role_at`/`index_of` | **`Shape`** |
| `arity` | `role_at`、`fill_command_cells` | **`Shape`** |
| `typst` | 词汇表对账测试 | **`Shape`** |
| `class` | `math_class` → `move_word` | **`Shape`**（理由见第五节） |
| `entry` | `entry_cell` | `editing::Rules` |
| `horizontal` | `idx_horizontal` | `editing::Rules` |
| `vertical` | `move_vertical` | `editing::Rules` |

规则表还多了一项拆分前**没有名字**的判断：`back_lands_at_end`。它以前写作"形状是 radical **或** `Kind` 是 `Table`/`Multiline`"——一条规则靠一个词汇表字段加一个 `Kind` 匹配拼出来。有了编辑层，它成为一条说得出口的规则：**从左右走到相邻格时，进入的那一格光标落在末尾**（根式的次数画在左边、表格一行是从最后一列接上的）。

两层靠**形状名**（`Shape::view`）连接：`editing::rules(shape.view)`。依赖方向是 editing → `Shape`，因为规则里写的是 `Role::Numerator` 这样的**名字**，而名字是 `Shape` 声明的。反过来的话 `Shape` 就重新变成编辑规则的宿主，也就是回到拆分前。

### 2.2 `slots` 不是"可编辑格子清单"，是 item 清单

它装四样东西，而这四样**不归同一层**：

| | 读者 | 归 |
| --- | --- | --- |
| `role` | **前端 `mathview.py` 的 `slot(children, role, index)`**——按角色**摆放**子节点 | **Shape** |
| `scale`（per-mille） | 前端（分子分母 ×0.9、脚标 ×0.7） | **Shape** |
| `arity` | box 有几个 item | **Shape** |
| ~~`optional`~~ | 零读者（死数据，见第四节） | —（已删） |

所以 `Shape` 的职责是：**这个 box 有哪几个具名 item（`role`）、每个多大（`scale`）、是不是重复模式（`arity`）、用哪种排布（`view`）。**

## 三、核心证据：可达性不是形状的属性

这一节是本文的立论基础。**它推翻了"在 `Shape` 上声明可编辑/不可编辑"这条路。**

### 3.1 同一个形状的同一个实例，两格可达性不同

实测 `#let foo(x) = $frac(#x, 2)$` 配 `$foo(y)$`：

```
[fraction]
  [cell] role=numerator      [macro-argument] [stop slices=[{atom:0,cell:0}]] [char y] [stop]
  [cell] role=denominator    [number] [cell] [char 2]        ← 一个 stop 都没有
```

**分子可达、分母不可达**——因为分子是**洞**（绑到调用点的格子），分母是**模板材料**。

对照文档里的普通分式 `$frac(a, 2)$`，**两格都可达**：

```
[fraction]
  [cell] role=numerator      [stop slices=[{atom:0,cell:0}]] [char a] [stop]
  [cell] role=denominator    [stop slices=[{atom:0,cell:1}]] [number 2] [stop]
```

两者用的是**同一个 `FRACTION_SHAPE`**。于是：

> **可达性不是形状的属性，是这个实例的绑定关系的属性。**

`Shape` 是 `const` 静态表、按形状名取，最多能说"fraction 有两格、格子之间怎么走"，**说不出"这一格这次是不是洞"**。要在 `Shape` 上声明可达性，两条路都不通：

| 出路 | 为什么不行 |
| --- | --- |
| 给每个形状加"可编辑/不可编辑"标志 | 分不开 3.1 里的分子与分母——它们共享同一个形状声明 |
| 每种形状各出两份（可编辑版 / 显示版） | 形状数翻倍；而模板材料与洞**可以出现在同一个 box 里**，翻倍也分不开 |

### 3.2 所以 editing-model 是两半

| 半 | 内容 | 粒度 | 在哪 |
| --- | --- | --- | --- |
| **规则** | 进入点、上下左右、列运算、按类跳词 | **静态，按形状** | `editing::RULES`、`math_class` |
| **绑定** | 哪一格是洞、洞指向哪个语法编辑点、因此哪一格可达 | **每实例** | 模板树的 `ViewTemplate::Hole`，绑定时被实参视图替换 |

第二半靠 `view_cell(data, path: Option<&[CursorSlice]>, …)` **一路传参**实现，不是声明：

- `path = Some(...)` → 这个格子里每个位置发一个 `stop`
- `path = None` → 一个都不发

宏模板正是用 `path = None` 建的那一次投影。所以**不可达的机制不是"选择性发 stop"，而是"默认不发，实参视图自己带着 stop 进来"**：模板材料没有洞 → 没有 `stop`；洞被实参视图填上 → 那个视图天然带着调用点的 `stop`。

### 3.3 顺带量到的代价

`stop` 占视图节点的一半：

| 公式 | 总节点 | `stop` |
| --- | --- | --- |
| `frac(a, b)` | 12 | 6 |
| `a + b + c + d + e + f + g + h` | 32 | 16 |
| `mat(1, 2; 3, 4)` | 36 | 18 |
| 展开的 `dbl(y)` | 14 | 4 |

**不能**砍到"每 box 一个"：光标要能停在任意两个原子之间，前端点击取最近的停靠点，内核的纵向导航也吃这些坐标（`stops → Action::Geometry → Editor.geometry → cursor.rs` 按 `x` 找同格邻居）。能省的只有不可达区域，而那部分已经省了。

## 四、顺带查出的死数据：`Slot.optional`

```rust
pub const fn blank(role: Role, scale: u16) -> Self { Self { role, scale, optional: true } }   // 已删
```

实测 `git grep '\.optional\b'`（排除 `vendor/`）：**零读者**——只有一个构造器设它，没有任何地方读。它大概是早期"空槽画洞、可放光标"那套的残留（那件事现在由 `absent` / `empty-cell` 视图节点表达）。

**已删除**，`Slot::blank` 也随之并入 `Slot::scaled`。删它正好印证本文的划法：「一格能不能空着」本来是**能不能放光标**的问题，属于 editing-model；放在 `Shape` 里既没人读、又误导人以为它管排版。

## 五、`class` 留在 `Shape`（不要跟着搬）

`Shape.class` **只被 `move_word` 读**（Ctrl+方向键按类分组跳词），所以看起来像"光标的事"。但它的**数据来源是 Typst 的数学间距类**：`char_class` 从字符算，`class: 7` 是分式与表格。

实测两件事：

- **前端从不读 `class`**（`desktop/mathview.py` 零处）
- **`class` 也不上线**（`crates/core/src/view.rs` 里没有这个字段）

所以它是**内核内部的排版属性**，只是恰好被光标逻辑用了。

> **它是"这个数学对象是什么类"——排版属性，不是光标属性。** 搬到 editing 就等于承认"这个类只为光标存在"，那会把因果说反。

## 六、两条不变量

### 6.1 不可编辑 ⇒ 光标不可达

这是本文的**原则**，也是它成立的理由：`Cursor` 是 `(slices, pos)`，**既是光标位置，又是被编辑的地址**——两者不可能分开。所以"能停在某个位置"与"能编辑那个位置"是同一件事。

它是**构造**，不是约定：模板材料没有洞 → 没有 `stop`；洞被填了实参视图 → 天然带着调用点的 `stop`。`tests/editing_model.rs::template_material_is_unreachable_from_the_caret` 守着——`#let dbl(x) = $#x + 1$` 配 `$dbl(y)$` 恰好 4 个 `stop`（外层 2 + 实参格 2），模板材料（`+` 与 `1`，共 3 个位置）一个都没有；同时每个 `stop` 的 `slices` 都必须在可编辑树里真的存在。

### 6.2 编辑层的标记绝不能到达 `write_atom`

曾经违反过，而且实测过：

```
#let mathbf(x) = $bold(upright(#x))$   配   $mathbf(u)$
  [macro] mathbf
    [raw_macro] text='bold(upright(#parameter0))'    ← 内部占位符被当成正文画出来（同一处漏了两层）
```

链路：`has_glyph_run` 对 `Parameter` 落到 `_ => false` → `bold` 借不到 `style` 形状 → 走 `raw_macro` → 而 `raw_macro` 的 `text` 取自 `write_atom(atom)`，模板里的 `Parameter` 写出来正是 `Write::Marker`。

现在这条是**构造**，但要说清是哪一件事构造了它——**只有一件**：

**`Write::Marker` 拼的是参数在定义里的真名**（`#x`），不是这里发明的名字。`Kind::Parameter { index, name }` 两个字段各司其职：`index` 是结构身份（哪个实参），`name` 是拼写。于是模板材料那句 `text`/`style_name` 落回**定义自己的源码**，`definition_raw_ranges` 也真能在定义里找到它。被删掉的是**构造 `#parameter0` 的那行代码**。

**（一处需要说清的边界。）** "模板存视图树、洞是变体"**不是**这条不变量的成因。它是真的，但它保证的东西比这里强不了多少：`View.kind` 仍然是 `String`，`view_atom` 仍然建得出一个 `kind == "parameter"` 的 `View`——拦住它的是"文档树里不可能有 `Kind::Parameter`"（解析期事实）加上"模板投影的结果总被 `ViewTemplate::of` 消费"（控制流事实）。**是控制流，不是类型。** 模板存视图树的价值在第九节，不在这一条。

`tests/editing_model.rs` 有两条守着：`the_wire_view_never_carries_the_hole_marker`（查线上，**先于实现写、当时是红的**）与 `template_material_spells_its_hole_the_way_the_definition_writes_it`（查拼写，也就是上面那件真正起作用的事）。

## 七、要守住的对齐测试

拆开之后，**`Shape` 的 item 清单与 editing 的规则引用同一批 `role` 名**，这是两份声明之间的耦合。守它的是：

```
editing.rs::named_roles_exist_in_the_schema_that_names_them
  对每个形状：Entry::Role{forward,backward} 与
              Vertical::Swap{up,down} 里的 role
              必须在同一个形状的 slots 里存在

editing.rs::every_shape_declares_its_editing_rules
  两个方向：每个形状都有规则；每条规则都指着一个真实存在的形状
```

第二条是拆分**新加**的：规则表按名字查，编译器管不到它，所以"漏了一条"必须由对账来发现，否则那个形状会静默按 `PLAIN` 导航。两条测试都从 `slots::fixtures::every_shape()` 取样本——**同一个清单**，这正是"跨两表"能成立的前提。

## 八、已考虑过、不采用的方向

只留理由，不重写细节，免得以后再绕回来。

| 方向 | 为什么不走 |
| --- | --- |
| **在 `Shape` 上声明可编辑/不可编辑** | 3.1 的反例：同一形状的同一实例里两格可达性可以不同。形状是静态的，绑不了每实例的事实 |
| **把编辑规则从 box 结构推导出来** | `fraction` 是两格但导航是"**锁定** + 上下互换"，`root` 也是两格却是"线性 + 落格尾"——规则与 box 结构**不成函数关系**，必须各自声明 |
| **`Origins` 旁挂表**（展开时另记"这段来自哪个实参"的区间表） | 表要支持**嵌套复合**（`outer` 模板里调 `inner`，内外两层来源叠加），最容易写错；而"洞"作为结构时，归属是构造出来的，不需要表 |
| **槽对象**（槽是模型实体，格子绑定槽，光标编辑槽） | 一旦把"洞"做成结构，"槽"就是**模板树里的一个变体**，不必再是独立实体；随之而来的一串问题（身份是投影期还是持久、实例还是模式、怎么上线）整个消失 |
| **可编辑/不可编辑格子标记** | 只是把 `path: Option` 显式化。实测**不可达区域本来就 不发停靠点**，所以换来的是可读性、不是能力 |
| **洞作为普通显示节点、绑定时按 `kind` 名字替换** | 这是拆分前的做法（`view.kind == "parameter"`），也是 6.2 漏出来的原因：名字能被拼进 `write_atom`，而"没人会这么拼"只是纪律。变体是同一件事的结构版本 |

## 九、与"模板存视图树"的关系

编辑层要能表达"**哪一格是洞、洞指向哪个语法编辑点**"，而这件事在模板这条路上最吃紧（见 3.1）。所以两层是配套的，而且**已经配套实现**：

- **editing-model** 定义"洞"与"可达"是什么
- **模板**是第一个真正需要它们的用例，而它现在是：注册期把定义体投影成显示树存下来（`MacroDefinition::template: Arc<ViewTemplate>`），模板树比显示树多两个变体——**洞**（`Hole { index }`）与**边**（`Edge { definition, cells }`）。展开是全函数：洞被实参视图替换、边递归展开，实参先在**调用方**语境里绑定（`Projector::material`）。嵌套两层时归属是构造出来的，不需要旁挂的来源表。

**但"配套"不等于"缺一不可"。** 拆 `Shape` 与"模板存视图树"之间**没有机制耦合**：`editing::RULES` 是一张按形状名取的表，`ViewTemplate` 是一棵存下来的树，谁也管不着谁。少了任何一边，另一边照样成立。所以这两件事要各自记账，理由也要各自说清。模板存视图树买到的是这四样，强度递减：

| 买到的 | 强度 | 退回原子树会怎样 |
| --- | --- | --- |
| 模板**无法**交给 `write_atom` | **类型级**，但防的是**潜在**路径 | 模板就是一份 `MathData`，`write_cell(&def.template)` 能编译——上一轮恢复的两条测试正是那么写的 |
| 绑定是**全函数**（洞与边是变体） | 类型级，且是**现存**路径：每次展开都走 | 回到两个 `kind` 字符串判断 + 原地改树（`bind_template_inner`） |
| 模板**不带会话**是类型事实 | 类型级，潜在路径 | 只剩"建的时候传 `None`"这一条纪律 |
| 定义体投影**一次**而非每调用点一次 | **常数因子**，实测一次 200 原子的投影约 93µs | 每个调用点多花几十微秒；总量被 `projection_size` 门在 4096 节点量级，所以**不是量级差** |

代价也说清：`View` 与 `ViewNode` 两份字段（11 个，编译器守得住）、注册期一次 `ViewTemplate::of` 转换（里面有全项目唯一一处按 `kind` 名字认洞/边）、`editing::RULES` 里多两个中间形状名（`parameter`/`template-call`）。

**一件容易误判、值得写下来的事**：这不是为了让 `Style` 更好修。两种存法下 `command_shape()` 都只看模板自己的 cells，洞都让它答 `None`——所以 `Style` 那个待修的缺陷在两边同样难。真正起作用的是上面第一行：**模板不是 `MathData`，所以"不展开"这件事不靠纪律**。这与这个项目其他几处的做法同类（内核/外围的 crate 边界、`Write::TemplateOnly` 的 `unreachable!`）。

模板那边曾经另记着一个**不解决**的问题：`Style` 形状判定要看**已绑定**主体的字形串，而判定发生在绑定之前（`has_glyph_run` 对还没填的洞答 `false`）。**已经修好**，而且修法正是这一节说的"挪到实例侧"：

注册期**不做这个决定**。模板里那个折叠节点保留着**带洞的拼写**（`bold(upright(#x))`），调用点先把实参 View 填入主体，再依据绑定后的结构决定是否使用 style。整个过程保留实参携带的光标与 stop，不再把实参写成字符串后重新解析。调用拼写（`bold(upright(u))`）仅供取字形和失败显示使用。所以 `$mathbf(u)$`（`#let mathbf(x) = $bold(upright(#x))$`）现在画成 `style{text:"bold(upright(u))", style_name:"bold"}`，而 `bold(frac(a, b))` 仍然退成整段调用的图：**分式不是字形串，这条判断本来就没变。**

两点值得记下来：

* **不需要给模板加"待定"状态。** 折叠节点自带的 `text` 就是那句带洞的拼写，所以"未定"是**既有数据**的表达，不是一个新的变体。这条是设计评审时指出来的：先做过一版 `ViewTemplate::Deferred`，被它取代。
* **字形请求的键跟着变成绑完之后的拼写**（`bold(upright(u))`）。这本来就是对的——引擎要编译的是表达式本身，而带洞的那句它读不懂。这正是原先那把钥匙打不开门的原因。

## 十、怎样算完成

| # | 判据 | 结果 |
| --- | --- | --- |
| 1 | 读 `entry`/`horizontal`/`vertical` 的地方属于编辑层，`Shape` 只剩 box 描述 | `crates/core/src/editing.rs`；`slots::Shape` 剩 5 个字段，`grep` 三者在 `Shape` 上零处 |
| 2 | `Slot.optional` 删除 | 已删，`Slot::blank` 并入 `Slot::scaled` |
| 3 | 第七节那条对齐测试跨两表继续生效 | `editing.rs` 两条（role 对齐 + 表互为全集） |
| 4 | 6.1 与 6.2 两条不变量有测试守着，且**先看到失败** | `tests/editing_model.rs`；6.1 一开始就是绿的（构造），6.2 **当时红、现在绿**；6.3（变体按绑完之后的形态画）先红、后绿，并做过变异检查 |
| 5 | 全绿（搬迁期间行为零变化） | 内核 **143 通过 / 0 失败 / 6 忽略**（6 条都要本机环境：Tinymist ×5、原生渲染器 ×1）+ 适配器 **21** + 桌面 **87**；`kind_inventory.py` 退出码 0 |
| 6 | 状态行改为"已实现"，`validation.md` 追加实测 | 本节与 [validation.md](validation.md) |

---

**改动本文时**：这是**架构**文档，不是历史记录。决定定下来就改正文。被推翻的方向只留一行理由（第八节），不重写细节——细节留着会让人以为它还在候选。
