use serde_json::json;
use typformula::{desktop::analyze, document::Document};

#[test]
fn native_projection_keeps_every_let_body_in_source_and_preserves_active_session() {
    // A `#let` body is source whether or not the definition expands, and the formula
    // **after** it is unaffected: an expandable body is a macro *template*, parsed with
    // holes, and that tree is not one a document formula may be projected from.
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
    assert_eq!(formulas[1]["editable"],false,"let 定义体一律保留源码，可展的也不例外");
    assert!(formulas[1]["reason"].as_str().unwrap().contains("宏模板"),"{:?}",formulas[1]["reason"]);
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

/// 每一个 `#let` 体里的 `$$` 都保留源码，**可展的也不例外**。
///
/// 可展定义的体是一个**宏模板**（`Kind::Parameter`/`TemplateCall` 的树，不是文档能写回的
/// 树），不可展定义体是任意 Typst —— 两者都不该在正文里变成公式框。定义之后的公式不受
/// 影响，那一条是本用例的另一半。
#[test]
fn every_dollar_inside_a_let_body_stays_source() {
    for source in [
        "#let dbl(x) = $#x + 1$\n$dbl(y)$",
        "#let title = [\n  $ x + 1 $\n]\n$title$ + $z$",
        "#let calc(a) = {\n  let t = $ a $\n  t\n}\n$calc(1) + $w$",
    ] {
        let mut document=Document::default();
        document.apply(json!({"action":"set_source","source":source})).unwrap();
        let analysis=analyze(&mut document);
        let lets:Vec<(usize,usize)>=analysis["styles"].as_array().unwrap().iter()
            .filter(|v|v["kind"]=="let")
            .map(|v|(v["start"].as_u64().unwrap() as usize,v["end"].as_u64().unwrap() as usize)).collect();
        assert!(!lets.is_empty(),"{source} 里应当有 let");
        let mut outside=0;
        for formula in analysis["formulas"].as_array().unwrap() {
            let start=formula["start"].as_u64().unwrap() as usize;
            let end=formula["end"].as_u64().unwrap() as usize;
            let inside=lets.iter().any(|&(a,b)|a<=start&&end<=b);
            assert_eq!(formula["editable"],json!(!inside),"{source}\n{start}..{end} 的可编辑性不对：{formula}");
            if inside {
                assert!(formula["view"].is_null(),"let 定义体不该有视图：{formula}");
                assert!(formula["reason"].as_str().unwrap().contains("let"),"{formula}");
            } else { outside+=1; }
        }
        assert!(outside>0,"{source}：定义之外的公式必须仍然是公式框");
    }
}

#[test]
fn valid_unstructured_math_falls_back_to_raw_while_opaque_let_is_explained() {
    let source="$x' + arrow.r + cases(1, 2) + f(a: 1)$\n#let hidden = [inside $x+y$]";
    let mut document=Document::default();document.apply(json!({"action":"set_source","source":source})).unwrap();
    let analysis=analyze(&mut document);let formulas=analysis["formulas"].as_array().unwrap();
    assert_eq!(formulas.len(),2);assert_eq!(formulas[0]["editable"],true);
    assert_eq!(formulas[1]["editable"],false);assert!(formulas[1]["reason"].as_str().unwrap().contains("不可展开"));
}
