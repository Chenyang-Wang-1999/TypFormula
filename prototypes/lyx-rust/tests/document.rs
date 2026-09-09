use lyx_typst_core::document::Document;
use serde_json::json;

#[test]
fn explicit_insertions_split_unicode_context_and_keep_source_equations_opaque() {
    let mut doc=Document::default();
    doc.apply(json!({"action":"set_context","index":0,"text":"前文 $x$ 后文"})).unwrap();
    assert_eq!(doc.formulas.len(),1);
    doc.apply(json!({"action":"insert_formula","index":0,"offset":"前文 ".len(),"display":false})).unwrap();
    doc.apply(json!({"action":"input","text":"x"})).unwrap();
    assert_eq!(doc.contexts[1],"$x$ 后文");
    assert_eq!(doc.formulas.len(),2);
    assert!(doc.source().starts_with("前文 $x$$x$ 后文"));
    doc.apply(json!({"action":"insert_formula","index":2,"offset":0,"display":true})).unwrap();
    assert_eq!(doc.formulas.len(),3);
    doc.apply(json!({"action":"undo"})).unwrap();
    assert_eq!(doc.formulas.len(),2);
    doc.apply(json!({"action":"redo"})).unwrap();
    assert!(doc.formulas[2].display);
}

#[test]
fn formulas_keep_independent_cursors_and_document_undo() {
    let mut doc=Document::default();
    doc.apply(json!({"action":"input","text":"ab"})).unwrap();
    doc.apply(json!({"action":"insert_formula","index":1,"offset":0,"display":false})).unwrap();
    doc.apply(json!({"action":"input","text":"xy"})).unwrap();
    doc.apply(json!({"action":"activate_formula","index":0})).unwrap();
    assert_eq!(doc.formulas[0].cursor.pos,2);
    doc.apply(json!({"action":"undo"})).unwrap();
    assert_eq!(doc.active,1); assert!(doc.formulas[1].root.is_empty());
    doc.apply(json!({"action":"remove_formula"})).unwrap();
    assert_eq!(doc.formulas.len(),1);
    doc.apply(json!({"action":"remove_formula"})).unwrap();
    assert_eq!(doc.formulas.len(),0);
    assert_eq!(doc.response()["contexts"].as_array().unwrap().len(),1);
}

#[test]
fn lexical_macros_and_raw_ranges_include_full_document_and_unicode() {
    let mut doc=Document::default();
    doc.apply(json!({"action":"import","source":"#let ratio(x) = $frac(#x, 2)$\n前文 $ratio(a) + cancel(x)$ 中间\n$ ratio(b) + cancel(y) $\n结尾"})).unwrap();
    assert_eq!(doc.formulas.len(),2);
    let response=doc.response();
    for raw in response["render"]["raw"].as_array().unwrap() {
        let start=raw["start"].as_u64().unwrap() as usize; let end=raw["end"].as_u64().unwrap() as usize;
        assert!(response["source"].as_str().unwrap()[start..end].starts_with("cancel("));
    }
    assert_eq!(response["render"]["raw"].as_array().unwrap().len(),2);
    doc.apply(json!({"action":"set_definitions","definitions":"#let ratio(x) = $sqrt(#x)$"})).unwrap();
    assert!(doc.response()["blocks"][1]["view"].to_string().contains("sqrt"));
    doc.apply(json!({"action":"undo"})).unwrap();
    assert!(doc.response()["blocks"][1]["view"].to_string().contains("fraction"));
    doc.apply(json!({"action":"activate_formula","index":1})).unwrap();
    doc.apply(json!({"action":"input","text":"\\alp"})).unwrap();
    let response=doc.response(); let command=&response["command"];
    assert!(command["source"].as_str().unwrap().ends_with("结尾"));
    let start=command["start"].as_u64().unwrap() as usize;
    let end=command["end"].as_u64().unwrap() as usize;
    assert_eq!(&command["source"].as_str().unwrap()[start..end],"alp");
}

#[test]
fn repeated_raw_arguments_and_definition_raw_have_stable_source_ranges() {
    let mut doc=Document::default();
    doc.apply(json!({"action":"import","source":"#let ratio(x) = $frac(#x, #x) + cancel(y)$\n$ ratio(cancel(z)) $"})).unwrap();
    let response=doc.response();
    assert_eq!(response["render"]["raw"].as_array().unwrap().len(),2,"{response}");
    assert!(response["view"].to_string().contains("render_id"));
}
