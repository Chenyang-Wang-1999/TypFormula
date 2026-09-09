// SPDX-License-Identifier: GPL-2.0-or-later
use lyx_typst_core::{Action, Editor, math::{Cursor, Kind}, typst};

fn input(e: &mut Editor, text: &str) { e.apply(Action::Input { text: text.into() }).unwrap(); }
fn key(e: &mut Editor, key: &str) { modified_key(e, key, false, false); }
fn modified_key(e: &mut Editor, key: &str, shift: bool, ctrl: bool) {
    e.apply(Action::Key { key: key.into(), shift, ctrl }).unwrap();
}

#[test]
fn only_enter_parses_spaces_and_parentheses() {
    let mut e = Editor::default();
    input(&mut e, "\\frac(a, b) + alpha beta");
    assert_eq!(e.pending(), Some("frac(a, b) + alpha beta"));
    assert_eq!(e.root.len(), 1);
    for name in ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Tab", "Home", "End"] {
        key(&mut e, name);
        assert_eq!(e.pending(), Some("frac(a, b) + alpha beta"), "{name}");
    }
    key(&mut e, "Enter");
    assert!(e.pending().is_none());
    assert!(matches!(e.root[0].kind, Kind::Frac));
    assert_eq!(typst::write_cell(&e.root), "frac(a, b) + alpha beta");
}

#[test]
fn arrows_edit_inside_draft_and_never_leave_at_boundaries() {
    let mut e = Editor::default(); input(&mut e, "\\α中");
    key(&mut e, "ArrowLeft"); input(&mut e, " + ");
    assert_eq!(e.pending(), Some("α + 中"));
    key(&mut e, "Delete"); key(&mut e, "Backspace");
    assert_eq!(e.pending(), Some("α +"));
    key(&mut e, "Home"); key(&mut e, "ArrowLeft"); key(&mut e, "Backspace");
    assert_eq!(e.pending(), Some("α +"));
    key(&mut e, "End"); key(&mut e, "ArrowRight"); key(&mut e, "Delete");
    assert_eq!(e.pending(), Some("α +"));
    modified_key(&mut e, "a", false, true); key(&mut e, "Backspace");
    key(&mut e, "Backspace"); assert_eq!(e.pending(), Some(""));
    key(&mut e, "Enter"); assert!(e.root.is_empty());
}

#[test]
fn completion_buttons_and_toolbar_fill_without_confirming() {
    let mut e = Editor::default(); input(&mut e, "\\alp");
    key(&mut e, "ArrowDown"); key(&mut e, "Tab");
    assert_eq!(e.pending(), Some("alp"));
    e.apply(Action::Complete { name: "alpha".into() }).unwrap();
    assert_eq!(e.pending(), Some("alpha"));
    e.apply(Action::Insert { name: "frac".into() }).unwrap();
    assert_eq!(e.pending(), Some("frac"));
    e.apply(Action::Click { cursor: Cursor::default(), shift: false }).unwrap();
    e.apply(Action::AddRow).unwrap(); e.apply(Action::AddColumn).unwrap();
    assert_eq!(e.pending(), Some("frac"));
    key(&mut e, " "); assert_eq!(e.pending(), Some("frac "));
    key(&mut e, "Enter");
    assert!(matches!(e.root[0].kind, Kind::Frac));
    assert_eq!(e.cursor.slices[0].cell, 0);
}

#[test]
fn invalid_expression_stays_editable_until_fixed() {
    let mut e = Editor::default(); input(&mut e, "\\frac(a, b");
    key(&mut e, "Enter");
    assert_eq!(e.pending(), Some("frac(a, b")); assert!(!e.message.is_empty());
    input(&mut e, ")"); key(&mut e, "Enter");
    assert_eq!(typst::write_cell(&e.root), "frac(a, b)");
    e.apply(Action::Undo).unwrap();
    assert_eq!(e.pending(), Some("frac(a, b)"));
    e.apply(Action::Redo).unwrap(); assert!(e.pending().is_none());
}

#[test]
fn paste_newlines_do_not_confirm_and_escape_cancels() {
    let mut e = Editor::default(); input(&mut e, "\\");
    e.apply(Action::Paste { text: "alpha\n+ beta".into() }).unwrap();
    assert_eq!(e.pending(), Some("alpha\n+ beta"));
    key(&mut e, "Enter"); assert_eq!(typst::write_cell(&e.root), "alpha + beta");
    input(&mut e, "\\sqrt(x)"); key(&mut e, "Escape");
    assert!(e.pending().is_none()); assert_eq!(typst::write_cell(&e.root), "alpha + beta");
}

#[test]
fn selection_and_caret_are_rendered_inside_draft() {
    let mut e = Editor::default(); input(&mut e, "\\alpha beta");
    modified_key(&mut e, "ArrowLeft", true, false);
    assert_eq!(e.response().selected_source, "a");
    input(&mut e, "o"); assert_eq!(e.pending(), Some("alpha beto"));
    key(&mut e, "Home");
    let response = e.response();
    let draft = &response.view.children[1];
    assert_eq!(draft.kind, "unknown");
    assert_eq!(draft.children[0].kind, "draft-caret");
    assert_eq!(draft.children.iter().filter(|v| v.kind == "draft-text").map(|v| v.text.as_str()).collect::<String>(), "alpha beto");
}

#[test]
fn lsp_replaces_only_the_token_and_keeps_command_mode() {
    use lyx_typst_core::cursor::CommandCompletion;
    let mut e=Editor::default();input(&mut e,"\\cases(alph");
    let context=e.command_context().unwrap();
    assert_eq!(&context.source[context.start..context.end], "cases(alph");
    e.apply(Action::LspCompletions {draft:context.draft,caret:context.draft_caret,items:vec![CommandCompletion {label:"alpha".into(),replacement:"cases(alpha".into(),caret:11}]}).unwrap();
    key(&mut e,"Tab");assert_eq!(e.pending(),Some("cases(alpha"));
    input(&mut e,", beta)");key(&mut e,"Enter");
    assert!(matches!(e.root[0].kind,Kind::Raw {..}));
    assert_eq!(typst::write_cell(&e.root),"cases(alpha, beta)");
}

#[test]
fn late_lsp_response_cannot_overwrite_a_new_draft() {
    use lyx_typst_core::cursor::CommandCompletion;
    let mut e=Editor::default();input(&mut e,"\\alph");let context=e.command_context().unwrap();
    input(&mut e,"a + beta");
    e.apply(Action::LspCompletions {draft:context.draft,caret:context.draft_caret,items:vec![CommandCompletion {label:"WRONG".into(),replacement:"wrong".into(),caret:5}]}).unwrap();
    key(&mut e,"Enter");assert_eq!(typst::write_cell(&e.root),"alpha + beta");
}

#[test]
fn rendered_fallback_is_one_atom_for_navigation_delete_and_undo() {
    let mut e=Editor::default();input(&mut e,"\\cancel(x)");key(&mut e,"Enter");
    assert_eq!(e.root.len(),1);assert!(e.root[0].cells.is_empty());
    key(&mut e,"ArrowLeft");assert_eq!(e.cursor.pos,0);assert!(e.cursor.slices.is_empty());
    key(&mut e,"ArrowRight");assert_eq!(e.cursor.pos,1);
    key(&mut e,"Backspace");assert!(e.root.is_empty());
    e.apply(Action::Undo).unwrap();assert_eq!(typst::write_cell(&e.root),"cancel(x)");
    key(&mut e,"Home");key(&mut e,"Delete");assert!(e.root.is_empty());
}

#[test]
fn command_context_includes_opaque_definitions_and_utf8_caret() {
    let mut e=Editor::default();
    e.apply(Action::Import {source:"#let twice(x) = $ #x + #x $\n$ twice(a) $".into()}).unwrap();
    key(&mut e,"ArrowRight");input(&mut e,"\\α + alph");
    let context=e.command_context().unwrap();
    assert!(context.source.starts_with("#let twice(x)"));
    assert_eq!(&context.source[context.start..context.caret],"α + alph");
    key(&mut e,"Escape");key(&mut e,"End");key(&mut e," ");
    input(&mut e,"\\twice(b)");key(&mut e,"Enter");
    assert!(matches!(&e.root[1].kind,Kind::MacroCall {name, ..} if name=="twice"));
    assert_eq!(typst::write_atom(&e.root[1]), "twice(b)");
}
