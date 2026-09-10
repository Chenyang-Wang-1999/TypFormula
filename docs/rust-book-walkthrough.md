# 《Rust 程序设计语言》逐章导读（面向 C++ 程序员）

教材：[The Rust Programming Language](https://doc.rust-lang.org/book/)（俗称"The Book"），官方入门书，免费，有[中文版](https://kaisery.github.io/trpl-zh-cn/)。
本导读的目录**扒自官方 `src/SUMMARY.md`**，不是凭记忆写的。

每个章节标一个性质，决定你怎么读它：

| 标记 | 含义 | 读法 |
| --- | --- | --- |
| **【对照】** | C++ 里有对应概念 | 看对照表，扫一遍即可 |
| **【独特】** | Rust 独有设计 | 细讲，要慢慢读 |
| **【混合】** | 有对应但语义不同 | 对照表 + 细讲部分 |

## 0. 先说三件必须先知道的事

**1. 中文版和英文版章号不一样。** 英文版较新，在中间插了一整章 async（第 17 章），后面的章号整体后移。你在中文版里找到的章号要和下表换算：

| 内容 | 中文版 | 英文版 |
| --- | --- | --- |
| 泛型、trait 与生命周期 | 10 | 10 |
| 智能指针 | 15 | 15 |
| 无畏并发 | 16 | 16 |
| **Async / await / Future** | **无此章** | **17** |
| 面向对象编程特性 | 17 | 18 |
| 模式和匹配 | 18 | 19 |
| 高级特征 | 19 | 20 |
| 多线程 Web 服务器 | 20 | 21 |

本导读**用英文版章号**（因为它是当前版本），括号里标中文版章号。

**2. 这本书不是为 C++ 程序员写的。** 它有大量篇幅在解释"什么是栈和堆""什么是编译错误"这类你早就会的东西。**前 3 章可以快速扫**，从第 4 章开始才是真正要读的。第 12、21 章是两个练手项目，对你的目标可以略读。

**3. 对你的实际目标（重做内核）来说，真正必须吃透的是 4、6、10、15、16、19 这六章。** 其余是查漏补缺。

## 1. 全书地图

### 完整目录（中文版，扒自官方 SUMMARY.md）

```
开始
  1  入门指南            安装 / Hello, World! / Hello, Cargo!
  2  猜数字游戏
  3  通用编程概念        变量和可变性 / 数据类型 / 函数 / 注释 / 控制流
  4  认识所有权          什么是所有权？/ 引用与借用 / 切片 slice
  5  使用结构体组织关联数据  定义和举例说明结构体 / 使用结构体的代码例子 / 方法语法
  6  枚举和模式匹配      定义枚举 / match 控制流运算符 / if let 简单控制流

基本 Rust 技能
  7  使用包、Crate 和模块管理不断增长的项目
                         包和 crate / 定义模块来控制作用域与私有性 /
                         路径用于引用模块树中的项 / use 关键字 / 将模块分割进不同文件
  8  常见集合            使用 vector 存储一列值 / 使用字符串存储 UTF-8 编码的文本 /
                         在哈希 map 中存储键和关联值
  9  错误处理            panic! 与不可恢复的错误 / Result 与可恢复的错误 / panic! 还是不 panic!
 10  泛型、trait 与生命周期  泛型数据类型 / trait：定义共享的行为 / 生命周期与引用有效性
 11  编写自动化测试      如何编写测试 / 控制测试如何运行 / 测试的组织结构
 12  一个 I/O 项目：构建命令行程序
                         接受命令行参数 / 读取文件 / 重构以改进模块化与错误处理 /
                         采用测试驱动开发完善库的功能 / 处理环境变量 / 将错误信息输出到标准错误

Rust 编程思想
 13  Rust 中的函数式语言功能：迭代器与闭包
                         闭包：可以捕获其环境的匿名函数 / 使用迭代器处理元素序列 /
                         改进之前的 I/O 项目 / 性能比较：循环对迭代器
 14  更多关于 Cargo 和 Crates.io 的内容
                         采用发布配置自定义构建 / 将 crate 发布到 Crates.io / Cargo 工作空间 /
                         使用 cargo install 从 Crates.io 安装二进制文件 / Cargo 自定义扩展命令
 15  智能指针            Box<T> / Deref trait / Drop Trait / Rc<T> / RefCell<T> 与内部可变性 /
                         引用循环会导致内存泄漏
 16  无畏并发            使用线程 / 消息传递 / 共享状态并发 / Sync 与 Send Trait
 17  Rust 的面向对象编程特性  面向对象语言的特点 / trait 对象 / 面向对象设计模式的实现

高级主题
 18  模式和匹配          所有可能会用到模式的位置 / Refutability（可反驳性）/ 模式语法
 19  高级特征            不安全的 Rust / 高级 trait / 高级类型 / 高级函数与闭包 / 宏
 20  最后的项目：构建多线程 Web 服务器
 附录  A 关键字 / B 运算符与符号 / C 可派生的 trait / D 实用开发工具 /
       E 版本 / F 本书译本 / G Rust 是如何开发的与 "Nightly Rust"
```

（英文版在 16 与 17 之间另有第 17 章 async，其后章号 +1。）

### 阅读优先级

| 优先级 | 章节 | 为什么 |
| --- | --- | --- |
| **必读精读** | 4 所有权 · 10 泛型/trait/生命周期 | 不读这两章，代码一行都写不出来 |
| **必读** | 6 枚举与匹配 · 9 错误处理 · 15 智能指针 | 决定你内核的数据结构长什么样 |
| **必读（你项目已在用）** | 7 模块 · 16 并发 · 19 高级特征 | `src/` 里到处是这三章的内容 |
| **快读** | 3 通用概念 · 5 结构体 · 8 集合 · 13 迭代器闭包 · 14 Cargo | C++ 对照为主，查表即可 |
| **略读** | 1 工具链 · 2 猜数字 · 11 测试 · 12/20 项目章 | 或按需回来查 |
| **按需** | 17 async（中文版无）· 18 OOP · 附录 | async 你项目没用；OOP 章反而是"Rust 为什么不用继承"的好材料 |

---

## 2. 逐章导读

### 第 1 章 · 入门指南 【对照】

**小节**：安装 / Hello, World! / Hello, Cargo!

C++ 的构建是"编译器 + 你自己选的构建系统 + 你自己选的包管理器"三件分离的事。Rust 把这三件合成了一件：`cargo`。

| C++ | Rust | 备注 |
| --- | --- | --- |
| 编译器（gcc/clang/msvc） | `rustc` | 你几乎不直接调用它 |
| CMake / Make / Bazel | `cargo` | 一个工具全包 |
| Conan / vcpkg | `cargo` + crates.io | 依赖声明在 `Cargo.toml` |
| `CMakeLists.txt` | `Cargo.toml` | 但简单得多 |
| `cmake --build` | `cargo build` | |
| `ctest` / gtest 手动接入 | `cargo test` | 测试框架内建 |
| `build/` | `target/` | |
| 手动管头文件搜索路径 | 无头文件 | 见第 7 章 |
| Doxygen | `cargo doc` | 从 `///` 注释生成 |

**细讲一处**：`cargo` 真正独特的地方不是"能构建"，而是**构建、依赖、测试、文档、格式化、静态检查、发布共用一个工具和一份声明**。C++ 里你换一个项目就要重新学它的构建方式；Rust 里全生态一致。这是它最被低估的设计。

**另一个独特点**：`edition`。`Cargo.toml` 里的 `edition = "2024"` **不是编译器版本，是语法版本**。同一个编译器可以按不同 edition 编译不同 crate，所以语言能演进而生态不碎。C++ 没有对应物——C++ 的 `-std=c++20` 是接近的类比，但 Rust 是**按 crate 独立**的，一个项目里不同依赖可以用不同 edition。

**本项目**：`Cargo.toml:4` 是 `edition = "2024"`，`Cargo.toml:28-34` 是 `[profile.release]`（`opt-level="s"` + `lto=true` + `strip=true`）。`build.rs` 是 **build script**，等价于 CMake 的 configure 阶段跑一段代码做代码生成：它读 `config/symbols.json`，生成一个 `.rs` 文件，再由 `math.rs:7` 用 `include!` 嵌进来。

### 第 2 章 · 猜数字游戏 【对照 / 可略读】

**内容**：一个完整小程序，串起 `let mut`、`String`、`stdin().read_line`、`parse`、`Result`、`match`、`loop`、外部 crate（`rand`）。

对 C++ 背景的人，这章的价值只是**熟悉语法形状**，20 分钟扫过。但注意三个 C++ 里没有的细节：

- `read_line(&mut String)` 返回 `Result<usize, io::Error>`——**输入也可能失败**，必须处理，C++ 的 `std::cin >>` 是静默失败。
- `parse::<u32>()` 返回 `Result`，而且**类型靠上下文推断**（这里由 `let guess: u32` 决定）。
- `match guess.cmp(&secret) { Ordering::Less => ..., }`——**枚举 + 穷尽匹配**第一次登场，为第 6 章铺路。

### 第 3 章 · 通用编程概念 【对照为主，4 个独特点】

**小节**：变量和可变性 / 数据类型 / 函数 / 注释 / 控制流

#### 对照表

| C++ | Rust | 备注 |
| --- | --- | --- |
| `int x = 5;` | `let x = 5;` | **默认不可变** |
| `int x = 5; x = 6;` | `let mut x = 5; x = 6;` | 可变要显式 |
| `const int N = 3;` | `const N: usize = 3;` | 必须有类型，编译期求值 |
| `constexpr` | `const` / `const fn` | 见 `typst.rs:95` |
| — | `static` | 全局，须 `Sync` |
| `int32_t` / `uint64_t` | `i32` / `u64` | |
| `size_t` | `usize` | 索引专用类型 |
| `bool` / `char` / `float` | `bool` / `char` / `f32`,`f64` | `char` 是 **4 字节 Unicode 标量**，不是字节 |
| `void f()` | `fn f()` | 返回 `()` |
| `a ? b : c` | `if a { b } else { c }` | `if` 是表达式 |
| `while` / `for(;;)` | `loop` / `while` / `for x in it` | |
| `switch` | `match` | 见第 6 章 |
| `//` `/* */` | `//` `/* */`，另有 `///` 文档注释 | |

#### 细讲 1：默认不可变，可变性是显式契约

```rust
let x = 5;
x = 6;              // 编译错误：cannot assign twice to immutable variable
let mut y = 5;
y = 6;              // OK
```

C++ 里你写 `int x` 它就可变，`const` 是额外的自律。Rust 反过来：**可变是额外的声明**。好处是读代码时，看到 `let x` 就知道它后面不会变——这是不用读全文就能得到的保证。

#### 细讲 2：Shadowing（遮蔽），C++ 里没有的写法

```rust
let spaces = "   ";          // &str
let spaces = spaces.len();   // usize，同名，类型都变了
```

同一个作用域里用 `let` 再声明一次同名变量，**旧变量被遮蔽**，新变量可以是不同类型。C++ 里同名重声明在**同一作用域是编译错误**，你只能换个名字或用嵌套块。

用途：把一个值逐步转换，而不用为每个中间态起名字。注意它和 `mut` 的区别：`mut` 是"同一个变量值可变"，shadowing 是"换了一个新变量"。

#### 细讲 3：表达式导向——函数体最后一行就是返回值

这是 Rust 语法上最容易让 C++ 人困惑的一点。

```rust
fn plus_one(x: i32) -> i32 {
    x + 1        // 注意：没有分号！这是返回值
}
```

加个分号 `x + 1;` 就变成"语句"，返回 `()`，类型不匹配编译报错。规则是：**块里的最后一个表达式，若无分号，就是它的值**。

推广到一切：

```rust
let y = if cond { 5 } else { 6 };   // if 是表达式
let z = { let a = 1; a + 2 };       // 块是表达式
let w = loop { break 7; };          // loop 也是表达式
```

C++ 里 `if` 是语句不能赋值；要靠三元运算符或函数。Rust 里没有三元运算符，因为不需要。

**本项目**：`services.rs:306-320` 的 `render` 全篇是这套写法——中间用 `?`，最后 `Ok(value)` 不带 `return`。`math.rs:58` 的 `usize::from(forward)` 也是表达式。

#### 细讲 4：没有隐式数值转换

```rust
let a: i32 = 5;
let b: i64 = a;        // 编译错误！C++ 里会隐式提升
let c: i64 = a as i64; // 必须显式转换
```

C++ 的隐式整型提升/截断是经典 bug 源，Rust 全部禁止，要转就写 `as`。代价是代码里 `as usize` 会很多——因为**索引必须是 `usize`**。

溢出行为也和 C++ 不同：

| 场景 | C++ | Rust |
| --- | --- | --- |
| debug 溢出 | UB 或回绕 | **panic**（帮你抓 bug） |
| release 溢出 | 回绕 | 回绕 |
| 想要确定语义 | 手工检查 | `saturating_add` / `wrapping_mul` / `checked_add` |

**本项目**：`typst.rs:107` 用 `saturating_add(1)`，`typst.rs:91` 用 `wrapping_mul(...)`——注意这两处是**刻意选了语义**的，不是随手写的。

#### 补充两个小点

- **`()`（unit type）** 就是 C++ 的 `void`，但它是真正的类型，所以 `Result<(), String>` 可以表示"成功但无返回值"。
- **数组越界是 panic，不是 UB**（`v[10]`）。想要 `Option` 用 `v.get(10)`。

#### 一些附注

关于 `loop(break 7;);`
##### Why this only works for `loop`

This is the key detail — **`loop` is the only loop that can do this**:

| Loop | Does `break value` compile? | Why |
| --- | --- | --- |
| `loop { }` | ✓ | Never exits "normally" — only via `break`, so the value is unambiguous |
| `while cond { }` | ✗ | Can exit because the condition turned false → what value then? |
| `for x in it { }` | ✗ | Can exit by exhausting the iterator → same problem |
| `while let ... { }` | ✗ | Same |

So `while`/`for` always evaluate to `()`. The type checker rejects `break 7;` inside them with something like "`break` with value outside of a `loop`".

This is exactly why `loop` exists as a separate construct at all — if you only had `while`/`for`, you'd have to declare a mutable variable outside and assign it inside, which the borrow checker then treats as "possibly uninitialized".

##### Why this is useful, not just a curiosity

The payoff is that the compiler proves the value is always assigned. Compare:

```rust
// Idiomatic Rust — no "possibly uninitialized" problem
let result = loop {
    let input = read_input();
    match parse(input) {
        Ok(v) => break v,
        Err(_) => continue,
    }
};

// C++ style translated literally
int result;                 // uninitialized
while (true) {
    auto input = read_input();
    if (parse(input, result)) break;
}
// compiler may warn: result may be uninitialized
```

Same behavior, but the first form makes "one of these paths must produce the value" a **type-level guarantee** rather than a warning you might ignore.

##### Bare `break` vs `break value`

```rust
let u = loop { break; };      // u == ()   (unit type)
let v = loop { break 7; };    // v == i32
```

Mixing them in one `loop` is a type error — every `break` in a given loop must agree, because they all feed the same expression's type.

##### A related fact I mentioned in the doc

There's also `loop`'s role with the never type `!`:

```rust
let x: i32 = loop { };   // compiles — this loop never exits, so it's valid as anything
```

A `loop` with no `break` has type `!`, which coerces to any type. That's the same machinery that makes `panic!()` and `return` usable in any position. It's covered in §第 20 章 of the walkthrough.


### 第 4 章 · 认识所有权 【独特 · 全书最重要】★

**小节**：什么是所有权？/ 引用与借用 / 切片 slice

这一章占全书分量最重，因为**它是 Rust 区别于其他所有语言的根**。C++ 里没有对应概念，所以下面全是细讲。

#### 4.1 三条规则

1. Rust 中每个值都有一个**所有者**（owner）。
2. 同一时刻只能有**一个**所有者。
3. 所有者离开作用域时，值被丢弃（`drop`）。

第 3 条是 RAII，你熟。第 1、2 条是新的。

#### 4.2 Move（移动）就是默认行为

```rust
let s1 = String::from("hello");
let s2 = s1;            // 所有权从 s1 移到 s2
println!("{s1}");       // 编译错误：borrow of moved value
```

C++ 这里会调用拷贝构造或移动构造（取决于 `s1` 是不是右值）。Rust 的语义是：**`String` 这样的堆分配类型，赋值就是移动，而且移动后源变量彻底失效**。

这和 C++ 的关键区别：C++ 移动后对象是"有效但未指定"，用了是逻辑错误、运行期才可能炸；Rust 移动后使用**编译不过**。

想真的复制：`let s2 = s1.clone();`。**`clone()` 是显式的、看得见的**——C++ 里一次函数传参可能悄悄拷贝一个 `std::string`，Rust 里那必然是 `.clone()` 或所有权转移。

#### 4.3 为什么函数传参也会移动

```rust
fn takes(s: String) { }        // 取得所有权
fn borrows(s: &String) { }     // 只借用

let s = String::from("x");
takes(s);
println!("{s}");               // 编译错误，s 已经被移进函数了
```

所以 Rust 代码里函数的签名会明确告诉你它会不会"吃掉"你的值——**看有没有 `&`**。这是 C++ 里靠 `const&` 还是 `&&` 还是值传递来表达、但没人能保证一致的东西，Rust 里由编译器强制。

#### 4.4 `Copy` 与 `Clone` 的分界

不是所有类型都移动。实现了 `Copy` trait 的类型是**按位复制**的，赋值后源变量仍然可用：

| 类别 | 行为 |
| --- | --- |
| 整数、`f32`/`f64`、`bool`、`char` | `Copy`，赋值即复制 |
| 只含 `Copy` 字段的元组/数组 | `Copy` |
| 共享引用 `&T` | `Copy` |
| `String`、`Vec<T>`、`Box<T>` | **不是 `Copy`**，赋值即移动 |
| `&mut T` | **不是 `Copy`**（刻意设计） |

记忆法：**管着堆内存或需要清理的类型不 `Copy`**。想给一个类型加 `Copy`，它必须先 `Clone`，且所有字段都 `Copy`。

#### 4.5 引用与借用规则

```rust
let mut s = String::from("hello");
let r1 = &s;         // 不可变借用，可以有很多个
let r2 = &s;
let w = &mut s;      // 编译错误：已有不可变借用时不能可变借用
```

**核心规则（记住这条，第 10 章的生命周期是它的延伸）：**

> 在同一时刻，对同一个值，**要么有任意多个 `&T`，要么有恰好一个 `&mut T`**，不能同时。

这条规则就是"无数据竞争"的全部来源。C++ 里你靠约定和代码评审保证"别在有引用时改这个容器"，Rust 把它变成编译期检查。

它也是**最开始最难受的地方**：很多 C++ 里顺手的写法（边遍历边插入、结构体内部互相引用）会编译不过。应对方式见 §3 的"报错怎么读"，最重要的一条是：**别对抗它，改设计**。

#### 4.6 NLL（非词法生命周期）

旧版 Rust 的借用按**词法块**结束，新版按**最后一次使用**结束：

```rust
let mut v = vec![1, 2, 3];
let first = &v[0];
println!("{first}");        // first 最后一次使用
v.push(4);                  // OK：first 的借用已经结束了
```

这在 C++ 里根本没有"违反不违反"的问题（编译器不管），但理解 NLL 能让你少写很多不必要的 `{}` 作用域。

#### 4.7 悬垂引用：编译期阻止

```rust
fn dangle() -> &String {
    let s = String::from("x");
    &s               // 编译错误：s 在函数结束时被 drop，引用会悬垂
}
```

C++ 里这是 UB，可能静默跑出垃圾数据。Rust 里直接编译不过——这是**第 10 章生命周期标注**要解决的问题：编译器需要知道"返回的引用活得和谁一样长"。

#### 4.8 切片 `&str` 与 `&[T]`

```rust
let s = String::from("hello world");
let hello = &s[0..5];        // &str，借用 s 的一部分
let nums = [1, 2, 3, 4];
let part = &nums[1..3];      // &[T]
```

`&str` 是**字符串切片**，`&[T]` 是数组切片。它们都是"指向一段连续内存 + 长度"的胖指针（C++ 里对应 `std::string_view` 和 `std::span`）。

书里 `first_word` 的例子点出了切片的真正价值：**切片把"这个结果和原数据有关联"编码进了类型**，所以"先取单词、再清空字符串、再用那个单词"这种 C++ 里的悬垂 bug，在 Rust 里编译不过。

**关键实践结论**：函数参数优先用 `&str` 而不是 `&String`，优先 `&[T]` 而不是 `&Vec<T>`。接受"更宽的借用"，调用方更自由。

#### 本项目

- `document.rs:21` 的 `Rc<RefCell<Option<(u64, Rc<Vec<...>>)>>>` 就是"共享所有权 + 内部可变"，见第 15 章。
- `math.rs:119-123` 的 `cell<'a>(root: &'a MathData, ...) -> &'a MathData` 是**生命周期标注**的直接例子：返回的引用借自 `root`。
- `typst.rs:44` 的 `source: impl Into<String>`：接受借用、内部转成拥有——这是"需要所有权时怎么优雅地收下参数"的标准做法。

### 第 5 章 · 使用结构体组织关联数据 【对照为主】

**小节**：定义和举例说明结构体 / 使用结构体的代码例子 / 方法语法

#### 对照表

| C++ | Rust | 备注 |
| --- | --- | --- |
| `struct S { int a; };` | `struct S { a: i32 }` | 字段**默认私有** |
| `class S { public: ... }` | `struct S { pub a: i32 }` | `struct` 和 `class` 无区别，只看 `pub` |
| `S s{1, 2};` | `S { a: 1, b: 2 }` | **必须写全字段名**（除非用 `..`） |
| 构造函数 `S()` | `fn new() -> Self`（**只是约定**） | 见下 |
| 成员函数 | `impl S { fn f(&self) }` | 定义在 `impl` 块里 |
| `this->a` | `self.a` | 显式写 `self` |
| `const` 成员函数 | `&self` | **类型系统强制** |
| 非 const 成员函数 | `&mut self` | |
| 静态成员函数 | `impl S { fn f() }` 无 `self` | 叫"关联函数" |
| 拷贝构造 | `#[derive(Clone)]` + `.clone()` | 不自动 |
| `operator<<` | `#[derive(Debug)]` + `{:?}` | |
| `operator==` | `#[derive(PartialEq)]` | |
| 静态成员变量 | 关联常量 `const X: i32 = 1;` | |

#### 细讲 1：没有构造函数，`new` 只是命名约定

Rust 里没有"构造函数"这个语言概念。`String::from("x")`、`Vec::new()` 都只是**返回 `Self` 的关联函数**。叫 `new` 是社区约定（`clippy` 会提醒你）。

```rust
impl MathAtom {
    pub fn character(value: char) -> Self {      // math.rs:43
        Self { kind: Kind::Char { value }, cells: vec![] }
    }
}
```

`Self` 是当前类型的别名。注意 `Self { kind, cells: vec![] }` 里的 `kind` 是**字段初始化简写**：变量名和字段名相同时可省 `kind: kind`。

#### 细讲 2：结构体更新语法

```rust
let b = S { a: 1, ..a_struct };   // 其余字段从 a_struct 取
```

C++ 里没有对应（要手写每个字段）。注意：**`..` 会移动非 `Copy` 字段**，所以之后 `a_struct` 可能不能整体用了。

#### 细讲 3：三种结构体

| 形式 | 例子 | 用途 |
| --- | --- | --- |
| 具名字段 | `struct S { a: i32 }` | 常规 |
| 元组结构体 | `struct Point(i32, i32);` | 字段无名，用 `.0` `.1` |
| 单元结构体 | `struct Marker;` | 不存数据，只用来实现 trait |

元组结构体配 `#[derive]` 是 newtype 模式的基础（第 19 章）。

#### 细讲 4：没有继承

Rust 的结构体**不能继承**。想要共享行为，用 trait（第 10 章）或组合（把另一个结构体作为字段）。这不是缺失，是刻意的设计决定——第 17 章整章在讨论这件事。

#### 本项目

`math.rs:14-17` 的 `MathAtom`（`struct` + `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]`）和 `view.rs:6-29` 的 `View`（字段上有 `#[serde(skip_serializing_if = ...)]`）都是典型例子。`math.rs:51` 的 `Self { kind, cells: vec![vec![]; cells] }` 用到了字段初始化简写。

### 第 6 章 · 枚举和模式匹配 【独特 · 第二重要】★

**小节**：定义枚举 / `match` 控制流运算符 / `if let` 简单控制流（英文版另有 `let...else`）

#### 先看对照，理解差距在哪

| C++ | Rust |
| --- | --- |
| `enum class Color { Red, Green };` | `enum Color { Red, Green }` |
| `std::variant<int, String>` | `enum E { I(i32), S(String) }` |
| `std::visit` + lambda | `match e { ... }` |
| `std::optional<T>` | `Option<T>` |
| `std::get_if` / `holds_alternative` | `if let Some(x) = ...` |
| `switch` 的 `default` | `_ =>` |

表面上像 `enum`，**实际上是代数数据类型（sum type）**——这才是 Rust 最有力的建模工具。

#### 细讲 1：枚举的每个变体可以带不同的数据

```rust
enum Kind {
    Char { value: char },
    Raw { source: String },
    Script { cell_1_is_up: bool },
    Grid { columns: usize },
}
```

（这就是你项目里 `math.rs:21-40` 的 `Kind`。）

C++ 的 `enum class` 只能表示"哪一个"；要附带数据必须上 `std::variant` + `std::visit`，写法笨重且容易漏分支。Rust 把两者合一，而且 `match` 是**语法级**支持。

**建模指导**：Rust 里表达"之一是 X、Y、Z"用 `enum`，表达"同时有 A、B、C"用 `struct`。**不要用继承层次表达"之一是"**——那是 C++ 的惯性，在 Rust 里会很难写。

#### 细讲 2：`match` 必须穷尽，且能绑定

```rust
match coin {
    Coin::Penny => 1,
    Coin::Nickel => 5,
    Coin::Dime => 10,
    Coin::Quarter(state) => { println!("{state:?}"); 25 }   // 绑定内部数据
}
```

三个关键点：

1. **必须覆盖所有可能**，漏一个就编译错误。C++ 的 `switch` 漏了 `case` 就静默走到 `default` 或什么都不做。
2. **无穿透**。C++ 的 `switch` 忘了 `break` 是经典 bug，Rust 每个分支独立。
3. **可以绑定变体里的数据**，`Coin::Quarter(state)` 直接把 `state` 取出来，不需要 `std::get`。

还能加**守卫**和匹配范围：

```rust
match n {
    0 => "zero",
    1..=9 => "digit",
    x if x < 0 => "negative",     // match guard
    _ => "big",
}
```

**这对你项目直接相关**：`view.rs:138-192` 和 `typst.rs:605-634` 都是穷尽 `match`，所以**加一个 `Kind` 变体会编译失败，直到你补上视图投影和源码回写**。这是编译器免费替你守的契约——上一轮讨论的"9 项义务里编译器只管 2 项"说的就是它。

#### 细讲 3：`Option<T>` 取代了 `null`

```rust
enum Option<T> { Some(T), None }
```

它不是内建魔法，就是标准库里的一个普通枚举。价值在于**类型系统逼你处理"可能没有"**：

```rust
fn f(x: Option<i32>) {
    x + 1;            // 编译错误：不能直接加
    if let Some(v) = x { v + 1 };   // 必须解构
}
```

C++ 的指针可以是 `nullptr`，`std::optional` 是可选用的——Rust 里**没有 `null`**，所有"可能为空"都是 `Option`，而且**不能不解构就用**。

#### 细讲 4：`if let` 与 `let...else`

当一个分支你不关心时，`match` 太啰嗦：

```rust
// match 写法
match config { Some(c) => run(c), None => {} }

// if let 写法
if let Some(c) = config { run(c); }
```

反向的"提前退出"用 `let...else`（这个中文版那节标题里还没有）：

```rust
let Some(index) = self.names.get(name) else { return None; };
```

（`typst.rs:63`，还用了 or-pattern：`let (Binding::Expandable(i) \| Binding::Opaque(i)) = ...`。）

`else` 分支**必须发散**（`return` / `panic!` / `continue`），因为它是"匹配失败就不能继续"的意思。C++ 里对应的是早退 + 一堆 `if (!p) return;`。

#### 本项目

`math.rs:21-40` 的 `Kind`、`cursor.rs:16-35` 的 `Action`、`typst.rs:54` 的 `Binding`、`view.rs:6-29` 的 `View`（这个是 struct）——你项目的数据模型几乎全是"枚举 + 结构体"的组合，这正是第 5、6 章的产物。

另外 `math.rs:77-80` 展示了 match guard 的实际用法：

```rust
match self.cells.len() {
    3 => Some(if up { 1 } else { 2 }),
    2 if cell_1_is_up == up => Some(1),     // guard
    _ => None,
}
```

### 第 7 章 · 使用包、Crate 和模块管理不断增长的项目 【混合】

**小节**：包和 crate / 定义模块来控制作用域与私有性 / 路径 / `use` 关键字 / 将模块分割进不同文件

#### 对照表

| C++ | Rust | 备注 |
| --- | --- | --- |
| `#include "x.h"` | `use crate::x;` | 但 Rust 不是文本包含 |
| `.h` 声明 + `.cpp` 实现 | **不分离**，同一个 `.rs` | 声明和实现在一起 |
| `#pragma once` / include guard | 不需要 | 模块本来就只加载一次 |
| `namespace foo { }` | `mod foo { }` | 概念最接近 |
| `foo::bar()` | `foo::bar()` | 路径写法几乎一样 |
| `using namespace std;` | `use std::collections::HashMap;` | 但 `use` 只引入**具体项** |
| `using Alias = T;` | `use Foo as Alias;` | |
| 一个 `.lib` / `.exe` | 一个 **crate** | 编译单元 |
| CMake target | **package**（含一个或多个 crate） | 一个 `Cargo.toml` |
| `private:` / `public:` | 默认私有，`pub` 显式 | |
| `static`（文件内可见） | 默认就是模块内可见 | |
| `friend class X;` | `pub(crate)` / `pub(super)` | |
| 前置声明 | 不需要 | 编译器全局解析 |

#### 细讲 1：crate 与 package 的区别

- **crate** = 一次编译的单元，产出一个 `.rlib` 或可执行文件。对应 C++ 的一个库/可执行目标。
- **package** = 一个 `Cargo.toml` + 它包含的 crate（最多一个库 crate，任意个二进制 crate）。

**和 C++ 最大的不同**：C++ 的库是"一堆目标文件 + 一堆头文件"，接口靠头文件表达；Rust 的 crate 是**单个编译单元**，接口由 `pub` 决定，编译器能看到全部内部结构。所以 Rust 能做跨模块内联和优化，C++ 只有 LTO 才能勉强接近。

#### 细讲 2：模块树与路径

```rust
crate::front_of_house::hosting::add_to_waitlist()
```

- `crate::` = 从当前 crate 根开始（绝对路径）
- `super::` = 父模块（≈ `..`）
- `self::` = 当前模块
- 外部 crate：直接用它名字开头，如 `std::io`

#### 细讲 3：隐私规则（和 C++ 反直觉的地方）

**默认私有。** 规则是：

> 一个项对**它所在的模块及其后代**可见；父模块**不能**访问子模块的私有项。

C++ 里 `private` 是"类内可访问"；Rust 的隐私是**模块级**的，而且方向和 C++ 的直觉相反：**子模块可以看父模块的私有项，父模块不能看子模块的私有项**。这条一开始很容易记反。

| 写 | 可见范围 |
| --- | --- |
| 无 | 本模块及后代 |
| `pub` | 任何人 |
| `pub(crate)` | 本 crate 内（≈ 内部 API） |
| `pub(super)` | 父模块 |
| `pub(in crate::x)` | 指定祖先模块 |

**注意**：结构体字段默认私有，但**枚举变体随枚举一起公开**（枚举公开则变体都公开）。这也是为什么 `Kind` 的构造只能在定义它的模块里写（`math.rs`），别处只能用 `MathAtom::character` 这类关联函数。

#### 细讲 4：多文件模块

Rust 2018+ 的规则很简洁：

```
src/lib.rs            →  crate 根
src/view.rs           →  mod view
src/math/mod.rs       →  mod math（旧风格）
src/math/mod.rs + src/math/symbols.rs  →  子模块
```

在 `lib.rs` 里写 `pub mod view;` 就等于把 `view.rs` 挂进树。**没有头文件，也没有 `#include` 的顺序问题。**

#### 细讲 5：`pub use` 重导出

```rust
pub use cursor::{Action, Editor};     // lib.rs:14
```

把子模块的项**在本层重新公开**，这样外部用 `visual_typst_core::Action` 而不必写 `visual_typst_core::cursor::Action`。这是**设计对外 API 的关键手段**：内部结构随便摆，对外只暴露想给的那几个名字。

#### 本项目

`lib.rs:3-12` 是模块树；`lib.rs:14` 的 `pub use cursor::{Action, Editor}` 就是重导出。`document.rs:3` 的 `use crate::{Action, Editor, math::*, typst};` 展示了 `crate::` 路径和 `*` 通配（`math::*` 把 `math.rs` 里所有 `pub` 项引进来，你项目里用得很重）。`main.rs` 是同一个 package 的第二个 crate（二进制）。

### 第 8 章 · 常见集合 【对照】

**小节**：vector / 字符串 / 哈希 map

#### 对照表

| C++ | Rust | 备注 |
| --- | --- | --- |
| `std::vector<T>` | `Vec<T>` | |
| `v.push_back(x)` | `v.push(x)` | |
| `v.pop_back()` | `v.pop()` | 返回 `Option<T>` |
| `v.size()` / `v.empty()` | `v.len()` / `v.is_empty()` | |
| `v[i]` | `v[i]` | 越界 **panic** |
| `v.at(i)` | `v.get(i)` | 返回 `Option<&T>` |
| `v.clear()` | `v.clear()` | |
| `std::string` | `String` | |
| `s += "x"` | `s.push_str("x")` / `s.push('c')` | |
| `s.length()` | `s.len()` | **字节数**，不是字符数 |
| `s.substr(a, n)` | `&s[a..a+n]` | 须在 UTF-8 边界上 |
| `s.find("x")` | `s.find("x")` | 返回 `Option<usize>` |
| `std::unordered_map<K,V>` | `HashMap<K, V>` | |
| `m[k]`（不存在则插入默认） | `m.insert(k, v)` | `m[&k]` 要求 `V: Default` |
| `m.find(k) != m.end()` | `m.get(&k)` → `Option<&V>` | |
| `m.count(k)` | `m.contains_key(&k)` | |
| `std::map`（有序） | `BTreeMap` | |
| `std::set` | `HashSet` / `BTreeSet` | |

#### 细讲 1：`String` 不能按整数索引

```rust
let s = String::from("hello");
let h = s[0];        // 编译错误！
```

因为 `String` 是 **UTF-8 字节序列**，一个"字符"可能是 1–4 字节，按字节索引会切出半个字符。C++ 的 `std::string` 是字节串，`s[0]` 合法但同样可能切坏多字节字符——Rust 直接禁止。

正确做法：

| 想要 | 写法 | 成本 |
| --- | --- | --- |
| 字节 | `s.bytes()` / `s.as_bytes()` | O(1) |
| Unicode 标量 | `s.chars()` | O(n) |
| 带位置的字符 | `s.char_indices()` | O(n) |
| 切片 | `&s[a..b]`，越界或不在边界则 **panic** | O(1) |
| 安全切片 | `s.get(a..b)` → `Option<&str>` | O(1) |

**`s.len()` 返回字节数**。要字符数得 `s.chars().count()`。

#### 细讲 2：`entry` API（C++ 里要写两步）

```rust
// C++: if (m.find(k) == m.end()) m[k] = 0; m[k] += 1;   // 两次查找
// Rust:
*map.entry(key).or_insert(0) += 1;
```

`entry` 返回一个"占位符"，`or_insert` 在缺失时插入。**只有一次查找**，且避免了"检查后再插入"的竞态。这是 Rust 集合里最值得学的惯用法。

#### 细讲 3：集合持有所有权

`Vec<T>` 拥有它的元素。想存**引用**就要带生命周期（第 10 章），所以实际代码里通常存拥有所有权的值，或者存 `String` 而不是 `&str`。

`HashMap` 的键必须实现 `Hash + Eq`。默认哈希是 SipHash（抗 HashDoS 攻击），比 C++ 的 `unordered_map` 慢一些但更安全；要更快可以换 hasher。

#### 细讲 4：迭代顺序

`HashMap` 的顺序**不确定**（每次运行可能不同）。C++ 的 `unordered_map` 同样如此，但很容易忘。要确定顺序用 `BTreeMap`。

#### 本项目

`Vec` / `String` / `HashMap` 无处不在。注意 `document.rs:146` 的 `HashMap<String,usize>` 作为"计数器"参数向下传（`counts`），以及 `view.rs:197` 的 `std::collections::HashMap` 全路径引用（那里没写 `use`）。

### 第 9 章 · 错误处理 【独特】★

**小节**：`panic!` 与不可恢复的错误 / `Result` 与可恢复的错误 / `panic!` 还是不 `panic!`

#### 对照表

| C++ | Rust |
| --- | --- |
| `throw` | `panic!`（不可恢复）或 `return Err(..)`（可恢复） |
| `try` / `catch` | `?` + `match` / `unwrap_or_else` |
| `std::expected<T,E>`（C++23） | `Result<T, E>` |
| `std::optional<T>` | `Option<T>` |
| `assert` / `abort` | `panic!` / `unreachable!` |
| `errno` / 返回错误码 | `Result` |
| `noexcept` | **默认**（panic 不走普通控制流） |
| 析构函数 | `Drop`（unwind 时也会跑） |

#### 细讲 1：两种错误，两条路

这是 Rust 错误处理的核心思想，也是和 C++ 最大的分歧：

> **可恢复的错误用 `Result` 返回值；不可恢复的错误用 `panic!`。绝不混用。**

C++ 用异常统一处理两者，结果是"这个函数会不会抛"变成一个人人都猜不准的问题（所以有了 `noexcept`、`-fno-exceptions` 这些补丁）。Rust 强制二分：**看函数签名就知道它会不会失败**。

- `fn f() -> i32` —— 不会失败（可能 panic，那是 bug）
- `fn f() -> Result<i32, E>` —— 会失败，**调用方必须处理**

#### 细讲 2：`panic!` 的机制

```rust
panic!("crash and burn");
```

默认行为：打印消息 + **栈回退（unwind）**，回退过程中所有值的 `Drop` 都会运行（这部分像异常）。可以在 `Cargo.toml` 里设 `panic = "abort"` 改成直接终止（体积更小）。

`panic!` **不能被普通 `catch` 捕获**（有 `catch_unwind`，但那是给 FFI 兜底用的，不是常规控制流）。所以 panic 表示"程序有 bug"，不是"预期内的失败"。

#### 细讲 3：`Result` 与 `?` 运算符

```rust
pub fn render(&self, mut req: RenderRequest) -> Result<Value, String> {
    crate::workspace::resolve(&self.workspace, &req.path)?;     // 失败就提前 return Err
    if let Some(end) = req.context_end.take() {
        req.source = context_source(&req.source, end)?;
    }
    ...
    Ok(value)
}
```

（`services.rs:306-320`）

`?` 做两件事：
1. **成功**：把 `Ok(v)` 里的 `v` 取出来，继续执行。
2. **失败**：立刻 `return Err(...)`，**并且自动经 `From` 做类型转换**。

第 2 点是关键：底层返回 `io::Error`，上层函数返回 `Result<_, String>`，`?` 会自动尝试 `String: From<io::Error>`。所以**错误类型能沿调用链自动升级**，不用写一堆 `try/catch` 或手工包装。

#### 细讲 4：常用手法

| 想干什么 | 写法 |
| --- | --- |
| 出错就崩（原型/测试） | `x.unwrap()` |
| 出错就崩，带说明 | `x.expect("配置缺失")` |
| 出错给默认值 | `x.unwrap_or(default)` / `unwrap_or_default()` |
| 出错时计算默认值 | `x.unwrap_or_else(\|e\| ...)` |
| 转换成功值 | `x.map(\|v\| v + 1)` |
| 转换错误值 | `x.map_err(\|e\| format!("失败：{e}"))` |
| 忽略错误（明确表示） | `let _ = f();` |
| 只关心成功与否 | `x.is_ok()` / `x.is_err()` |

**`let _ = f();` 是"我故意忽略"的标记**，不是随手写的。本项目 `services.rs:177` 就是这样处理 `kill()` 和 `wait()` 的返回值。

#### 细讲 5：什么时候该 panic

书里给的原则：

| 该 panic | 不该 panic |
| --- | --- |
| 违反了代码自身的不变量 | 用户输入不合法 |
| 示例、原型、测试 | 文件不存在、网络失败 |
| 调用方无法合理恢复的 bug | 任何"可以报告给用户"的情况 |

**库代码几乎不该 panic**。你项目里 `pub fn` 大量返回 `Result<_, String>`，错误字符串是给用户看的中文文案（如 `"取图区间不在编译上下文内"`），这正是"错误是 API 的一部分"的实践。

#### 本项目

全项目统一用 `Result<T, String>`（见 `services.rs:306`、`document.rs:80`、`typst.rs:388`）。用 `String` 当错误类型是**最简单但有代价**的选择：写法上省事，但调用方无法按错误种类分支处理。C++ 里对应"只用 `std::string` 当异常消息"。

另注意 `typst.rs:127`：锁中毒时**不 panic 而是恢复**，注释解释了理由——这是"panic 语义需要刻意设计"的好例子。

### 第 10 章 · 泛型、trait 与生命周期 【混合 · 必读精读】★

**小节**：泛型数据类型 / trait：定义共享的行为 / 生命周期与引用有效性

这一章是 Rust 抽象能力的全部来源，三部分各自独立。

#### 对照表

| C++ | Rust | 备注 |
| --- | --- | --- |
| `template<class T>` | `<T>` | |
| `concept C<T>` | `trait C` | 概念几乎同构 |
| `requires` 子句 | `where T: C` | |
| SFINAE / `enable_if` | **无对应** | 用 trait bound 表达 |
| 模板特化 | **无对应** | 用 trait 的不同 `impl` |
| CRTP 静态多态 | 泛型（默认就是静态分发） | |
| 抽象基类 + 虚函数 | `trait` + `dyn Trait` | |
| `std::function<R(A)>` | `Box<dyn Fn(A) -> R>` | |
| 模板必须放头文件 | 泛型可在任意文件 | 无此限制 |
| 编译期多态 | **单态化**（monomorphization） | |
| 运行期多态（vtable） | `dyn Trait`（胖指针） | |
| — | **生命周期** | C++ 无对应，Rust 独有 |

#### 细讲 1：泛型是零开销的，靠单态化

```rust
fn largest<T: PartialOrd>(list: &[T]) -> &T { ... }
```

编译器**为每个实际类型生成一份代码**（单态化），和 C++ 模板实例化一样。所以：

- 好处：零运行期开销，能内联。
- 代价：**代码膨胀**（每个类型一份），编译变慢。
- 对比：`dyn Trait` 是运行期虚表分发，只有一份代码但有间接调用开销。

**选择原则**：库的 API 用泛型（性能），需要"不同类型的集合"用 `dyn Trait`。

#### 细讲 2：trait = 接口 + 概念 + 运算符重载

```rust
pub trait Summary {
    fn summarize(&self) -> String;
    fn preview(&self) -> String {          // 默认实现，可以覆写
        format!("{}...", &self.summarize()[..20])
    }
}
impl Summary for Article { fn summarize(&self) -> String { ... } }
```

trait 一个概念覆盖了 C++ 里的：抽象基类、`concept`、`std::hash` 这类定制点、运算符重载（`impl Add`）、`std::function` 的接口部分。

**三种 bound 写法**，语义相同：

```rust
fn f<T: Summary>(x: T) { }              // 内联
fn f<T>(x: T) where T: Summary { }      // where 子句（复杂时更清晰）
fn f(x: impl Summary) { }               // 参数位 impl Trait
```

`impl Trait` 在**返回位**表示"返回某个实现了该 trait 的类型，但我不告诉你是哪个"：

```rust
fn make() -> impl Summary { Article { ... } }
```

#### 细讲 3：孤儿规则（coherence）

> 你只能为**自己的类型**实现 trait，或为**别人的类型**实现**自己的 trait**。不能给外部类型实现外部 trait。

这是为了**防止实现冲突**（两个 crate 给同一个类型实现同一个 trait，编译器无法选择）。C++ 的 ADL 和模板特化就没有这个保护，所以有 ODR 冲突。

想给外部类型加外部 trait 的方法叫 **newtype**（第 19 章）：用元组结构体包一层。

#### 细讲 4：生命周期——先纠正一个常见误解

> **生命周期不是"这个值活多久"，而是"这些引用之间的有效期关系"。**

编译器追踪的是"哪个引用至少活得和哪个一样长"。标注 `'a` 是在**声明关系**，不是在设置时长。

```rust
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
```

意思是："返回的引用，活得不会超过 `x` 和 `y` 中较短的那个"。调用方据此检查。

**省略规则**（记住这三条，你 90% 的情况都不用写生命周期）：

1. 每个引用参数各得一个生命周期。
2. 只有一个输入生命周期时，输出生命周期 = 它。
3. 有 `&self` 或 `&mut self` 时，输出生命周期 = `self` 的。

所以方法返回借用时通常不用标注——规则 3 自动处理。本项目 `math.rs:119` 的 `cell<'a>` 其实也能省略（只有一个输入引用），写上是为了可读性。

#### 细讲 5：结构体里存引用

```rust
struct Parser<'a> { input: &'a str }     // 必须标生命周期
```

存引用就要标注，这是很多人第一次真正需要写 `'a` 的地方。**简单做法是先不存引用**——存 `String` 拥有它，等性能真的成问题再优化。

**`'static`** 表示"活到程序结束"，字符串字面量就是 `&'static str`（`math.rs:138` 的 `Option<&'static str>`）。trait bound 里的 `T: 'static` 意思是"T 里不含短命引用"，和"活的久"不完全是一回事。

#### 本项目

- 泛型函数：`typst.rs:127` 的 `poison_free<T>(lock: &Mutex<T>) -> MutexGuard<'_, T>`（泛型 + 生命周期 + `'_` 省略）。
- `impl Trait` 参数：`math.rs:44` 的 `source: impl Into<String>`。
- 生命周期：`math.rs:119`、`document.rs:171` 的 `struct Locator<'a>`（**结构体存引用**的例子）。
- trait 实现：`impl Drop for RenderAdapter`（`services.rs:177`）、`impl Summary` 这类业务 trait 你项目还没定义，用到的全是标准库 trait（`Serialize`/`Deserialize`/`Clone`/`Debug`/`PartialEq`/`Drop`/`From`/`Into`）。

### 第 11 章 · 编写自动化测试 【对照】

**小节**：如何编写测试 / 控制测试如何运行 / 测试的组织结构

#### 对照表

| C++ | Rust | 备注 |
| --- | --- | --- |
| gtest / Catch2 / doctest | **内建，无需框架** | |
| `TEST(Suite, Name)` | `#[test] fn name()` | |
| `EXPECT_EQ(a, b)` | `assert_eq!(a, b)` | |
| `EXPECT_TRUE(x)` | `assert!(x)` | |
| `ASSERT_*`（中止用例） | 无区分，panic 即失败 | |
| `EXPECT_THROW` | `#[should_panic]` | |
| `SetUp` / `TearDown` | 无内建，用函数或 `Drop` | |
| CMake 注册测试 | 自动发现 | |
| `ctest -R pattern` | `cargo test pattern` | |
| 单独的测试可执行文件 | 与被测代码**同一文件** | |
| 头文件里的测试 | — | |
| — | 集成测试放 `tests/` | 只能测 `pub` API |
| — | **文档测试**（`///` 里的代码会被编译运行） | 独特 |

#### 细讲 1：测试和代码同文件，但只在测试时编译

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_equation_index_follows_every_source_change() {
        let mut doc = Document::default();
        doc.apply(json!({"action": "set_source", "source": "$a$"})).unwrap();
        assert_eq!(doc.equations().len(), 1);
    }
}
```

（`document.rs:255-283`）

`#[cfg(test)]` 是**条件编译**：这段代码在 `cargo build` 时根本不存在，只在 `cargo test` 时编译进去。所以测试可以访问**私有项**（`use super::*`），也不需要额外文件。

C++ 里测试要访问私有成员通常要 `friend` 或 `#define private public` 这种脏手段，Rust 里天然可以。

#### 细讲 2：单元测试 vs 集成测试

| | 位置 | 能测什么 | 像什么 |
| --- | --- | --- | --- |
| 单元测试 | 同文件 `#[cfg(test)] mod tests` | 包括私有项 | gtest 白盒测试 |
| 集成测试 | `tests/` 目录，每个文件是独立 crate | 只有 `pub` API | 黑盒/端到端 |

**`tests/` 里的测试是独立 crate**，只能 `use` 你库的公开接口。这是"验证对外 API 是否够用"的手段。

#### 细讲 3：常用命令

```powershell
cargo test                      # 全部
cargo test document::           # 名字或路径含 document:: 的
cargo test -- --nocapture       # 显示 println! 输出（测试默认捕获）
cargo test -- --test-threads=1  # 串行（默认并行）
cargo test --release            # 优化后测（测性能相关时）
cargo test --doc                # 只跑文档测试
```

测试**默认并行**运行，所以共享全局状态的测试要注意（你项目的 `MACROS` 是 `static Mutex`，就是第 16 章的并发问题）。

#### 细讲 4：文档测试（Rust 独有，很值钱）

`///` 注释里的代码块会被**当作测试编译并运行**：

````rust
/// 把两个数相加。
///
/// ```
/// assert_eq!(my_crate::add(1, 2), 3);
/// ```
pub fn add(a: i32, b: i32) -> i32 { a + b }
````

好处：**文档示例不会过时**——API 一改，文档测试就失败。C++ 里文档示例烂掉是常态，只能靠人肉 review。

#### 本项目

`document.rs:255` 有 `#[cfg(test)] mod tests`，`native-adapter/src/render.rs:162` 也有。运行入口见 `README.md:53-56`。

### 第 12 章 · 一个 I/O 项目：构建命令行程序 【对照 / 可略读】

**小节**：接受命令行参数 / 读取文件 / 重构以改进模块化与错误处理 / 采用 TDD / 处理环境变量 / 输出到标准错误

这是练手项目（写一个 `grep` 简化版）。C++ 对应物是 `getopt` + `ifstream` + `std::cerr`，不需要专门读。

**但有两处值得单独看**：

**1. `Box<dyn Error>` 作为错误类型的兜底**

```rust
fn main() -> Result<(), Box<dyn Error>> { ... }
```

底层可能返回 `io::Error`、`ParseIntError` 等不同错误，`?` 需要能统一转换。`Box<dyn Error>` 是"任意错误"的容器（≈ C++ 的 `catch (const std::exception&)` 但显式声明）。这是**快速原型阶段的标准做法**；想要精确的错误类型就用自定义 enum 或 `thiserror`。

**2. 把校验放进返回 `Result` 的关联函数**

```rust
impl Config {
    fn build(args: &[String]) -> Result<Config, &'static str> { ... }
}
```

而不是"先 `Config::new()` 再 `config.validate()`"。Rust 惯用法是：**能构造出来就一定是合法的**（让非法状态无法表示）。这是类型驱动设计的起点，C++ 里对应"构造函数 + 抛异常"，但 Rust 用返回 `Result` 的方式避免异常。

### 第 13 章 · Rust 中的函数式语言功能：迭代器与闭包 【混合】

**小节**：闭包 / 迭代器 / 改进 I/O 项目 / 性能比较

#### 对照表

| C++ | Rust | 备注 |
| --- | --- | --- |
| `[](int x){ return x+1; }` | `\|x\| x + 1` | 类型通常可推断 |
| `[&]` 按引用捕获 | **默认行为** | |
| `[=]` 按值捕获 | `move` 闭包 | |
| `[x]` 捕获某一个 | 自动按最小需要捕获 | 更精细 |
| `std::function<R(A)>` | `impl Fn(A) -> R` / `Box<dyn Fn(A) -> R>` | |
| `std::transform` | `.map()` | |
| `std::copy_if` | `.filter()` | |
| `std::accumulate` | `.fold(init, f)` / `.sum()` | |
| `std::ranges::views` | 迭代器适配器链 | |
| `std::count_if` | `.filter(..).count()` | |
| 手写 `for` 循环 | 迭代器链 | 通常更快或一样快 |

#### 细讲 1：闭包语法非常轻

```rust
let add_one = |x| x + 1;
let sum = |a, b| a + b;
```

参数和返回类型**通常完全不用写**（编译器推断）。C++ 的 lambda 语法（`[](){}`、`mutable`、显式返回类型）相比之下笨重得多。

#### 细讲 2：`Fn` / `FnMut` / `FnOnce`——这是 Rust 独特且重要的设计

闭包会根据它**怎么捕获环境**，自动实现三个 trait 中的一个或多个：

| trait | 捕获方式 | 能调用几次 | 例子 |
| --- | --- | --- | --- |
| `Fn` | 只读借用 `&T` | 任意次 | 纯读取外部变量 |
| `FnMut` | 可变借用 `&mut T` | 任意次 | 修改外部计数器 |
| `FnOnce` | **取得所有权** | **仅一次** | 把捕获的值移出去 |

```rust
let s = String::new();
let f = move || drop(s);      // 取得所有权，是 FnOnce
f();
f();                          // 编译错误：已经调用过了
```

**这是 C++ 完全没有的概念**。C++ 的 lambda 要么拷贝要么引用，没有"只能调用一次"的类型级约束。Rust 用这个区分，让编译器能精确检查"这个闭包会不会把资源吃掉"，也让 `move` 闭包能安全地跨线程传递（第 16 章）。

**实践含义**：函数参数写 `impl Fn(...)` 时，如果调用方传的闭包是 `FnOnce`，就编译不过——这时要放宽成 `impl FnOnce(...)`。**约束往宽了写是安全的**。

#### 细讲 3：`move` 闭包

```rust
std::thread::spawn(move || { for body in receiver { ... } });
```

（`services.rs:187`）

`move` 把闭包用到的变量**所有权移进闭包**。跨线程必须用 `move`，因为线程可能比创建它的函数活得久，不能借用栈上的东西。

C++ 里对应 `[=]` 按值捕获，但 Rust 的 `move` 是**移动而非拷贝**（对 `String` 这类类型），而且移动后原变量失效。

#### 细讲 4：迭代器是惰性的

```rust
let v = vec![1, 2, 3];
v.iter().map(|x| x * 2);              // 什么都不发生！
let doubled: Vec<_> = v.iter().map(|x| x * 2).collect();   // 现在才执行
```

适配器（`map`/`filter`/`zip`/`take`/`skip`/`chain`）只是**构造新迭代器**，不做计算。要**消费者**（`collect`/`sum`/`fold`/`count`/`for_each`/`for` 循环）才真正跑。

好处：编译器能把整条链融合成一个循环，**没有中间容器**——这就是"零成本抽象"。C++ 的 `std::ranges::views` 思路相同，但 Rust 生态里这是默认写法。

#### 细讲 5：`iter()` / `iter_mut()` / `into_iter()`

| 方法 | 产出 | 消耗集合吗 |
| --- | --- | --- |
| `.iter()` | `&T` | 否 |
| `.iter_mut()` | `&mut T` | 否 |
| `.into_iter()` | `T`（移动） | **是** |
| `for x in &v` | `&T` | 否 |
| `for x in v` | `T` | **是** |

最后一行是常见坑：`for x in v` 会把 `v` 移走，之后不能再用 `v`。想要借用就写 `for x in &v`。

#### 本项目

- `services.rs:187` 的 `move` 闭包（跨线程）。
- `view.rs:115` 的 `.filter(|_| ...)`。
- `document.rs:71-73` 的 `.iter().map(...).collect()`。
- `typst.rs:350` 的 `data.iter().all(|atom| ...)`——`all` 是消费者。
- `typst.rs:454` 的 `.any(|n| matches!(...))`。
- `typst.rs:99` 的 `.position(|entry| ...)` 返回 `Option<usize>`。

你项目里**几乎没有索引循环**，全是迭代器链——这是符合惯例的写法。

### 第 14 章 · 更多关于 Cargo 和 Crates.io 的内容 【独特 / 简讲】

**小节**：发布配置 / 发布到 crates.io / Cargo 工作空间 / `cargo install` / 自定义扩展命令

C++ 对照有限，直接讲。

#### 1. `[profile.*]`：构建配置

```toml
[profile.release]
opt-level = "s"    # 优化体积（默认 3 是优化速度）
lto = true         # 链接期优化
strip = true       # 去掉符号

[profile.dev]
debug = 0          # 关调试信息（默认是 2）
```

（这就是 `Cargo.toml:28-34`。）对应 C++ 的 `-O2 -flto -s -g0`，但**声明在项目文件里**而不是构建脚本里。可以自定义额外 profile（如 `[profile.profiling]` 继承自 release）。

#### 2. `features`：编译期可选功能

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
```

（`Cargo.toml:17`）`serde` 的 `derive` 支持是一个可选功能，不打开就不能用 `#[derive(Serialize)]`。

**这是 C++ 的 `#ifdef` + CMake option 的替代品**，但有两个关键优势：

- **声明式**：不写 `#ifdef`，被 feature 保护的代码用 `#[cfg(feature = "x")]`，编译器层面就看不到。
- **可传递合并**：如果 A 和 B 都依赖 C，A 开了 C 的 feature，**整个构建里 C 的该 feature 就是开着的**。这叫 feature unification，也是"feature 应当只增不减"这条社区规则的原因。

#### 3. `[workspace]`：多 crate 项目

多个 crate 共享一个 `Cargo.lock` 和 `target/`：

```toml
[workspace]
members = ["core", "adapter"]
```

对应 C++ 的 `add_subdirectory`，但依赖版本也是统一的。

**本项目**：根 `Cargo.toml` 和 `native-adapter/Cargo.toml` 是**两个独立 package**，没做成 workspace。所以构建原生适配器要显式指定 manifest 和 target-dir：

```powershell
cargo test --manifest-path native-adapter/Cargo.toml --target-dir target/adapter
```

（`README.md:55`）——`--target-dir` 是为了不和主项目的 `target/` 冲突。如果改成 workspace，这两条命令能简化成 `cargo test --workspace`。这是个可选的整理点，不是问题。

#### 4. 其他

- `cargo install <crate>`：从 crates.io 装上可执行文件（≈ `pip install`）。Tinymist 就可以这样装。
- `cargo publish`：发布到 crates.io（你项目 `Cargo.toml:6` 是 `publish = false`，明确不发布）。
- **自定义子命令**：任何名字为 `cargo-xxx` 的可执行文件在 `PATH` 上，就能用 `cargo xxx` 调用。这是 Cargo 的插件机制。

### 第 15 章 · 智能指针 【混合 · 必读】★

**小节**：`Box<T>` / `Deref` / `Drop` / `Rc<T>` / `RefCell<T>` 与内部可变性 / 引用循环

#### 对照表

| C++ | Rust | 备注 |
| --- | --- | --- |
| `std::unique_ptr<T>` | `Box<T>` | 但 `Box` **不可为空** |
| `std::shared_ptr<T>` | `Arc<T>`（多线程）/ `Rc<T>`（单线程） | |
| `std::weak_ptr<T>` | `Weak<T>` | |
| `operator*` / `operator->` | `Deref` / `DerefMut` | 且**会自动转换** |
| `~T()` 析构函数 | `Drop::drop` | |
| `mutable` 成员 | `Cell` / `RefCell` / `Mutex` | |
| 手工引用计数 | 自动 | |
| 可空指针 | `Option<Box<T>>` | |

#### 细讲 1：`Box<T>` —— 堆分配 + 唯一所有权

```rust
let b = Box::new(5);
```

三个用途：
1. **递归类型**：`enum List { Next(Box<List>) }`——不装箱的话编译器算不出大小（C++ 里递归类型也必须用指针，同理）。
2. **trait object**：`Box<dyn Trait>`。
3. **大对象转移**：避免在栈上拷贝（C++ 里靠移动语义，Rust 靠 Box）。

和 `unique_ptr` 的差别：`Box<T>` **不能是空的**，要可空必须写 `Option<Box<T>>`。这消除了 C++ 里"这个 unique_ptr 是不是 null"的不确定性。

#### 细讲 2：`Deref` 强制转换（Rust 独有机制）

```rust
let s = String::from("hi");
let r: &str = &s;          // &String 自动变成 &str
```

当类型实现了 `Deref<Target = U>` 时，`&T` 可以**自动**转成 `&U`。这解释了 Rust 里很多"看起来类型不对却编译通过"的地方：

- `&String` → `&str`
- `&Vec<T>` → `&[T]`
- `&Box<T>` → `&T`
- `&Rc<T>` → `&T`

C++ 里 `operator*` 和 `operator->` 需要你显式写，没有这种链式自动转换。这是 Rust 让人感觉"智能指针用起来像裸引用"的原因。

#### 细讲 3：`Drop` —— 不能显式调用

`Drop` 就是析构函数（`services.rs:177` 的 `impl Drop for RenderAdapter`）。一个独特限制：

```rust
let x = Thing::new();
x.drop();              // 编译错误：不能显式调用
drop(x);               // 正确：std::mem::drop 取得所有权后释放
```

为什么禁止？否则会出现**双重释放**（手动 drop 一次，离开作用域再 drop 一次）。C++ 里手动 `delete` 后再析构就是 UB，Rust 直接编译不过。

**释放顺序**：同一作用域内变量按**声明顺序的逆序** drop（和 C++ 一致）。

#### 细讲 4：`Rc<T>` —— 共享所有权（单线程）

```rust
let a = Rc::new(vec![1, 2, 3]);
let b = Rc::clone(&a);      // 只是计数 +1，不是深拷贝！
```

**命名陷阱**：`Rc::clone` 里"clone"听起来像深拷贝，实际上**只增加引用计数**。社区约定是写 `Rc::clone(&a)` 而不是 `a.clone()`，就是为了让读代码的人一眼看出这里没有拷贝数据。

限制：`Rc` **不是线程安全的**（计数不是原子的），不能跨线程。要多线程用 `Arc`。

#### 细讲 5：`RefCell<T>` 与内部可变性（Rust 独有）

问题：如果你只有 `&self`，怎么修改内部状态？C++ 的答案是 `mutable` 成员。

```rust
pub struct Document {
    index: RefCell<Option<(u64, Rc<Vec<...>>)>>,     // document.rs:21
}
```

`RefCell` 把借用检查**从编译期挪到运行期**：

```rust
let c = RefCell::new(5);
*c.borrow_mut() += 1;        // 可变借用
let r = c.borrow();          // 不可变借用
c.borrow_mut();              // panic！已有不可变借用
```

规则和第 4 章一样（一个 `&mut` 或任意个 `&`），只是**违反时 panic 而不是编译错误**。所以 `RefCell` 是"我知道这里安全，请放行"的显式声明——**代价是把编译期保证换成了运行期风险**。

**`Rc<RefCell<T>>`** 是"共享 + 可变"的经典组合，你项目 `document.rs:21` 用的就是它。对应关系：

| | 单线程 | 多线程 |
| --- | --- | --- |
| 共享所有权 | `Rc<T>` | `Arc<T>` |
| 内部可变 | `RefCell<T>` | `Mutex<T>` / `RwLock<T>` |
| 组合 | `Rc<RefCell<T>>` | `Arc<Mutex<T>>` |

`Cell<T>` 是更轻量的版本，只能整体读/写（不能借用内部），适合 `Copy` 类型的小状态。

#### 细讲 6：引用循环会泄漏

`Rc` 互相引用会让计数永不归零，内存泄漏。解决办法是用 `Weak<T>` 打破循环（`Weak::upgrade()` 返回 `Option<Rc<T>>`）。

**注意 Rust 的诚实之处**：它**不保证不泄漏内存**。泄漏是"安全"的（不会 UB），只是浪费。用 `Rc<RefCell<>>` 构造图结构时，泄漏是你要自己负责的事。

#### 本项目

`document.rs:21` 的 `Rc<RefCell<Option<(u64, Rc<Vec<(Equation, SyntaxNode)>>)>>>` 一次用全了三个概念：`Rc` 共享、`RefCell` 内部可变、`Option` 惰性缓存。`document.rs:62-68` 演示了**怎么用 `RefCell` 才不 panic**（把 `borrow()` 的作用域收紧到判断内部，再 `borrow_mut()`）。`typst.rs:82-83` 的 `OnceLock<Arc<MacroRegistry>>` 是"全局共享 + 多线程"的对应写法。

### 第 16 章 · 无畏并发 【混合 · 必读】★

**小节**：线程 / 消息传递 / 共享状态并发 / `Send` 与 `Sync`

#### 对照表

| C++ | Rust |
| --- | --- |
| `std::thread t(f);` | `std::thread::spawn(f)` |
| `t.join()` | `handle.join()` |
| `std::mutex` + `std::lock_guard` | `Mutex<T>` + `MutexGuard` |
| `std::condition_variable` | `Condvar` |
| （无标准库对应） | `mpsc::channel` |
| `std::atomic<T>` | `AtomicUsize` 等 |
| `std::shared_mutex` | `RwLock<T>` |
| **数据竞争 = UB** | **数据竞争 = 编译错误** |
| — | `Send` / `Sync` 标记 trait |

#### 细讲 1：为什么叫"无畏"

C++ 的并发问题是：数据竞争是 **UB**，编译器不会拦你，运行期随机崩溃，调试极难。

Rust 的所有权和借用规则**顺便**消灭了数据竞争：

- 共享可变状态必须经过 `Mutex` / `RwLock` / 原子类型，否则编译器不让你共享。
- 跨线程传值必须实现 `Send`。

所以"无畏"不是说并发变简单了，而是**并发错误在编译期就被抓出来**。

#### 细讲 2：`thread::spawn` 与 `move`

```rust
use std::thread;

let handle = thread::spawn(move || {
    println!("来自线程");
});
handle.join().unwrap();
```

`spawn` 要求闭包满足两个条件：

- **`'static`**：不能借用当前栈上的东西（线程可能在那之后还活着）。
- **`Send`**：闭包捕获的值必须能安全跨线程。

所以**几乎总是要写 `move`**，把数据的所有权移进线程。忘了 `move` 会得到"closure may outlive the current function"的报错。

`join()` 返回 `Result`——**子线程 panic 了，`join` 会返回 `Err`**。这是 Rust 里错误传播跨越线程边界的方式。

#### 细讲 3：`mpsc` 通道——用通信代替共享

```rust
let (tx, rx) = mpsc::channel::<Vec<u8>>();

thread::spawn(move || {
    for body in rx { /* 消费 */ }        // services.rs:187
});
tx.send(data).unwrap();
```

- `mpsc` = **m**ulti-**p**roducer **s**ingle-**c**onsumer。`tx` 可以 `clone()` 给多个生产者，`rx` 只有一个。
- `send` **转移所有权**到通道里——这是"用通信来共享内存，而不是用共享内存来通信"的字面实现。
- `recv()` 阻塞直到有消息；通道所有 `tx` 被丢弃后 `recv` 返回 `Err`，`for body in rx` 循环随之结束。

**C++ 标准库没有通道**（C++20 也没有），这是 Rust 相对 C++ 的一个实际便利。

本项目 `services.rs:184-196` 是标准用法：一个线程写 stdin，一个线程读 stdout，用通道把结果送回。

#### 细讲 4：`Arc<Mutex<T>>` —— 共享可变状态的经典组合

```rust
let counter = Arc::new(Mutex::new(0));
let c = Arc::clone(&counter);
thread::spawn(move || { *c.lock().unwrap() += 1; });
```

- **`Arc`** 负责"多线程共享所有权"（原子引用计数）。
- **`Mutex`** 负责"互斥访问"。

**Rust 的 `Mutex` 和 C++ 的关键区别**：数据**装在锁里面**。

```rust
let m = Mutex::new(5);
// m 不能直接用，必须先 lock
let mut guard = m.lock().unwrap();     // MutexGuard
*guard += 1;                            // 通过 guard 访问数据
// guard 离开作用域自动解锁（RAII）
```

C++ 里 `mutex` 和它保护的数据是两回事，靠约定关联（所以有了 `std::scoped_lock` + 手动配对）。Rust 里**不拿到锁就拿不到数据**，保护关系由类型系统保证，忘了解锁也不可能（`MutexGuard` 自动 `Drop`）。

`lock()` 返回 `Result` 是因为**中毒**（持有锁的线程 panic 了）——见 `typst.rs:127` 的处理。

#### 细讲 5：`Send` 与 `Sync` —— 编译期并发安全

两个**标记 trait**（没有方法，只用来做类型标记）：

| trait | 含义 |
| --- | --- |
| `Send` | 该类型的**所有权**可以转移到另一个线程 |
| `Sync` | 该类型的**引用** `&T` 可以共享给多个线程 |

规则：`T: Sync` 等价于 `&T: Send`。

绝大多数类型自动实现。关键的**例外**：

| 类型 | Send | Sync | 原因 |
| --- | --- | --- | --- |
| `Rc<T>` | ✗ | ✗ | 计数非原子 |
| `Arc<T>` | ✓ | ✓（若 `T: Sync`） | 计数原子 |
| `RefCell<T>` | ✓ | ✗ | 运行期借用检查非线程安全 |
| `Mutex<T>` | ✓ | ✓（若 `T: Send`） | 有锁保护 |
| 裸指针 `*const T` | ✗ | ✗ | 无安全保证 |

这解释了为什么**跨线程要把 `Rc` 换成 `Arc`、`RefCell` 换成 `Mutex`**——不是风格问题，是编译器强制。

`unsafe impl Send for X {}` 是"我手动保证它安全"的逃生口，标准库自己内部大量使用。

#### 本项目

`services.rs:176-205` 的 `RenderAdapter` 是这一章的综合案例：`mpsc::channel` + 两个 `thread::spawn(move ...)` + `Mutex<Option<RenderAdapter>>` 作为服务端单例（`services.rs:210`）。`typst.rs:77-83` 的注释还解释了**为什么用进程级 `static Mutex` 而不是 thread-local**——因为 HTTP/stdio 服务每个请求一个线程，thread-local 缓存等于没有缓存。

### 第 17 章 · 异步编程：Async、Await、Future 与 Stream 【独特 · 中文版无此章】

**小节**：Futures 与 async 语法 / 用 async 应用并发 / 处理任意数量的 future / Streams / async 的 trait / Futures、Tasks 与 Threads

**你项目没用 async，本章可以完全跳过。** 但既然它是英文版新增的一整章，给个定位：

| 概念 | 一句话 |
| --- | --- |
| `async fn` | 返回一个 `Future`，**惰性的**——不 `.await` 就不执行 |
| `.await` | 挂起当前任务（不阻塞线程），等结果就绪 |
| `Future` trait | 基于 `poll` 的状态机，`async`/`.await` 是它的语法糖 |
| 执行器 | **标准库没有**，要 `tokio` / `async-std` 等 runtime |
| `Stream` | 异步版的迭代器（`while let Some(x) = s.next().await`） |

**何时用**：IO 密集、海量并发连接（几千个 socket）。
**何时不用**：CPU 密集、少量后台任务。

**为什么你的项目不需要**：桌面 GUI 是"主线程 + 少量后台线程 + 一次一个编译请求"，线程模型完全够用且更简单。async 会引入 runtime 依赖，还有"函数颜色"问题（async 函数和普通函数不能随意互调）。

**建议**：现在不学。等真的需要处理大量并发 IO 时再回来。

### 第 18 章 · Rust 的面向对象编程特性 【独特】（中文版第 17 章）★

**小节**：面向对象语言的特点 / trait 对象 / 面向对象设计模式的实现

这一章对你特别重要，因为你在考虑重做内核——**它会告诉你 C++ 的哪些设计惯性在 Rust 里必须换掉。**

#### 细讲 1：Rust 支持 OOP 的哪些部分

| 特性 | Rust | 说明 |
| --- | --- | --- |
| 封装 | ✓ | `pub` / 私有 + `impl` |
| **继承（数据）** | **✗** | 结构体不能继承 |
| **继承（实现）** | **✗** | 没有"父类方法直接可用" |
| 多态 | ✓ | trait object 或泛型 |

**Rust 明确拒绝继承**，理由是：继承常被用来做两件不同的事，而两件都有更好的工具。

| 你想干什么 | C++ 的做法 | Rust 的做法 |
| --- | --- | --- |
| 复用代码 | 继承父类实现 | **组合**（作为字段）或 trait 默认方法 |
| 共享接口 | 抽象基类 + 虚函数 | `trait` |
| 表达"之一是" | 类层次 | `enum` |

最后一行是关键：**你在 C++ 里用继承层次表达的"之一是"，在 Rust 里应该用 `enum`。** 你项目的 `Kind`（`math.rs:21`）就是这个模式的教科书例子——用枚举而不是 `FracAtom : MathAtom` 这样的类层次。

#### 细讲 2：trait object 与 `dyn`

```rust
let items: Vec<Box<dyn Draw>> = vec![Box::new(Button), Box::new(Select)];
for item in &items { item.draw(); }
```

`dyn Trait` 是**胖指针**：一个数据指针 + 一个虚表指针。和 C++ 的关键区别：

| | C++ | Rust |
| --- | --- | --- |
| 虚表指针存哪 | **对象内部**（vptr） | **指针里** |
| 对象大小 | 变大（有 vptr） | 不变 |
| 如何多态调用 | 对象直接调用 | 必须通过 `&dyn` / `Box<dyn>` |

所以 Rust 里 `dyn Trait` **必须通过指针使用**，不能有 `dyn Trait` 类型的局部变量（它的大小未知，是 DST，见第 20 章）。

#### 细讲 3：对象安全（object safety）——设计 trait 时的硬约束

不是所有 trait 都能当 `dyn` 用。要求（简化版）：

- 方法不能返回 `Self`
- 方法不能有泛型参数
- 不能有 `Self: Sized` 之外的静态方法

**实践含义**：如果你打算让这个 trait 能当 `dyn Trait` 用（放进 `Vec<Box<dyn T>>`），**就不能在它里面写泛型方法**。这是很多人设计 trait 时踩的坑。

#### 细讲 4：泛型 vs `dyn` —— 怎么选

| | 泛型 `<T: Trait>` | `dyn Trait` |
| --- | --- | --- |
| 分发方式 | 静态（单态化） | 动态（虚表） |
| 性能 | 可内联，更快 | 一次间接调用 |
| 代码量 | 每个类型一份 | 一份 |
| 二进制体积 | 大 | 小 |
| 异构集合 | **做不到** | 可以 |
| 编译期检查 | 更彻底 | 只能检查 trait 部分 |

**决策规则**：
- 类型在编译期就知道、追求性能 → 泛型。
- 需要把不同类型放进同一个集合 → `dyn`。
- 库的公共 API 想稳定 ABI / 减小体积 → `dyn`。

#### 细讲 5：状态模式——Rust 用类型系统做得更强

书里先按传统 OOP 写状态模式（用一个 `state` 字段 + trait object），然后给出**更好的 Rust 版本**：把每种状态做成**不同的类型**，于是"在草稿状态调用发布"这种非法操作**编译期就不存在**。

这叫"让非法状态无法表示"（make illegal states unrepresentable）。C++ 里也能做到（模板），但 Rust 的类型系统和枚举让这件事自然得多。

#### 对内核设计的直接启示

| C++ 惯性 | Rust 里应该 |
| --- | --- |
| `MathAtom` 基类 + 各种子类 | `enum Kind`（你已经这么做了） |
| 虚函数表做多态 | `match`（快且穷尽）或 `trait` + `dyn` |
| 用继承共享代码 | 组合或 trait 默认方法 |
| 运行时用 `type` 字段判断类型 | `match` 枚举，编译器保证穷尽 |

**你项目现有设计（`Kind` 枚举 + `match`）是完全正确的 Rust 风格**，不需要改成 OOP。

### 第 19 章 · 模式和匹配 【独特】（中文版第 18 章）

**小节**：所有可能会用到模式的位置 / Refutability / 模式语法

第 6 章讲了 `match` 的基础，这一章是完整的模式语法。

#### 细讲 1：模式能出现的地方（比 C++ 多得多）

| 位置 | 例子 |
| --- | --- |
| `match` 分支 | `Some(x) => ...` |
| `if let` | `if let Some(x) = opt { }` |
| `while let` | `while let Some(x) = it.next() { }` |
| `for` | `for (i, v) in v.iter().enumerate()` |
| `let` | `let (a, b) = pair;` |
| 函数参数 | `fn f((a, b): (i32, i32))` |
| 闭包参数 | `\|(a, b)\| a + b` |

C++17 有结构化绑定（`auto [a, b] = pair;`），但只有 `let` 那一行对应。Rust 的模式贯穿整个语言。

#### 细讲 2：可反驳性（refutability）

| 类型 | 含义 | 能用在哪 |
| --- | --- | --- |
| **不可反驳** | 一定能匹配 | `let`、函数参数、`for` |
| **可反驳** | 可能不匹配 | `match`、`if let`、`while let` |

```rust
let Some(x) = opt;              // 编译错误：可反驳模式不能用于 let
let Some(x) = opt else { ... };  // 正确：let-else 处理不匹配的情况
```

`let` 没有"不匹配怎么办"的分支，所以只接受一定成立的模式。这条规则解释了为什么需要 `let...else`。

#### 细讲 3：模式语法全表

| 语法 | 例子 | 说明 |
| --- | --- | --- |
| 字面量 | `1 => ...` | 精确匹配 |
| 命名变量 | `x => ...` | **绑定**（不是比较！见下） |
| 多模式 | `1 \| 2 => ...` | 或 |
| 范围 | `1..=5 => ...` | 闭区间（`..=`，不是 `..`） |
| 解构结构体 | `Point { x, y } => ...` | 字段简写 |
| 解构枚举 | `Some(x) => ...` | |
| 嵌套解构 | `Some(Point { x, .. })` | |
| 忽略整体 | `_ => ...` | 不绑定 |
| 忽略部分 | `Point { x, .. }` | |
| 守卫 | `x if x > 0 => ...` | 附加条件 |
| `@` 绑定 | `n @ 1..=5 => ...` | 既匹配范围又绑定值 |

#### 细讲 4：最大的陷阱——`match` 里的裸变量名总是绑定

```rust
let x = Some(5);
match x {
    Some(5) => println!("五"),
    y => println!("其他：{y:?}"),      // y 捕获一切，不是"和外部变量 y 比较"
}
```

C++ 的 `switch` 里 `case` 是**常量比较**；Rust 的 `match` 里写一个名字是**新绑定**，会匹配任何值。

这有个隐蔽变体：

```rust
let expected = 5;
match n {
    expected => println!("相等"),     // 永远匹配！expected 成了新变量
    _ => println!("不等"),            // 永远不执行，且编译器会警告
}
```

要用常量比较，得写 `5` 或 `const EXPECTED: i32 = 5;` 然后用那个常量名（常量是常量模式，不是绑定）。

#### 细讲 5：`ref` 与 match ergonomics

```rust
match &opt { Some(x) => ... }    // x 自动是 &T，不用写 ref
```

现代 Rust 里，匹配一个引用时会**自动**按引用绑定（match ergonomics），所以 `ref` 关键字很少需要手写。知道它存在即可。

### 第 20 章 · 高级特征 【独特】（中文版第 19 章）★

**小节**：不安全的 Rust / 高级 trait / 高级类型 / 高级函数与闭包 / 宏

#### 细讲 1：`unsafe` 的五种超能力

`unsafe` **不是"关掉所有检查"**，只开放五件事：

1. 解引用裸指针 `*const T` / `*mut T`
2. 调用 `unsafe` 函数或方法
3. 访问或修改可变 `static`
4. 实现 `unsafe` trait
5. 访问 `union` 的字段

**重要的社区惯例**：每个 `unsafe` 块都要写 `// SAFETY:` 注释说明为什么安全。`clippy` 会提醒你。

FFI（调用 C 库）就是 `extern "C"` + `unsafe`：

```rust
unsafe extern "C" { fn abs(input: i32) -> i32; }
```

**你项目基本不需要 `unsafe`**——依赖里的 `typst` 内部有，但你自己的 `src/` 是纯安全代码。这是好事。

#### 细讲 2：高级 trait

| 概念 | 语法 | C++ 对应 |
| --- | --- | --- |
| 关联类型 | `trait T { type Item; }` | 模板里的 `typedef` |
| 默认泛型参数 | `trait Add<Rhs = Self>` | 无 |
| 运算符重载 | `impl Add for Point` | `operator+` |
| 父 trait | `trait A: B` | 无直接对应 |
| 完全限定语法 | `<Dog as Animal>::baby_name()` | `Dog::Animal::baby_name()` |
| newtype | `struct Wrapper(Vec<String>)` | 无（但思路是包装类） |
| 关联常量 | `trait T { const N: usize; }` | `static constexpr` 成员 |

**关联类型 vs 泛型参数的差别**：`Iterator` 用关联类型（`type Item`），所以一个类型只能有一种 `Item`。这比 C++ 模板更严格，也更符合直觉。

**newtype 的用途**：绕过孤儿规则（第 10 章）。想给 `Vec<T>` 实现一个外部 trait，就用 `struct Wrapper(Vec<T>)` 包一层。

#### 细讲 3：高级类型

| 概念 | 说明 |
| --- | --- |
| **newtype** | 零开销的类型安全包装：`struct Meters(f64)` 和 `struct Feet(f64)` 不能混用 |
| **类型别名** | `type Km = HashMap<String, i32>;`——**只是别名，不是新类型** |
| **never type `!`** | 永不返回的函数的返回类型（`panic!`、`loop{}`、`return`）。可强制转换成任何类型 |
| **DST（动态大小类型）** | `str`、`[T]`、`dyn Trait`——大小未知，**必须通过指针使用**（`&str`、`Box<dyn T>`） |
| **`Sized`** | 泛型默认要求 `T: Sized`，用 `?Sized` 放宽 |

**`!` 的实际影响**：`let x = loop { break 5; };` 能工作就是因为 `loop` 的类型是 `!`，可以被当作任何类型。`unreachable!()` 和 `panic!()` 也一样。

#### 细讲 4：函数指针与返回闭包

```rust
fn apply(f: fn(i32) -> i32, x: i32) -> i32 { f(x) }    // 函数指针
fn make() -> impl Fn(i32) -> i32 { |x| x + 1 }          // 返回闭包
```

`fn(i32) -> i32`（小写）是**函数指针类型**，对应 C++ 的函数指针。闭包**不能**写成这个类型（它可能有捕获），要用 `impl Fn` 或 `Box<dyn Fn>`。

返回闭包必须用 `impl Fn` 或 `Box<dyn Fn>`，因为闭包类型没有名字。

#### 细讲 5：宏

| 类型 | 例子 | 说明 |
| --- | --- | --- |
| 声明宏 | `macro_rules! vec { ... }` | 模式匹配式，**卫生**（hygienic） |
| 派生宏 | `#[derive(Serialize)]` | 过程宏，生成 impl |
| 属性宏 | `#[route(GET, "/")]` | 过程宏，改写被标注的项 |
| 函数式宏 | `sql!("SELECT ...")` | 过程宏，像函数一样调用 |

**"卫生"是什么意思**：宏内部引入的变量名不会意外和外部的同名变量冲突。C++ 的 `#define` 是**纯文本替换**，这是 `#define max(a,b)` 各种诡异 bug 的根源。Rust 的宏在语法树层面工作，安全得多。

**你项目用到的宏**：

| 宏 | 位置 |
| --- | --- |
| `vec!` | 到处都是（`math.rs`、`view.rs`） |
| `format!` | `typst.rs:131`、`view.rs:183` |
| `json!` | `document.rs`、`services.rs`（serde_json） |
| `matches!` | `typst.rs:454`、`math.rs:98` |
| `include!` | `math.rs:7`（嵌 build.rs 生成的文件） |
| `concat!` / `env!` | `math.rs:7`（拼 OUT_DIR 路径） |
| `#[derive(...)]` | `math.rs:13`、`view.rs:6`（派生宏） |
| `#[cfg(test)]` | `document.rs:255` |
| `?`（不是宏，是语法） | `services.rs` 全篇 |

### 第 21 章 · 最后的项目：构建多线程 Web 服务器 【可略读】（中文版第 20 章）

**小节**：单线程 Web 服务器 / 改为多线程 / 优雅停机与清理

练手项目，但**有两节值得看**：

**1. 21.2 的多线程部分** = `Arc<Mutex<mpsc::Receiver<Task>>>` 的经典用法。

线程池的 worker 们要共享**同一个** `Receiver`，但 `Receiver` 不是 `Sync`，所以必须用 `Arc<Mutex<...>>` 包起来，每个 worker 循环里 `lock()` 抢任务。这是"共享状态"和"消息传递"两种并发风格的结合点，很能加深对第 16 章的理解。

**2. 21.3 的优雅停机**用 `impl Drop for ThreadPool` 实现——**和 `services.rs:177` 的 `impl Drop for RenderAdapter` 是同一个手法**（在 `Drop` 里 kill/join 子进程或线程）。这一节可以直接对照你项目的代码读。

另外 `take(2)` 限制读取长度（而不是信任客户端给的 Content-Length）是"输入验证"的好例子。

### 附录 A–G

| 附录 | 内容 | 对你有用程度 |
| --- | --- | --- |
| A 关键字 | Rust 全部关键字表 | 查表 |
| B 运算符与符号 | 含 `?` `@` `..=` `::` `\|` 等 | 查表 |
| **C 可派生的 trait** | `#[derive]` 能用的全部 trait 及含义 | **重要**，见下 |
| D 实用开发工具 | rustfmt / clippy / rust-analyzer / cargo fix | 必装 |
| **E 版本（editions）** | edition 机制 | **重要**（你项目是 2024） |
| F 本书译本 | 各语言版本 | 中文版链接 |
| G Nightly Rust | nightly 机制 | 暂时无关 |

**附录 C 补充**（`#[derive]` 能派生的 trait）：

| trait | 作用 | 注意 |
| --- | --- | --- |
| `Clone` | 显式深拷贝 | 所有字段都须 `Clone` |
| `Copy` | 赋值即复制 | 须先 `Clone`，且所有字段 `Copy` |
| `Debug` | `{:?}` 打印 | 调试必备，几乎所有类型都加 |
| `PartialEq` / `Eq` | `==` | `Eq` 要求自反（浮点数不满足） |
| `PartialOrd` / `Ord` | `<` `>` 排序 | |
| `Hash` | 可作 `HashMap` 键 | 通常和 `Eq` 一起 |
| `Default` | `Default::default()` | 常配 `..Default::default()` |

**注意**：`derive` 是**全有或全无**——所有字段都实现了该 trait 才能派生。你项目 `math.rs:13` 的 `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]` 就是典型组合（`Serialize`/`Deserialize` 来自 serde 的派生宏，不是标准库）。

---

## 附录 A. 学习路线（针对你的目标）

| 阶段 | 做什么 | 时间 |
| --- | --- | --- |
| **1. 打地基** | 读第 4、5、6 章。所有权、结构体、枚举+匹配 | 2–3 天 |
| **2. 会写代码** | 读第 9、10、7 章。错误处理、泛型/trait/生命周期、模块 | 3–4 天 |
| **3. 动手** | [Rustlings](https://github.com/rust-lang/rustlings) 刷完（约 90 个练习） | 3–5 天 |
| **4. 读写你的代码** | 读第 15、16、13、8 章，同时读 `src/` | 2–3 天 |
| **5. 改你的代码** | 读第 19、20 章（模式、高级特性），做 §6 的"加一个 Kind"练习 | 3–5 天 |
| **6. 按需** | 第 11 章测试、第 14 章 Cargo、第 18 章 OOP、第 17 章 async | 用时再读 |

**第 12、21 章和附录可以完全不读**，或者只挑我上面标出的那几节。

**一个加速建议**：直接从你项目的 `math.rs`（142 行）开始读。它短、独立、用了 `enum` / `trait` 派生 / 泛型 / 生命周期 / `matches!` / 迭代器——**是第 5、6、10、19 章的综合练习**。读完它再读 `view.rs` 和 `document.rs`，会比按顺序读教材更快建立直觉。

## 附录 B. 相关资料

| 资料 | 链接 | 用途 |
| --- | --- | --- |
| The Book（英文） | <https://doc.rust-lang.org/book/> | 本导读对应的教材 |
| The Book（中文） | <https://kaisery.github.io/trpl-zh-cn/> | 对照阅读 |
| Rust for C++ Developers | <https://microsoft.github.io/RustTraining/c-cpp-book/> | **同背景，讲法最贴合** |
| Comprehensive Rust（[中文](https://google.github.io/comprehensive-rust/zh-CN/)） | <https://google.github.io/comprehensive-rust/> | 更快的系统化课程 |
| cheats.rs | <https://cheats.rs/> | 单页速查 |
| Rust by Example | <https://doc.rust-lang.org/rust-by-example/> | 例子驱动 |
| Rustlings | <https://github.com/rust-lang/rustlings> | 动手练习 |
| std 文档 | <https://doc.rust-lang.org/std/> | 查 API |
| Rust Reference | <https://doc.rust-lang.org/reference/> | 语言规格 |
| Rustonomicon | <https://doc.rust-lang.org/nomicon/> | `unsafe` 深入（本项目用不到） |

配套的本仓库文档：`docs/rust-for-cpp.md`（C++ 对照速查 + 本项目代码讲解 + 报错速查）。

## 附录 C. 一个自检清单

读完上面这些，你应该能不查资料回答：

- [ ] `let s2 = s1;` 之后为什么不能用 `s1`？什么时候可以？
- [ ] 同一时刻最多有几个 `&mut T`？和 `&T` 能共存吗？
- [ ] `clone()` 和 `Copy` 的区别是什么？
- [ ] `match` 里写一个裸变量名会发生什么？（陷阱）
- [ ] `?` 运算符做了哪两件事？
- [ ] `RefCell` 和 `Mutex` 分别用在什么线程模型下？
- [ ] `Rc::clone` 会拷贝数据吗？
- [ ] `dyn Trait` 为什么必须通过指针使用？
- [ ] 为什么 Rust 没有继承？你项目里 `Kind` 为什么用枚举而不是类层次？
- [ ] `Send` 和 `Sync` 分别是什么的简称？
- [ ] trait 的"孤儿规则"是什么？怎么绕过？
- [ ] `let...else` 的 `else` 分支为什么必须发散？

答不上来的，回对应章节看。

