// SPDX-License-Identifier: GPL-2.0-or-later
//! Source is authoritative. Exactly one reusable structural editing session.
use crate::{Action, Editor, math::*, typst};
use serde_json::{Value, json};
use std::{collections::HashMap,ops::Range};
use typst_syntax::{Source, SyntaxKind, SyntaxNode};

#[derive(Clone, Debug)]
pub struct Equation { pub start: usize, pub end: usize, pub display: bool }
pub struct Document {
    pub source: String,
    pub editor: Editor,
    pub active: Option<Equation>,
    syntax: Source,
    pub last_reparsed: Range<usize>,
    revision: u64,
}
impl Default for Document {
    fn default()->Self { Self {source:String::new(),editor:Editor::default(),active:None,syntax:Source::detached(""),last_reparsed:0..0,revision:0} }
}
fn equations(node: &SyntaxNode, start: usize, out: &mut Vec<Equation>) {
    if node.kind() == SyntaxKind::Equation {
        let text = node.full_text();
        if text.ends_with('$') && text.len() >= 2 {
            let body = &text[1..text.len()-1];
            out.push(Equation { start, end: start + node.len(), display: body.starts_with(char::is_whitespace) && body.ends_with(char::is_whitespace) });
        }
        return;
    }
    let mut offset = start;
    for child in node.children() { equations(child, offset, out); offset += child.len(); }
}
fn equations_with_nodes(node:&SyntaxNode,start:usize,out:&mut Vec<(Equation,SyntaxNode)>) {
    if node.kind()==SyntaxKind::Equation {
        let text=node.full_text();
        if text.ends_with('$')&&text.len()>=2 {
            let body=&text[1..text.len()-1];
            out.push((Equation{start,end:start+node.len(),display:body.starts_with(char::is_whitespace)&&body.ends_with(char::is_whitespace)},node.clone()));
        }
        return;
    }
    let mut offset=start;for child in node.children(){equations_with_nodes(child,offset,out);offset+=child.len();}
}
fn equation_at(node:&SyntaxNode,at:usize,target:usize)->Option<&SyntaxNode> {
    if node.kind()==SyntaxKind::Equation{return (at==target).then_some(node);}
    let mut pos=at;
    for child in node.children(){if pos<=target&&target<pos+child.len(){if let Some(found)=equation_at(child,pos,target){return Some(found);}}pos+=child.len();}
    None
}
impl Document {
    pub fn source(&self) -> String { self.source.clone() }
    pub fn syntax(&self)->&Source {&self.syntax}
    pub fn equations(&self) -> Vec<Equation> {
        let mut out = vec![];
        equations(self.syntax.root(), 0, &mut out); out
    }
    pub fn equation_nodes(&self)->Vec<(Equation,SyntaxNode)> {let mut out=vec![];equations_with_nodes(self.syntax.root(),0,&mut out);out}
    pub fn activate_equation(&mut self,range:Equation,node:&SyntaxNode)->Result<(),String> {
        let definitions=&self.source[..range.start];
        let parsed=typst::parse_formula_node(node,definitions)?;
        self.editor=Editor::default();self.editor.root=parsed.root;self.editor.definitions=definitions.into();self.editor.display=range.display;self.active=Some(range);Ok(())
    }
    pub fn apply(&mut self, value: Value) -> Result<(), String> {
        let name = value["action"].as_str().ok_or("缺少操作")?;
        match name {
            "set_source" => {
                if value["reset_warmups"].as_bool()==Some(true) { typst::clear_warmup_results(); }
                let source=value["source"].as_str().ok_or("缺少源码")?;
                self.last_reparsed=self.syntax.replace(source);self.source=source.into();
                self.active = None; self.editor = Editor::default(); self.revision += 1;
            }
            "edit_source" => {
                if self.editor.pending().is_some(){return Err("请先确认或取消公式命令草稿".into());}
                let start=value["start"].as_u64().ok_or("缺少编辑起点")? as usize;
                let end=value["end"].as_u64().ok_or("缺少编辑终点")? as usize;
                let text=value["text"].as_str().ok_or("缺少替换源码")?;
                if start>end||end>self.source.len()||!self.source.is_char_boundary(start)||!self.source.is_char_boundary(end){return Err("源码编辑区间无效".into());}
                self.source.replace_range(start..end,text);self.last_reparsed=self.syntax.edit(start..end,text);
                self.active=None;self.editor=Editor::default();self.revision+=1;
            }
            "activate_formula" => {
                if self.editor.pending().is_some() { return Err("请先确认或取消命令草稿".into()); }
                let start = value["start"].as_u64().ok_or("缺少公式位置")? as usize;
                let (range,node)=self.equation_nodes().into_iter().find(|(e,_)|e.start==start).ok_or("公式区间已变化")?;
                self.activate_equation(range,&node)?;
            }
            "deactivate_formula" => {
                if self.editor.pending().is_some() { return Err("请先确认或取消命令草稿".into()); }
                self.active = None; self.editor = Editor::default();
            }
            "state" => {},
            "macro_warmup" => {
                if self.editor.pending().is_some() { return Err("命令草稿期间暂缓更新宏模板".into()); }
                let results: Vec<(String,bool)> = serde_json::from_value(value["results"].clone()).map_err(|e|e.to_string())?;
                let before:Vec<_>=typst::macro_registry(&self.editor.definitions).entries.iter().map(|d|d.expandable).collect();
                if value["clear"].as_bool()==Some(true) { typst::clear_warmup_results(); }
                typst::set_warmup_results(&results);
                let after:Vec<_>=typst::macro_registry(&self.editor.definitions).entries.iter().map(|d|d.expandable).collect();
                if let Some(range) = &self.active && before!=after {
                    let parsed = typst::parse_formula_node(equation_at(self.syntax.root(),0,range.start).ok_or("公式语法节点已变化")?,&self.editor.definitions)?;
                    self.editor.root = parsed.root;
                    self.editor.cursor = Cursor::default();
                    self.editor.anchor = None;self.editor.geometry.clear();
                }
            }
            _ => {
                let range = self.active.as_mut().ok_or("请先进入公式")?;
                let before = typst::write_document_mode(&self.editor.root, "", self.editor.display);
                self.editor.apply(serde_json::from_value::<Action>(value).map_err(|e| e.to_string())?)?;
                let after = typst::write_document_mode(&self.editor.root, "", self.editor.display);
                if before != after {
                    self.source.replace_range(range.start..range.end, &after);
                    self.last_reparsed=self.syntax.edit(range.start..range.end,&after);
                    range.end = range.start + after.len(); range.display = self.editor.display;
                    self.revision += 1;
                }
            }
        }
        Ok(())
    }
    pub fn response(&mut self) -> Value {
        let mut response = serde_json::to_value(self.editor.response()).unwrap();
        let mut raw = vec![]; let mut blocks = vec![]; let mut formulas = vec![];
        if let Some(range) = &self.active {
            let equation = &self.source[range.start..range.end];
            fn owner(node: &SyntaxNode, at: usize, target: usize, source: &str) -> Option<String> {
                if target < at || target >= at+node.len() { return None; }
                let mut found = (node.kind()==SyntaxKind::LetBinding).then(||source[..at+node.len()].to_owned());
                let mut pos = at;
                for child in node.children() { if let Some(key)=owner(child,pos,target,source) { found=Some(key); } pos+=child.len(); }
                found
            }
            fn warmup(view: &mut Value, key: &Option<String>, equation: &str, offset: usize, counts: &mut HashMap<String,usize>) {
                if view["kind"]=="raw" && view.get("warmup_key").is_none() { if let Some(key)=key {
                    view["warmup_key"]=json!(key);
                    let text=view["text"].as_str().unwrap_or_default();
                    let ordinal=counts.entry(text.to_owned()).or_default();
                    if let Some((a,b))=crate::prewarm::raw_ranges(equation,offset,text).get(*ordinal) { view["warmup_range"]=json!([a,b]); }
                    *ordinal+=1;
                } }
                if let Some(children)=view["children"].as_array_mut() { for child in children { warmup(child,key,equation,offset,counts); } }
            }
            warmup(&mut response["view"], &owner(self.syntax.root(),0,range.start,&self.source),equation,range.start,&mut HashMap::new());
            if equation == typst::write_document_mode(&self.editor.root, "", self.editor.display) {
                annotate(&mut response["view"], &self.editor.root, &self.source, range.start, equation, &mut raw, &mut HashMap::new());
            }
            formulas.push(json!({"id":"0","start":range.start,"end":range.end}));
            if let Some(command) = response["command"].as_object_mut() {
                if let Some(s) = command.get_mut("source") { if let Some(text) = s.as_str() { *s = json!(format!("{text}{}", &self.source[range.end..])); } }
            }
            blocks.push(json!({"index":0,"display":range.display,"definitions":self.editor.definitions,"view":response["view"],"source":equation}));
        }
        response["source"] = json!(self.source); response["revision"] = json!(self.revision);
        response["reparsed_range"]=json!({"start":self.last_reparsed.start,"end":self.last_reparsed.end});
        response["active_formula"] = json!(0);
        response["active_range"] = self.active.as_ref().map_or(Value::Null, |r| json!({"start":r.start,"end":r.end,"display":r.display}));
        response["equations"] = json!(self.equations().iter().map(|e|json!({"start":e.start,"end":e.end,"display":e.display})).collect::<Vec<_>>());
        response["formula_definitions"] = json!(self.editor.definitions); response["blocks"] = json!(blocks);
        response["render"] = json!({"source":self.source,"raw":raw,"formulas":formulas}); response
    }
}
fn annotate(view: &mut Value, root: &MathData, source: &str, offset: usize, equation: &str, raw: &mut Vec<Value>, counts: &mut HashMap<String,usize>) {
    if view["kind"] == "raw" {
        let text = view["text"].as_str().unwrap_or_default().to_owned();
        let range = if let Ok(cursor) = serde_json::from_value::<Cursor>(view["edit"].clone()) {
            let mut copy = root.clone();
            let mut marker = "visualtypstrangemarker".to_string(); while source.contains(&marker) { marker.push('x'); }
            cell_mut(&mut copy, &cursor.slices)[cursor.pos] = MathAtom::raw(&marker);
            let marked = typst::write_document_mode(&copy,"",equation.starts_with("$ "));
            marked.find(&marker).map(|start| (offset+start,offset+start+text.len()))
        } else if let Ok([start,end]) = serde_json::from_value::<[usize;2]>(view["warmup_range"].clone()) { Some((start,end)) } else {
            let context = view["definitions"].as_str().unwrap_or_default(); let origin = view["origin"].as_str().unwrap_or_default();
            let start = context.len(); let mut ranges = vec![];
            if source.get(start..start+origin.len()) == Some(origin) { ranges=crate::prewarm::raw_ranges(origin,start,&text); }
            ranges.first().copied()
        };
        if let Some((start,end)) = range.filter(|(s,e)| source.get(*s..*e) == Some(text.as_str())) {
            let id = format!("{start}:{end}"); let occurrence = counts.entry(id.clone()).or_insert(0);
            view["render_id"] = json!(format!("{id}:0:{}",*occurrence)); *occurrence += 1;
            if !raw.iter().any(|r| r["id"] == id) { raw.push(json!({"id":id,"start":start,"end":end})); }
        }
    }
    if let Some(children) = view["children"].as_array_mut() { for child in children { annotate(child,root,source,offset,equation,raw,counts); } }
}
