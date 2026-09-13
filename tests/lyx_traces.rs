// SPDX-License-Identifier: GPL-2.0-or-later
// Golden traces derived from the named LyX functions in PORTING.md.
use typformula_core::{Action, Editor, cursor::StopGeometry, math::*, typst};

fn input(e: &mut Editor, text: &str) { e.apply(Action::Input { text: text.into() }).unwrap(); }
fn key(e: &mut Editor, key: &str) { e.apply(Action::Key { key: key.into(), shift: false, ctrl: false }).unwrap(); }
fn source(e: &Editor) -> String { typst::write_cell(&e.root) }
fn load(text: &str) -> Editor { let mut e = Editor::default(); e.apply(Action::Import { source: text.into() }).unwrap(); e }
fn insert(e: &mut Editor, name: &str) { e.apply(Action::Insert { name: name.into() }).unwrap(); }

#[test]
fn command_is_a_draft_until_confirmed_and_uses_typst_names() {
    let mut e = Editor::default(); input(&mut e, "\\alpha");
    assert_eq!(e.pending(), Some("alpha")); assert!(e.completions().is_empty());
    key(&mut e, "Enter");
    assert!(matches!(&e.root[0].kind, Kind::Symbol { name, glyph } if name == "alpha" && Some(glyph.as_str()) == symbol("alpha")));
    assert_eq!(source(&e), "alpha"); assert!(e.pending().is_none());
}
#[test]
fn fraction_tab_cycles_and_space_exits_instead_of_tab_exiting() {
    let mut e = Editor::default(); input(&mut e, "\\frac"); key(&mut e, "Enter"); input(&mut e, "a");
    key(&mut e, "Tab"); input(&mut e, "b");
    key(&mut e, "Tab"); assert_eq!(e.cursor.slices[0].cell, 0); assert_eq!(e.cursor.pos, 0);
    key(&mut e, " "); assert!(e.cursor.slices.is_empty()); assert_eq!(e.cursor.pos, 1);
    assert_eq!(source(&e), "frac(a, b)");
}
#[test]
fn fraction_right_from_numerator_end_exits_the_fraction() {
    let mut e = load("frac(a, b)"); key(&mut e,"ArrowRight"); key(&mut e,"End");
    key(&mut e,"ArrowRight"); assert!(e.cursor.slices.is_empty()); assert_eq!(e.cursor.pos,1);
}
#[test]
fn escape_cancels_command_or_selection_then_pops_structure() {
    let mut e = Editor::default(); input(&mut e,"\\frac"); key(&mut e,"Escape"); assert!(e.root.is_empty());
    insert(&mut e,"sqrt"); input(&mut e,"x");
    e.apply(Action::Key { key:"ArrowLeft".into(), shift:true, ctrl:false }).unwrap();
    key(&mut e,"Escape"); assert!(e.selection().is_none()); assert_eq!(e.cursor.slices.len(),1);
    key(&mut e,"Escape"); assert!(e.cursor.slices.is_empty());
}
#[test]
fn backspace_at_cell_start_pulls_only_current_argument() {
    let mut e = load("frac(a, b)"); key(&mut e,"ArrowRight"); key(&mut e,"Tab"); key(&mut e,"Backspace");
    assert_eq!(source(&e),"b"); assert!(e.cursor.slices.is_empty()); assert_eq!(e.cursor.pos,0);
    e.apply(Action::Undo).unwrap(); assert_eq!(source(&e),"frac(a, b)"); assert_eq!(e.cursor.slices[0].cell,1);
}
#[test]
fn large_object_requires_two_backspaces() {
    let mut e = load("sqrt(x)"); key(&mut e,"End"); key(&mut e,"Backspace");
    assert_eq!(source(&e),"sqrt(x)"); assert!(e.selection().is_some());
    key(&mut e,"Backspace"); assert!(e.root.is_empty());
}
#[test]
fn scripts_reuse_the_nucleus_and_keep_lyx_cell_order() {
    let mut e = Editor::default(); input(&mut e,"x_1"); key(&mut e," "); input(&mut e,"^2");
    assert_eq!(e.root.len(),1); assert_eq!(e.root[0].script_idx(true),Some(1)); assert_eq!(e.root[0].script_idx(false),Some(2));
    assert_eq!(source(&e),"x_(1)^(2)");
    key(&mut e,"ArrowDown"); assert_eq!(e.cursor.slices[0].cell,0); assert_eq!(e.cursor.pos,1);
    key(&mut e,"ArrowDown"); assert_eq!(e.cursor.slices[0].cell,2);
}
#[test]
fn script_backward_entry_enters_nucleus_not_superscript() {
    let mut e = load("x^2"); key(&mut e,"End"); key(&mut e,"ArrowLeft");
    assert_eq!(e.cursor.slices[0].cell,0); assert_eq!(e.cursor.pos,1);
}
#[test]
fn selected_text_goes_into_script_not_base() {
    let mut e = load("x 2"); key(&mut e,"End");
    e.apply(Action::Key { key:"ArrowLeft".into(), shift:true, ctrl:false }).unwrap(); input(&mut e,"^");
    assert_eq!(source(&e),"x^(2)");
}
#[test]
fn root_cell_zero_is_the_index_and_one_is_the_nucleus() {
    let mut e = Editor::default(); insert(&mut e,"root"); assert_eq!(e.cursor.slices[0].cell,0);
    // The `3` is a number run, so leaving the index cell takes two moves: one out
    // of the run and one across the two cells.
    input(&mut e,"3"); key(&mut e,"ArrowRight"); key(&mut e,"ArrowRight");
    assert_eq!(e.cursor.slices[0].cell,1); input(&mut e,"x");
    assert_eq!(source(&e),"root(3, x)");
    let parsed = load(&source(&e)); assert_eq!(parsed.root,e.root);
}
#[test]
fn matrices_have_lyx_cells_and_typst_semicolon_output() {
    let mut e = Editor::default(); insert(&mut e,"mat");
    for s in ["a","b","c","d"] { input(&mut e,s); key(&mut e,"Tab"); }
    assert_eq!(source(&e),"mat(a, b; c, d)"); assert_eq!(e.cursor.slices[0].cell,0);
    key(&mut e,"ArrowDown"); assert_eq!(e.cursor.slices[0].cell,2);
    e.apply(Action::AddColumn).unwrap(); assert_eq!(e.root[0].cells.len(),6); assert_eq!(e.cursor.slices[0].cell,3);
    e.apply(Action::AddRow).unwrap(); assert_eq!(e.root[0].cells.len(),9);
}
#[test]
fn a_matrix_loses_the_row_and_the_column_the_caret_is_in() {
    // Filled column by column so the caret ends where it started: the first row of
    // the first column, which is the row and column these commands name.
    let mut e = Editor::default(); insert(&mut e,"mat");
    for s in ["a","b","c","d"] { input(&mut e,s); key(&mut e,"Tab"); }
    assert_eq!(source(&e),"mat(a, b; c, d)"); assert_eq!(e.cursor.slices[0].cell,0);
    e.apply(Action::RemoveColumn).unwrap();
    assert_eq!(source(&e),"mat(b; d)");
    // The caret stays in the row and the column it was in: `a`'s column went away, so
    // its index now holds what was in the next column of the same row. One row down
    // the same column is where `RemoveRow` then acts.
    assert_eq!(e.cursor.slices[0].cell,0); assert_eq!(e.cursor.pos,0);
    e.apply(Action::RemoveRow).unwrap();
    assert_eq!(source(&e),"mat(d)");
    e.apply(Action::RemoveColumn).unwrap();
    assert_eq!(source(&e),"mat(d)"); assert_eq!(e.message,"只剩一列，无法删除");
    e.apply(Action::RemoveRow).unwrap();
    assert_eq!(source(&e),"mat(d)"); assert_eq!(e.message,"只剩一行，无法删除");
    // `mat(a)` is a spelling, so one cell is where a matrix legitimately stops.
    assert_eq!(load(&source(&e)).root,e.root);
}
#[test]
fn the_caret_row_decides_which_row_of_a_matrix_goes() {
    let mut e=load("mat(a, b; c, d)");
    // Into the table, then down one row: the caret is in `c`'s row.
    key(&mut e,"ArrowRight"); key(&mut e,"ArrowDown");
    assert_eq!(e.cursor.slices[0].cell,2);
    e.apply(Action::RemoveRow).unwrap();
    assert_eq!(source(&e),"mat(a, b)");
    // The last row went away, so the caret moves up to the row that is left.
    assert_eq!(e.cursor.slices[0].cell,0);
}
#[test]
fn an_alignment_keeps_a_marker_or_says_why_it_cannot_shrink() {
    // Rows of two, one and two: the removed row's own width is what has to be dropped
    // from the flat cell list, padding included.
    let mut e=load("a & b \\ c \\ d & e");
    key(&mut e,"ArrowRight"); key(&mut e,"ArrowDown"); assert_eq!(e.cursor.slices[0].cell,2);
    e.apply(Action::RemoveRow).unwrap();
    assert_eq!(source(&e),"a & b \\\nd & e");
    assert_eq!(e.cursor.slices[0].cell,2);
    assert_eq!(load(&source(&e)).root,e.root);
    e.apply(Action::RemoveColumn).unwrap();
    assert_eq!(source(&e),"b \\\ne");
    assert_eq!(load(&source(&e)).root,e.root);
    // One row and one column left: `b \\ e` would write as `e`, which reads back as a
    // plain formula rather than an alignment, so the last step is refused.
    e.apply(Action::RemoveRow).unwrap();
    assert_eq!(source(&e),"b \\\ne"); assert_eq!(e.message,"多行公式只剩一格，无法删除");
    e.apply(Action::RemoveColumn).unwrap();
    assert_eq!(source(&e),"b \\\ne"); assert_eq!(e.message,"只剩一列，无法删除");
}
#[test]
fn deleting_a_row_is_one_undo_step() {
    let mut e=load("mat(a, b; c, d)");
    key(&mut e,"ArrowRight");
    e.apply(Action::RemoveRow).unwrap(); assert_eq!(source(&e),"mat(c, d)");
    e.apply(Action::Undo).unwrap(); assert_eq!(source(&e),"mat(a, b; c, d)");
    e.apply(Action::Redo).unwrap(); assert_eq!(source(&e),"mat(c, d)");
}
#[test]
fn a_caret_outside_a_grid_is_told_where_the_command_works() {
    let mut e=load("frac(a, b)");
    e.apply(Action::RemoveRow).unwrap();
    assert_eq!(e.message,"请先进入矩阵或对齐公式的一个格子");
    assert_eq!(source(&e),"frac(a, b)");
}
#[test]
fn typst_parser_roundtrips_supported_layouts_and_keeps_code_hashes() {
    for s in ["frac(12, sqrt(x))", "root(3, y)", "x_1^2", "(a + b)", "mat(a, b; c, d)", "\"hello world\"", "#foo + alpha"] {
        let first=load(s);let serialized=source(&first);let second=load(&serialized);
        assert_eq!(source(&second),serialized,"roundtrip: {s}");
    }
}
#[test]
fn invalid_import_does_not_replace_the_current_tree() {
    let mut e=load("sqrt(x)");let before=e.root.clone();
    assert!(e.apply(Action::Import{source:"$ frac(a,".into()}).is_err());assert_eq!(e.root,before);
}
#[test]
fn ordinary_letters_stay_separate_and_slash_opens_a_fraction() {
    let mut e=Editor::default();input(&mut e,"alpha/");
    assert_eq!(source(&e),"a l p h frac(a, \"\")");assert!(e.pending().is_none());
}
#[test]
fn stale_layout_stops_cannot_move_the_cursor_out_of_its_cell() {
    let mut e=load("frac(1 + 2 + 3, x)");
    let numerator=Cursor { slices: vec![CursorSlice{atom:0,cell:0}], pos: 0, occurrence: String::new() };
    e.apply(Action::Click { cursor: numerator, shift: false }).unwrap();
    // One stop describes this cell, the other claims a position past the end of
    // the cell the cursor moves into: what a layout measured before the cell
    // shrank looks like. Both used to be trusted as a cursor source.
    e.apply(Action::Geometry { stops: vec![
        StopGeometry { cursor: Cursor { slices: vec![CursorSlice{atom:0,cell:0}], pos: 1, occurrence: String::new() }, x: 10.0, y: 0.0 },
        StopGeometry { cursor: Cursor { slices: vec![CursorSlice{atom:0,cell:1}], pos: 9, occurrence: String::new() }, x: 10.0, y: 0.0 },
    ]}).unwrap();
    key(&mut e,"ArrowDown");
    assert_eq!(e.cursor.slices,vec![CursorSlice{atom:0,cell:1}]);
    assert!(valid(&e.root,&e.cursor),"{:?}",e.cursor);
    // The editor stays usable: no rejected step and no broken cursor is left.
    input(&mut e,"y");
    assert_eq!(source(&e),"frac(1 + 2 + 3, y x)");
}


