# Rust 速成：给 C++ 程序员

面向本项目内核（`src/`、`native-adapter/`）。假设你熟悉 C++11/17：RAII、模板、智能指针、`std::variant`、并发。

工具链：`rustc 1.98.1`，`edition = "2024"`（见 `Cargo.toml:4`）。edition 决定语法可用性，不是编译器版本——本项目允许 `if let` 链式写法，正是因为 2024。

## 0. 怎么用这份文档

- 只想读代码 → 看 §1 和 §4（§4 全部用你自己的代码讲）。
- 想动手写 → 看 §2 速查表，遇到报错回来查 §3。
- 想先做个小改动 → 直接跳 §6 的自测题。

> **代码片段可能已经跑在前面了。** 这份文档讲的是 **Rust 的语法与概念**，例子取自本项目的某个时刻。内核经过多轮重构（`Kind` 收拢、`Decl` 拆成 `Grammar`/`Shape`、宏与命令表的改动……），片段里的行号与具体写法未必还对得上。想找对应代码时**以片段点名的文件为准，不要照抄片段**；凡是我知道已经过时的，就地写了说明。

数组语法层面 Rust 和 C++ 很像，麻烦都在**所有权**和**trait**。这两块 §1 讲透，其余都是查表。

## 1. 五个心智转换

### 1.1 移动是默认，且移动后使用是编译错误

C++ 的移动构造是可选优化，移动后对象处于"有效但未指定"状态，用了是逻辑错误。Rust 里移动是**默认行为**，移动后使用是**编译错误**——不是 UB，是编译不过。

```rust
let a = String::from("x");
let b = a;              // 移动，不是拷贝
println!("{a}");        // 编译错误：borrow of moved value
```

想真的复制要显式 `a.clone()`。好处：C++ 里"这个函数会不会偷走我的参数"要靠看签名和注释，Rust 里赋值/传参的**默认答案永远是"会"**，需要复制就写出来。

代价：`clone()` 会出现在热路径上，而且是可见的——这是特性不是缺陷。本项目 `typst.rs:99` 那种 `entry.key == definitions` 的比较之所以不 clone，就是为了避开它。

### 1.2 借用检查器：一个 `&mut` 独占，或任意多个 `&`

这是 Rust 唯一需要你**改变设计**的地方。规则一句话：**同一个对象，要么有任意多个只读借用 `&T`，要么有恰好一个可变借用 `&mut T`，不能同时。**

C++ 里这靠约定（"别在有引用时改容器"），Rust 靠编译器。所以 C++ 里常见的"边遍历边改"，Rust 直接编译不过。

绕过它有三个**显式**工具，你会在本项目里反复看到：

| 工具 | 线程 | 用途 | 本项目实例 |
| --- | --- | --- | --- |
| `RefCell<T>` | 单线程 | 运行期借用检查，违反会 panic | `document.rs:21` |
| `Cell<T>` | 单线程 | 只能换整值，无借用 | — |
| `Mutex<T>` | 多线程 | 锁**把数据装在内部** | `typst.rs:81` |

注意 `Mutex` 和 C++ 不同：C++ 是 `mutex` + 你自己保证保护了哪个变量；Rust 是 `Mutex<T>`，**不拿到锁就碰不到数据**，保护关系由类型系统保证。见 `typst.rs:127`。

`Rc<RefCell<T>>` 就是 C++ 的 `shared_ptr<T>` 加上"可变"，本项目 `document.rs:21` 用它做惰性缓存。

### 1.3 没有继承和虚函数，只有 trait

| C++ | Rust |
| --- | --- |
| `class Derived : public Base` | `impl Trait for Type`（**无数据继承**） |
| `virtual` + vtable | `dyn Trait`（动态）或泛型单态化（静态） |
| `template<class T>` | `<T: Trait>` 或 `<T>` + `where` |
| 重载 | **不允许**，改用不同函数名或泛型 |
| 默认参数 | **不允许**，改用 `Option<T>` 参数或 builder |
| 运算符重载 | `impl Add for T` 等标准 trait |
| 虚析构 | 不需要，`Drop` 自动按值调用 |

关键差别：Rust 没有"实现继承"，只有"接口继承 + 组合"。你项目里 `Kind` 是枚举而不是类层次（`math.rs:21-40`），这正是 Rust 的惯用做法——**用枚举表达"之一"，用 struct 表达"并且"**，别用继承。

`impl Trait` 出现在参数位就等价于模板：

```rust
pub fn raw(source: impl Into<String>) -> Self   // math.rs:44
```

调用方传 `&str` 或 `String` 都行，编译期单态化，零开销——和 C++ 模板一样，但不需要在头文件里展开。

### 1.4 没有异常，错误是返回值

| C++ | Rust |
| --- | --- |
| `throw` / `try` / `catch` | `Result<T, E>` + `?` |
| 不可恢复错误 `assert`/`abort` | `panic!` |
| `std::optional<T>` | `Option<T>` |
| `std::expected<T,E>` | `Result<T,E>` |

`?` 是核心语法糖：**成功就取出值，失败就 `return Err(e)`**，并自动经 `From` 做类型转换。

```rust
pub fn render(&self, mut req: RenderRequest) -> Result<Value, String> {
    crate::workspace::resolve(&self.workspace, &req.path)?;   // 出错直接返回
    ...
    let value = result?;
    if let Some(error) = value["error"].as_str() { return Err(error.into()); }
    Ok(value)
}
```
（`services.rs:306-320`，注意最后的 `Ok(value)`——函数体最后一个表达式就是返回值，**不需要 `return`**。）

`panic!` 对应"程序有 bug"，不是"可预期的失败"。`unwrap()`/`expect()` 就是"这里 panic 我能接受"的标记。你项目 `typst.rs:127` 的 `unwrap_or_else` 是刻意的：锁中毒时恢复而不是跟着崩。

**没有 `null`**。可能为空就是 `Option<T>`，编译器逼你处理 `None`。这是 Rust 最大的日常收益之一。

### 1.5 生命周期：多数时候不用写

生命周期就是"这个引用能活多久"，绝大多数情况编译器能推断（**省略规则**）。你只在返回引用且来源有歧义时才写。

```rust
pub fn cell<'a>(root: &'a MathData, slices: &[CursorSlice]) -> &'a MathData {
    let mut data = root;
    for slice in slices { data = &data[slice.atom].cells[slice.cell]; }
    data
}
```
（`math.rs:119-123`）

`'a` 的意思是"返回值活得和 `root` 一样久"。省略规则在这里其实也能推出来（只有一个输入引用），但显式写更清楚。**看到 `'a` 不要怕**，它只是在说"这个引用不凭空产生"。

`&'static str` = 活到程序结束的字符串字面量，见 `math.rs:138`。

## 2. 对照速查表

### 类型与容器

| C++ | Rust |
| --- | --- |
| `#include <vector>` | 多数类型在 prelude，直接用；其余 `use std::collections::HashMap;` |
| `std::vector<T> v;` | `let mut v: Vec<T> = Vec::new();` |
| `v.push_back(x)` | `v.push(x)` |
| `v.size()` / `v.empty()` | `v.len()` / `v.is_empty()` |
| `v[i]` | `v[i]`（越界 **panic**，不是 UB） |
| `v.at(i)` | `v.get(i)` → `Option<&T>` |
| `std::string` | `String`（拥有，堆分配，UTF-8） |
| `const char*` / `string_view` | `&str`（借用，不拥有） |
| `s.c_str()` | `s.as_str()` |
| `std::map` / `unordered_map` | `BTreeMap` / `HashMap` |
| `std::array<T,N>` | `[T; N]` |
| `T*`（可空） | `Option<&T>` / `Option<Box<T>>` |
| `std::tuple` | `(A, B)` 元组；结构体更常见 |
| `std::pair` | `(A, B)` |
| `sizeof(T)` | `std::mem::size_of::<T>()` |

### 智能指针与所有权

| C++ | Rust |
| --- | --- |
| `T obj;` 栈上 | `let obj = T::new();` 栈上，**默认就在栈上** |
| `new T()` / `delete` | `Box::new(T)`（唯一所有权，自动释放） |
| `std::unique_ptr<T>` | `Box<T>` |
| `std::shared_ptr<T>` | `Arc<T>`（多线程）/ `Rc<T>`（单线程） |
| `std::weak_ptr<T>` | `Weak<T>` |
| `std::mutex` + `lock_guard` | `Mutex<T>`，数据在锁里 |
| `std::atomic<T>` | `AtomicUsize` 等 |
| 全局 `static` | `static`（须 `Sync`）或 `const` |
| `T&&`（右值引用） | 无对应；移动是默认 |

### 枚举、模式匹配、控制流

| C++ | Rust |
| --- | --- |
| `enum class E { A, B };` | `enum E { A, B }` |
| `std::variant<A,B>` + `std::visit` | `enum E { A(TA), B(TB) }` + `match` |
| `switch`（可穿透） | `match`：**无穿透，必须穷尽** |
| `default:` | `_ => ...` |
| `if (auto p = f())` | `if let Some(p) = f()` |
| — | `let Some(x) = f() else { return; };`（let-else） |
| — | `if a && let Some(x) = b`（let-chain，2024） |
| `goto` | 无 |

`match` 的穷尽性检查是你项目的重要依赖：`view.rs:138-192` 和 `typst.rs:605-634` 都是穷尽 match，**加一个 `Kind` 变体会编译失败直到你补上**。这是免费的契约强制，C++ 的 `switch` 给不了。

### trait 与泛型

| C++ | Rust |
| --- | --- |
| `template<class T> void f(T x)` | `fn f<T>(x: T)` |
| `template<class T> requires C<T>` | `fn f<T: C>(x: T)` 或 `where T: C` |
| 概念 `concept` | `trait`（几乎同构） |
| 抽象基类 | `trait` |
| `virtual void f() = 0` | trait 里的方法签名 |
| 纯虚 + 继承实现 | `impl Trait for Type` |
| 接口 + vtable | `dyn Trait`（需 `Box`/`&`） |
| CRTP 静态多态 | 泛型（默认就是静态分发） |
| `std::function<R(A)>` | `Box<dyn Fn(A) -> R>` |
| 函数指针 | `fn(A) -> R` |
| 运算符重载 | `impl Add for T { ... }` |
| `operator==` | `impl PartialEq for T`，或 `#[derive(PartialEq)]` |
| 拷贝构造 | `#[derive(Clone)]` + `.clone()` |
| 析构函数 `~T()` | `impl Drop for T { fn drop(&mut self) }` |

`#[derive(...)]` 是自动生成实现，最常用：`#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]`（见 `math.rs:13`）。相当于编译器帮你写拷贝构造、`operator<<`、`operator==`。

### 字符串与输出

| C++ | Rust |
| --- | --- |
| `printf("%d", n)` | `println!("{n}")` |
| `std::ostringstream` | `format!("{a} {b}")` |
| `sprintf` 拼接 | `format!` 返回 `String` |
| `s += "x"` | `s.push_str("x")` / `push('c')` |
| `s.length()` | `s.len()`（**字节数**，非字符数） |
| `s.substr(i,n)` | `&s[i..i+n]`（字节区间，须在字符边界） |

Rust 的格式化是编译期检查的：`"{}"` 写错参数个数编译不过，不像 `printf` 是 UB。

### 模块与可见性

| C++ | Rust |
| --- | --- |
| 头文件 + `#pragma once` | 无头文件；`mod` 声明（`lib.rs:3-12`） |
| `namespace foo { }` | `mod foo { }` + 路径 `foo::bar` |
| `#include "x.h"` | `use crate::x;`（或 `mod x;`） |
| 默认 public（struct） | **默认 private**，要 `pub` |
| `private:` | 默认即 private |
| `friend` | `pub(crate)` 或同模块 |
| `extern "C"` | `extern "C"` + `unsafe` |

## 3. 十个坑

**1. `String` vs `&str` 是两件事。** `String` 拥有（≈`std::string`），`&str` 是借用（≈`string_view`）。函数参数优先 `&str`（自动从 `&String` 转），返回值如果是新造的用 `String`。写 `&String` 参数几乎总是错的。

**2. 整数不隐式转换。** `usize` 和 `u32` 是不同类型，混用编译不过。索引必须是 `usize`，所以到处是 `as usize`（如 `document.rs:91`）。整数溢出：debug 下 panic，release 下回绕——要明确语义用 `saturating_add`（`typst.rs:107`）或 `wrapping_mul`（`typst.rs:91`）。

**3. `clone()` 要看得见。** 热路径上的 `.clone()` 就是性能问题。本项目 `view.rs:174` 的 `children[0].clone_view()` 是刻意为之（需要独立的所有权），注释也写明了原因。

**4. 别用索引循环。** `for i in 0..v.len() { v[i] }` 在 Rust 里既慢又容易和借用检查打架。用迭代器：`for x in &v`、`v.iter().map(...)`、`v.iter().any(|n| ...)`（`typst.rs:454`）。

**5. 迭代器是惰性的。** `map`/`filter` 不做事，直到 `collect()`/`sum()`/`for` 消费它。`Option` 和 `Result` 也是迭代器，所以 `data.iter().all(|x| ...)`（`typst.rs:350`）能直接用在集合上。

**6. 闭包默认按引用捕获，跨线程要 `move`。**
```rust
std::thread::spawn(move || { for body in receiver { ... } });   // services.rs:187
```
`move` 把捕获的变量所有权移进闭包。不加 `move` 时借用是暂时的，线程可能活得比它久，编译不过。

**7. `match` 里绑定的默认是引用。** `match &atom.kind { Kind::Char { value } => ... }` 中 `value` 是 `&char`。需要值时解引用或 `*value`（`math.rs:66`）。

**8. 结构体字段默认私有，且构造要写全。** 想用 `T { a, b }` 构造，字段得 `pub`，或在同模块内。`#[derive(Default)]` + `..Default::default()` 是常用套路。

**9. `RefCell` 的借用是运行期的。** 单线程里 `borrow_mut()` 嵌套调用会 panic。本项目 `document.rs:62-68` 是刻意把 `borrow()` 的作用域收紧到一行，避免和后续 `borrow_mut()` 撞上。

**10. 没有隐式 `this` 生命周期。** 方法 `fn f(&self) -> &T` 返回的引用默认绑在 `self` 上（省略规则）。要返回别的来源必须显式标注。

## 4. 用你自己的代码学

以下每一段都是本仓库的真实代码，按"知识点 → 位置 → 解释"排列。建议按顺序读。

### 4.1 常量与 `const fn`

```rust
static MACROS: Mutex<MacroCache> = Mutex::new(MacroCache::new());
static EMPTY_MACROS: OnceLock<Arc<MacroRegistry>> = OnceLock::new();
```
（`typst.rs:81-82`）

`static` 是全局变量。在 `static` 里调用函数要求那是 **`const fn`**（编译期可求值）——所以 `MacroCache::new` 声明成 `const fn new()`（`typst.rs:95`）。C++ 的 `constexpr` 构造函数是同一回事。

`OnceLock<T>` = 只初始化一次的全局，等价于 C++11 的 `static` 局部变量（magic static）或 `std::call_once`。`get_or_init` 见 `typst.rs:83`。

### 4.2 锁中毒

```rust
fn poison_free<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(|error| error.into_inner())
}
```
（`typst.rs:127`）

Rust 独有概念：持有锁的线程 panic 了，锁会被标记"中毒"，之后 `lock()` 返回 `Err`。这里选择恢复（`into_inner()` 取出数据）而不是向上传播——注释解释了理由。`MutexGuard<'_, T>` 是 RAII 守卫（≈`lock_guard`），`'_` 是省略的生命周期。

### 4.3 let-else 与 or-pattern

```rust
pub fn get(&self, name: &str) -> Option<&MacroDefinition> {
    let (Binding::Expandable(index) | Binding::Opaque(index)) = self.names.get(name)? else { return None; };
    self.entries.get(*index)
}
```
（`typst.rs:62-65`）

一次讲四件事：
- `?` 在 `Option` 上表示"是 `None` 就返回 `None`"；
- `let ... else { }` 是"匹配不上就走 else"，else 分支**必须发散**（`return`/`panic!`/`continue`）；
- `|` 是 or-pattern，两个变体绑定同一个 `index`；
- 返回 `Option<&MacroDefinition>` 的生命周期由省略规则绑到 `&self`。

`self.entries.get(*index)` 里 `*index` 是因为 `index` 从 `&Binding` 里借出来是 `&usize`。

### 4.4 let-chain（2024 特有）

```rust
if supported && let Some(ast::Expr::Equation(eq)) = body {
```
（`typst.rs:345`）

`if` 条件里直接写 `let` 模式，把"布尔条件"和"解构"合成一句。C++ 里要嵌套两层 `if`。这是 edition 2024 才稳定的语法。

### 4.5 嵌套函数

```rust
fn check(data: &MathData, params: &[String], used: &mut [bool]) -> bool {
    data.iter().all(|atom| { ... })
}
if !check(&template, &def.params, &mut used) { ... }
```
（`typst.rs:349-361`）

Rust 允许函数内定义函数，作用域局部，**不捕获环境**（要捕获就用闭包）。因为不捕获，所以 `params`/`used` 必须显式传参。这样比闭包好：不会和借用检查纠缠。

注意 `&mut [bool]` 参数——可变切片借用，调用方传 `&mut used`（`Vec<bool>` 自动转 `&mut [bool]`）。

### 4.6 闭包、`Option` 组合子、方法链

```rust
if let Some(def) = bound.filter(|_| typst::projection_size(atom, &registry, typst::PROJECTION_LIMIT) <= typst::PROJECTION_LIMIT) {
```
（`view.rs:115`）

`Option::filter` 保留满足条件的 `Some`。`|_|` 是忽略参数的闭包。这种"组合子链"在 Rust 里比 `if` 嵌套更常见。

再看一处更典型的：

```rust
pub fn equations(&self) -> Vec<Equation> {
    self.equation_index().iter().map(|(equation,_)|equation.clone()).collect()
}
```
（`document.rs:71-73`）

`iter().map(...).collect()` 是 Rust 的 `std::transform` + 构造容器。`.collect()` 的目标类型由返回类型**推断**出来（这里是 `Vec<Equation>`）。

### 4.7 `matches!` 与 `is_some_and`

```rust
if children.first().is_some_and(|n| n.kind() == SyntaxKind::LeftParen) && ...
if children.iter().any(|n| matches!(n.kind(), SyntaxKind::Linebreak | SyntaxKind::MathAlignPoint)) {
```
（`typst.rs:449,454`）

`matches!(v, Pat)` 是"匹配吗"的布尔简写，不用写完整 `match`。
`Option::is_some_and` / `Result::is_ok_and` 是"有值且满足谓词"。

### 4.8 match guard 与字段简写

```rust
// 示意图：这条按名字给出形状的 match 已经不存在了——现在形状查的是
// `config/commands.json`（`slots::configured_shape`），节点也统一存成
// `MacroCall`，不再按名字造出各自的 `Kind`。留下它是因为下面两个语法点没变。
let kind = match (name.as_str(), args.len()) {
    ("frac", 2) => Kind::Fraction,
    ("abs", 1) => Kind::Fenced { left: "|".into(), right: "|".into() },
    ("mat", n) if n > 0 && widths.iter().all(|w| *w == widths[0]) => Kind::Table {
        columns: widths[0], row_lengths: vec![widths[0]; n], name: name.clone(),
    },
    _ => return vec![MathAtom::from_source(node.full_text())],
};
```

`("mat", n) if n > 0 && ...` 里的 `if` 是 **match guard**：先按元组匹配名字与参数个数，再检查附加条件（各行列数一致才当矩阵，否则落到 `_` 退化成 Raw）。C++ 的 `switch` 没有这个，得写嵌套 `if`。

注意 `->` 后面跟 `return`：分支的**类型必须一致**，`_` 分支用 `return` 发散（类型 `!`）才能和其余分支产出的 `Kind` 共存——这是第 20 章 never type 的实际用处。

`Self { kind, cells: vec![...] }`（`math.rs:51`）是**字段初始化简写**：变量名和字段同名时可省 `kind: kind`。

### 4.9 `usize::from(bool)`

```rust
self.cursor.pos = slice.atom + usize::from(forward);
```
（`cursor.rs:301`）

`bool` 转 `usize` 用 `usize::from(b)` 而不是 `b as usize`——前者是安全转换（`From` trait），后者是强制转换。Rust 鼓励前者。

### 4.10 内部可变性与 Rc

```rust
pub struct Document {
    ...
    index: RefCell<Option<(u64, Rc<Vec<(Equation, SyntaxNode)>>)>>,
}
```
（`document.rs:21`）

- `Rc<Vec<..>>`：共享所有权（单线程），`clone()` 只加计数。返回时给调用方一份共享视图。
- `RefCell<..>`：让 `&self` 方法（`equation_index(&self)`，`document.rs:61`）也能改缓存。
- `Option<(u64, Rc<...>)>`：`u64` 是世代号，和当前 `syntax_revision` 比对决定缓存是否过期（`document.rs:62-63`）。

这就是 C++ 里 `mutable std::shared_ptr<...>` + 手工判断失效的写法，但借用规则是运行期检查的。

注意 `document.rs:62-68` 的写法：`borrow()` 的结果只活在 `if let` 条件里，紧接着的 `borrow_mut()` 才不会 panic。

### 4.11 线程 + 通道 + Drop

```rust
struct RenderAdapter { child: Child, requests: mpsc::Sender<Vec<u8>>, replies: mpsc::Receiver<Result<Value,String>> }
impl Drop for RenderAdapter { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }
```
（`services.rs:176-177`）

- `impl Drop` = C++ 析构函数，作用域结束自动调用（RAII 同构）。**不需要虚析构**，因为不靠继承。
- `mpsc::channel()` = 多生产者单消费者队列（≈无界 `BlockingQueue`）。
- `let _ = expr;` 是**刻意忽略返回值**。`#[must_use]` 的返回值不能静默丢弃，`let _ =` 是"我知道，故意不要"。

实际起线程的写法：

```rust
let (requests, receiver) = mpsc::channel::<Vec<u8>>();
std::thread::spawn(move || { for body in receiver { if input.write_all(&body)...is_err() { break; } } });
```
（`services.rs:185-187`）

`move` 把 `receiver` 和 `input` 移进闭包——因为线程可能比当前函数活得久。

### 4.12 serde：序列化即协议

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind { Char { value: char }, Symbol { name: String, glyph: String }, ... }
```
（`math.rs:19-21`）

`#[derive(Serialize, Deserialize)]` 自动生成 JSON 序列化（≈手写 `nlohmann::json` 的 `to_json`/`from_json`，但零手写）。`#[serde(tag = "type")]` 生成**内部标签**格式：`{"type":"char","value":"x"}`。这正是前后端 JSON 协议的基础。

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub display_glyph: Option<String>,
```
（`view.rs:10-11`）

`Option` 字段为 `None` 时不输出该键——减小协议体积。客户端那边就是 `node.get("display_glyph")`。

### 4.13 枚举作代数数据类型

```rust
pub enum Kind {
    Char { value: char },
    Raw { source: String },
    Script,
    Grid { columns: usize },
    ...
}
```
（`math.rs` 里的 `Kind`）

这是 C++ 里要 `std::variant` + `std::visit` 才能表达的东西。Rust 里 enum 的每个变体可以带不同的数据，`match` 时精确解构——也可以不带数据（如 `Script`）。你项目的 `Kind`、`Action`（`cursor.rs:16-35`）、`Binding`（`typst.rs:54`）全是这个模式。

**这也是为什么加一个 `Kind` 会编译报错直到你补全所有 match**——穷尽性检查在替你守契约。

### 4.14 用 `From`/`Into` 做优雅转换

```rust
pub fn raw(source: impl Into<String>) -> Self { Self { kind: Kind::Raw { source: source.into() }, cells: vec![] } }
```
（`math.rs:44`）

`impl Into<String>` 让 `&str` 和 `String` 都能传。`source.into()` 做实际转换。这是 Rust 里替代重载的标准手法：**不用写两个函数，接受 `impl Into<T>`**。

同理 `services.rs:319` 的 `error.into()` 在同类型时是恒等。

### 4.15 递归，以及操作无类型 JSON

```rust
fn annotate(view: &mut Value, root: &MathData, locator: &Locator, raw: &mut Vec<Value>, counts: &mut HashMap<String,usize>) {
    if view["kind"] == "raw" {
        ...
    }
    if let Some(children) = view["children"].as_array_mut() { for child in children { annotate(child,root,locator,raw,counts); } }
}
```
（`document.rs:227-252`）

三个要点：

- `view["kind"]` 对 `serde_json::Value` 用 `[]` 索引，**不会 panic**——键不存在就得到 `Value::Null`。这和 C++ 的 `map::operator[]` 不同（那个会插入默认值）。
- 和字符串比较能直接写 `== "raw"`，因为 `Value` 实现了和 `&str` 的 `PartialEq`。
- `as_array_mut()` 返回 `Option<&mut Vec<Value>>`，用 `if let` 解构后递归。**可变借用沿着树往下传**是 Rust 里改写树的标准写法。

Rust **没有 `yield`**（生成器未稳定），所以树遍历要么写成这样的递归函数，要么写成 `impl Iterator<Item = &T>` 适配器链。Python 侧的 `window.py:499-501` 用了 `yield`，Rust 里没有对应物。

## 5. 构建与测试

```powershell
# 构建（含原生取图适配器）
.\build-desktop.cmd

# 核心测试
cargo test --offline --locked

# 原生适配器测试
cargo test --offline --locked --manifest-path native-adapter/Cargo.toml --target-dir target/adapter

# 桌面套件（离屏 Qt，需先构建原生程序）
$env:QT_QPA_PLATFORM='offscreen'; python -m unittest desktop.test_desktop -v
```

日常快循环：

```powershell
cargo check                    # 只类型检查，最快
cargo check --message-format=short
cargo clippy                   # 静态检查，Rust 的"经验审查"
cargo fmt                      # 格式化（Rust 统一风格，不用争论）
cargo test document::          # 只跑文件名/路径匹配的测试
cargo test -- --nocapture      # 显示 println! 输出
```

`--offline --locked` 的原因：`Cargo.lock` 已锁定依赖，离线构建保证可复现（也适应无网环境）。加了新依赖才需要 `cargo add`。

`#[cfg(test)] mod tests`（`document.rs:255`）是单元测试惯例：测试代码和实现同文件，但只在该模块编译进测试时存在。

## 6. 自测题：加一个新的 `Kind`

这是最好的练习，而且直接连着内核扩展性的讨论。按 §1.3 的穷尽性，编译器会带你走完 Rust 侧的每一步——**先改枚举，然后跟着编译错误走**。

1. `math.rs:21` 加一个变体，例如 `Cancel`（`\cancel` 斜线）。
2. `cargo check` → 报错：`view.rs:138` 的 match 不穷尽。补上视图投影。
3. 再 `cargo check` → 报错：`typst.rs:605` 的 `write_atom` 不穷尽。补上源码回写。
4. `typst.rs:488` `parse_atom` 加构造分支（**这步编译器不提醒，靠自觉**，忘了就静默退化成 `Raw`）。
5. 手动检查四处**编译器不管**的：`math.rs:56 entry_cell`、`math.rs:97 idx_horizontal`、`math.rs:65 math_class`。
6. 前端：`mathview.py` 加布局分支，并让它发出 `stops`（`mathview.py:406`）否则光标进不去。

做完你会清楚感受到：Rust 侧有 2 处强制、3 处静默；Python 侧全静默。这正是上一轮讨论的"契约只在一半路程上被强制"。

## 7. 外部资料

**系统学（选一个读完就够）**

- [The Book（官方，中文版）](https://kaisery.github.io/trpl-zh-cn/) —— 首选，第 4 章所有权、第 10 章泛型/trait/生命周期是核心。
- [Comprehensive Rust](https://google.github.io/comprehensive-rust/)（[中文](https://google.github.io/comprehensive-rust/zh-CN/)）—— Google 的四天课程，进度快，适合已有系统编程经验的人。
- [Rust for C++ Developers（Microsoft RustTraining）](https://microsoft.github.io/RustTraining/c-cpp-book/) —— **专门写给 C++ 程序员的**，讲法就是本文 §1 的展开版。

**查表**

- [cheats.rs](https://cheats.rs/) —— 最全的单页速查，建议打印/收藏。
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/) —— 每个概念配可运行代码。
- [std 文档](https://doc.rust-lang.org/std/) —— 搜类型名，文档里每个方法都有例子。

**动手**

- [Rustlings](https://github.com/rust-lang/rustlings) —— 编译错误驱动的练习题，专门训练"看懂借用检查报错"。**对 C++ 背景的人收益最大。**

**深入（暂时不需要）**

- [Rust Reference](https://doc.rust-lang.org/reference/) —— 语言规格，查语义细节。
- [The Rustonomicon](https://doc.rust-lang.org/nomicon/) —— `unsafe` 与底层语义。本项目基本用不到。
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) —— 写库的命名与设计约定。

## 8. 报错怎么读

Rust 编译器以报错友好著称，且**几乎总是给你修改建议**。三个高频错误：

| 报错 | 含义 | 常见修法 |
| --- | --- | --- |
| `borrow of moved value` | 移动后又用了 | 加 `.clone()`，或改成借用 `&x`，或调整顺序 |
| `cannot borrow ... as mutable more than once` | 同时两个 `&mut` | 缩小作用域，或先取出需要的值再改 |
| `does not live long enough` | 引用活得比数据长 | 让数据存在于更外层，或返回拥有所有权的值 |

关键是**别对抗借用检查器**。它的报错通常指向一个真实的设计问题——C++ 里同样的代码只是没被检查出来。遇到很难绕的情况，先想"是不是该换个数据结构"，而不是急着上 `RefCell`。

`RUST_BACKTRACE=1` 打开 panic 栈回溯（`$env:RUST_BACKTRACE=1`）。
