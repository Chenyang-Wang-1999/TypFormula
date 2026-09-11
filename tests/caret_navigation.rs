// SPDX-License-Identifier: GPL-2.0-or-later
//! Caret navigation, as declared by the slot table in `src/slots.rs`.
//!
//! Each case pins one entry of that table. The file exists because two entries
//! were **not** covered by the suite when the table was introduced: flipping
//! `Frac`'s `end_up` or `Grid`'s `Entry::GridMiddle` left every existing test
//! green, so a wrong entry there would have shipped silently. Every case below
//! was measured against the real binary before it was written down.
//!
//! Covered here, one declaration field each:
//!
//! * `Frac`     — `entry` (Roles), `vertical` swap, `end_up: false`
//! * `Root`     — `entry` (Roles, the reverse of Typst's argument order),
//!                `end_up: true`, `horizontal: Pair`
//! * `Script`   — `entry` (Base), `vertical: Attach`
//! * `Grid`     — `entry: GridMiddle`, `horizontal`/`vertical: Column`
//! * `Aligned`  — `entry: Edge`

use visual_typst_core::{Action, Editor};

fn load(source: &str) -> Editor {
    let mut editor = Editor::default();
    editor.apply(Action::Import { source: source.into() }).unwrap_or_else(|error| panic!("导入 {source:?} 失败：{error}"));
    editor
}

fn key(editor: &mut Editor, name: &str) {
    editor.apply(Action::Key { key: name.into(), shift: false, ctrl: false }).unwrap();
}

/// The caret as the cell index inside each level, plus the position in the
/// innermost cell. The empty vector means the formula's outermost cell.
fn caret(editor: &Editor) -> (Vec<usize>, usize) {
    (editor.cursor.slices.iter().map(|slice| slice.cell).collect(), editor.cursor.pos)
}

#[test]
fn a_fraction_enters_its_numerator_and_swaps_cells_vertically() {
    let mut editor = load("frac(a + b, c)");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![0], 0), "向右进入分式应落在分子首");
    key(&mut editor, "ArrowDown");
    assert_eq!(caret(&editor).0, vec![1], "下键应换到分母");
    key(&mut editor, "ArrowUp");
    // `end_up: false` for a fraction: the caret comes back to the START of the
    // numerator (length 3 here), not to its end.
    assert_eq!(caret(&editor), (vec![0], 0), "上键换到分子并落在格首");
}

#[test]
fn a_fraction_entered_from_the_right_reaches_its_denominator() {
    let mut editor = load("frac(a + b, c)");
    key(&mut editor, "End");
    key(&mut editor, "ArrowLeft");
    // `entry.backward` is the denominator, entered at the end of its cell.
    assert_eq!(caret(&editor), (vec![1], 1), "向左进入分式应落在分母末");
}

#[test]
fn a_radical_enters_its_degree_which_is_not_typsts_first_argument() {
    // Cells are `[radicand, index]`, the reverse of Typst's `root(index,
    // radicand)`: entering forward must reach the degree, not the radicand.
    let mut editor = load("root(3, x + 1)");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![1], 0), "向右进入根式应落在根指数");
    key(&mut editor, "ArrowDown");
    assert_eq!(caret(&editor).0, vec![0], "下键应换到被开方式");
    key(&mut editor, "ArrowUp");
    // `end_up: true` here: the degree reads before the radicand, so the caret
    // lands at the END of the degree cell (length 1).
    assert_eq!(caret(&editor), (vec![1], 1), "上键换到根指数并落在格尾");
}

#[test]
fn a_radical_walks_its_two_cells_and_then_leaves() {
    // `horizontal: Pair`: the two cells are reachable from each other, but a
    // move first traverses whatever the current cell already holds, so the walk
    // is: degree, its content, back to the radicand, its content, then out.
    //
    // The degree is the digit `3`, and a number run is a container with one cell
    // (`Kind::Number`, like a text run), so its own content is walked too: in at
    // its first position, out at its last, and only then on to the radicand.
    let mut editor = load("root(3, x)");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![1], 0), "先到根指数");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![1, 0], 0), "再进数字串的首位");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![1, 0], 1), "走完数字串的内容");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![1], 1), "出数字串，停在根指数格尾");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![0], 0), "再从根指数折回被开方式");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![0], 1), "走完被开方式的内容");
    key(&mut editor, "ArrowRight");
    assert!(editor.cursor.slices.is_empty(), "成对的两个格子走完就离开根式");
}

#[test]
fn a_radical_is_entered_backward_at_the_end_of_its_radicand() {
    // A radical's cells are stored `[radicand, index]` while the degree is drawn to
    // the LEFT, so walking left out of the radicand reaches the index cell from its
    // right and the caret lands at that cell's END.
    //
    // This is `move_horizontal`'s `root_back`, not `entry_cell`: entering the node
    // from outside goes through the entry role and lands at the end either way, which
    // is why an earlier version of this test passed even with the rule disabled. It
    // takes being *inside* the radicand at its start and stepping left across the cell
    // boundary to reach it.
    let mut editor = load("root(3, x + 1)");
    key(&mut editor, "End");
    key(&mut editor, "ArrowLeft");
    // Backward entry is the radicand, met from its right, so the caret lands at the
    // end of `x + 1` (3 atoms).
    assert_eq!(caret(&editor), (vec![0], 3), "向左进入根式落在被开方式末尾");
    key(&mut editor, "Home");
    assert_eq!(caret(&editor), (vec![0], 0), "先回到被开方式开头");
    key(&mut editor, "ArrowLeft");
    assert_eq!(caret(&editor), (vec![1], 1), "向左跨格到根指数，从右侧进入所以落在格尾");
}

#[test]
fn an_attachment_is_reached_from_the_end_of_its_base() {
    let mut editor = load("x^2");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![0], 0), "向右进入上下标应落在基底首");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![0], 1), "基底内右移到末");
    key(&mut editor, "ArrowUp");
    // `vertical: Attach`: only from the END of the base, and it needs the live
    // script index, which is why it is not a role swap.
    assert_eq!(caret(&editor).0, vec![1], "从基底末尾向上进入上标");
}

#[test]
fn a_matrix_is_entered_in_its_middle_row() {
    // `Entry::GridMiddle`: a 3x3 matrix is entered at row 1, not at a corner.
    let mut editor = load("mat(1, 2, 3; 4, 5, 6; 7, 8, 9)");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor).0, vec![3], "向右进入矩阵应落在中间行首列");
    let mut editor = load("mat(1, 2, 3; 4, 5, 6; 7, 8, 9)");
    key(&mut editor, "End");
    key(&mut editor, "ArrowLeft");
    assert_eq!(caret(&editor).0, vec![5], "向左进入矩阵应落在中间行末列");
}

#[test]
fn a_matrix_walks_by_row_and_column_without_crossing_a_column() {
    let mut editor = load("mat(1, 2; 3, 4)");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor).0, vec![0], "先进入矩阵");
    key(&mut editor, "ArrowDown");
    assert_eq!(caret(&editor).0, vec![2], "下键在同一列向下走一行");
    key(&mut editor, "ArrowUp");
    assert_eq!(caret(&editor).0, vec![0], "上键回到原格");
    // A move traverses the current cell's content first, then crosses a column.
    // The cell holds the number run `1`, which is a container of its own, so both
    // of its positions are walked before the column boundary is even reached.
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![0, 0], 0), "先进数字串");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![0, 0], 1), "走完数字串");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![0], 1), "出数字串，走完本格内容");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![1], 0), "列内右移到本行末列");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![1, 0], 0), "末列的内容也是数字串");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![1, 0], 1), "走完数字串");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor), (vec![1], 1), "出数字串，走完末列内容");
    key(&mut editor, "ArrowRight");
    // `horizontal: Column`: the caret must not step from the first row's last
    // column into the second row's first column.
    assert!(editor.cursor.slices.is_empty(), "列边界阻止右移并离开矩阵");
}

#[test]
fn an_alignment_is_entered_at_its_edges() {
    // `Entry::Edge`, unlike a matrix's middle row.
    let mut editor = load("a & b \\\n c & d");
    key(&mut editor, "ArrowRight");
    assert_eq!(caret(&editor).0, vec![0], "向右进入对齐应落在首格");
    let mut editor = load("a & b \\\n c & d");
    key(&mut editor, "End");
    key(&mut editor, "ArrowLeft");
    assert_eq!(caret(&editor).0, vec![3], "向左进入对齐应落在末格");
}

#[test]
fn an_attachment_lives_in_a_fixed_cell_and_tab_visits_both() {
    // Script storage is `[base, upper, lower]` whatever the source spells. Two
    // intended consequences of that reshaping: a lone subscript moved from cell
    // 1 to cell 2, and Tab gained a stop on the empty attachment.
    let mut editor = load("x_1");
    key(&mut editor, "ArrowRight");
    key(&mut editor, "ArrowRight");
    key(&mut editor, "ArrowDown");
    assert_eq!(caret(&editor).0, vec![2], "下标固定在第三格");

    let mut editor = load("x^2");
    key(&mut editor, "ArrowRight");
    for expected in [1, 2, 0] {
        key(&mut editor, "Tab");
        assert_eq!(caret(&editor).0, vec![expected], "Tab 依次经过上标与空下标再回到基底");
    }
}

#[test]
fn a_text_run_is_one_class_so_ctrl_arrow_moves_by_run() {
    // The closing class set used to be spelled `")] }"` with a stray space, which
    // made a typed space class 5 and split an inline string into alternating
    // classes -- so Ctrl+→ advanced exactly one character. A space can really be
    // a `Char` here: `interpret_char` inserts into a text cell before its `' '`
    // branch. With the space ordinary, the run is a single group.
    let mut editor = Editor::default();
    for ch in "\"a b c".chars() {
        editor.apply(Action::Input { text: ch.to_string() }).unwrap();
    }
    key(&mut editor, "Home");
    assert_eq!(caret(&editor).1, 0, "先回到文本开头");
    editor.apply(Action::Key { key: "ArrowRight".into(), shift: false, ctrl: true }).unwrap();
    assert_eq!(caret(&editor).1, 5, "整段普通字符按一个组处理");
}
