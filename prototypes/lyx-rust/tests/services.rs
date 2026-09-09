// SPDX-License-Identifier: GPL-2.0-or-later
#![cfg(not(target_arch = "wasm32"))]
use lyx_typst_core::{Action, Editor, services::{Services, CompletionRequest, RenderRequest}};

#[test]
#[ignore = "requires local Tinymist"]
fn differential_symbol_keeps_its_name_and_renders() {
    let service=Services::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let mut editor=Editor::default();editor.apply(Action::Input{text:"\\dif".into()}).unwrap();
    let context=editor.command_context().unwrap();
    let reply=service.complete(CompletionRequest {source:context.source,start:context.start,end:context.end,caret:context.caret}).unwrap();
    editor.apply(Action::LspCompletions {draft:context.draft,caret:context.draft_caret,items:reply.items}).unwrap();
    editor.apply(Action::Key {key:"Enter".into(),shift:false,ctrl:false}).unwrap();
    let expression=lyx_typst_core::typst::write_cell(&editor.root);
    assert!(matches!(&editor.root[0].kind,lyx_typst_core::math::Kind::Raw {source} if source=="dif"));
    let output=service.render(RenderRequest { display:true,expression:expression.clone(),definitions:String::new()}).unwrap();
    let svg=output["svg"].as_str().unwrap();
    assert!(svg.contains("<svg") && svg.contains("<path"));assert_eq!(expression,"dif");
}

#[test]
#[ignore = "requires a locally installed Tinymist executable"]
fn real_tinymist_completions_and_typst_svg() {
    let service=Services::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    assert_eq!(service.status()["available"], true);
    for (draft, expected) in [("alph", "alpha"), ("arrow.r", "r"), ("cases(alph", "alpha")].into_iter().cycle().take(9) {
        let mut editor=Editor::default();editor.apply(Action::Input{text:format!("\\{draft}")}).unwrap();
        let context=editor.command_context().unwrap();
        let reply=service.complete(CompletionRequest {source:context.source,start:context.start,end:context.end,caret:context.caret}).unwrap();
        assert!(reply.items.iter().any(|i| i.label.contains(expected)),"missing {expected} for {draft}");
    }
    for expression in ["cancel(x)", "cases(x, y)", "arrow.r.double"] {
        let result=service.render(RenderRequest { display:true, expression:expression.into(),definitions:String::new() }).unwrap();
        assert!(result["svg"].as_str().unwrap().contains("<svg"));
    }
    assert!(service.render(RenderRequest { display:true,expression:"not_a_defined_function(x)".into(),definitions:String::new()}).is_err());
    let result = service.render(RenderRequest { display:true,expression:"cancel(twice(x))".into(),definitions:"#let twice(x) = $ #x + #x $".into()}).unwrap();
    assert!(result["svg"].as_str().unwrap().contains("<svg"));
}

#[test]
#[ignore = "requires a locally installed Tinymist executable"]
fn macro_calls_and_scoped_raw_compile_with_native_typst() {
    let service = Services::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let definitions = "#let ratio(x, y) = $frac(#x, #x + #y)$\n#let marked(x) = $cancel(#x)$";
    for expression in ["ratio(a, b)", "ratio(\"\", \"\")", "marked(c)", "ratio(marked(a), b)"] {
        let result = service.render(RenderRequest { display:true, expression:expression.into(), definitions:definitions.into() }).unwrap();
        assert!(result["svg"].as_str().unwrap().contains("<svg"));
    }
}
