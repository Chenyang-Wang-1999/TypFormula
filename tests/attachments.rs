// SPDX-License-Identifier: GPL-2.0-or-later
use typformula_core::{Action, Editor, view::View};

fn branches(view: &View) -> usize {
    usize::from(view.attachment.is_some()) + view.children.iter().map(branches).sum::<usize>()
}

#[test]
fn newly_created_empty_scripts_keep_their_placeholder_and_cursor() {
    fn find<'a>(v:&'a View,kind:&str)->Option<&'a View> {
        if v.kind==kind {Some(v)} else {v.children.iter().find_map(|c|find(c,kind))}
    }
    for (key,index,other) in [("^",1,2),("_",2,1)] {
        let mut e=Editor::default();
        e.apply(Action::Input{text:"a".into()}).unwrap();
        e.apply(Action::Input{text:key.into()}).unwrap();
        let response=e.response();let script=find(&response.view,"scripts").unwrap();
        assert_eq!(script.children[index].kind,"empty-cell");
        assert!(script.children[index].children.iter().any(|n|n.kind=="stop" && n.active));
        assert_eq!(script.children[other].kind,"absent");
        assert_eq!(typformula_core::typst::write_cell(&e.root),"a");
        e.apply(Action::Input{text:"2".into()}).unwrap();
        assert_eq!(typformula_core::typst::write_cell(&e.root),format!("a{key}(2)"));
        e.apply(Action::Key{key:"Backspace".into(),shift:false,ctrl:false}).unwrap();
        let response=e.response();let script=find(&response.view,"scripts").unwrap();
        assert_eq!(script.children[index].kind,"empty-cell");
    }
}
#[test]
fn query_only_top_level_scripts_without_replacing_the_tree() {
    let mut e = Editor::default();
    e.apply(Action::Import { source: "$ sum_i + x^2 + frac(sum_j, y) $".into() }).unwrap();
    let root = e.root.clone();
    let response = e.response();
    assert_eq!(branches(&response.view), 2); // No operator-name prefilter.
    assert_eq!(e.root, root);
}

#[test]
fn alignment_cells_keep_top_level_attachment_requests_but_fractions_do_not() {
    let mut e=Editor::default();
    e.apply(Action::Import{source:"$ H_(\"int\") = & g sum_(j) sigma_(x) \\\n= & frac(sum_k, y) $".into()}).unwrap();
    let root=e.root.clone();let view=e.response().view;
    fn collect(view:&View,out:&mut Vec<String>) {
        if let Some(text)=&view.attachment {out.push(text.clone());}
        for child in &view.children {collect(child,out);}
    }
    let mut requests=vec![];collect(&view,&mut requests);
    assert_eq!(requests,vec!["H_(\"int\")","sum_(j)","sigma_(x)"]);
    assert_eq!(e.root,root);
}
#[test]
fn display_mode_roundtrips_and_participates_in_undo() {
    let mut e = Editor::default();
    e.apply(Action::Import { source: "$sum_i$".into() }).unwrap();
    assert!(!e.response().display);
    assert!(e.response().source.starts_with("$sum"));
    let root = e.root.clone();
    e.apply(Action::SetDisplay { display: true }).unwrap();
    assert!(e.response().source.starts_with("$ sum"));
    assert_eq!(e.root, root);
    e.apply(Action::Undo).unwrap();
    assert!(!e.response().display);
    assert_eq!(e.root, root);
}


