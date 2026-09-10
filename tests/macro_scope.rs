use visual_typst_core::{typst, document::Document, services::{RawRange, RenderRequest, Services}};
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
fn expandability_is_decided_by_structure_alone() {
    // A template that no render result can affect: the editor can show every
    // parameter in a slot, which is the whole condition.
    let source = "#let opaque(x) = x + 1\n#let f(x, y) = $cancel(a) + #x/#y$\n#[#let g(z) = $#z + cancel(b)$\n]";
    let registry = typst::macro_registry(source);
    assert!(registry.get("f").unwrap().expandable, "{}", registry.get("f").unwrap().reason);
    // A binding inside a completed content block is out of scope at the end of the
    // prefix, so it is not a definition of this prefix at all.
    assert!(registry.get("g").is_none());
    // A definition whose body is not a formula, or that hides a parameter, stays
    // source-mode regardless of what its fragments would render as.
    assert!(!typst::macro_registry("#let opaque(x) = x + 1").get("opaque").unwrap().expandable);
    assert!(!typst::macro_registry("#let hidden(x) = $cancel(a)$").get("hidden").unwrap().expandable);
}

#[test]
fn a_template_fragment_is_asked_for_at_its_own_place_in_the_definition() {
    let source = "#let f(x, y) = $cancel(a) + #x/#y$";
    let definition = typst::macro_registry(source).get("f").cloned().unwrap();
    let ranges = typst::definition_raw_ranges(&definition, "cancel(a)");
    assert_eq!(ranges.len(),1,"{ranges:?}");
    assert_eq!(&source[ranges[0].0..ranges[0].1],"cancel(a)");
    // The prefix the range is measured in is the document prefix, so the offset is
    // the document's own — nothing here is a private copy of the definition.
    assert_eq!(typst::definition_prefix(&definition),source);
}

#[test]
fn a_call_site_fragment_carries_the_definition_range_it_is_rendered_from() {
    let source="#let fixed(x) = $#x + cancel(a)$\n$ fixed(z) $";
    let mut doc=Document::default();doc.apply(json!({"action":"set_source","source":source})).unwrap();
    doc.apply(json!({"action":"activate_formula","start":source.rfind("$ fixed").unwrap()})).unwrap();
    let response=doc.response();
    let definition=source.find("cancel(a)").unwrap();
    let raw=response["render"]["raw"].as_array().unwrap().clone();
    assert_eq!(raw.len(),1,"{response}");
    assert_eq!(raw[0]["start"].as_u64().unwrap() as usize,definition);
    assert_eq!(raw[0]["end"].as_u64().unwrap() as usize,definition+"cancel(a)".len());
    let view=response["view"].to_string();
    assert!(view.contains(&format!("\"source_range\":[{definition},")),"{view}");
    assert_eq!(response["source"],source,"asking for an image never edits the document");
}

#[test]
fn duplicate_fragments_in_one_definition_keep_distinct_ranges() {
    let source="#[#let unused = $cancel(a)$]\n#let fixed(x) = $#x + cancel(a) + b^(cancel(a))$\n$ fixed(z) $";
    let definition=typst::macro_registry(source).get("fixed").cloned().unwrap();
    let ranges=typst::definition_raw_ranges(&definition,"cancel(a)");
    assert_eq!(ranges.len(),2,"{ranges:?}");
    assert_ne!(ranges[0],ranges[1]);
    let owner=source.find("#let fixed").unwrap();
    assert!(ranges.iter().all(|(start,_)| *start>owner),"a fragment of another definition is not this one's: {ranges:?}");
    let mut doc=Document::default();doc.apply(json!({"action":"set_source","source":source})).unwrap();
    doc.apply(json!({"action":"activate_formula","start":source.rfind("$ fixed").unwrap()})).unwrap();
    let raw=doc.response()["render"]["raw"].as_array().unwrap().clone();
    assert_eq!(raw.len(),2,"{raw:?}");
    assert_eq!(raw[0]["start"].as_u64().unwrap() as usize,ranges[0].0);
    assert_eq!(raw[1]["start"].as_u64().unwrap() as usize,ranges[1].0);
}

#[test]
#[ignore = "requires the built native renderer"]
fn a_call_site_fragment_renders_from_the_document_its_own_call() {
    let service=Services::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let source="#let fixed(x) = $#x + cancel(a)$\n$ fixed(z) $";
    let definition=typst::macro_registry(source).get("fixed").cloned().unwrap();
    let (start,end)=typst::definition_raw_ranges(&definition,"cancel(a)")[0];
    let request=RenderRequest{preview:false,pdf:false,overlays:Default::default(),path:"main.typ".into(),
        source:source.into(),raw:vec![RawRange{id:format!("{start}:{end}"),start,end}],formulas:vec![],preview_hashes:vec![],context_end:None};
    let result=service.render(request).unwrap();
    let items=result["items"].as_array().unwrap();
    assert_eq!(items.len(),1,"the call site is what compiles the fragment: {result}");
    assert_eq!(items[0]["start"].as_u64().unwrap() as usize,start);
    assert!(items[0]["svg"].as_str().unwrap().contains("<svg"));
}
