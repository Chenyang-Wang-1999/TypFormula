// SPDX-License-Identifier: GPL-2.0-or-later
//! 解析器**真正会存进树里**的 `Kind` 有哪些。
//!
//! 判据（见 `docs/architecture.md`）：一个构造要是自己的 `Kind`，只有两个理由——
//! **带着名字与配置都给不出的实例数据**（`Table.columns`、`Fenced.left/right`、
//! `Multiline.row_lengths`、`Raw.source`、`Char.text`……），或者**要保住书写形式**
//! （`a/b` 的 `SkewedFraction` 属于这条）。
//!
//! 这条判据曾经是在**收拾一批多余的变体**：`Sqrt`/`Root`/`Accent`/`Line`/`Style`
//! 五个变体留在枚举里当"形状描述符"，没有任何源码能把它们放进树里。那时这条用例是
//! 唯一的守卫，因为把它们重新变成会存的节点照样能编译、照样能往返。
//!
//! **现在它们已经被删掉了**（见 `docs/validation.md` 的"清理不必要的 Kind"），
//! 于是这条用例守的东西变小、但更硬：枚举里只剩会被存的变体，所以它断言的是
//! "哪些变体真的会出现"——多一个或少一个都说明解析路径变了。`variant` 仍然是穷尽
//! `match`，新增一个变体时编译器会要求在这里回答一次。
//!
//! 两个变体永远不在这个集合里，但**不是**因为它们多余：`Parameter` 与
//! `TemplateCall` 只存在于宏模板的内部树，上线前就被 `bind_template_inner` 换掉
//! （`docs/kind-inventory.md` 第五节），这里读的是可编辑树，所以看不到它们。

use std::collections::BTreeSet;
use typformula_core::math::Kind;
use typformula_core::{Action, Editor};

fn variant(kind: &Kind) -> &'static str {
    // 穷尽 match：新增一个 `Kind` 时编译器会要求在这里给它一个名字，
    // 于是"它算不算会被存下来的 kind"这个问题必须被回答一次。
    //
    // 五个形状描述符（`Sqrt`/`Root`/`Accent`/`Line`/`Style`）**已经不在这里**：
    // 它们从没被存进树，而唯一由它们携带、`Shape` 说不出的东西（`abs` 是哪对定界符、
    // `overline` 在上还是在下）已经搬进 `config/commands.json`，所以它们整个从
    // `Kind` 枚举里删掉了。从此"这个变体会不会被存"这个问题不再需要回答——枚举里
    // 只剩会被存的变体。
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
        Kind::Scripts => "Scripts",
        Kind::Fenced { .. } => "Fenced",
        Kind::Table { .. } => "Table",
        Kind::Multiline { .. } => "Multiline",
        Kind::Unknown { .. } => "Unknown",
    }
}

fn collect(data: &[typformula_core::math::MathAtom], out: &mut BTreeSet<&'static str>) {
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

/// 反过来的一面：借形状的命令**存成 `MacroCall`**，它长什么样全部由名字查出来。
///
/// 这条用例取代了原来的 `four_kinds_are_shape_descriptors_that_are_never_stored`。
/// 那时 `Sqrt`/`Root`/`Accent`/`Line`/`Style` 五个变体留在 `Kind` 里当"形状描述符"，
/// 用例守的是"它们不该被存进树"。现在它们**整个从枚举里删掉了**——枚举里只剩会被存的
/// 变体，于是"会不会被存"不再是一个需要回答的问题。
///
/// 留下的是这条更直接的断言：这些命令只产生 `MacroCall`，而它的形状、线上排布与
/// `marker` 全部能从配置查出来。配置是文件，所以这条检查必须存在。
#[test]
fn a_named_command_stores_a_call_and_borrows_its_shape() {
    // (命令, 借的形状, 线上排布, marker)
    for (name, shape, wire, marker) in [
        ("frac", "fraction", "fraction", Some("-")),
        ("sqrt", "sqrt", "decorated", Some("radical")),
        ("root", "root", "root", Some("radical")),
        ("hat", "decoration", "decorated", Some("hat")),
        ("cancel", "decoration", "decorated", Some("cancel")),
        ("overline", "line", "decorated", Some("overline")),
        ("underline", "line", "decorated", Some("underline")),
        ("abs", "delim", "decorated", Some("delim")),
        ("norm", "delim", "decorated", Some("delim")),
        ("bold", "style", "style", None),
    ] {
        let borrowed = typformula_core::slots::configured_shape(name)
            .unwrap_or_else(|| panic!("{name} 应当有形状"));
        assert_eq!(borrowed.view, shape, "{name} 借的形状");
        assert!(typformula_core::slots::configured_draw(name).is_some(), "{name} 应当有画法");

        let mut editor = Editor::default();
        editor.apply(Action::Import { source: format!("${name}(x)$") }).unwrap();
        assert_eq!(
            variant(&editor.root[0].kind), "MacroCall",
            "{name}(x) 应当存成 MacroCall（形状由名字查出来），而不是一个专门的 Kind",
        );
        // The wire kind and marker come off that path too, not off any stored field.
        let response = editor.response();
        let top = &response.view.children[1];
        assert_eq!(top.kind, wire, "{name} 的线上排布");
        assert_eq!(top.marker.as_deref(), marker, "{name} 的 marker");
        // A font variant carries its name instead: that is what the frontend asks the
        // engine for the substituted glyphs by.
        assert_eq!(top.style_name.as_deref(), (wire == "style").then_some(name), "{name} 的 style_name");
    }
}
