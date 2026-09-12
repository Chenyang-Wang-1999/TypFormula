// SPDX-License-Identifier: GPL-2.0-or-later
//! 渲染失败的片段：它没有图像，左右键是唯一的修复入口。
//!
//! 片段范例是一个**裸标识符**，不是一个调用。这一点是"放宽 RawMacro"之后才成立的：
//! 调用只要实参全是位置实参就会被建成 `MacroCall`（名字未知时画成 `raw_macro`，光标
//! 在外画自己的一张图、进去画名字与参数槽），所以 `undefinedname` 现在**是可编辑
//! 的结构节点**，不再是"没有图像的片段"。标识符永远不会变成调用，因此它是"编辑器不
//! 建模的片段"里最稳的那个范例——`docs/desktop.md` 举的 `sum`/`dif` 也是这一类。
use typformula_core::{Action,Editor,typst};
fn key(e:&mut Editor,k:&str){e.apply(Action::Key{key:k.into(),shift:false,ctrl:false}).unwrap();}
fn load()->Editor{let mut e=Editor::default();e.apply(Action::Import{source:"undefinedname".into()}).unwrap();e}
fn status(e:&mut Editor,failed:bool){e.apply(Action::PreviewResult{source:"undefinedname".into(),definitions:e.definitions.clone(),display:e.display,failed}).unwrap();}

#[test]
fn failed_calls_open_source_and_cancel_restores_their_structure() {
    for source in ["undefinedfn(a)","bold(a)"] {
        let mut e=Editor::default();e.apply(Action::Import{source:source.into()}).unwrap();
        let root=e.root.clone();
        e.apply(Action::PreviewResult{source:source.into(),definitions:e.definitions.clone(),display:e.display,failed:true}).unwrap();
        key(&mut e,"ArrowRight");assert_eq!(e.pending(),Some(source));
        key(&mut e,"Escape");assert_eq!(e.root,root);
        key(&mut e,"ArrowLeft");assert_eq!(e.pending(),Some(source));
        e.apply(Action::Key{key:"a".into(),ctrl:true,shift:false}).unwrap();
        e.apply(Action::Input{text:"sqrt(3)".into()}).unwrap();key(&mut e,"Enter");
        assert_eq!(typst::write_cell(&e.root),"sqrt(3)");
    }
}

#[test]
fn failed_block_enters_from_either_side_at_the_corresponding_caret() {
    let mut e=load();status(&mut e,true);key(&mut e,"ArrowRight");
    assert_eq!(e.pending(),Some("undefinedname"));
    assert_eq!(e.command_context().unwrap().draft_caret,0);
    key(&mut e,"Escape");assert_eq!(e.cursor.pos,1);
    key(&mut e,"ArrowLeft");assert_eq!(e.command_context().unwrap().draft_caret,"undefinedname".len());
    e.apply(Action::Key{key:"a".into(),ctrl:true,shift:false}).unwrap();
    e.apply(Action::Input{text:"sqrt(3)".into()}).unwrap();key(&mut e,"Enter");
    assert_eq!(e.root[0].shape().view, "sqrt", "粘贴的调用应当成为根式形状的节点");
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
    e.apply(Action::PreviewResult{source:"undefinedname".into(),definitions:"#let a = 1\n".into(),display:e.display,failed:true}).unwrap();
    assert_eq!(e.root,root);assert_eq!(e.revision,revision);
    key(&mut e,"ArrowRight");assert!(e.pending().is_none());
    status(&mut e,true);status(&mut e,false);key(&mut e,"ArrowLeft");
    assert!(e.pending().is_none());assert_eq!(e.cursor.pos,0);
}

#[test]
fn one_batch_reports_every_fragment_of_a_render_pass() {
    let mut e=load();let definitions=e.definitions.clone();let display=e.display;
    e.apply(Action::PreviewResults{sources:vec!["undefinedname".into()],definitions:definitions.clone(),display,failed:true}).unwrap();
    key(&mut e,"ArrowRight");assert_eq!(e.pending(),Some("undefinedname"));
    key(&mut e,"Escape");assert_eq!(e.cursor.pos,1);
    // A fragment that turns out to render after all is taken out of the failed set.
    e.apply(Action::PreviewResults{sources:vec!["undefinedname".into()],definitions,display,failed:false}).unwrap();
    key(&mut e,"ArrowLeft");
    assert!(e.pending().is_none());assert_eq!(e.cursor.pos,0);
}

#[test]
fn an_unparseable_repair_of_a_fragment_is_kept_as_source() {
    let mut e=load();status(&mut e,true);key(&mut e,"ArrowRight");
    e.apply(Action::Key{key:"a".into(),ctrl:true,shift:false}).unwrap();
    // A fragment that can only be entered to be repaired must not lose the repair on
    // Enter. The example is `lr(…)` for two reasons: `cases` became a structured name,
    // and — since calls with purely positional arguments are now structured too — a
    // repair only stays source when its arguments are *not* all positional, which the
    // named `size:` here guarantees.
    e.apply(Action::Input{text:"lr(1 & x > 0, size: #100%)".into()}).unwrap();
    key(&mut e,"Enter");
    assert!(e.pending().is_none());
    assert_eq!(typst::write_cell(&e.root),"lr(1 & x > 0, size: #100%)");
    assert_eq!(e.cursor.pos,1);
    // Escape after a repair restores the fragment the draft was opened from.
    e.apply(Action::PreviewResults{sources:vec!["lr(1 & x > 0, size: #100%)".into()],definitions:e.definitions.clone(),display:e.display,failed:true}).unwrap();
    key(&mut e,"ArrowLeft");
    assert_eq!(e.pending(),Some("lr(1 & x > 0, size: #100%)"));
    e.apply(Action::Key{key:"a".into(),ctrl:true,shift:false}).unwrap();
    e.apply(Action::Input{text:"nonsense(".into()}).unwrap();
    assert_eq!(e.pending(),Some("nonsense("));
    key(&mut e,"Escape");
    assert!(e.pending().is_none());
    assert_eq!(typst::write_cell(&e.root),"lr(1 & x > 0, size: #100%)");
}


