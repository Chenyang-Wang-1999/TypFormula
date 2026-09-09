// SPDX-License-Identifier: GPL-2.0-or-later
//! Source is authoritative. Exactly one reusable structural editing session.
use crate::{Action, Editor, math::*, typst};
use serde_json::{Value, json};
use std::collections::HashMap;
use typst_syntax::{Source, SyntaxKind, SyntaxNode};

#[derive(Clone, Debug)]
pub struct Equation { pub start: usize, pub end: usize, pub display: bool }
#[derive(Default)]
pub struct Document {
    pub source: String,
    pub editor: Editor,
    pub active: Option<Equation>,
    revision: u64,
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
impl Document {
    pub fn source(&self) -> String { self.source.clone() }
    pub fn equations(&self) -> Vec<Equation> {
        let mut out = vec![];
        equations(Source::detached(&self.source).root(), 0, &mut out); out
    }
    pub fn apply(&mut self, value: Value) -> Result<(), String> {
        let name = value["action"].as_str().ok_or("缺少操作")?;
        match name {
            "set_source" => {
                self.source = value["source"].as_str().ok_or("缺少源码")?.into();
                self.active = None; self.editor = Editor::default(); self.revision += 1;
            }
            "activate_formula" => {
                if self.editor.pending().is_some() { return Err("请先确认或取消命令草稿".into()); }
                let start = value["start"].as_u64().ok_or("缺少公式位置")? as usize;
                let range = self.equations().into_iter().find(|e| e.start == start).ok_or("公式区间已变化")?;
                let definitions = &self.source[..range.start];
                let parsed = typst::parse_formula(&self.source[range.start..range.end], definitions)?;
                self.editor = Editor::default(); self.editor.root = parsed.root;
                self.editor.definitions = definitions.into(); self.editor.display = range.display;
                self.active = Some(range);
            }
            "deactivate_formula" => {
                if self.editor.pending().is_some() { return Err("请先确认或取消命令草稿".into()); }
                self.active = None; self.editor = Editor::default();
            }
            "state" => {},
            _ => {
                let range = self.active.as_mut().ok_or("请先进入公式")?;
                let before = typst::write_document_mode(&self.editor.root, "", self.editor.display);
                self.editor.apply(serde_json::from_value::<Action>(value).map_err(|e| e.to_string())?)?;
                let after = typst::write_document_mode(&self.editor.root, "", self.editor.display);
                if before != after {
                    self.source.replace_range(range.start..range.end, &after);
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
        response["active_formula"] = json!(0);
        response["active_range"] = self.active.as_ref().map_or(Value::Null, |r| json!({"start":r.start,"end":r.end,"display":r.display}));
        response["equations"] = json!(self.equations().iter().map(|e|json!({"start":e.start,"end":e.end,"display":e.display})).collect::<Vec<_>>());
        response["formula_definitions"] = json!(self.editor.definitions); response["blocks"] = json!(blocks);
        response["render"] = json!({"source":self.source,"raw":raw,"formulas":formulas}); response
    }
}
fn syntax_ranges(node: &SyntaxNode, offset: usize, text: &str, out: &mut Vec<(usize,usize)>) {
    if node.full_text().as_str() == text { out.push((offset, offset+node.len())); return; }
    let mut pos = offset;
    for child in node.children() { syntax_ranges(child,pos,text,out); pos += child.len(); }
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
        } else {
            let context = view["definitions"].as_str().unwrap_or_default(); let origin = view["origin"].as_str().unwrap_or_default();
            let start = context.len(); let mut ranges = vec![];
            if source.get(start..start+origin.len()) == Some(origin) { syntax_ranges(Source::detached(origin).root(),start,&text,&mut ranges); }
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
