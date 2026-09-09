// SPDX-License-Identifier: GPL-2.0-or-later
use visual_typst_core::{Action, Editor, view::View};

fn branches(view: &View) -> usize {
    usize::from(view.attachment.is_some()) + view.children.iter().map(branches).sum::<usize>()
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


