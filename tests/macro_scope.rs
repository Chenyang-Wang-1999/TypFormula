use typformula_core::typst;
use typformula_core::view::ViewTemplate;
use typformula::{document::Document, services::{RawRange, RenderRequest, Services}};
use serde_json::json;

/// One template's material, read as a line a person can check.
///
/// This is a **test's eye**, not a spelling. A spelling is `write_atom`'s job and
/// happens on the editable atoms; what a definition stores is the display tree its
/// body projects to (`docs/editing-model.md` §9), so a test that wants to say "this
/// definition is `1`" has to read that tree. Leaves contribute their text, a hole
/// contributes `#` and an edge its definition index, so the two editor-only nodes are
/// visible to an assertion rather than silently skipped.
fn material(template: &ViewTemplate) -> String {
    match template {
        ViewTemplate::Hole { .. } => "#".into(),
        ViewTemplate::Edge { definition, .. } => format!("<{definition}>"),
        ViewTemplate::Node(node) if node.children.is_empty() => node.text.clone(),
        ViewTemplate::Node(node) => node.children.iter().map(material).collect(),
    }
}

#[test]
fn lexical_blocks_shadow_without_leaking_and_definitions_keep_old_bindings() {
    let source = "#let amount = $1$\n#let saved(x) = $amount + #x$\n#[\n#let amount = $2$\n$ saved(z) + amount $\n]\n$ amount $";
    let inside = source.find("$ saved").unwrap();
    let inner = typst::macro_registry(&source[..inside]);
    assert_eq!(material(&inner.get("amount").unwrap().template), "2");
    let outer = typst::macro_registry(&source[..source.rfind("$ a").unwrap()]);
    assert_eq!(material(&outer.get("amount").unwrap().template), "1");
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
    assert_eq!(material(&typst::macro_registry("#let a = $1$\n#{ let a = 3; }\n").get("a").unwrap().template),"1");
}

#[test]
fn expandability_is_decided_by_structure_alone() {
    // A template that no render result can affect: the editor can show every
    // parameter in a slot, which is the whole condition.
    let source = "#let opaque(x) = x + 1\n#let f(x, y) = $lr(a, size: #100%) + #x/#y$\n#[#let g(z) = $#z + lr(b, size: #100%)$\n]";
    let registry = typst::macro_registry(source);
    assert!(registry.get("f").unwrap().expandable, "{}", registry.get("f").unwrap().reason);
    // A binding inside a completed content block is out of scope at the end of the
    // prefix, so it is not a definition of this prefix at all.
    assert!(registry.get("g").is_none());
    // A definition whose body is not a formula, or that hides a parameter, stays
    // source-mode regardless of what its fragments would render as.
    assert!(!typst::macro_registry("#let opaque(x) = x + 1").get("opaque").unwrap().expandable);
    assert!(!typst::macro_registry("#let hidden(x) = $lr(a, size: #100%)$").get("hidden").unwrap().expandable);
}

#[test]
fn a_template_fragment_is_asked_for_at_its_own_place_in_the_definition() {
    let source = "#let f(x, y) = $lr(a, size: #100%) + #x/#y$";
    let definition = typst::macro_registry(source).get("f").cloned().unwrap();
    let ranges = typst::definition_raw_ranges(&definition, "lr(a, size: #100%)");
    assert_eq!(ranges.len(),1,"{ranges:?}");
    assert_eq!(&source[ranges[0].0..ranges[0].1],"lr(a, size: #100%)");
    // The prefix the range is measured in is the document prefix, so the offset is
    // the document's own — nothing here is a private copy of the definition.
    assert_eq!(typst::definition_prefix(&definition),source);
}

#[test]
fn every_definition_of_a_reduced_prefix_keeps_the_document_prefix_that_ends_at_it() {
    // Prose between the definitions is what the reduction drops, and dropping it is exactly
    // what moves the second definition's offsets: `retarget` has to put them back into the
    // document's coordinates. One definition cannot show this, because the prefix that ends
    // at the first one starts at the document's start and so is the same in both texts.
    //
    // The three arguments mirror `document::activate_equation`: `macro_text` is the reduced
    // prefix (short, and unchanged by an edit to the prose), `prefix` is the document's own
    // prefix (where fragments are located), and `context` maps between them.
    let source = "#let first(x) = $#x + 1$\n\n一些正文，归约时会整段丢掉。\n\n#let second(y) = $#y + lr(a, size: #100%)$\n\n$ second(z) $";
    let at = source.rfind("$ second").unwrap();
    let context = typformula_core::context::context(&source, &[(at, source.len() - 1)]).unwrap();
    let (moved_start, _) = context.ranges[0];
    let macro_text = context.prefix(moved_start);
    let prefix = &source[..at];
    assert!(macro_text.len() < prefix.len(), "the reduction dropped nothing, so this proves nothing");
    let registry = typst::macro_registry_of(macro_text, prefix, Some(&context));

    // The prefix a fragment's offset is measured in must be the document's own text ending
    // at that definition -- a wrong end offset shows up here as "the prefix is not the
    // source" rather than as a slice out of bounds.
    let first_end = source.find('\n').unwrap();
    let second_end = source.find("#let second").unwrap() + "#let second(y) = $#y + lr(a, size: #100%)$".len();
    assert_eq!(typst::definition_prefix(registry.get("first").unwrap()), source[..first_end],
        "the first definition's prefix is not the document's");
    assert_eq!(typst::definition_prefix(registry.get("second").unwrap()), source[..second_end],
        "the second definition's prefix is not the document's -- the reduction moved it");

    // A fragment inside the second definition is located past the prose the reduction threw
    // away, so its offset is a document offset and the text there is the fragment itself.
    let ranges = typst::definition_raw_ranges(registry.get("second").unwrap(), "lr(a, size: #100%)");
    assert_eq!(ranges.len(), 1, "{ranges:?}");
    assert_eq!(&source[ranges[0].0..ranges[0].1], "lr(a, size: #100%)");

    // `definition_raw_ranges` keeps only fragments at or after `definition_start`, so an
    // understated start lets the earlier definition claim its neighbour's fragments.
    let first = registry.get("first").unwrap();
    assert!(typst::definition_raw_ranges(first, "lr(a, size: #100%)").is_empty(),
        "the first definition claims a fragment that belongs to the second");
    assert_eq!(typst::definition_raw_ranges(first, "#x + 1").len(), 1,
        "the first definition lost its own fragment");
}

#[test]
fn a_call_site_fragment_carries_the_definition_range_it_is_rendered_from() {
    let source="#let fixed(x) = $#x + lr(a, size: #100%)$\n$ fixed(z) $";
    let mut doc=Document::default();doc.apply(json!({"action":"set_source","source":source})).unwrap();
    doc.apply(json!({"action":"activate_formula","start":source.rfind("$ fixed").unwrap()})).unwrap();
    let response=doc.response();
    let definition=source.find("lr(a, size: #100%)").unwrap();
    let raw=response["render"]["raw"].as_array().unwrap().clone();
    assert_eq!(raw.len(),1,"{response}");
    assert_eq!(raw[0]["start"].as_u64().unwrap() as usize,definition);
    assert_eq!(raw[0]["end"].as_u64().unwrap() as usize,definition+"lr(a, size: #100%)".len());
    let view=response["view"].to_string();
    assert!(view.contains(&format!("\"source_range\":[{definition},")),"{view}");
    assert_eq!(response["source"],source,"asking for an image never edits the document");
}

#[test]
fn duplicate_fragments_in_one_definition_keep_distinct_ranges() {
    let source="#[#let unused = $lr(a, size: #100%)$]\n#let fixed(x) = $#x + lr(a, size: #100%) + b^(lr(a, size: #100%))$\n$ fixed(z) $";
    let definition=typst::macro_registry(source).get("fixed").cloned().unwrap();
    let ranges=typst::definition_raw_ranges(&definition,"lr(a, size: #100%)");
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
    let source="#let fixed(x) = $#x + lr(a, size: #100%)$\n$ fixed(z) $";
    let definition=typst::macro_registry(source).get("fixed").cloned().unwrap();
    let (start,end)=typst::definition_raw_ranges(&definition,"lr(a, size: #100%)")[0];
    let request=RenderRequest{preview:false,pdf:false,overlays:Default::default(),path:"main.typ".into(),
        source:source.into(),raw:vec![RawRange{id:format!("{start}:{end}"),start,end,call:None,occurrence:0}],formulas:vec![],preview_hashes:vec![],context_end:None};
    let result=service.render(request).unwrap();
    let items=result["items"].as_array().unwrap();
    assert_eq!(items.len(),1,"the call site is what compiles the fragment: {result}");
    assert_eq!(items[0]["start"].as_u64().unwrap() as usize,start);
    assert!(items[0]["svg"].as_str().unwrap().contains("<svg"));
}
