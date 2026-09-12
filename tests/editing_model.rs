// SPDX-License-Identifier: GPL-2.0-or-later
//! 新架构（`docs/editing-model.md`）**已经主张、但今天没有测试守着**的两条不变量。
//!
//! 这两条不是"补一点覆盖"，而是拆分层之前必须先知道**今天是构造保证还是纪律**：
//!
//! * 若是绿色 → 今天靠结构成立，重构只要别弄坏；
//! * 若是红色 → 今天靠"反正没接编辑"侥幸成立，重构必须把它变成构造。
//!
//! 所以这两个用例**先于实现写**，并在下方各自注明本轮的实测结果。

use serde_json::{json, Value};
use typformula::document::Document;
use typformula_core::math::{CursorSlice, MathAtom, MathData};

/// 打开一份文档，激活**最后一个**以 `needle` 开头的公式，返回它的 response。
fn activated(source: &str, needle: &str) -> (Document, Value) {
    let mut doc = Document::default();
    doc.apply(json!({ "action": "set_source", "source": source })).unwrap();
    let start = source.rfind(needle).unwrap_or_else(|| panic!("{source:?} 里没有 {needle:?}"));
    doc.apply(json!({ "action": "activate_formula", "start": start })).unwrap();
    let response = doc.response();
    (doc, response)
}

/// 沿 `slices` 走可编辑树，取到那一格。
fn cell_at<'a>(root: &'a MathData, slices: &[CursorSlice]) -> Option<&'a MathData> {
    let mut data = root;
    for slice in slices {
        let atom: &MathAtom = data.get(slice.atom)?;
        data = atom.cells.get(slice.cell)?;
    }
    Some(data)
}

/// 收集视图里每个 `stop` 的 cursor。
fn stops(view: &Value, out: &mut Vec<Value>) {
    if view["kind"] == json!("stop") {
        out.push(view["cursor"].clone());
    }
    for child in view["children"].as_array().into_iter().flatten() {
        stops(child, out);
    }
}

/// **不变量一：洞的标记绝不能上线。**
///
/// 模板里的 `Parameter` 曾经以 `Write::Marker` 的拼写 `#parameter0` 顺着
/// `raw_macro` 的 `text`（取自 `write_atom`）漏到视图里，被**当成正文画出来**。
///
/// 这条**先于实现写**，当时是红的（实测视图里确实出现
/// `text='bold(upright(#parameter0))'`，同一处漏了两层）。现在它是绿的。
#[test]
fn the_wire_view_never_carries_the_hole_marker() {
    // `#let` + 一个调用点：展开会把模板里的洞换成实参。
    let source = "#let mathbf(x) = $bold(upright(#x))$\n$ mathbf(u) $";
    let (_, response) = activated(source, "$ mathbf");
    let view = response["view"].to_string();
    assert!(!view.contains("#parameter"), "洞的标记漏上了视图：{view}");
    assert!(!view.contains("parameter"), "视图里出现了 `parameter` 这个名字：{view}");
    // The same shape twice over: a hole inside material that is *itself* a call the
    // kernel cannot shape (`bold`/`upright` see no glyph run until the hole is filled),
    // which is the case that used to leak. What must survive is the argument.
    assert!(view.contains("\"text\":\"u\""), "实参没被填进洞里：{view}");
}

/// **不变量一的另一半，而且它才是真正修好它的那一半。**
///
/// 上面那条查的是**线上**的视图。漏点的**起点**不在这里，而在 `write_atom` 对一份
/// 模板材料的拼写上：`Parameter` 原来写成 `#parameter{index}` ——一个在这里发明的
/// 名字，然后被当成源码画了出来。
///
/// 这里直接构造模板里的那个节点，断言它写出来是**定义自己的拼写**。这条钉住的是
/// 一句话：漏出去的那个字符串来自 `write_atom`，而它现在拼的是参数在定义里的真名，
/// 所以**无论模板怎么存**，这条路都不会再写出发明的名字。
///
/// 这一条是对上一轮说法的一处更正：6.2 变绿靠的是 `Kind::Parameter` 带上参数名，
/// 不是"模板改存视图树"。后面那件事的价值在别处（`docs/editing-model.md` 节九），
/// 但把这条钉住，是为了不让那个结论随实现漂移。
#[test]
fn template_material_spells_its_hole_the_way_the_definition_writes_it() {
    use typformula_core::math::{Kind, MathAtom};
    use typformula_core::typst::write_atom;
    let hole = || MathAtom { kind: Kind::Parameter { index: 0, name: "x".into() }, cells: vec![] };
    let upright = MathAtom { kind: Kind::MacroCall { name: "upright".into(), function: true }, cells: vec![vec![hole()]] };
    let bold = MathAtom { kind: Kind::MacroCall { name: "bold".into(), function: true }, cells: vec![vec![upright]] };
    assert_eq!(write_atom(&bold), "bold(upright(#x))");
    // A definition's own name is a name the definition's text contains, which is what
    // `definition_raw_ranges` needs to answer where the fragment lives at all.
    assert!(!write_atom(&bold).contains("parameter"));
}

/// **不变量三：宏模板里被洞撑起来的字体变体，按绑完之后的形态画。**
///
/// `#let mathbf(x) = $bold(upright(#x))$` 配 `$mathbf(u)$` 曾经画成两层 `raw_macro`
/// ——一整段调用的图——因为**注册期**问"主体是不是字形串"时，主体是**洞**，而洞不是
/// 字符。字形请求的键是调用自己的拼写，所以 `bold(upright(#x))` 那把钥匙打不开门，
/// 整块就退成了图。
///
/// 判据本身从来没错（把绑完的主体喂给它，它答得完全正确），错的是**问得太早**。
/// 修法是把这一步挪到实例侧：模板里那个折叠节点保留着带洞的拼写，调用点把它绑完、
/// 重解析、用普通的 `view_atom` 再投影一次（`docs/editing-model.md` §9）。
///
/// 三条断言各管一件事：形态是 `style`、命令名是 `bold`、而 `text` 是**绑完之后的
/// 拼写**——最后这条是真正要命的那条，因为它是字形请求的键。
#[test]
fn a_variant_a_template_built_around_a_hole_is_shaped_once_it_is_bound() {
    let source = "#let mathbf(x) = $bold(upright(#x))$\n$ mathbf(u) $";
    let (_, response) = activated(source, "$ mathbf");
    let view = &response["view"];
    let mut styles = vec![];
    find(view, "style", &mut styles);
    assert!(!styles.is_empty(), "绑完之后应当是字体变体节点，而不是一整段调用的图：{view}");
    let outer = styles[0];
    assert_eq!(outer["style_name"].as_str(), Some("bold"));
    assert_eq!(outer["text"].as_str(), Some("bold(upright(u))"),
               "字形请求的键必须是**绑完之后**的拼写，`#x` 打不开引擎那扇门");
    // 结构体不会被误判成字形串：这一条必须仍然退成图。
    let source = "#let fracx(x) = $bold(frac(#x, 2))$\n$ fracx(a) $";
    let (_, response) = activated(source, "$ fracx");
    let mut collapsed = vec![];
    find(&response["view"], "raw_macro", &mut collapsed);
    assert!(!collapsed.is_empty(), "分式主体不是字形串，仍该画整段调用的图：{}", response["view"]);
}

/// Every node of one view kind, in reading order.
fn find<'a>(view: &'a Value, kind: &str, out: &mut Vec<&'a Value>) {
    if view["kind"] == json!(kind) { out.push(view); }
    for child in view["children"].as_array().into_iter().flatten() { find(child, kind, out); }
}

/// **不变量二：不可编辑的地方，光标不可达。**
///
/// 两条断言各管一半：
///
/// * **每个 stop 都必须是可编辑树里合法的光标位置**——模板材料不在可编辑树里，
///   所以它一旦开始发 stop，路径就会指到树外，这一条立刻红；
/// * **stop 的总数必须等于可编辑位置的个数**——这是真正的牙齿：模板材料
///   （`+` 与 `1`，共 3 个位置）若变得可达，总数会从 4 涨上去。
///
/// 本轮实测：**绿色。** 展开 `#let dbl(x) = $#x + 1$` 配 `$dbl(y)$` 恰好 4 个 stop
/// —— 外层 2 个 + 实参格 2 个，模板材料一个都没有。所以今天这条是**构造**
/// （`view_cell(..., path=None)` 不发 stop），重构时不能弄坏。
#[test]
fn template_material_is_unreachable_from_the_caret() {
    let source = "#let dbl(x) = $#x + 1$\n$ dbl(y) $";
    let (doc, response) = activated(source, "$ dbl");
    let mut found = vec![];
    stops(&response["view"], &mut found);
    assert_eq!(found.len(), 4, "可编辑位置应当是 4 个（外层 2 + 实参格 2）：{found:?}");
    for cursor in &found {
        let slices: Vec<CursorSlice> = serde_json::from_value(cursor["slices"].clone())
            .unwrap_or_else(|error| panic!("stop 的 slices 不是合法路径：{cursor}（{error}）"));
        let pos = cursor["pos"].as_u64().unwrap_or(u64::MAX) as usize;
        let cell = cell_at(&doc.editor.root, &slices)
            .unwrap_or_else(|| panic!("stop 指到可编辑树外：{cursor}"));
        assert!(pos <= cell.len(), "stop 的位置越界：{cursor}（该格只有 {} 个原子）", cell.len());
    }
}
