// SPDX-License-Identifier: GPL-2.0-or-later
//! 解析器**真正会存进树里**的 `Kind` 有哪些。
//!
//! 这次重构把一批变体留成了**形状描述符**：`slots::configured_kind` 会返回它们、
//! `Decl` 描述它们、`view_atom` 按它们画，但**已经没有任何源码能把它们放进树里**。
//! 这个区别正是重构的要点，而此前**没有任何测试守着它**——把 `Kind::Sqrt` 重新变成
//! 会存的节点，照样能编译、照样能往返，把收拢悄悄退回去。
//!
//! 判据（见 `docs/architecture.md`）：一个构造要是自己的 `Kind`，只有两个理由——
//! **带着名字与配置都给不出的实例数据**（`Table.columns`、`Fenced.left/right`、
//! `Multiline.row_lengths`、`Raw.source`、`Char.text`……），或者**要保住书写形式**
//! （`a/b` 的 `SkewedFraction` 属于这条）。
//!
//! 两个变体永远不在这个集合里，但**不是**因为它们多余：`Parameter` 与
//! `TemplateCall` 只存在于宏模板的内部树，上线前就被 `bind_template_inner` 换掉
//! （`docs/kind-inventory.md` 第五节），这里读的是可编辑树，所以看不到它们。

use std::collections::BTreeSet;
use visual_typst_core::math::Kind;
use visual_typst_core::{Action, Editor};

fn variant(kind: &Kind) -> &'static str {
    // 穷尽 match：新增一个 `Kind` 时编译器会要求在这里给它一个名字，
    // 于是"它算不算会被存下来的 kind"这个问题必须被回答一次。
    match kind {
        Kind::Char { .. } => "Char",
        Kind::Symbol { .. } => "Symbol",
        Kind::Number => "Number",
        Kind::Raw { .. } => "Raw",
        Kind::MacroCall { .. } => "MacroCall",
        Kind::TemplateCall { .. } => "TemplateCall",
        Kind::Parameter { .. } => "Parameter",
        Kind::Text => "Text",
        Kind::Fraction => "Fraction",
        Kind::Sqrt => "Sqrt",
        Kind::Root => "Root",
        Kind::Scripts => "Scripts",
        Kind::Fenced { .. } => "Fenced",
        Kind::Table { .. } => "Table",
        Kind::Multiline { .. } => "Multiline",
        Kind::Accent { .. } => "Accent",
        Kind::Line { .. } => "Line",
        // 穷尽 match 在这里就问了那个必须回答的问题：`Style` 会不会被存进树？答案是不会
        // ——它和 `Accent`/`Line` 同类，是**形状描述符**：`bold(A)` 存成
        // `MacroCall{ name: "bold" }`，形状由名字查出来。所以它不在下面的 `expected` 里，
        // 而这个 match 让"新增一个 Kind 却不回答这个问题"变成编译错误。
        Kind::Style { .. } => "Style",
        Kind::Unknown { .. } => "Unknown",
    }
}

fn collect(data: &[visual_typst_core::math::MathAtom], out: &mut BTreeSet<&'static str>) {
    for atom in data {
        out.insert(variant(&atom.kind));
        for cell in &atom.cells { collect(cell, out); }
    }
}

/// 覆盖每一种"命令写法"与每一种"语法写法"的一份语料。
///
/// 两栏都在，是因为收敛之后**同一种排版有两条路**：`frac(a, b)` 走名字表、`a/b`
/// 走 `MathFrac` 语法，`sqrt(x)` 与 `√x` 同理。它们落到同一个节点（`MacroCall`）是
/// 有意的；`a/b` 落在 `Kind::Fraction` 上也是——那一个还没定（见待定项
/// `SkewedFraction`）。
const SOURCES: &[&str] = &[
    // 叶子与容器
    "x", "alpha", "12.5", "dif", "\"txt\"",
    // 语法构造（没有名字）
    "x^2", "x_1^2", "(a + b)", "|x|", "a/b", "a & b \\\n c & d",
    // 命令写法：九个借形状的名字
    "frac(a, b)", "sqrt(x)", "root(3, x)", "hat(x)", "overline(x)", "underline(x)",
    "abs(x)", "norm(x)", "cancel(x)",
    // radical 语法折进同一个节点
    "√x", "∛x",
    // 一个 grid 形状的三个名字
    "mat(1, 2; 3, 4)", "mat(a, b; c)", "vec(1, 2, 3)", "cases(1, 2)",
    // 名字没进表 → 引擎自己画
    "bb(A)", "lr((x), size: #150%)",
];

#[test]
fn the_parser_stores_only_the_kinds_that_carry_their_own_data() {
    let mut seen: BTreeSet<&'static str> = BTreeSet::new();
    for source in SOURCES {
        let mut editor = Editor::default();
        editor.apply(Action::Import { source: (*source).into() })
            .unwrap_or_else(|error| panic!("导入 {source:?} 失败：{error}"));
        collect(&editor.root, &mut seen);
    }
    // 命令草稿是纯编辑器状态，没有 Typst 拼写，只能靠动作到达。
    let mut draft = Editor::default();
    draft.apply(Action::Input { text: "\\fra".into() }).unwrap();
    collect(&draft.root, &mut seen);

    let expected: BTreeSet<&'static str> = [
        "Char", "Symbol", "Number", "Raw", "MacroCall", "Text",
        "Fraction", "Scripts", "Fenced", "Table", "Multiline", "Unknown",
    ].into_iter().collect();
    assert_eq!(
        seen, expected,
        "解析器存下来的 Kind 集合变了。\n\
         多出来的：{:?}\n少掉的：{:?}\n\
         一个构造要留下，得带着名字与配置都给不出的实例数据，或者要保住书写形式；\
         否则它该是一个从名字查出来的**形状**，而不是树里的节点（见 docs/architecture.md）。",
        seen.difference(&expected).collect::<Vec<_>>(),
        expected.difference(&seen).collect::<Vec<_>>(),
    );
}

/// 反过来说：四个变体**只在形状表里活着**，`configured_kind` 会返回它们，但没有任何
/// 源码能把它们存进树——`Sqrt`/`Root` 的两种写法都折进了 `MacroCall`，`Accent`/`Line`
/// 的命令名由配置供给。
///
/// 它们仍然必须存在：`configured_kind` 拿它们当形状返回，`Decl` 描述它们的槽位，
/// `view_atom` 按它们画。所以这不是"该删没删"，而是**这个 `Kind` 变体身兼两职**的
/// 证据——这也是为什么 `command_shape()` 与 `is_macro()` 必须存在。
#[test]
fn four_kinds_are_shape_descriptors_that_are_never_stored() {
    for (name, shape) in [
        ("sqrt", "Sqrt"), ("root", "Root"), ("hat", "Accent"), ("overline", "Line"),
    ] {
        let descriptor = visual_typst_core::slots::configured_kind(name)
            .unwrap_or_else(|| panic!("{name} 应当有形状"));
        assert_eq!(variant(&descriptor), shape, "{name} 的形状");
        let editor = {
            let mut editor = Editor::default();
            editor.apply(Action::Import { source: format!("${name}(x)$") }).unwrap();
            editor
        };
        assert_eq!(
            variant(&editor.root[0].kind), "MacroCall",
            "{name}(x) 应当存成 MacroCall（形状由名字查出来），而不是 {shape}",
        );
    }
}
