// SPDX-License-Identifier: GPL-2.0-or-later
use typformula_core::{Action,Editor,math::{Kind,Cursor},typst,view::View};
fn input(e:&mut Editor,s:&str){e.apply(Action::Input{text:s.into()}).unwrap();}
fn key(e:&mut Editor,s:&str){e.apply(Action::Key{key:s.into(),ctrl:false,shift:false}).unwrap();}
fn source(e:&Editor)->String{typst::write_cell(&e.root)}
fn find_raw(v:&View)->Option<&View>{if v.kind=="raw"{Some(v)}else{v.children.iter().find_map(find_raw)}}
/// 这个名字在当前的宏定义表里还绑着一条**可展**定义吗。
///
/// 这一问从前用 `Kind` 回答（未绑定是 `Raw`、绑定的可展宏是 `MacroCall`）；放宽之后
/// 两者都是 `MacroCall`，判据一度挪到**画法**上（`macro` / `raw_macro`）。两处都不对：
/// 画法是显示层的事，而这个问题是模型的——定义表里这个名字绑到的是什么，
/// `macro_registry` 直接答得了，不必绕一圈去问它被画成什么样。
fn expands(e: &Editor, name: &str) -> bool {
    typst::macro_registry(&e.definitions).get(name).is_some_and(|d| d.expandable)
}

#[test]
fn one_fallback_command_has_one_source_and_no_inner_cursor() {
    let mut e=Editor::default();let command="lr(sqrt(x)  + dif y, size: #100%)";
    input(&mut e,&format!("\\{command}"));key(&mut e,"Enter");
    assert_eq!(e.root.len(),1);assert_eq!(source(&e),command);assert!(e.root[0].cells.is_empty());
    key(&mut e,"Backspace");assert!(e.root.is_empty());e.apply(Action::Undo).unwrap();assert_eq!(source(&e),command);
}

#[test]
fn special_syntax_keeps_spelling_without_implicit_strings() {
    // Plain input is one character per key, and the two characters are written
    // apart: joined, `>=` would be re-read as one shorthand token.
    let mut e=Editor::default();input(&mut e,">=");assert_eq!(source(&e),"> =");
    for command in ["x >= y", r"\/", "lr(x >= y, size: #100%)"] {
        let mut e=Editor::default();input(&mut e,&format!("\\{command}"));key(&mut e,"Enter");
        assert_eq!(source(&e),command,"{command}");
    }
    for src in [">=", r"\/"] {
        let parsed=typst::parse_document(src).unwrap();assert_eq!(typst::write_cell(&parsed.root),src);
    }
}

#[test]
fn failed_source_can_be_corrected_or_restored_on_escape() {
    let mut e=Editor::default();input(&mut e,"\\undefinedname");key(&mut e,"Enter");
    let view=e.response().view;let raw=find_raw(&view).unwrap();let edit=raw.edit.clone().unwrap();
    e.apply(Action::EditSource{cursor:edit.clone(),source:raw.text.clone()}).unwrap();
    assert_eq!(e.pending(),Some("undefinedname"));
    key(&mut e,"Escape");assert_eq!(source(&e),"undefinedname");
    e.apply(Action::EditSource{cursor:edit,source:"undefinedname".into()}).unwrap();
    e.apply(Action::Key{key:"a".into(),ctrl:true,shift:false}).unwrap();input(&mut e,"lr(x, size: #100%)");key(&mut e,"Enter");
    assert_eq!(source(&e),"lr(x, size: #100%)");
    e.apply(Action::Undo).unwrap();assert_eq!(e.pending(),Some("lr(x, size: #100%)"));
    key(&mut e,"Escape");assert_eq!(source(&e),"undefinedname");
    e.apply(Action::EditSource{cursor:Cursor::default(),source:"stale".into()}).unwrap();assert!(e.pending().is_none());
}

#[test]
fn quote_mode_keeps_literal_operators_and_finishes_with_quote_or_enter() {
    let mut e=Editor::default();input(&mut e,"\"a / b >= c \\ _^");
    assert!(e.string_mode());assert!(e.completions().is_empty());assert!(matches!(e.root[0].kind,Kind::Text));
    assert_eq!(source(&e),r#""a / b >= c \\ _^""#);
    key(&mut e,"Enter");assert!(!e.string_mode());input(&mut e,"x");assert_eq!(e.root.len(),2);
    input(&mut e,"\"hello\"");assert!(!e.string_mode());assert!(e.cursor.slices.is_empty());
    input(&mut e,"\"");assert!(e.string_mode());
    e.apply(Action::Paste{text:r#""a\"b""#.into()}).unwrap();assert!(e.pending().is_some());key(&mut e,"Enter");
    assert!(source(&e).ends_with(r#""a\"b""#));
}

#[test]
fn a_text_cell_is_left_by_arrows_at_its_ends_not_only_by_enter() {
    // A string is a box the caret can walk out of. Swallowing Left/Right at either end
    // stranded the caret: the only ways out were `"` and Enter, and neither is what a
    // person reaches for when they want to leave a box. Up/Down already popped out, so
    // the horizontal pair was the inconsistent one.
    let mut e=Editor::default();input(&mut e,"\"");input(&mut e,"ab");
    assert!(e.string_mode());assert_eq!(e.cursor.slices.len(),1);
    key(&mut e,"ArrowLeft");key(&mut e,"ArrowLeft");
    assert!(e.string_mode(),"walking to the start of the string stays inside it");
    assert_eq!(e.cursor.pos,0);
    // The third Left is the one with nowhere to go, so it leaves the cell for the level
    // above -- the formula root here, which is still *inside* the formula.
    key(&mut e,"ArrowLeft");
    assert!(!e.string_mode(),"an arrow at the string's start leaves the text cell");
    assert!(e.cursor.slices.is_empty(),"leaving the cell returns to the level above it");
    assert_eq!(source(&e),r#""ab""#,"leaving the box must not touch the string");
    // And the same from the right-hand end, walking in the other direction.
    let mut e=Editor::default();input(&mut e,"\"");input(&mut e,"ab");
    key(&mut e,"ArrowRight");
    assert!(!e.string_mode(),"an arrow at the string's end leaves the text cell");
    assert!(e.cursor.slices.is_empty());
    assert_eq!(source(&e),r#""ab""#);
    // Up/Down do **not** leave: a string has one line, so a vertical move means nothing
    // inside it, and only the axis the text is read along carries the caret out.
    for k in ["ArrowUp","ArrowDown"] {
        let mut e=Editor::default();input(&mut e,"\"");input(&mut e,"ab");
        key(&mut e,k);
        assert!(e.string_mode(),"{k} has no meaning in a one-line string, so it must not leave");
        assert_eq!(e.cursor.slices.len(),1);
    }
}

#[test]
fn quote_inside_command_uses_enter_to_close_string_before_confirming() {
    // The string is left open on purpose: Enter closes it before the command is
    // confirmed. The named argument is typed only after that, so the draft still
    // holds an odd number of quotes while the assertion about string mode runs.
    let mut e=Editor::default();input(&mut e,"\\lr(\"a >= b");
    assert!(e.string_mode());assert!(e.response().command.is_none());
    key(&mut e,"Enter");assert!(!e.string_mode());assert_eq!(e.pending(),Some("lr(\"a >= b\""));
    input(&mut e,", size: #100%)");key(&mut e,"Enter");assert_eq!(source(&e),"lr(\"a >= b\", size: #100%)");assert!(e.pending().is_none());
}

#[test]
fn differential_is_not_a_builtin_substitution() {
    assert!(typformula_core::math::symbol("dif").is_none());
    let mut e=Editor::default();input(&mut e,"\\dif");key(&mut e,"Enter");
    assert!(matches!(&e.root[0].kind,Kind::Raw{source} if source=="dif"));
    assert_eq!(source(&e),"dif");
}

fn views<'a>(view: &'a View, kind: &str) -> Vec<&'a View> {
    let mut out = vec![];
    if view.kind == kind { out.push(view); }
    for child in &view.children { out.extend(views(child, kind)); }
    out
}
const RATIO: &str = "#let ratio(x, y) = $frac(#x, #x + #y)$";
fn macro_editor() -> Editor {
    let mut e = Editor::default();
    e.apply(Action::Import { source: format!("{RATIO}\n$ ratio(a, b) + ratio(c, d) $") }).unwrap();
    e
}
#[test]
fn macro_projections_share_only_their_own_argument_and_keep_clicked_occurrence() {
    let mut e = macro_editor();
    let response = e.response();
    let args = views(&response.view, "macro-argument");
    assert_eq!(args.len(), 6);
    assert_eq!(args.iter().map(|a| a.columns).collect::<Vec<_>>(), [0,0,1,0,0,1]);
    let first = views(args[0], "stop")[1].cursor.clone().unwrap();
    let second = views(args[1], "stop")[1].cursor.clone().unwrap();
    assert_eq!(first.slices, second.slices);
    assert_ne!(first.occurrence, second.occurrence);
    e.apply(Action::Click { cursor: second.clone(), shift: false }).unwrap();
    input(&mut e, "z");
    let response = e.response();
    assert_eq!(response.cursor.occurrence, second.occurrence.replace(".p1", ".p2"));
    assert_eq!(source(&e), "ratio(a z, b) + ratio(c, d)");
    let args = views(&response.view, "macro-argument");
    assert_eq!(views(args[0], "char").iter().map(|v| v.text.as_str()).collect::<String>(), "az");
    assert_eq!(views(args[1], "char").iter().map(|v| v.text.as_str()).collect::<String>(), "az");
    assert_eq!(views(args[3], "char")[0].text, "c");
    assert_eq!(views(&response.view, "stop").iter().filter(|v| v.active).count(), 1);
    e.apply(Action::Undo).unwrap();
    assert_eq!(source(&e), "ratio(a, b) + ratio(c, d)");
    e.apply(Action::Redo).unwrap();
    assert_eq!(source(&e), "ratio(a z, b) + ratio(c, d)");
}
#[test]
fn macro_classification_uses_typst_references_instead_of_text_replacement() {
    let defs = [RATIO,
        "#let opaque(x) = $frac(1, lr(#x, size: #100%))$",
        "#let fixed(x) = $frac(#x, lr(y, size: #100%))$",
        "#let literal(x) = $frac(x, x)$",
        "#let word(alpha) = $frac(alpha, alpha)$",
        "#let quoted(x) = $#x + lr(\"x\", size: #100%)$",
        "#let dynamic(x) = if x == 1 { $1$ } else { $2$ }",
        "#let defaults(x: 1) = $#x$",
        "#let constant = $sqrt(2)$",
        "#let unused(x) = $2$",
        "#let recursive(x) = $recursive(#x)$",
    ].join("\n");
    let registry = typst::macro_registry(&defs);
    let flags: Vec<_> = registry.entries.iter().map(|d| d.expandable).collect();
    assert_eq!(flags, [true,false,true,false,true,true,false,false,true,false,false]);
    let parsed = typst::parse_document(&format!("{defs}\n$ opaque(a) + ratio(a) + ratio(a, b) + constant $" )).unwrap();
    assert!(matches!(parsed.root[0].kind, Kind::Raw { .. }));
    assert!(matches!(parsed.root[2].kind, Kind::Raw { .. }));
    assert!(matches!(parsed.root[4].kind, Kind::MacroCall { .. }));
    assert!(matches!(parsed.root[6].kind, Kind::MacroCall { function:false, .. }));
}
#[test]
fn macro_edits_reclassify_without_losing_arguments_and_undo_restores_definitions() {
    let mut e = macro_editor();
    let old = e.response().source;
    e.apply(Action::SetDefinitions { definitions:"#let ratio(x, y) = $lr(#x + #y, size: #100%)$".into() }).unwrap();
    assert!(matches!(e.root[0].kind, Kind::Raw { .. }));
    assert_eq!(source(&e), "ratio(a, b) + ratio(c, d)");
    e.apply(Action::Undo).unwrap();
    assert_eq!(e.response().source, old);
    e.apply(Action::SetDefinitions { definitions:String::new() }).unwrap();
    assert!(!expands(&e, "ratio"), "定义被清掉后它不再是可展宏，只是名字未知的调用");
    assert!(matches!(e.root[0].kind, Kind::MacroCall { .. }), "参数仍然在树上，不该丢");
    e.apply(Action::SetDefinitions { definitions:RATIO.into() }).unwrap();
    assert!(expands(&e, "ratio"), "定义回来后又成了可展的宏调用");
    assert_eq!(source(&e), "ratio(a, b) + ratio(c, d)");
    let old = e.response().source;
    for invalid in ["#let ratio(x) = $", "$x$", "#import \"a.typ\""] {
        assert!(e.apply(Action::SetDefinitions { definitions:invalid.into() }).is_err());
        assert_eq!(e.response().source, old);
    }
    let parsed = typst::parse_document(&old).unwrap();
    assert_eq!(parsed.root, e.root);
}
#[test]
fn macro_cache_reuses_analysis_and_free_raw_uses_definition_scope() {
    let defs = "#let amount = 2\n#let fixed(x) = $frac(#x, lr(amount, size: #100%))$\n#let amount = 5\n";
    let registry = typst::macro_registry(defs);
    let again = typst::macro_registry(defs);
    assert!(std::sync::Arc::ptr_eq(&registry, &again));
    let mut e = Editor::default();
    e.apply(Action::Import { source:format!("{defs}$fixed(a)$") }).unwrap();
    let view = e.response().view;
    let raw = views(&view, "raw")[0];
    assert_eq!(raw.text, "lr(amount, size: #100%)");
    assert!(!raw.definitions.as_ref().unwrap().contains("amount = 5"));
    assert!(std::sync::Arc::ptr_eq(&registry, &typst::macro_registry(defs)));
    assert!(!std::sync::Arc::ptr_eq(&registry, &typst::macro_registry(RATIO)));
}
#[test]
fn macro_names_shadow_builtins_and_empty_command_slots_are_editable() {
    let mut e = Editor::default();
    e.apply(Action::SetDefinitions { definitions:RATIO.into() }).unwrap();
    input(&mut e, "\\ratio"); key(&mut e, "Enter");
    assert!(e.pending().is_none());
    assert_eq!(e.cursor.slices.len(), 1);
    assert_eq!(e.root[0].cells.len(), 2);
    input(&mut e, "a"); key(&mut e, "Tab"); input(&mut e, "b");
    assert_eq!(source(&e), "ratio(a, b)");
    key(&mut e, "Home"); key(&mut e, "Backspace");
    assert!(matches!(e.root[0].kind, Kind::MacroCall { .. }));
    assert_eq!(e.response().selected_source, "ratio(a, b)");
    let parsed = typst::parse_document("#let frac(x, y) = $lr(#x + #y, size: #100%)$\n$frac(a, b)$").unwrap();
    assert!(matches!(parsed.root[0].kind, Kind::Raw { .. }));
    let registry = typst::macro_registry("#let ratio(x) = $#x$\n#let ratio = 3");
    assert!(registry.entries[0].expandable && registry.entries[0].shadowed);
    assert!(!registry.get("ratio").unwrap().expandable);
}
#[test]
fn repeated_nested_calls_have_a_bounded_projection() {
    let mut expr = "a".to_string();
    for _ in 0..16 { expr = format!("twice({expr})"); }
    let mut e = Editor::default();
    e.apply(Action::Import { source:format!("#let twice(x) = $#x + #x$\n$ {expr} $") }).unwrap();
    let view = e.response().view;
    assert!(!views(&view, "raw_macro").is_empty());
    assert!(views(&view, "stop").len() < 10000);
    assert_eq!(source(&e), expr);
}

const PD: &str = "#let pd(f, x) = $frac(partial #f, partial #x)$";
const JAC: &str = "#let jac(f1, f2, x1, x2) = $mat(pd(#f1, #x1), pd(#f1, #x2); pd(#f2, #x1), pd(#f2, #x2))$";
#[test]
fn jacobian_expands_dependencies_and_links_outer_parameters() {
    let mut e = Editor::default();
    e.apply(Action::Import { source:format!("{PD}\n{JAC}\n$ jac(a, b, x, y) $") }).unwrap();
    let state = e.response();
    assert!(state.macros.iter().all(|d| d.expandable));
    assert_eq!(views(&state.view, "table").len(), 1);
    assert_eq!(views(&state.view, "fraction").len(), 4);
    assert_eq!(e.root.len(), 1);
    assert_eq!(e.root[0].cells.len(), 4);
    let args = views(&state.view, "macro-argument");
    assert_eq!(args.iter().map(|v| v.columns).collect::<Vec<_>>(), [0,2,0,3,1,2,1,3]);
    assert_eq!(args.iter().map(|v| v.text.as_str()).collect::<Vec<_>>(), ["f1","x1","f1","x2","f2","x1","f2","x2"]);
    let cursor = views(args[2], "stop")[1].cursor.clone().unwrap();
    assert_eq!(cursor.slices.len(), 1);
    e.apply(Action::Click { cursor:cursor.clone(), shift:false }).unwrap();
    input(&mut e, "z");
    let state = e.response();
    assert_eq!(state.cursor.occurrence, cursor.occurrence.replace(".p1", ".p2"));
    assert_eq!(source(&e), "jac(a z, b, x, y)");
    let args = views(&state.view, "macro-argument");
    for i in [0,2] { assert_eq!(views(args[i], "char").iter().map(|v| v.text.as_str()).collect::<String>(), "az"); }
    assert_eq!(views(&state.view, "stop").iter().filter(|v| v.active).count(), 1);
    // Two assertions used to sit here: `views(&state.view, "parameter").is_empty()` and
    // `views(&state.view, "template-call").is_empty()`. They are **deleted, not moved**:
    // once `Parameter`/`TemplateCall` become variants of the template tree rather than
    // arrangements a node can be drawn with, a `parameter`/`template-call` view node is
    // impossible to build, so those assertions could only ever be vacuously true. What
    // they meant to guard — "internal markers never reach the wire" — is now guarded
    // harder by `tests/editing_model.rs`, which checks the **spelling** that leaked
    // (`#parameter0`) instead of a kind name that never appears.
    e.apply(Action::Undo).unwrap();
    assert_eq!(source(&e), "jac(a, b, x, y)");
    e.apply(Action::Redo).unwrap();
    assert_eq!(source(&e), "jac(a z, b, x, y)");
    let exported = e.response().source;
    assert_eq!(typst::parse_document(&exported).unwrap().root, e.root);
}
#[test]
fn nested_templates_capture_versions_even_after_opaque_shadowing() {
    let defs = format!("{PD}\n{JAC}\n#let pd(f, x) = $lr(#f + #x, size: #100%)$\n#let later(f, x) = $pd(#f, #x)$");
    let mut e = Editor::default();
    e.apply(Action::Import { source:format!("{defs}\n$jac(a, b, x, y) + pd(a, x) + later(a, x)$") }).unwrap();
    let state = e.response();
    assert!(state.macros[0].expandable && state.macros[0].shadowed);
    assert!(state.macros[1].expandable);
    assert!(!state.macros[2].expandable && !state.macros[3].expandable);
    assert_eq!(views(&state.view, "fraction").len(), 4);
    assert!(matches!(e.root[2].kind, Kind::Raw { .. }));
    assert!(matches!(e.root[4].kind, Kind::Raw { .. }));

    let defs = format!("{PD}\n{JAC}\n#let pd(f, x) = $sqrt(#f + #x)$");
    e.apply(Action::SetDefinitions { definitions:defs }).unwrap();
    let state = e.response();
    assert_eq!(views(&state.view, "fraction").len(), 4);
    assert_eq!(views(&state.view, "decorated").len(), 1);
    let registry = typst::macro_registry(&format!("{PD}\n#let local(pd, f, x) = $pd(#f, #x)$"));
    assert!(!registry.get("local").unwrap().expandable);
    let registry = typst::macro_registry(&format!("{PD}\n#let pd(f, x) = $pd(#f, #x)$"));
    assert!(!registry.get("pd").unwrap().expandable, "self recursion must not bind the previous pd");
    e.apply(Action::Import { source:"#let value = $sqrt(2)$\n#let value = $value + 1$\n$value$".into() }).unwrap();
    assert_eq!(views(&e.response().view, "decorated").len(), 1, "normal initializers see the preceding binding");
}
#[test]
fn composed_arguments_keep_outer_colors_and_lexical_raw_contexts() {
    let defs = "#let amount = 2\n#let pair(x) = $frac(#x, #x + lr(amount, size: #100%))$\n#let amount = 5\n#let outer(y) = $pair(#y + 1)$\n#let amount = 9";
    let mut e = Editor::default();
    e.apply(Action::Import { source:format!("{defs}\n$outer(a)$") }).unwrap();
    let state = e.response();
    let args = views(&state.view, "macro-argument");
    assert_eq!(args.len(), 2);
    assert!(args.iter().all(|a| a.text == "y" && a.columns == 0));
    let raw = views(&state.view, "raw")[0];
    assert_eq!(raw.text, "lr(amount, size: #100%)");
    assert!(!raw.definitions.as_ref().unwrap().contains("amount = 5"));
    assert!(!raw.definitions.as_ref().unwrap().contains("amount = 9"));
    // A reference in the argument expression belongs to the caller's scope.
    e.apply(Action::SetDefinitions { definitions:defs.replace("pair(#y + 1)", "pair(#y + lr(amount, size: #100%))") }).unwrap();
    let state = e.response();
    let raws = views(&state.view, "raw");
    assert_eq!(raws.len(), 3);
    assert!(raws[0].definitions.as_ref().unwrap().contains("amount = 5"));
    assert!(raws[1].definitions.as_ref().unwrap().contains("amount = 5"));
    assert!(!raws[2].definitions.as_ref().unwrap().contains("amount = 5"));
}
#[test]
fn definition_edits_reuse_only_the_unchanged_prefix_and_rebuild_dependents() {
    let original = format!("{PD}\n{JAC}\n#let tail(x) = $jac(#x, #x, #x, #x)$");
    let first = typst::macro_registry(&original);
    // Reuse is decided per definition by comparing its own source text, so an
    // edited definition is rebuilt while the earlier ones keep their templates.
    // The registry cache is process-wide and shared with the server threads, so
    // which cached entry supplied the reuse is not part of the contract: compare
    // the templates themselves, not their addresses.
    let tail_edit = typst::macro_registry(&original.replace("#let tail(x)", "#let renamed(x)"));
    assert_eq!(first.entries[0].template, tail_edit.entries[0].template);
    assert_eq!(first.entries[1].template, tail_edit.entries[1].template);
    // The edited definition is re-analyzed under its new binding name. Its
    // template body is unchanged, so equal templates are correct here.
    assert_eq!(tail_edit.entries[2].name, "renamed");
    assert_eq!(tail_edit.entries[2].params, vec!["x".to_string()]);
    let pd_edit = typst::macro_registry(&original.replace("frac(partial #f, partial #x)", "lr(#f + #x, size: #100%)"));
    assert!(pd_edit.entries.iter().all(|d| !d.expandable));
    let restored = typst::macro_registry(&original);
    assert!(restored.entries.iter().all(|d| d.expandable));
    // An identical definition set is served from the cache, address for address.
    assert!(std::sync::Arc::ptr_eq(&first, &restored));
    // `jac` calls `pd`, which this registry does not define. That used to make `jac`
    // unexpandable: the calibration was "a parameter used inside a `Raw`", and an
    // unknown call *was* a `Raw`, so `#f1` sat inside one and could not be offered as a
    // slot. Now an unknown call whose arguments are all positional is a `MacroCall`
    // (`raw_macro`: one image of the call, or its name and the argument slots once the
    // caret enters), so the parameters do reach cells and expansion is safe.
    let removed = typst::macro_registry(JAC);
    assert!(removed.entries[0].expandable,
            "被调名字未知不再阻止展开：那个调用会画成 raw_macro，参数仍然可编辑");
    let mut e = Editor::default();
    e.apply(Action::Import { source:format!("{original}\n$jac(a,b,x,y)$") }).unwrap();
    e.apply(Action::SetDefinitions { definitions:JAC.into() }).unwrap();
    // `JAC` defines `jac` but not `pd`. That used to keep the call unexpanded (it was a
    // `Raw`); now it expands, and what is unknown about it shows up *inside* the
    // expansion as a `raw_macro` — the call itself, drawn as one image until the caret
    // enters it.
    assert!(matches!(e.root[0].kind, Kind::MacroCall { .. }));
    assert!(!views(&e.response().view, "raw_macro").is_empty(),
            "未定义的 pd(…) 在展开结果里画成 raw_macro");
    e.apply(Action::Undo).unwrap();
    assert_eq!(views(&e.response().view, "fraction").len(), 4);
}
#[test]
fn dependency_graphs_are_shared_and_projection_limits_do_not_change_classification() {
    let mut defs = "#let layer0(x) = $#x$".to_string();
    for i in 1..30 { defs.push_str(&format!("\n#let layer{i}(x) = $layer{}(#x) + layer{}(#x)$", i-1, i-1)); }
    let registry = typst::macro_registry(&defs);
    assert!(registry.entries.iter().all(|d| d.expandable));
    // The template is a **display** tree now, so what used to be "the root cell holds
    // at most three atoms" is "the root node holds at most three children": the same
    // count of the same material, read off the tree the registry actually stores.
    let material = |d: &typst::MacroDefinition| match &*d.template {
        typformula_core::view::ViewTemplate::Node(node) => node.children.len(),
        _ => 0,
    };
    assert!(registry.entries.iter().all(|d| material(d) <= 3));
    let mut e = Editor::default();
    e.apply(Action::Import { source:format!("{defs}\n$layer29(a)$") }).unwrap();
    let state = e.response();
    assert_eq!(views(&state.view, "raw_macro").len(), 1);
    assert_eq!(source(&e), "layer29(a)");
    let mut chain = "#let layer0(x) = $#x$".to_string();
    for i in 1..70 { chain.push_str(&format!("\n#let layer{i}(x) = $layer{}(#x)$", i-1)); }
    e.apply(Action::Import { source:format!("{chain}\n$layer69(a)$") }).unwrap();
    assert!(e.response().macros.iter().all(|d| d.expandable));
    assert_eq!(views(&e.response().view, "raw_macro").len(), 1);
}


