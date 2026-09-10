//! Native desktop protocol. One document and one reusable formula session.
use crate::{document::Document, typst};
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
    json!({"formulas":formulas,"styles":styles})
}

fn scan_syntax(document:&Document,classify:bool)->(Vec<(usize,usize)>,Vec<Value>) {
    let source=&document.source;
    let mut blocked = vec![];let mut styles=vec![];
    fn walk(node: &SyntaxNode, at: usize, source: &str, classify:bool, blocked: &mut Vec<(usize,usize)>, styles: &mut Vec<Value>) {
        let end = at + node.len();
        let kind = node.kind();
        if kind == SyntaxKind::LetBinding {
            styles.push(json!({"kind":"let","start":at,"end":end}));
            if classify {
                let registry = typst::macro_registry(&source[..end]);
                if !registry.entries.iter().any(|d| d.definition_start == at && d.expandable) { blocked.push((at,end)); }
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

fn project(document:&mut Document,equation:crate::document::Equation,node:&SyntaxNode,blocked:&[(usize,usize)])->Value {
    let editable=!blocked.iter().any(|&(a,b)|a<=equation.start && equation.end<=b);
    let mut item=json!({"start":equation.start,"end":equation.end,"display":equation.display,"editable":editable});
    if !editable {item["reason"]=json!("位于不可展开的 let 定义中，按设计保留源码模式");}
    else {match document.activate_equation(equation.clone(),node) {
        Ok(())=>{let response=document.response();item["view"]=response["view"].clone();item["render"]=response["render"].clone();document.editor=Default::default();document.active=None;},
        Err(error)=>item["reason"]=json!(format!("结构公式转换失败：{error}")),
    }}
    if item.get("view").is_none(){item["editable"]=json!(false);}item
}

pub fn analyze_formula(document:&mut Document,start:usize)->Option<Value> {
    let (equation,node)=document.equation_nodes().into_iter().find(|(equation,_)|equation.start==start)?;
    fn opaque(node:&SyntaxNode,at:usize,target:usize,source:&str)->bool {
        if target<at||target>=at+node.len(){return false;}
        if node.kind()==SyntaxKind::LetBinding {
            let registry=typst::macro_registry(&source[..at+node.len()]);
            if !registry.entries.iter().any(|d|d.definition_start==at&&d.expandable){return true;}
        }
        let mut pos=at;
        for child in node.children(){if opaque(child,pos,target,source){return true;}pos+=child.len();}
        false
    }
    let blocked=if opaque(document.syntax().root(),0,start,&document.source){vec![(start,equation.end)]}else{vec![]};
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
