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
//! 即"可往返的 Kind"共 16 个：Char, Symbol, Number, Raw, MacroCall, Text,
//! Fraction, Sqrt, Root, Scripts, Fenced, Table, Multiline, Accent, Line, Style。
//! 其中 `Style` 与 `Sqrt`/`Root`/`Accent`/`Line` 一样，**只作为形状存在**：树里
//! 存的是借了它形状的 `MacroCall`（`bold(x)`），回写就是这个调用自己的拼写。

use typformula_core::{Action, Editor, math::{Kind, MathAtom}, typst};

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
        "lr( x/y  + dif x, size: #100%)",
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
fn the_radical_syntax_and_its_command_are_one_node() {
    // `√x`/`∛x` are syntax, `sqrt(x)`/`root(3, x)` are calls, and both end up as the
    // same `MacroCall` — because neither radical shape carries instance data of its
    // own, so the name is the whole of what a node has to remember. Writing normalizes
    // to the command form, which is what it already did: `√x` has always been written
    // `sqrt(x)`.
    for (source, written) in [("√x", "sqrt(x)"), ("∛x", "root(3, x)"), ("∜(a + b)", "root(4, a + b)")] {
        let editor = load(source);
        assert!(
            matches!(&editor.root[0].kind, Kind::MacroCall { .. }),
            "{source:?} 应当与它的命令写法是同一个节点，实际是 {:?}",
            editor.root[0].kind
        );
        assert_eq!(typst::write_cell(&editor.root), written, "{source:?} 的写法");
        round_trips(source);
    }
    // The two spellings really are the same tree, which is the point of folding them.
    assert_eq!(load("√x").root, load("sqrt(x)").root);
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

/// Short matrix rows are padded with real, editable empty cells.
#[test]
fn a_matrix_pads_short_rows_with_empty_cells() {
    let editor = load("mat(a, b; c)");
    assert!(
        matches!(&editor.root[0].kind, Kind::Table { columns: 2, row_lengths, name }
                 if row_lengths == &vec![2, 2] && name == "mat"),
        "mat(a, b; c) 应当是两列、行宽 [2, 2] 的表，实际是 {:?}",
        editor.root[0].kind
    );
    // A blank cell that is not trailing padding has to come back as a blank cell.
    let blank = load("mat(a, ; c, d)");
    assert!(
        matches!(&blank.root[0].kind, Kind::Table { columns: 2, row_lengths, .. } if row_lengths == &vec![2, 2]),
        "mat(a, ; c, d) 的两行都该是两格，实际是 {:?}",
        blank.root[0].kind
    );
    assert_eq!(typst::write_cell(&blank.root), "mat(a, ; c, d)");
    for source in [
        "mat(a, b; c)", "mat(a; b, c)", "mat(a, ; c, d)", "mat(1, 2; 3, 4)",
        "mat(a, b; c, d; e, f)", "mat(1; 2; 3)", "mat(1, 2, 3)",
    ] {
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
fn accent_atoms_round_trip() {
    // A mark beside the base; the name is the callee and the cell is the body.
    // `vec(x)` is deliberately absent: `vec` is not an accent in Typst (it is a
    // column vector), so the editor no longer builds one for it and the source stays
    // a `Raw`. See `docs/kind-inventory.md`.
    //
    // `cancel(x)` *is* here. The engine lays it out as `CancelItem` rather than
    // `AccentItem`, but the editor's model is the same for both — one body with a
    // mark drawn over it — so it borrows the `decoration` shape by a name in
    // `config/commands.json`. Only where the mark sits differs, which is the
    // frontend's business.
    for source in ["hat(x)", "hat(a + b)", "hat(frac(a, b))", "cancel(x)", "cancel(a + b)"] {
        round_trips(source);
    }
}

/// A command the editor structures *only* because `config/commands.json` names it.
///
/// Round-tripping cannot see this and must not be trusted for it: drop the entry and
/// `cancel(x)` becomes a `Raw`, which writes itself back verbatim, so
/// `accent_atoms_round_trip` would stay green while the formula quietly stopped being
/// editable. The same blind spot as `math::is_number` — a rule whose source is outside
/// the round trip needs its own assertion.
#[test]
fn a_mark_that_only_the_command_file_names_is_structured() {
    let editor = load("cancel(x)");
    assert!(
        matches!(&editor.root[0].kind, Kind::MacroCall { name, .. } if name == "cancel"),
        "cancel 应当由 commands.json 建成调用节点，实际是 {:?}",
        editor.root[0].kind
    );
    assert_eq!(editor.root[0].shape().view, "decoration", "并且借到重音的形状");
    assert_eq!(editor.root[0].cells.len(), 1, "cancel 的正文是一格");
    assert_eq!(typst::write_cell(&editor.root), "cancel(x)");
}

#[test]
fn line_atoms_round_trip() {
    // `overline`/`underline` are Typst's `Line`, whose item holds nothing but the
    // position: the two commands must come back as the position they spell, not as
    // one command with a flag somewhere else.
    for source in ["overline(x)", "underline(x)", "overline(frac(a, b))", "underline(a + b)"] {
        round_trips(source);
    }
}

/// 字体变体（`Style`）存成借形状的调用，所以回写必须是**这个调用自己的拼写**。
///
/// 这一条比别的回写更容易悄悄坏掉：`Style` 的形状不带数据，写回完全靠名字，
/// 而它的**正文可能又是一层变体**（`bold(upright(a))`），于是内外两层各自都要
/// 写对、括号不能多也不能少。嵌套那一例如果退化成逐层替换的结果
/// （`bold(𝐚)`），读回来就是另一棵树。
#[test]
fn style_atoms_round_trip() {
    for source in ["bold(x)", "upright(A)", "bold(upright(a))", "bold(x + 1)"] {
        round_trips(source);
        let editor = load(source);
        assert!(
            matches!(&editor.root[0].kind, Kind::MacroCall { name, .. } if name == "bold" || name == "upright"),
            "{source} 应当由 commands.json 建成借形状的调用，实际是 {:?}",
            editor.root[0].kind
        );
        assert_eq!(editor.root[0].shape().view, "style", "{source} 借到的是样式的形状");
    }
    // 正文不是**一行字形**时内核不建样式节点（`typst::has_glyph_run`），于是同一个
    // 命令落回普通调用：它照样往返，但借到的是 `macro` 而不是 `style`。这一对
    // 用例是有意成对写的——只写前面那半，会让人以为"`upright` 总是样式节点"。
    for source in ["upright(alphabets)", "bold(frac(a, b))"] {
        round_trips(source);
        let editor = load(source);
        assert_eq!(editor.root[0].shape().view, "macro", "{source} 没有字形串，应当是普通调用");
    }
}

/// A `MacroCall` that names a configured command borrows *that command's* shape.
///
/// This is the mechanism stage 2 of the refactor rests on: the node stores only a
/// name, and the shape — its slots, its view, its spelling — is looked up, never
/// stored. The test builds the call by hand because the parser still produces the
/// dedicated kinds; what is pinned here is the borrowing itself, so that flipping the
/// parser later is a change of *which node is built*, not of what a node means.
#[test]
fn a_call_naming_a_command_borrows_that_shapes_slots_and_spelling() {
    let call = |name: &str, cells: usize| MathAtom::nest(Kind::MacroCall { name: name.into(), function: true }, cells);
    for (name, cells, shape, spelling) in [
        ("sqrt", 1, "sqrt", "sqrt(x)"),
        ("frac", 2, "fraction", "frac(x, y)"),
        ("hat", 1, "decoration", "hat(x)"),
        ("overline", 1, "line", "overline(x)"),
        ("underline", 1, "line", "underline(x)"),
        ("abs", 1, "delim", "abs(x)"),
        ("norm", 1, "delim", "norm(x)"),
        ("cancel", 1, "decoration", "cancel(x)"),
    ] {
        let mut atom = call(name, cells);
        for cell in &mut atom.cells { cell.push(MathAtom::character('x')); }
        if cells == 2 { atom.cells[1] = vec![MathAtom::character('y')]; }
        assert_eq!(atom.shape().view, shape, "{name} 应当借用 {shape} 的槽位图式");
        assert_eq!(atom.shape().slots.len(), cells, "{name} 的格子数应与形状一致");
        assert_eq!(typst::write_atom(&atom), spelling, "{name} 的拼写应由形状给出");
    }
    // A name the command file does not know keeps the generic call schema: one
    // linear cell per argument, and the call is what gets written.
    let mut plain = MathAtom::nest(Kind::MacroCall { name: "f".into(), function: true }, 2);
    plain.cells[0] = vec![MathAtom::character('x')];
    plain.cells[1] = vec![MathAtom::character('y')];
    assert_eq!(plain.shape().view, "macro");
    assert_eq!(plain.shape().slots.len(), 1, "未配置的名字沿用参数格图式");
    assert_eq!(typst::write_atom(&plain), "f(x, y)");
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
