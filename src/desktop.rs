//! Native desktop protocol. One document and one reusable formula session.
use crate::document::Document;
use typformula_core::typst;
use serde_json::{Value, json};
use typst_syntax::{SyntaxKind, SyntaxNode};
use std::io::{self, BufRead, Write};

pub fn analyze(document: &mut Document) -> Value {
    let (blocked,mut styles)=scan_syntax(document,true);
    let saved_editor=std::mem::take(&mut document.editor);
    let saved_active=document.active.take();
    let formulas:Vec<_>=document.equation_nodes().into_iter().map(|(equation,node)|project(document,equation,&node,&blocked)).collect();
    for formula in &formulas {if formula["editable"]==false {styles.push(json!({"kind":"formula_error","start":formula["start"],"end":formula["end"],"text":formula["reason"]}));}}
    document.editor=saved_editor; document.active=saved_active;
    // A font variant is applied by substituting codepoints, and the table that does it is
    // outside the kernel's reach. The **spellings to ask about** travel with the analysis,
    // so the window can ask the layout service once instead of the core asking the adapter
    // from inside its own request loop (the desktop protocol is one request at a time).
    let glyphs = style_expressions(&formulas);
    json!({"formulas":formulas,"styles":styles,"glyphs":glyphs})
}

/// Every `style` call spelling in the analysis, for the glyph lookup.
///
/// Deduplicated: two formulas that both say `bold(a)` need one answer, and nesting is
/// included because each layer is drawn on its own once the caret is inside it.
pub fn style_expressions(formulas: &[Value]) -> Vec<String> {
    fn walk(view: &Value, out: &mut Vec<String>) {
        if view["kind"] == "style" {
            if let Some(text) = view["text"].as_str() { out.push(text.to_string()); }
        }
        if let Some(children) = view["children"].as_array() { for child in children { walk(child, out); } }
    }
    let mut out = vec![];
    for formula in formulas {
        if formula["editable"] == false { continue; }
        walk(&formula["view"], &mut out);
    }
    out.sort(); out.dedup(); out
}

fn scan_syntax(document:&Document,classify:bool)->(Vec<(usize,usize,&'static str)>,Vec<Value>) {
    let source=&document.source;
    let mut blocked = vec![];let mut styles=vec![];
    fn walk(node: &SyntaxNode, at: usize, source: &str, classify:bool, blocked: &mut Vec<(usize,usize,&'static str)>, styles: &mut Vec<Value>) {
        let end = at + node.len();
        let kind = node.kind();
        if kind == SyntaxKind::LetBinding {
            styles.push(json!({"kind":"let","start":at,"end":end}));
            if classify {
                // **Every** `#let` body is source, whatever it contains — a `$$` written
                // inside one is never a formula box.
                //
                // An unexpandable definition is arbitrary Typst, so a `$$` in it is not a
                // formula this editor can project. An expandable one is the stronger
                // reason: its body is a **template**, parsed with holes (`Kind::Parameter`)
                // and template edges, and that tree is not the tree a document formula is
                // written back from. Projecting it as a formula box put template-only
                // nodes one step away from the document, which is exactly what
                // `docs/editing-model.md` §6 exists to prevent.
                //
                // Nothing is lost by it: a fragment inside a body is rendered where the
                // macro is **called**, and the call site's own view carries it together
                // with the range in the definition it came from (`view::Projector` writes
                // `definitions`/`origin`/`source_range` for exactly that).
                let expandable = typst::macro_registry(&source[..end]).entries.iter().any(|d| d.definition_start == at && d.expandable);
                let reason = if expandable { "位于 let 定义体中，按设计保留源码模式（定义体是宏模板）" }
                             else { "位于不可展开的 let 定义中，按设计保留源码模式" };
                blocked.push((at,end,reason));
            }
        }
        let style = match kind { SyntaxKind::Strong => Some("strong"), SyntaxKind::Emph => Some("emph"), SyntaxKind::Heading => Some("heading"), SyntaxKind::LineComment | SyntaxKind::BlockComment => Some("comment"), _ => None };
        if let Some(style) = style { styles.push(json!({"kind":style,"start":at,"end":end,"text":node.full_text()})); }
        let mut pos=at;
        for child in node.children() { walk(child,pos,source,classify,blocked,styles); pos+=child.len(); }
    }
    walk(document.syntax().root(),0,source,classify,&mut blocked,&mut styles);
    (blocked,styles)
}

fn project(document:&mut Document,equation:crate::document::Equation,node:&SyntaxNode,blocked:&[(usize,usize,&'static str)])->Value {
    let covering=blocked.iter().find(|&&(a,b,_)|a<=equation.start && equation.end<=b);
    let mut item=json!({"start":equation.start,"end":equation.end,"display":equation.display,"editable":covering.is_none()});
    if let Some((_,_,reason))=covering {item["reason"]=json!(reason);}
    else {match document.activate_equation(equation.clone(),node) {
        Ok(())=>{let response=document.response();item["view"]=response["view"].clone();item["render"]=response["render"].clone();document.editor=Default::default();document.active=None;},
        Err(error)=>item["reason"]=json!(format!("结构公式转换失败：{error}")),
    }}
    if item.get("view").is_none(){item["editable"]=json!(false);}item
}

pub fn analyze_formula(document:&mut Document,start:usize)->Option<Value> {
    let (equation,node)=document.equation_nodes().into_iter().find(|(equation,_)|equation.start==start)?;
    /// The reason this formula cannot be projected, or `None` when it can.
    fn opaque(node:&SyntaxNode,at:usize,target:usize,source:&str)->Option<&'static str> {
        if target<at||target>=at+node.len(){return None;}
        if node.kind()==SyntaxKind::LetBinding {
            // Same rule as `scan_syntax`: a `#let` body is source, expandable or not.
            let expandable=typst::macro_registry(&source[..at+node.len()]).entries.iter().any(|d|d.definition_start==at&&d.expandable);
            return Some(if expandable { "位于 let 定义体中，按设计保留源码模式（定义体是宏模板）" }
                        else { "位于不可展开的 let 定义中，按设计保留源码模式" });
        }
        let mut pos=at;
        for child in node.children(){if let Some(reason)=opaque(child,pos,target,source){return Some(reason);}pos+=child.len();}
        None
    }
    let blocked=match opaque(document.syntax().root(),0,start,&document.source){Some(reason)=>vec![(start,equation.end,reason)],None=>vec![]};
    let saved_editor=std::mem::take(&mut document.editor);let saved_active=document.active.take();
    let item=project(document,equation,&node,&blocked);
    document.editor=saved_editor;document.active=saved_active;Some(item)
}

pub fn scan(document:&Document)->Value {
    let (_,styles)=scan_syntax(document,false);
    let formulas=document.equations().into_iter().map(|equation|{
        json!({"start":equation.start,"end":equation.end,"display":equation.display})
    }).collect::<Vec<_>>();
    json!({"formulas":formulas,"styles":styles})
}

pub fn serve() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut document=Document::default();
    let stdin=io::stdin(); let mut output=io::stdout().lock();
    for line in stdin.lock().lines() {
        let line=line?;
        if line.len()>8*1024*1024 { return Err("Desktop request too large".into()); }
        let reply=(|| -> Result<Value,String> {
            let request:Value=serde_json::from_str(&line).map_err(|e|e.to_string())?;
            if request["action"]=="analyze" { return Ok(analyze(&mut document)); }
            if request["action"]=="analyze_formula" { return Ok(request["start"].as_u64().and_then(|start|analyze_formula(&mut document,start as usize)).unwrap_or(Value::Null)); }
            if request["action"]=="scan" { return Ok(scan(&document)); }
            document.apply(request)?;
            Ok(document.response())
        })();
        let reply=match reply { Ok(result)=>json!({"result":result}), Err(error)=>json!({"error":error}) };
        writeln!(output,"{reply}")?;output.flush()?;
    }
    Ok(())
}
