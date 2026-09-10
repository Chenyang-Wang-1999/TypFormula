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
fn fragments(doc:&mut Document)->Vec<(String,String)> {
    fn walk(view:&serde_json::Value,out:&mut Vec<(String,String)>) {
        if view["kind"]==json!("raw") {
            out.push((view["text"].as_str().unwrap_or_default().to_owned(),view["render_id"].as_str().unwrap_or_default().to_owned()));
        }
        for child in view["children"].as_array().into_iter().flatten() { walk(child,out); }
    }
    let mut out=vec![];let view=doc.response()["view"].clone();walk(&view,&mut out);out
}
fn ranged(doc:&mut Document)->Vec<String> {
    let reply=doc.response();let source=doc.source();
    reply["render"]["raw"].as_array().unwrap().iter()
        .map(|r|source[r["start"].as_u64().unwrap() as usize..r["end"].as_u64().unwrap() as usize].to_owned()).collect()
}
#[test]
fn raw_ranges_do_not_depend_on_the_canonical_spelling() {
    // The writer emits `sum_(n = 0)` with spaces; the author wrote `n=0`. Requiring
    // the two to be equal left every fragment of such a formula without a range,
    // so the editor could never ask Typst for its image.
    let mut doc=load("正文 $ sum_(n=0)^oo a_n $ 尾部");
    doc.apply(json!({"action":"activate_formula","start":"正文 ".len()})).unwrap();
    assert_eq!(ranged(&mut doc),vec!["sum","oo"]);
    // Every Raw node of the editable tree carries the range it was given.
    for (text,id) in fragments(&mut doc) {
        let (start,end)=(id.split(':').next().unwrap().parse::<usize>().unwrap(),id.split(':').nth(1).unwrap().parse::<usize>().unwrap());
        assert_eq!(&doc.source[start..end],text.as_str());
    }
}
#[test]
fn repeated_fragments_of_one_formula_take_their_own_occurrences() {
    let mut doc=load("$ partial f + partial g + dif x dif y $");
    doc.apply(json!({"action":"activate_formula","start":0})).unwrap();
    let ranges=ranged(&mut doc);
    assert_eq!(ranges,vec!["partial","partial","dif","dif"]);
    let starts:Vec<u64>=doc.response()["render"]["raw"].as_array().unwrap().iter().map(|r|r["start"].as_u64().unwrap()).collect();
    let mut sorted=starts.clone();sorted.sort();sorted.dedup();
    assert_eq!(sorted.len(),starts.len(),"each occurrence needs its own range");
}
#[test]
fn repairing_a_fragment_writes_the_edited_source_back() {
    let mut doc=load("$ undefinedfunc(α) $");
    doc.apply(json!({"action":"activate_formula","start":0})).unwrap();
    // The desktop reports the fragments of one render pass as a set.
    doc.apply(json!({"action":"preview_results","sources":["undefinedfunc(α)"],"definitions":doc.editor.definitions.clone(),"display":doc.editor.display,"failed":true})).unwrap();
    // Entering is the only repair path left for a fragment with no image at all.
    doc.apply(json!({"action":"key","key":"ArrowRight"})).unwrap();
    assert_eq!(doc.editor.pending(),Some("undefinedfunc(α)"));
    doc.apply(json!({"action":"key","key":"a","ctrl":true})).unwrap();
    doc.apply(json!({"action":"input","text":"undef(β)"})).unwrap();
    assert_eq!(doc.source(),"$ undefinedfunc(α) $","a draft stays inside the session");
    doc.apply(json!({"action":"key","key":"Enter"})).unwrap();
    assert_eq!(doc.source(),"$ undef(β) $");
    let start=doc.source.find("undef(β)").unwrap();
    assert_eq!(doc.response()["render"]["raw"][0]["start"].as_u64().unwrap() as usize,start);
}
#[test]
fn a_command_draft_never_reaches_the_authoritative_source() {
    let mut doc=load("$a$");
    doc.apply(json!({"action":"activate_formula","start":0})).unwrap();
    doc.apply(json!({"action":"key","key":"End"})).unwrap();
    let committed=doc.source();
    // Every draft keystroke is transient: the half-typed command is not Typst
    // and must not become a document edit or an undo step.
    for text in ["\\", "f", "r", "a", "c", "(", "a", ",", " ", "b", ")"] {
        doc.apply(json!({"action":"input","text":text})).unwrap();
        assert_eq!(doc.source(),committed,"{text:?} must stay inside the session");
        assert_eq!(doc.response()["source"],json!(committed));
    }
    assert_eq!(doc.editor.pending(),Some("frac(a, b)"));
    doc.apply(json!({"action":"key","key":"Enter"})).unwrap();
    assert_eq!(doc.source(),"$a frac(a, b)$","confirming commits exactly once");
    assert_eq!(doc.response()["blocks"][0]["source"],json!("$a frac(a, b)$"));
    // Escape restores the previous spelling, so nothing is left to undo.
    doc.apply(json!({"action":"input","text":"\\alpha"})).unwrap();
    assert_eq!(doc.source(),"$a frac(a, b)$");
    doc.apply(json!({"action":"key","key":"Escape"})).unwrap();
    assert_eq!(doc.source(),"$a frac(a, b)$");
    assert_eq!(doc.response()["blocks"][0]["source"],json!("$a frac(a, b)$"));
}
#[test]
fn a_definition_confirmed_inside_a_formula_reclassifies_the_whole_cell() {
    // Redefining an already expanded macro with a different arity used to leave
    // the old call in the tree and panic while projecting it.
    let mut doc=load("#let twice(a, b) = $ #a + #b $\n$ twice(1, 2) $");
    let start=doc.equations().last().unwrap().start;
    doc.apply(json!({"action":"activate_formula","start":start})).unwrap();
    assert_eq!(doc.response()["view"]["children"][1]["kind"],json!("macro"));
    let draft="#let twice(x) = $ #x + #x $";
    for ch in format!("\\{draft}").chars() { doc.apply(json!({"action":"input","text":ch.to_string()})).unwrap(); }
    assert_eq!(doc.editor.pending(),Some(draft));
    doc.apply(json!({"action":"key","key":"Enter"})).unwrap();
    let response=doc.response();
    // The cell is re-derived against the new definition instead of keeping a
    // two-argument call bound to a one-argument macro.
    assert!(matches!(doc.editor.root[0].kind,visual_typst_core::math::Kind::Raw{..}),"{:?}",doc.editor.root[0].kind);
    assert_eq!(response["view"]["children"][1]["kind"],json!("raw"));
    assert_eq!(doc.source(),"#let twice(a, b) = $ #a + #b $\n$ twice(1, 2) $","the document itself is untouched");
}

