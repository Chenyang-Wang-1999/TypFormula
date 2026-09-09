// SPDX-License-Identifier: GPL-2.0-or-later
use lyx_typst_core::{Action,Editor,math::Kind,typst};
fn key(e:&mut Editor,k:&str){e.apply(Action::Key{key:k.into(),shift:false,ctrl:false}).unwrap();}
fn load()->Editor{let mut e=Editor::default();e.apply(Action::Import{source:"undefinedfunc(α)".into()}).unwrap();e}
fn status(e:&mut Editor,failed:bool){e.apply(Action::PreviewResult{source:"undefinedfunc(α)".into(),definitions:e.definitions.clone(),display:e.display,failed}).unwrap();}

#[test]
fn failed_block_enters_from_either_side_at_the_corresponding_caret() {
    let mut e=load();status(&mut e,true);key(&mut e,"ArrowRight");
    assert_eq!(e.pending(),Some("undefinedfunc(α)"));
    assert_eq!(e.command_context().unwrap().draft_caret,0);
    key(&mut e,"Escape");assert_eq!(e.cursor.pos,1);
    key(&mut e,"ArrowLeft");assert_eq!(e.command_context().unwrap().draft_caret,"undefinedfunc(α)".len());
    e.apply(Action::Key{key:"a".into(),ctrl:true,shift:false}).unwrap();
    e.apply(Action::Input{text:"sqrt(3)".into()}).unwrap();key(&mut e,"Enter");
    assert!(matches!(e.root[0].kind,Kind::Sqrt));
    assert_eq!(typst::write_cell(&e.root),"sqrt(3)");
}

#[test]
fn ready_blocks_and_shift_selection_remain_atomic() {
    let mut e=load();status(&mut e,false);key(&mut e,"ArrowRight");
    assert!(e.pending().is_none());assert_eq!(e.cursor.pos,1);
    status(&mut e,true);
    e.apply(Action::Key{key:"ArrowLeft".into(),ctrl:false,shift:true}).unwrap();
    assert!(e.pending().is_none());assert_eq!(e.selection(),Some((0,1)));
    key(&mut e,"Backspace");assert!(e.root.is_empty());
}

#[test]
fn render_status_has_no_source_or_undo_effect_and_is_scoped_to_definitions() {
    let mut e=load();let revision=e.revision;let root=e.root.clone();
    e.apply(Action::PreviewResult{source:"undefinedfunc(α)".into(),definitions:"#let a = 1\n".into(),display:e.display,failed:true}).unwrap();
    assert_eq!(e.root,root);assert_eq!(e.revision,revision);
    key(&mut e,"ArrowRight");assert!(e.pending().is_none());
    status(&mut e,true);status(&mut e,false);key(&mut e,"ArrowLeft");
    assert!(e.pending().is_none());assert_eq!(e.cursor.pos,0);
}
