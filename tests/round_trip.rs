// SPDX-License-Identifier: GPL-2.0-or-later
//! 能出现在可编辑源码树里的每个原子，都必须能在自己的 Typst 拼写里往返。
//!
//! `write_atom` 是唯一一项**没有安全网**的义务：编辑器把它的结果写回权威源码
//! （`document.rs` 的 `replace_range`），所以一个节点序列化成"读回来是另一棵树"
//! 的形式，就是静默损坏文档。其余义务（视图投影、导航规则）错了顶多是显示或
//! 光标不对，这一项错了是文档内容变了。
//!
//! 因此这个文件的判据不是"看起来对"，而是**写出去、读回来、必须等于原树**。
//! 新加一个 `Kind` 时，如果没有在这里留下它的代表用例，就等于没有验证过回写。
//!
//! 三个 Kind 有意不在此列，且无法在此覆盖：
//!
//! * `TemplateCall` 与 `Parameter` 只存在于宏模板里，不属于可编辑源码树；
//!   `write_atom` 对 `TemplateCall` 直接标了 `unreachable!`。
//! * `Unknown` 承载半打完的命令草稿，是纯编辑器状态，没有 Typst 拼写：
//!   `fra` 不是一个公式。
//!
//! 即"可往返的 Kind"共 14 个：Char, Symbol, Number, Raw, MacroCall, Text,
//! Fraction, Sqrt, Root, Scripts, Fenced, Table, Multiline, Decoration。

use visual_typst_core::{Action, Editor, typst};

fn load(source: &str) -> Editor {
    let mut editor = Editor::default();
    if let Err(error) = editor.apply(Action::Import { source: source.into() }) {
        panic!("导入 {source:?} 失败：{error}");
    }
    editor
}

/// 写出整篇文档再读回来，要求得到同一棵树。
///
/// 用文档级拼写（含 `#let` 前缀与 `$…$`）而不是裸片段，因为宏调用必须带着
/// 定义才能读回成调用：脱离定义，`f(2)` 会退化成 Raw。
#[track_caller]
fn round_trips(source: &str) {
    let original = load(source);
    assert!(!original.root.is_empty(), "{source:?} 读成了空树");
    let written = typst::write_document_mode(&original.root, &original.definitions, original.display);
    let again = load(&written);
    assert_eq!(
        again.root, original.root,
        "源码 {source:?} 写成 {written:?} 后读回了不同的树"
    );
}

#[test]
fn char_atoms_round_trip() {
    // 写回时必须插入分隔符，否则两个字符会连成一个记号；分隔符本身也必须
    // 读回来还是两个字符（见 structured_input.rs 的同名用例）。
    for source in ["x", "+", "-", "x y", "> =", "- >", "| |", ". . ."] {
        round_trips(source);
    }
}

#[test]
fn number_atoms_round_trip() {
    // 数字串按引擎的规则（全 ASCII 数字、至多一个点、至少一个数字）收成一个
    // 原子，写回时原样写出，所以这里也必须是逐字节往返。
    for source in ["1", "123", "1.5", "0.5", "123.456", "1.2.3", ".5", "1."] {
        round_trips(source);
    }
}

#[test]
fn symbol_atoms_round_trip() {
    // 符号是"源码片段 → 显示字形"的映射，写回用的是源片段本身。
    for source in ["alpha", "pi", "Omega", "times", "dot", "div"] {
        round_trips(source);
    }
}

#[test]
fn raw_atoms_round_trip() {
    // Raw 保留原文，所以往返必须是逐字节的：这正是"不建模就原样留着"的承诺。
    for source in [
        "dif", "sum", "integral", "arrow.r", "sin", "lr", "text",
        "cancel( x/y  + dif x )",
        "lr((x), size: #150%)",
        "text(\"hello\")",
    ] {
        round_trips(source);
    }
}

#[test]
fn text_atoms_round_trip() {
    for source in [r#""a >= b & c""#, r#""hello""#] {
        round_trips(source);
    }
}

#[test]
fn fraction_atoms_round_trip() {
    for source in ["frac(x, y)", "frac(1, 2)", "frac(frac(a, b), c)", "frac(a + b, c - d)"] {
        round_trips(source);
    }
}

#[test]
fn radical_atoms_round_trip() {
    for source in ["sqrt(x)", "sqrt(frac(a, b))", "sqrt(x + 1)"] {
        round_trips(source);
    }
}

#[test]
fn root_atoms_round_trip() {
    // `root` 的槽位顺序与 Typst 参数顺序相反：内部是 [被开方式, 根指数]，
    // Typst 是 `root(指数, 被开方式)`。解析时 swap、写回时倒着取，两处必须一致。
    for source in ["root(3, x)", "root(n, x + 1)", "root(3, frac(a, b))"] {
        round_trips(source);
    }
}

#[test]
fn script_atoms_round_trip() {
    for source in [
        "x^2", "x_1", "x_1^2", "x^2_1",
        "sum_1^2", "sum_1", "sum^2",
        "x^(a + b)", "x_(a + b)", "x^(2 + 1)",
        "x^(y_1)", "x_1^(y^2)",
        // 多原子基底走 write_atom 里 `cells[0].len() > 1` 加括号的那条分支：
        // 括号是分组语法，读回来必须还是同一个多原子基底，而不是一个 Delim。
        "(a + b)^2", "(a + b)_1", "(a + b)^(c + d)",
    ] {
        round_trips(source);
    }
}

#[test]
fn delimiter_atoms_round_trip() {
    // `abs`/`norm` 是 Delim 的具名拼写，写回时必须还原成具名形式，
    // 否则会变成手写的括号而对不上 Kind。
    for source in ["abs(x)", "norm(x)", "abs(frac(a, b))", "(x)", "[x]"] {
        round_trips(source);
    }
}

#[test]
fn grid_atoms_round_trip() {
    for source in ["mat(1, 2; 3, 4)", "mat(a, b; c, d; e, f)", "mat(1; 2; 3)"] {
        round_trips(source);
    }
}

#[test]
fn aligned_atoms_round_trip() {
    // 对齐公式靠 `&` 分列、`\\` 换行；行宽信息 (`row_lengths`) 也要一起还原。
    for source in ["a & b \\\n c & d", "x & = & 1 \\\n y & = & 2"] {
        round_trips(source);
    }
}

#[test]
fn decoration_atoms_round_trip() {
    for source in [
        "hat(x)", "vec(x)", "dot(x)", "overline(x)", "underline(x)",
        "hat(a + b)", "overline(frac(a, b))",
    ] {
        round_trips(source);
    }
}

#[test]
fn macro_call_atoms_round_trip() {
    // 调用必须带着定义一起往返，否则 `f(2)` 读不回来。
    round_trips("#let f(a) = $a + 1$\n$ f(2) $");
    round_trips("#let g(a, b) = $a - b$\n$ g(x, y) $");
    round_trips("#let f(a) = $a + 1$\n$ frac(f(2), f(3)) $");
}

#[test]
fn a_template_does_not_re_expand_braces_that_came_from_a_cell() {
    // `write_atom` fills a `slots::Write::Template` in one pass. Replacing `{0}`
    // and then `{1}` would let a cell's own braces be expanded as though they
    // were part of the template, writing a different node into the document.
    // The written form is asserted directly, because a two-pass substitution
    // would still round-trip through *some* tree -- just not this one.
    let original = load("frac(\"x{1}y\", b)");
    let written = typst::write_document_mode(&original.root, &original.definitions, original.display);
    assert!(written.contains("\"x{1}y\""), "回写改动了文本里的花括号：{written}");
    round_trips("frac(\"x{1}y\", b)");
    round_trips("frac(a, \"{0}\")");
}

#[test]
fn nested_structures_round_trip() {
    // 混合结构：这里是"槽位顺序"最容易出错的地方，因为每一层都要同时对。
    for source in [
        "frac(sqrt(3) + 1, 2)",
        "frac(x^2, sqrt(y))",
        "hat(frac(a, b))",
        "mat(frac(a, b), c; d, e)",
        "x^(frac(a, b))",
        "abs(x)^2",
    ] {
        round_trips(source);
    }
}
