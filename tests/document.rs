use typformula::document::Document;
use serde_json::json;
fn load(source:&str)->Document {let mut doc=Document::default();doc.apply(json!({"action":"set_source","source":source})).unwrap();doc}

fn nodes<'a>(v:&'a serde_json::Value,kind:&str,out:&mut Vec<&'a serde_json::Value>) {
    if v["kind"]==kind {out.push(v);}
    if let Some(children)=v["children"].as_array(){for child in children {nodes(child,kind,out);}}
}

#[test]
fn bound_calls_keep_source_locations_and_distinct_invocations() {
    let source="#let wrap(x) = $bold(#x/2)$\n$wrap(a) + wrap(b)$";
    let mut doc=load(source);let at=doc.equations().last().unwrap().start;
    doc.apply(json!({"action":"activate_formula","start":at})).unwrap();
    let state=doc.response();let mut calls=vec![];nodes(&state["view"],"raw_macro",&mut calls);
    assert_eq!(calls.len(),2);
    assert_eq!(calls[0]["text"],"bold(frac(a, 2))");
    assert_eq!(calls[1]["text"],"bold(frac(b, 2))");
    for (node,call) in calls.iter().zip(["wrap(a)","wrap(b)"]) {
        let request=&node["render_request"];
        let a=request["start"].as_u64().unwrap() as usize;let b=request["end"].as_u64().unwrap() as usize;
        assert_eq!(&source[a..b],"bold(#x/2)");
        let a=request["call"][0].as_u64().unwrap() as usize;let b=request["call"][1].as_u64().unwrap() as usize;
        assert_eq!(&source[a..b],call);
    }
    assert_ne!(calls[0]["render_request"]["id"],calls[1]["render_request"]["id"]);
    assert_eq!(doc.source(),source);
}

#[test]
fn a_bound_style_preserves_argument_cursor_and_editing() {
    let source="#let styled(x) = $bold(upright(#x))$\n$styled(a)$";
    let mut doc=load(source);let at=doc.equations().last().unwrap().start;
    doc.apply(json!({"action":"activate_formula","start":at})).unwrap();
    doc.apply(json!({"action":"key","key":"ArrowRight"})).unwrap();
    let state=doc.response();let mut stops=vec![];nodes(&state["view"],"stop",&mut stops);
    assert!(stops.iter().any(|n|n["active"]==true && !n["cursor"]["slices"].as_array().unwrap().is_empty()));
    doc.apply(json!({"action":"input","text":"z"})).unwrap();
    assert!(doc.source().ends_with("$styled(z a)$"));
}

#[test]
fn bound_style_empty_arguments_keep_their_typst_spelling() {
    let mut doc=load("#let styled(long_name) = $bold(#long_name)$\n$styled(\"\")$");
    let at=doc.equations().last().unwrap().start;
    doc.apply(json!({"action":"activate_formula","start":at})).unwrap();
    let state=doc.response();let mut styles=vec![];nodes(&state["view"],"style",&mut styles);
    assert_eq!(styles[0]["text"],"bold(\"\")");
}

#[test]
fn entering_a_collapsed_call_requests_its_located_inner_fragments() {
    let mut doc=load("$bold(arrow.r)$");doc.apply(json!({"action":"activate_formula","start":0})).unwrap();
    let outside=doc.response();let mut inner=vec![];nodes(&outside["view"],"raw",&mut inner);
    assert!(inner[0]["render_request"].is_object());
    assert_eq!(outside["render"]["raw"].as_array().unwrap().len(),1);
    doc.apply(json!({"action":"key","key":"ArrowRight"})).unwrap();
    let inside=doc.response();let request=&inside["render"]["raw"][0];
    let a=request["start"].as_u64().unwrap() as usize;let b=request["end"].as_u64().unwrap() as usize;
    assert_eq!(&doc.source[a..b],"arrow.r");
}
#[test]
fn padded_matrix_cells_are_written_and_survive_reentry() {
    for (source, cell, expected) in [("$mat(a, b; c)$",3,"$mat(a, b; c, z)$"), ("$mat(a; b, c)$",1,"$mat(a, z; b, c)$")] {
        let mut doc=load(source);
        doc.apply(json!({"action":"activate_formula","start":0})).unwrap();
        assert_eq!(doc.source(),source,"activation does not rewrite source");
        doc.apply(json!({"action":"click","cursor":{"slices":[{"atom":0,"cell":cell-1}],"pos":1}})).unwrap();
        doc.apply(json!({"action":"key","key":"Tab"})).unwrap();
        assert_eq!(doc.editor.cursor.slices[0].cell,cell);
        doc.apply(json!({"action":"input","text":"z"})).unwrap();
        assert_eq!(doc.source(),expected);
        doc.apply(json!({"action":"deactivate_formula"})).unwrap();
        doc.apply(json!({"action":"activate_formula","start":0})).unwrap();
        assert_eq!(typformula_core::typst::write_cell(&doc.editor.root[0].cells[cell]),"z");
    }
}
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
    let mut doc=load("中文😀 $lr(x, size: #100%)$ 后文");
    doc.apply(json!({"action":"activate_formula","start":"中文😀 ".len()})).unwrap();
    let reply=doc.response();
    for range in reply["render"]["raw"].as_array().unwrap() {
        assert_eq!(&doc.source[range["start"].as_u64().unwrap() as usize..range["end"].as_u64().unwrap() as usize],"lr(x, size: #100%)");
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
    let mut doc=load("$ undefinedname $");
    doc.apply(json!({"action":"activate_formula","start":0})).unwrap();
    // The desktop reports the fragments of one render pass as a set.
    doc.apply(json!({"action":"preview_results","sources":["undefinedname"],"definitions":doc.editor.definitions.clone(),"display":doc.editor.display,"failed":true})).unwrap();
    // Entering is the only repair path left for a fragment with no image at all.
    doc.apply(json!({"action":"key","key":"ArrowRight"})).unwrap();
    assert_eq!(doc.editor.pending(),Some("undefinedname"));
    doc.apply(json!({"action":"key","key":"a","ctrl":true})).unwrap();
    doc.apply(json!({"action":"input","text":"undef(β)"})).unwrap();
    assert_eq!(doc.source(),"$ undefinedname $","a draft stays inside the session");
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
    assert!(matches!(doc.editor.root[0].kind,typformula_core::math::Kind::Raw{..}),"{:?}",doc.editor.root[0].kind);
    assert_eq!(response["view"]["children"][1]["kind"],json!("raw"));
    assert_eq!(doc.source(),"#let twice(a, b) = $ #a + #b $\n$ twice(1, 2) $","the document itself is untouched");
}
