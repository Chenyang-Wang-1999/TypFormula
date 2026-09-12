// SPDX-License-Identifier: GPL-2.0-or-later
use typformula_core::{Action, Editor};
use typformula::services::{Services, CompletionRequest, RenderRequest, RawRange};

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
    let expression=typformula_core::typst::write_cell(&editor.root);
    assert!(matches!(&editor.root[0].kind,typformula_core::math::Kind::Raw {source} if source=="dif"));
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

// The live preview is Tinymist's own feature, reached through the LSP session the
// language methods already use. What the desktop hangs off is the **reply's shape**, so
// that shape is what this pins: `staticServerPort` is where the window loads the preview
// page from, and `dataPlanePort` is the WebSocket that page connects back to.
//
// Measured against tinymist 0.15.8. The protocol is not public API — it lives in
// Tinymist's own VS Code client — so a version bump is what this test is here to catch.
#[test]
#[ignore = "requires a locally installed Tinymist executable"]
fn tinymist_serves_the_live_preview_on_the_ports_it_reports() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let service = Services::new(root.clone());
    assert_eq!(service.status()["available"], true);
    let start = service.preview(serde_json::json!({
        "path": "main.typ", "source": "= Live preview\n\nHello $x^2$.\n", "action": "start" }))
        .unwrap_or_else(|error| panic!("doStartPreview failed: {error}"));
    let static_port = start["staticServerPort"].as_u64().expect("reply must name the static server port");
    let data_port = start["dataPlanePort"].as_u64().expect("reply must name the data plane port");
    assert!(static_port > 0 && data_port > 0, "ports must be real: {start}");
    assert_eq!(start["isPrimary"], true, "the window's preview must be the primary one: {start}");
    // The page is served by the same port by default, which is why the window only needs
    // one URL: the app and the WebSocket it opens come from the same address.
    assert_eq!(static_port, data_port, "static and data plane share a port by default: {start}");

    // It really answers, and answers with the preview application rather than a stub.
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", static_port as u16)).expect("static server must accept a connection");
    std::io::Write::write_all(&mut stream, b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();
    let mut page = Vec::new();
    std::io::Read::read_to_end(&mut stream, &mut page).unwrap();
    let page = String::from_utf8_lossy(&page);
    assert!(page.starts_with("HTTP/1.1 200") || page.starts_with("HTTP/1.0 200"), "static server must answer 200: {}", &page[..page.len().min(200)]);
    assert!(page.contains("<html") && page.contains("WebSocket"), "the served page is the preview application");

    service.preview(serde_json::json!({ "path": "main.typ", "source": "", "action": "kill" }))
        .unwrap_or_else(|error| panic!("doKillPreview failed: {error}"));
    // The port must stop answering once the preview is killed, or a closed pane would
    // leave a server behind for the rest of the session.
    assert!(std::net::TcpStream::connect(("127.0.0.1", static_port as u16)).is_err(), "killed preview must stop listening");
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


