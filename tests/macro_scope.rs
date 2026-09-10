use visual_typst_core::{typst, prewarm, document::Document};
use serde_json::json;

#[test]
fn lexical_blocks_shadow_without_leaking_and_definitions_keep_old_bindings() {
    let source = "#let amount = $1$\n#let saved(x) = $amount + #x$\n#[\n#let amount = $2$\n$ saved(z) + amount $\n]\n$ amount $";
    let inside = source.find("$ saved").unwrap();
    let inner = typst::macro_registry(&source[..inside]);
    assert_eq!(typst::write_cell(&inner.get("amount").unwrap().template), "2");
    let outer = typst::macro_registry(&source[..source.rfind("$ a").unwrap()]);
    assert_eq!(typst::write_cell(&outer.get("amount").unwrap().template), "1");
    let mut doc = Document::default();
    doc.apply(json!({"action":"set_source","source":source})).unwrap();
    doc.apply(json!({"action":"activate_formula","start":inside})).unwrap();
    let view = doc.response()["view"].to_string();
    assert!(view.contains("\"text\":\"1\""),"{view}");assert!(view.contains("\"text\":\"2\""),"{view}");
    assert_eq!(doc.source(),source);
}

#[test]
fn function_parameters_and_later_bindings_do_not_resolve_outer_macros() {
    let prefix = "#let a = $1$\n#let f(a) = ";
    assert!(typst::macro_registry(prefix).get("a").is_none());
    assert!(typst::macro_registry("#let a = $1$\n#{ let a = 3; ").get("a").is_some_and(|d|!d.expandable));
    assert_eq!(typst::write_cell(&typst::macro_registry("#let a = $1$\n#{ let a = 3; }\n").get("a").unwrap().template),"1");
}

#[test]
fn warmups_pass_explicit_empty_strings_and_never_edit_source() {
    let source = "#let opaque(x) = x + 1\n#let f(x, y) = $cancel(a) + #x/#y$\n#[#let g(z) = $#z + cancel(b)$\n]";
    let jobs = prewarm::plans(source);
    assert_eq!(jobs.len(),2);
    assert!(jobs[0].request["source"].as_str().unwrap().contains("f(\"\", \"\")"),"{}", jobs[0].request);
    assert!(!jobs[0].request["raw"].as_array().unwrap().is_empty(),"fixed Raw nodes have source mappings: {}", jobs[0].request);
    assert!(jobs[1].request["source"].as_str().unwrap().ends_with(']'));
    assert!(!source.contains("f(\"\""));
    let key = jobs[0].key.clone();
    typst::set_warmup_results(&[(key.clone(),true)]);
    assert!(!typst::macro_registry(&key).get("f").unwrap().expandable);
    typst::set_warmup_results(&[(key.clone(),false)]);
    assert!(typst::macro_registry(&key).get("f").unwrap().expandable);
}

#[test]
fn warmup_failure_reclassifies_active_call_without_source_or_history_edits() {
    let source="#let fixed(x) = $#x + cancel(a)$\n$ fixed(z) $";
    let mut doc=Document::default();doc.apply(json!({"action":"set_source","source":source})).unwrap();
    doc.apply(json!({"action":"activate_formula","start":source.rfind("$ fixed").unwrap()})).unwrap();
    let before=doc.response();let key=prewarm::plans(source)[0].key.clone();
    doc.apply(json!({"action":"macro_warmup","results":[[key,true]]})).unwrap();
    let after=doc.response();assert_eq!(after["source"],before["source"]);assert_eq!(after["revision"],before["revision"]);assert_eq!(after["undo"],before["undo"]);
    assert_eq!(after["view"]["children"][1]["kind"],"raw");
    typst::set_warmup_results(&[(key,false)]);
}

#[test]
fn templates_only_map_their_own_raw_ranges_and_keep_duplicate_occurrences() {
    let source="#[#let unused = $cancel(a)$]\n#let fixed(x) = $#x + cancel(a) + b^(cancel(a))$\n$ fixed(z) $";
    let plans=prewarm::plans(source);let plan=plans.iter().find(|p|p.key.contains("#let fixed")).unwrap();
    let raw=plan.request["raw"].as_array().unwrap();assert_eq!(raw.len(),2);
    assert!(raw.iter().all(|r|r["start"].as_u64().unwrap() as usize>source.find("#let fixed").unwrap()));
    let mut doc=Document::default();doc.apply(json!({"action":"set_source","source":source})).unwrap();
    doc.apply(json!({"action":"activate_formula","start":source.rfind("$ fixed").unwrap()})).unwrap();
    let response=doc.response();assert_eq!(response["render"]["raw"].as_array().unwrap().len(),2);
}

#[test]
#[ignore = "requires the built native renderer"]
fn native_warmups_render_without_real_calls_and_isolate_failures() {
    let service=visual_typst_core::services::Services::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let source="#let fixed(x) = $#x + cancel(a)$\n#let broken(x) = $#x + nonexistent(1)$\n#[#set text(size: 23pt)\n#let local(x) = $#x + cancel(b)$\n]\n#{ let code(x) = $#x + cancel(c)$; }\n#let amount = 7\n#let hashed(x) = $#x + #amount$";
    let body=json!({"path":"main.typ","source":source});
    let result=service.prewarm(body.clone()).unwrap();
    let items=result["results"].as_array().unwrap();assert_eq!(items.len(),5,"{result}");
    assert_eq!(items[0]["failed"],false,"{result}");
    assert!(!items[0]["items"].as_array().unwrap().is_empty(),"{result}");
    assert_eq!(items[0]["items"][0]["template_source"],"cancel(a)");
    assert_eq!(items[1]["failed"],true,"{result}");
    assert_eq!(items[2]["failed"],false,"{result}");
    assert_eq!(items[2]["items"][0]["environment_font_size_pt"],23.0,"{result}");
    assert_eq!(items[3]["failed"],false,"{result}");
    assert!(!items[3]["items"].as_array().unwrap().is_empty(),"{result}");
    assert_eq!(items[4]["failed"],false,"{result}");assert_eq!(items[4]["items"][0]["template_source"],"#amount","{result}");
    assert_eq!(service.prewarm(body).unwrap(),result,"cached response is stable");
}
