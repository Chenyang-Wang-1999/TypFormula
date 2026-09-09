// SPDX-License-Identifier: GPL-2.0-or-later
//! Document composition around the existing formula editor. Context stays opaque.
use crate::{Action, Editor, cursor::Snapshot, math::*, typst};
use serde_json::{Value, json};
use typst_syntax::{Source, SyntaxKind, SyntaxNode};
use std::collections::HashMap;

struct Saved { formulas: Vec<Snapshot>, contexts: Vec<String>, definitions: String, active: usize }
pub struct Document {
    pub formulas: Vec<Editor>,
    pub contexts: Vec<String>,
    pub definitions: String,
    pub active: usize,
    history: Vec<Saved>, future: Vec<Saved>, typing: Option<String>, revision: u64,
}
impl Default for Document {
    fn default() -> Self { Self { formulas: vec![Editor::default()], contexts: vec![String::new(),String::new()], definitions: String::new(), active: 0, history: vec![], future: vec![], typing: None, revision: 0 } }
}
impl Document {
    fn save(&self) -> Saved { Saved { formulas: self.formulas.iter().map(Editor::snapshot).collect(), contexts: self.contexts.clone(), definitions: self.definitions.clone(), active: self.active } }
    fn restore(&mut self, saved: Saved) {
        self.formulas = saved.formulas.into_iter().map(|s| { let mut e = Editor::default(); e.restore(s); e }).collect();
        self.contexts = saved.contexts; self.definitions = saved.definitions; self.active = saved.active;
    }
    fn prefix(&self) -> String {
        let mut prefix = self.definitions.clone();
        if !prefix.is_empty() && !prefix.ends_with('\n') { prefix.push('\n'); }
        prefix
    }
    pub fn source(&self) -> String {
        let mut source = self.prefix();
        for (i, editor) in self.formulas.iter().enumerate() {
            source.push_str(&self.contexts[i]);
            source.push_str(&typst::write_document_mode(&editor.root, "", editor.display));
        }
        source.push_str(self.contexts.last().unwrap()); source
    }
    // Prefixes include preceding prose, code and equations. Only Typst evaluates
    // them; the macro analyzer reads top-level let bindings for structural views.
    fn sync_contexts(&mut self, reparse: bool) -> Result<(), String> {
        let mut prefix = self.prefix();
        for (i, editor) in self.formulas.iter_mut().enumerate() {
            prefix.push_str(&self.contexts[i]);
            let equation = typst::write_document_mode(&editor.root, "", editor.display);
            if reparse && editor.definitions != prefix {
                if editor.pending().is_some() { return Err("请先确认或取消公式命令草稿。".into()); }
                editor.root = typst::parse_formula(&equation, &prefix)?.root;
                editor.cursor = Cursor::default(); editor.anchor = None;
            }
            editor.definitions = prefix.clone();
            prefix.push_str(&equation);
        }
        Ok(())
    }
    pub fn apply(&mut self, value: Value) -> Result<(), String> {
        let name = value["action"].as_str().ok_or("缺少操作")?;
        if name == "activate_formula" {
            let index = value["index"].as_u64().ok_or("缺少公式位置")? as usize;
            if index >= self.formulas.len() { return Err("公式不存在".into()); }
            if self.formulas.get(self.active).is_some_and(|e| e.pending().is_some()) && self.active != index { return Err("请先确认或取消命令草稿。".into()); }
            self.active = index; self.typing = None; return Ok(());
        }
        if matches!(name, "state" | "geometry" | "lsp_completions" | "preview_result") {
            let index = if name == "preview_result" { value["formula"].as_u64().map(|v|v as usize).unwrap_or(self.active) } else { self.active };
            if let Some(editor) = self.formulas.get_mut(index) { editor.apply(serde_json::from_value(value).map_err(|e| e.to_string())?)?; }
            return Ok(());
        }
        let undo = name == "undo" || (name == "key" && value["ctrl"] == true && value["key"].as_str().is_some_and(|k| k.eq_ignore_ascii_case("z")));
        let redo = name == "redo" || (name == "key" && value["ctrl"] == true && value["key"].as_str().is_some_and(|k| k.eq_ignore_ascii_case("y")));
        if undo || redo {
            let saved = if redo { self.future.pop() } else { self.history.pop() };
            if let Some(saved) = saved { let current = self.save(); if redo { self.history.push(current); } else { self.future.push(current); } self.restore(saved); }
            self.typing = None; self.revision += 1; return Ok(());
        }
        let before = self.save(); let source_before = self.source();
        let typing = match name {
            "set_context" => Some(format!("context:{}", value["index"])),
            "input" => Some(format!("formula:{}",self.active)), _ => None,
        };
        let result = (|| -> Result<(), String> {
            match name {
                "set_context" => {
                    let index = value["index"].as_u64().ok_or("缺少正文位置")? as usize;
                    *self.contexts.get_mut(index).ok_or("正文不存在")? = value["text"].as_str().ok_or("缺少正文")?.into();
                    self.sync_contexts(true)?;
                }
                "insert_formula" => {
                    let index = value["index"].as_u64().ok_or("缺少插入位置")? as usize;
                    let offset = value["offset"].as_u64().unwrap_or(0) as usize;
                    let text = self.contexts.get_mut(index).ok_or("正文不存在")?;
                    if offset > text.len() || !text.is_char_boundary(offset) { return Err("插入位置无效".into()); }
                    let tail = text.split_off(offset);
                    let display = value["display"].as_bool().ok_or("缺少公式类型")?;
                    if display { if !text.ends_with('\n') { text.push('\n'); } }
                    self.contexts.insert(index+1, if display && !tail.starts_with('\n') { format!("\n{tail}") } else { tail });
                    let mut editor = Editor::default(); editor.display = display;
                    self.formulas.insert(index, editor); self.active = index; self.sync_contexts(true)?;
                }
                "remove_formula" => {
                    if self.active < self.formulas.len() {
                        self.formulas.remove(self.active);
                        let tail = self.contexts.remove(self.active+1); self.contexts[self.active].push_str(&tail);
                        self.active = self.active.min(self.formulas.len().saturating_sub(1)); self.sync_contexts(true)?;
                    }
                }
                "set_definitions" => {
                    let definitions = value["definitions"].as_str().ok_or("缺少定义")?;
                    typst::validate_definitions(definitions)?; self.definitions = definitions.into(); self.sync_contexts(true)?;
                }
                "import" => {
                    // Explicit import converts top-level equations. Typing/pasting
                    // into a context textarea never invokes this parser.
                    let text = value["source"].as_str().ok_or("缺少源码")?;
                    let source = Source::detached(text);
                    if !source.root().children().any(|n| n.kind() == SyntaxKind::Equation) {
                        let parsed = typst::parse_document(text)?;
                        self.formulas = vec![Editor::default()]; self.formulas[0].root = parsed.root; self.formulas[0].display = parsed.display;
                        self.definitions = parsed.definitions; self.contexts = vec![String::new(),String::new()];
                    } else {
                        if let Some(error) = source.root().errors_and_warnings().0.first() { return Err(error.message.to_string()); }
                        self.formulas.clear(); self.contexts = vec![String::new()]; self.definitions.clear();
                        let mut offset = 0; let mut prefix = true;
                        for node in source.root().children() {
                            let part = &text[offset..offset+node.len()]; offset += node.len();
                            if node.kind() == SyntaxKind::Equation {
                                let parsed = typst::parse_formula(part, &text[..offset-node.len()])?;
                                let mut editor = Editor::default(); editor.root = parsed.root; editor.display = parsed.display;
                                self.formulas.push(editor); self.contexts.push(String::new()); prefix = false;
                            } else if prefix && matches!(node.kind(), SyntaxKind::Hash | SyntaxKind::LetBinding | SyntaxKind::Space | SyntaxKind::Parbreak | SyntaxKind::LineComment | SyntaxKind::BlockComment) {
                                self.definitions.push_str(part);
                            } else { prefix = false; self.contexts.last_mut().unwrap().push_str(part); }
                        }
                        if self.definitions.trim().is_empty() { self.definitions.clear(); }
                    }
                    self.active = 0; self.sync_contexts(false)?;
                }
                _ => {
                    let action: Action = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                    let editor = self.formulas.get_mut(self.active).ok_or("请先插入公式")?;
                    editor.apply(action)?;
                    // Embedded #let commands keep their existing behavior.
                    self.sync_contexts(false)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result { self.restore(before); return Err(error); }
        if self.source() != source_before {
            if typing.is_none() || typing != self.typing { self.history.push(before); }
            if self.history.len() > 200 { self.history.remove(0); }
            self.future.clear();
        }
        self.typing = typing; self.revision += 1; Ok(())
    }
    pub fn response(&mut self) -> Value {
        let source = self.source();
        let mut raw = vec![]; let mut blocks = vec![]; let mut formulas = vec![];
        let mut offset = self.prefix().len(); let mut active = None;
        for (index, editor) in self.formulas.iter_mut().enumerate() {
            offset += self.contexts[index].len();
            let equation = typst::write_document_mode(&editor.root, "", editor.display);
            let mut response = serde_json::to_value(editor.response()).unwrap();
            annotate(&mut response["view"], &editor.root, &source, offset, &equation, &mut raw, &mut HashMap::new(), index);
            formulas.push(json!({"id":index.to_string(),"start":offset,"end":offset+equation.len()}));
            if let Some(command) = response["command"].as_object_mut() {
                let suffix_start = offset + equation.len();
                if let Some(s) = command.get_mut("source") { if let Some(text) = s.as_str() { *s = json!(format!("{text}{}",&source[suffix_start..])); } }
            }
            blocks.push(json!({"index":index,"display":editor.display,"definitions":editor.definitions,"view":response["view"],"source":equation}));
            if index == self.active { active = Some(response); }
            offset += equation.len();
        }
        let mut response = active.unwrap_or_else(|| serde_json::to_value(Editor::default().response()).unwrap());
        response["source"] = json!(source); response["revision"] = json!(self.revision);
        response["definitions"] = json!(self.definitions); response["macros"] = json!(typst::macro_registry(&self.definitions).entries);
        response["formula_definitions"] = json!(self.formulas.get(self.active).map(|e| e.definitions.clone()).unwrap_or_default());
        response["undo"] = json!(!self.history.is_empty()); response["redo"] = json!(!self.future.is_empty());
        response["active_formula"] = json!(self.active); response["blocks"] = json!(blocks); response["contexts"] = json!(self.contexts);
        response["render"] = json!({"source":source,"raw":raw,"formulas":formulas}); response
    }
}

fn syntax_ranges(node: &SyntaxNode, offset: usize, text: &str, out: &mut Vec<(usize,usize)>) {
    if node.full_text().as_str() == text { out.push((offset, offset+node.len())); return; }
    let mut pos = offset;
    for child in node.children() { syntax_ranges(child,pos,text,out); pos += child.len(); }
}
fn annotate(view: &mut Value, root: &MathData, source: &str, offset: usize, equation: &str, raw: &mut Vec<Value>, counts: &mut HashMap<String,usize>, formula: usize) {
    if view["kind"] == "raw" {
        let text = view["text"].as_str().unwrap_or_default().to_owned();
        let range = if let Ok(cursor) = serde_json::from_value::<Cursor>(view["edit"].clone()) {
            let mut copy = root.clone();
            let mut marker = "visualtypstrangemarker".to_string(); while source.contains(&marker) { marker.push('x'); }
            cell_mut(&mut copy, &cursor.slices)[cursor.pos] = MathAtom::raw(&marker);
            let marked = typst::write_document_mode(&copy,"",equation.starts_with("$ "));
            marked.find(&marker).map(|start| (offset+start,offset+start+text.len()))
        } else {
            let context = view["definitions"].as_str().unwrap_or_default();
            let origin = view["origin"].as_str().unwrap_or_default();
            let start = context.len();
            let mut ranges = vec![];
            if source.get(start..start+origin.len()) == Some(origin) {
                syntax_ranges(Source::detached(origin).root(),start,&text,&mut ranges);
            }
            ranges.first().copied()
        };
        if let Some((start,end)) = range.filter(|(s,e)| source.get(*s..*e) == Some(text.as_str())) {
            let id = format!("{start}:{end}"); let occurrence = counts.entry(id.clone()).or_insert(0);
            view["render_id"] = json!(format!("{id}:{formula}:{}",*occurrence)); *occurrence += 1;
            if !raw.iter().any(|r| r["id"] == id) { raw.push(json!({"id":id,"start":start,"end":end})); }
        }
    }
    if let Some(children) = view["children"].as_array_mut() {
        for child in children { annotate(child,root,source,offset,equation,raw,counts,formula); }
    }
}
