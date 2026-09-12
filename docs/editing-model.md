# 编辑模型（editing model）

**状态：架构已定，尚未实现。** 本文定下**三层职责**的分工，以及"编辑"这一层为什么必须独立于 `Shape` 存在。

先读 [architecture.md](architecture.md) 的「槽位模型」一节。本文接手它，但**改变它的结论**：那里说 `Decl` 拆成 `Grammar`（语法）+ `Shape`（渲染与编辑），本文论证"编辑"不该在 `Shape` 里。

## 一、三层

| 层 | 回答什么 | 取用键 | 与另一层的关系 |
| --- | --- | --- | --- |
| **`Grammar`** | 怎么写回源码 | `Kind` | 独立 |
| **`Shape`** | **这个 box 有哪些 item、怎么排、怎么嵌套** | 形状名（可被命令借） | 提供 item 的 `role` 名 |
| **editing-model** | 光标怎么走、`stop` 怎么发、**哪一段能编辑** | 静态部分按形状，动态部分按**实例** | **引用** `Shape` 的 `role` 名 |

一句话：**`Shape` 只管字面意义上的"形状"（box 与排布），光标、停靠点、可编辑性全部归 editing-model。**

## 二、为什么 `Shape` 要缩到 box

### 2.1 今天它装了四件事

`Shape` 有八个字段，实测读者如下：

| 字段 | 读者 | 属于 |
| --- | --- | --- |
| `view` | `view_atom`（选前端排布） | **box 排布** |
| `slots` | `view_atom`（盖 `role`）、`fill_command_cells`、`role_at`/`index_of` | **item 清单**（见 2.2） |
| `arity` | `role_at`、`fill_command_cells`（`cursor.rs:87`） | **box 有几个 item** |
| `typst` | 词汇表对账测试（1 处） | 对账标签 |
| `entry` | `math.rs:223` `entry_cell` | **编辑** |
| `horizontal` | `math.rs:262` `idx_horizontal` | **编辑** |
| `vertical` | `cursor.rs:779` `move_vertical` | **编辑** |
| `class` | `math.rs` `math_class` → `cursor.rs` `move_word` | **排版属性**（见第五节） |

后四个是编辑规则，被塞进了一张本该描述形状的表里。

### 2.2 `slots` 不是"可编辑格子清单"，是 item 清单

它装四样东西，而这四样**不归同一层**：

| | 读者 | 归 |
| --- | --- | --- |
| `role` | **前端 `mathview.py:252` 的 `slot(children, role, index)`**——按角色**摆放**子节点（用在 `:334`、`:356` 等处） | **Shape** |
| `scale`（per-mille） | 前端（分子分母 ×0.9、脚标 ×0.7） | **Shape** |
| `arity` | box 有几个 item | **Shape** |
| **`optional`** | **零读者（死数据，见第四节）** | — |

所以 `Shape` 的职责应该表述为：**这个 box 有哪几个具名 item（`role`）、每个多大（`scale`）、是不是重复模式（`arity`）、用哪种排布（`view`）。**

### 2.3 依赖方向是 editing → Shape

editing-model 的规则**引用** `role` 名：

```rust
entry: Entry::Role { forward: Role::Numerator, backward: Role::Denominator },
vertical: Vertical::Swap { up: Role::Numerator, down: Role::Denominator, end_up: false },
```

**不是** `Shape` 里写"这一格能编辑"、编辑层去读。方向反了会让 `Shape` 重新变成编辑规则的宿主，也就是回到今天。

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

| 半 | 内容 | 粒度 | 今天在哪 |
| --- | --- | --- | --- |
| **规则** | 进入点、上下左右、列运算、按类跳词 | **静态，按形状** | `Shape` 的 `entry`/`horizontal`/`vertical`、`math_class` |
| **绑定** | 哪一格是洞、洞指向哪个语法编辑点、因此哪一格可达 | **每实例** | **没有归属** |

第二半今天是靠 `view_cell(data, path: Option<&[CursorSlice]>, …)` **一路传参**实现的，不是声明：

- `path = Some(...)` → 这个格子里每个位置发一个 `stop`
- `path = None` → 一个都不发

宏展开时，实参视图带着**调用点的** `path` 拼进模板，模板材料用 `path = None` 建。**不可达的机制不是"选择性发 stop"，而是"默认不发，实参视图自己带着 stop 进来"。**

### 3.3 顺带量到的代价

`stop` 占视图节点的一半：

| 公式 | 总节点 | `stop` |
| --- | --- | --- |
| `frac(a, b)` | 12 | 6 |
| `a + b + c + d + e + f + g + h` | 32 | 16 |
| `mat(1, 2; 3, 4)` | 36 | 18 |
| 展开的 `dbl(y)` | 14 | 4 |

**不能**砍到"每 box 一个"：光标要能停在任意两个原子之间，前端点击取最近的停靠点，内核的纵向导航也吃这些坐标（`stops → Action::Geometry → Editor.geometry → cursor.rs` 按 `x` 找同格邻居）。能省的只有不可达区域，而那部分今天已经省了。

## 四、顺带查出的死数据：`Slot.optional`

```rust
pub const fn blank(role: Role, scale: u16) -> Self { Self { role, scale, optional: true } }
```

实测 `git grep '\.optional\b'`（排除 `vendor/`）：**零读者**——只有一个构造器设它，没有任何地方读。它大概是早期"空槽画洞、可放光标"那套的残留（那件事现在由 `absent` / `empty-cell` 视图节点表达）。

**这一轮直接删掉。** 删它正好印证本文的划法：「一格能不能空着」本来是**能不能放光标**的问题，属于 editing-model；放在 `Shape` 里既没人读、又误导人以为它管排版。

## 五、`class` 留在 `Shape`（不要跟着搬）

`Shape.class` 今天**只被 `move_word` 读**（`cursor.rs:752-763`，Ctrl+方向键按类分组跳词），所以看起来像"光标的事"。但它的**数据来源是 Typst 的数学间距类**：`char_class` 从字符算，`class: 7` 是分式与表格。

实测两件事：

- **前端从不读 `class`**（`desktop/mathview.py` 零处）
- **`class` 也不上线**（`crates/core/src/view.rs` 里没有这个字段）

所以它是**内核内部的排版属性**，只是恰好被光标逻辑用了。

> **它是"这个数学对象是什么类"——排版属性，不是光标属性。** 搬到 editing 就等于承认"这个类只为光标存在"，那会把因果说反。

## 六、两条不变量

### 6.1 不可编辑 ⇒ 光标不可达

这是本文的**原则**，也是它成立的理由：`Cursor` 是 `(slices, pos)`，**既是光标位置，又是被编辑的地址**——两者不可能分开。所以"能停在某个位置"与"能编辑那个位置"是同一件事。

今天这条**已经成立**（模板材料一个 `stop` 都没有）。本文要做的是把它**从约定变成构造**：模板材料没有洞 → 没有 `stop`；洞被填了实参视图 → 天然带着调用点的 `stop`。

### 6.2 编辑层的标记绝不能到达 `write_atom`

`#parameter0` 那个已实测的缺陷**就是它被违反的结果**：

```
#let mathbf(x) = $bold(upright(#x))$   配   $mathbf(u)$
  [macro] mathbf
    [raw_macro] text='bold(upright(#parameter0))'    ← 内部占位符被当成正文画出来
```

链路：`has_glyph_run` 对 `Parameter` 落到 `_ => false` → `bold` 借不到 `style` 形状 → 走 `raw_macro` → 而 `raw_macro` 的 `text` 取自 `write_atom(atom)`，模板里的 `Parameter` 写出来正是 `Write::Marker` = `#parameter0`。

今天这条靠三个**偶然**的东西维持：`Write::Marker` 的拼写、`Write::TemplateOnly` 的 `unreachable!`、以及"模板树不进文档"。**编辑层独立出来之后，该把它变成构造事实**（见 6.1 的做法），而不是继续靠纪律。

## 七、要守住的对齐测试

拆开之后，**`Shape` 的 item 清单与 editing 的规则引用同一批 `role` 名**，这是两份声明之间的耦合。今天已有测试守着：

```
slots.rs::named_roles_exist_in_the_schema_that_names_them
  对每个形状：Entry::Role{forward,backward} 与
              Vertical::Swap{up,down} 里的 role
              必须在同一个形状的 slots 里存在
```

**拆完这条测试要跨两个表继续守**——它是防止两份声明漂移的唯一那道闸，别拆丢了。

## 八、已考虑过、不采用的方向

只留理由，不重写细节，免得以后再绕回来。

| 方向 | 为什么不走 |
| --- | --- |
| **在 `Shape` 上声明可编辑/不可编辑** | 3.1 的反例：同一形状的同一实例里两格可达性可以不同。形状是静态的，绑不了每实例的事实 |
| **把编辑规则从 box 结构推导出来** | `fraction` 是两格但导航是"**锁定** + 上下互换"，`root` 也是两格却是"线性 + 落格尾"——规则与 box 结构**不成函数关系**，必须各自声明 |
| **`Origins` 旁挂表**（展开时另记"这段来自哪个实参"的区间表） | 表要支持**嵌套复合**（`outer` 模板里调 `inner`，内外两层来源叠加），最容易写错；而"洞"作为结构时，归属是构造出来的，不需要表 |
| **槽对象**（槽是模型实体，格子绑定槽，光标编辑槽） | 一旦把"洞"做成结构，"槽"就是**模板树里的一个变体**，不必再是独立实体；随之而来的一串问题（身份是投影期还是持久、实例还是模式、怎么上线）整个消失 |
| **可编辑/不可编辑格子标记** | 只是把今天的 `path: Option` 显式化。实测**不可达区域今天就已经不发停靠点**，所以换来的是可读性、不是能力 |

## 九、与"模板存视图树"的关系

编辑层要能表达"**哪一格是洞、洞指向哪个语法编辑点**"，而这件事在模板这条路上最吃紧（见 3.1）。所以两层是配套的：

- **editing-model** 定义"洞"与"可达"是什么
- **模板**是第一个真正需要它们的用例：模板材料没有洞，实参填进洞里

模板那边还有一个已知**不解决**的问题要分开记账：`Style` 形状判定要看**已绑定**主体的字形串，而判定发生在绑定之前。这与编辑层的拆分无关，改动介入点在"绑定之后要有一次重新定形"。

## 十、怎样算完成

1. 三层职责在代码里是可指认的：读 `entry`/`horizontal`/`vertical` 的地方属于编辑层，`Shape` 只剩 box 描述
2. `Slot.optional` 删除
3. 第七节那条对齐测试跨两表继续生效
4. 6.1 与 6.2 两条不变量有测试守着，且**在实现之前先看到它们失败**（否则不知道它们有没有牙）
5. 现有 137 + 87 + 21 全绿（搬迁期间行为零变化）
6. 本文状态行改为"已实现"，并在 [validation.md](validation.md) 追加一节记录实测

---

**改动本文时**：这是**架构**文档，不是历史记录。决定定下来就改正文。被推翻的方向只留一行理由（第八节），不重写细节——细节留着会让人以为它还在候选。
