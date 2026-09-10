// SPDX-License-Identifier: GPL-2.0-or-later
#![cfg(not(target_arch = "wasm32"))]
use visual_typst_core::{Action, Editor, services::{Services, CompletionRequest, RenderRequest, RawRange}};

fn render(service: &Services, expression: &str, definitions: &str) -> Result<serde_json::Value,String> {
    let prefix=format!("{definitions}\n$ ");
    let start=prefix.len();
    let result=service.render(RenderRequest {preview:false,pdf:false,overlays:Default::default(),path:"main.typ".into(),source:format!("{prefix}{expression} $"),raw:vec![RawRange{id:"raw".into(),start,end:start+expression.len()}],formulas:vec![],preview_hashes:vec![],context_end:None})?;
    Ok(result["items"][0].clone())
}

#[test]
#[ignore = "requires local Tinymist"]
fn differential_symbol_keeps_its_name_and_renders() {
    let service=Services::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let mut editor=Editor::default();editor.apply(Action::Input{text:"\\dif".into()}).unwrap();
    let context=editor.command_context().unwrap();
    let reply=service.complete(CompletionRequest {source:context.source,start:context.start,end:context.end,caret:context.caret}).unwrap();
    editor.apply(Action::LspCompletions {draft:context.draft,caret:context.draft_caret,items:reply.items}).unwrap();
    editor.apply(Action::Key {key:"Enter".into(),shift:false,ctrl:false}).unwrap();
    let expression=visual_typst_core::typst::write_cell(&editor.root);
    assert!(matches!(&editor.root[0].kind,visual_typst_core::math::Kind::Raw {source} if source=="dif"));
    let output=render(&service, &expression, "").unwrap();
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
        let result=render(&service, expression, "").unwrap();
        assert!(result["svg"].as_str().unwrap().contains("<svg"));
    }
    assert!(render(&service, "not_a_defined_function(x)", "").is_err());
    let result = render(&service, "cancel(twice(x))", "#let twice(x) = $ #x + #x $").unwrap();
    assert!(result["svg"].as_str().unwrap().contains("<svg"));
}

#[test]
#[ignore = "requires a locally installed Tinymist executable"]
fn macro_calls_and_scoped_raw_compile_with_native_typst() {
    let service = Services::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let definitions = "#let ratio(x, y) = $frac(#x, #x + #y)$\n#let marked(x) = $cancel(#x)$";
    for expression in ["ratio(a, b)", "ratio(\"\", \"\")", "marked(c)", "ratio(marked(a), b)"] {
        let result = render(&service, expression, definitions).unwrap();
        assert!(result["svg"].as_str().unwrap().contains("<svg"));
    }
}

// The image request compiles only as far as the fragments reach, so a mistake in
// the rest of the document no longer leaves every fragment without an image.
#[test]
#[ignore = "requires a locally installed Tinymist executable"]
fn a_fragment_context_renders_past_a_later_document_error() {
    let service = Services::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let source = "#set text(size: 11pt)\n\n正文 $ sum_(n=1)^oo x^n $。\n\n#panic(\"坏了\")\n";
    // One fragment, exactly as the editor reports it, and the end of its formula.
    let start = source.find("sum").unwrap();
    let end = start + "sum".len();
    let context_end = source.find("^n $").unwrap() + 4;
    let raw = || vec![RawRange { id: format!("{start}:{end}"), start, end }];
    let request = |context_end: Option<usize>| RenderRequest {
        preview: false, pdf: false, overlays: Default::default(), path: "main.typ".into(),
        source: source.into(), raw: raw(), formulas: vec![], preview_hashes: vec![], context_end,
    };
    let whole = service.render(request(None));
    assert!(whole.is_err(),"the whole document must fail: {whole:?}");
    let cut = service.render(request(Some(context_end))).unwrap_or_else(|error| panic!("cut request failed: {error}"));
    assert_eq!(cut["items"].as_array().unwrap().len(), 1);
    assert!(cut["items"][0]["svg"].as_str().unwrap().contains("<svg"));
}


