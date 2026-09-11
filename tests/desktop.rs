use serde_json::json;
use visual_typst::{desktop::analyze, document::Document};

#[test]
fn native_projection_keeps_opaque_definitions_in_source_and_preserves_active_session() {
    let source="#let opaque(x) = $lr(#x, size: #100%)$\n#let expanded(x) = $#x + lr(a, size: #100%)$\n中文 $expanded(y)$";
    let mut document=Document::default();
    document.apply(json!({"action":"set_source","source":source})).unwrap();
    let start=source.rfind('$').unwrap();
    let start=source[..start].rfind('$').unwrap();
    document.apply(json!({"action":"activate_formula","start":start})).unwrap();
    document.apply(json!({"action":"input","text":"z"})).unwrap();
    let before=document.response();let projection=analyze(&mut document);let after=document.response();
    assert_eq!(before,after,"projection must preserve source, cursor, revision and active session");
    let formulas=projection["formulas"].as_array().unwrap();
    assert_eq!(formulas.len(),3);
    assert_eq!(formulas[0]["editable"],false);
    assert_eq!(formulas[1]["editable"],true);
    assert!(formulas[2]["view"].is_object());
}

#[test]
fn native_analysis_excludes_dollar_strings_and_marks_source_styles() {
    let source="= 标题\n*bold* _italic_ `not $math$`\n#let value = \"$text$\"\n$ a/b $";
    let mut document=Document::default();document.apply(json!({"action":"set_source","source":source})).unwrap();
    let projection=analyze(&mut document);
    assert_eq!(projection["formulas"].as_array().unwrap().len(),1);
    for role in ["heading","strong","emph","let"] { assert!(projection["styles"].as_array().unwrap().iter().any(|v|v["kind"]==role)); }
    assert_eq!(document.source,source);
}

#[test]
fn source_edits_use_typsts_incremental_reparse_and_keep_distant_equations() {
    let source=(0..80).map(|i|format!("paragraph {i}\n\n$x_{i}$\n\n")).collect::<String>();
    let mut document=Document::default();document.apply(json!({"action":"set_source","source":source})).unwrap();
    let before=document.equations();
    document.apply(json!({"action":"edit_source","start":5,"end":5,"text":" updated"})).unwrap();
    assert!(document.last_reparsed.end-document.last_reparsed.start<document.source.len()/4,"{:?}",document.last_reparsed);
    let after=document.equations();assert_eq!(after.len(),before.len());
    assert_eq!(after.last().unwrap().start,before.last().unwrap().start+8);
    assert_eq!(document.syntax().text(),document.source);
}

#[test]
fn valid_unstructured_math_falls_back_to_raw_while_opaque_let_is_explained() {
    let source="$x' + arrow.r + cases(1, 2) + f(a: 1)$\n#let hidden = [inside $x+y$]";
    let mut document=Document::default();document.apply(json!({"action":"set_source","source":source})).unwrap();
    let analysis=analyze(&mut document);let formulas=analysis["formulas"].as_array().unwrap();
    assert_eq!(formulas.len(),2);assert_eq!(formulas[0]["editable"],true);
    assert_eq!(formulas[1]["editable"],false);assert!(formulas[1]["reason"].as_str().unwrap().contains("不可展开"));
}
