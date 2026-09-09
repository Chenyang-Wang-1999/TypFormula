use visual_typst_core::document::Document;
use serde_json::json;
fn load(source:&str)->Document {let mut doc=Document::default();doc.apply(json!({"action":"set_source","source":source})).unwrap();doc}
#[test]
fn normal_source_is_lossless_even_when_incomplete() {
    for text in ["", "中文😀\r\n#let x = (", "// $not$\n`$raw$`\n$x$", "$ a/b $\n$x$"] {
        let mut doc=load(text);assert_eq!(doc.source(),text);assert!(doc.active.is_none());
        for eq in doc.equations() {
            doc.apply(json!({"action":"activate_formula","start":eq.start})).unwrap();
            doc.apply(json!({"action":"key","key":"ArrowRight"})).unwrap();
            doc.apply(json!({"action":"deactivate_formula"})).unwrap();
            assert_eq!(doc.source(),text,"entering a formula must not normalize its spelling");
        }
    }
}
#[test]
fn syntax_scanner_ignores_dollars_in_comments_and_strings() {
    let doc=load("// $comment$\n#let s = \"$string$\"\n`$raw$`\n中文 $x$ 和 $ y $");
    let eq=doc.equations();assert_eq!(eq.len(),2);assert!(!eq[0].display);assert!(eq[1].display);
}
#[test]
fn only_active_range_changes_and_session_is_reused() {
    let prefix="中文😀 #let f(x) = x\n";
    let mut doc=load(&format!("{prefix}$a/b$ 后文 $y$"));
    doc.apply(json!({"action":"activate_formula","start":prefix.len()})).unwrap();
    doc.apply(json!({"action":"input","text":"x"})).unwrap();
    assert!(doc.source().starts_with(prefix));assert!(doc.source().ends_with(" 后文 $y$"));
    let second=doc.equations()[1].start;
    doc.apply(json!({"action":"activate_formula","start":second})).unwrap();
    assert_eq!(doc.editor.cursor.pos,0);assert_eq!(doc.response()["blocks"].as_array().unwrap().len(),1);
    doc.apply(json!({"action":"input","text":"z"})).unwrap();
    assert!(doc.source().ends_with("$z y$"));
    let source=doc.source();doc.apply(json!({"action":"set_source","source":source})).unwrap();
    assert!(doc.active.is_none());assert!(doc.editor.root.is_empty());
}
#[test]
fn pending_command_blocks_session_switch() {
    let mut doc=load("$x$ $y$");doc.apply(json!({"action":"activate_formula","start":0})).unwrap();
    doc.apply(json!({"action":"input","text":"\\"})).unwrap();
    assert!(doc.apply(json!({"action":"deactivate_formula"})).is_err());
    doc.apply(json!({"action":"key","key":"Escape"})).unwrap();
    assert!(doc.apply(json!({"action":"deactivate_formula"})).is_ok());
}
#[test]
fn raw_mapping_is_in_full_document_utf8_coordinates() {
    let mut doc=load("中文😀 $cancel(x)$ 后文");
    doc.apply(json!({"action":"activate_formula","start":"中文😀 ".len()})).unwrap();
    let reply=doc.response();
    for range in reply["render"]["raw"].as_array().unwrap() {
        assert_eq!(&doc.source[range["start"].as_u64().unwrap() as usize..range["end"].as_u64().unwrap() as usize],"cancel(x)");
    }
}

