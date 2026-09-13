// SPDX-License-Identifier: GPL-2.0-or-later
//! Source is authoritative. Exactly one reusable structural editing session.
use typformula_core::{Action, Editor, math::*, typst};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};
use typst_syntax::{Source, SyntaxKind, SyntaxNode};

#[derive(Clone, Debug, PartialEq)]
pub struct Equation { pub start: usize, pub end: usize, pub display: bool }
pub struct Document {
    pub source: String,
    pub editor: Editor,
    pub active: Option<Equation>,
    syntax: Source,
    pub last_reparsed: Range<usize>,
    revision: u64,
    // The equation index is derived from the syntax tree, and both the desktop
    // load path and every response ask for it. Building it walks the whole
    // document, so it is kept until the tree itself changes.
    syntax_revision: u64,
    index: RefCell<Option<(u64, Rc<Vec<(Equation, SyntaxNode)>>)>>,
}
impl Default for Document {
    fn default()->Self { Self {source:String::new(),editor:Editor::default(),active:None,syntax:Source::detached(""),last_reparsed:0..0,revision:0,syntax_revision:0,index:RefCell::new(None)} }
}
// The production path builds the index once; this independent walk exists so a
// test can prove the cached index still matches the syntax tree.
#[cfg(test)]
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
#[cfg(test)]
fn equations_flat(document:&Document)->Vec<Equation> {
    let mut out = vec![]; equations(document.syntax.root(), 0, &mut out); out
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
impl Document {
    pub fn source(&self) -> String { self.source.clone() }
    pub fn syntax(&self)->&Source {&self.syntax}
    // Complete equation nodes, from the syntax tree the document already owns.
    // Reused until an edit replaces the tree, so callers may ask per formula.
    fn equation_index(&self)->Rc<Vec<(Equation,SyntaxNode)>> {
        if let Some((generation,index))=&*self.index.borrow() {
            if *generation==self.syntax_revision { return index.clone(); }
        }
        let mut out=vec![];
        equations_with_nodes(self.syntax.root(),0,&mut out);
        let index=Rc::new(out);
        *self.index.borrow_mut()=Some((self.syntax_revision,index.clone()));
        index
    }
    pub fn equations(&self) -> Vec<Equation> {
        self.equation_index().iter().map(|(equation,_)|equation.clone()).collect()
    }
    pub fn equation_nodes(&self)->Vec<(Equation,SyntaxNode)> { self.equation_index().as_ref().clone() }
    pub fn activate_equation(&mut self,range:Equation,node:&SyntaxNode)->Result<(),String> {
        // TODO: this prefix is the whole of what stands before the formula. Reducing it to the
        // part the formula can see (`typformula_core::context`) would make the macro registry's
        // cache survive prose edits, but the view hands `definitions` and the definition
        // offsets to the host, which maps them back into the document by assuming they are the
        // real prefix (`self.source[..range.start]`, see `Locator` in `document.rs`). The
        // reduction needs an offset mapping alongside it before that can change.
        let definitions=&self.source[..range.start];
        let parsed=typst::parse_formula_node(node,definitions)?;
        self.editor=Editor::default();self.editor.root=parsed.root;self.editor.definitions=definitions.into();self.editor.display=range.display;self.active=Some(range);Ok(())
    }
    pub fn apply(&mut self, value: Value) -> Result<(), String> {
        let name = value["action"].as_str().ok_or("缺少操作")?;
        match name {
            "set_source" => {
                let source=value["source"].as_str().ok_or("缺少源码")?;
                self.last_reparsed=self.syntax.replace(source);self.source=source.into();
                self.active = None; self.editor = Editor::default(); self.revision += 1;
                self.syntax_revision += 1;
            }
            "edit_source" => {
                if self.editor.pending().is_some(){return Err("请先确认或取消公式命令草稿".into());}
                let start=value["start"].as_u64().ok_or("缺少编辑起点")? as usize;
                let end=value["end"].as_u64().ok_or("缺少编辑终点")? as usize;
                let text=value["text"].as_str().ok_or("缺少替换源码")?;
                if start>end||end>self.source.len()||!self.source.is_char_boundary(start)||!self.source.is_char_boundary(end){return Err("源码编辑区间无效".into());}
                self.source.replace_range(start..end,text);self.last_reparsed=self.syntax.edit(start..end,text);
                self.active=None;self.editor=Editor::default();self.revision+=1;
                self.syntax_revision += 1;
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
            _ => {
                let range = self.active.clone().ok_or("请先进入公式")?;
                let committed = self.source.get(range.start..range.end).unwrap_or_default().to_string();
                // The trigger is a change of the tree, never of its serialization:
                // a draft atom writes itself out as its own text, and a source
                // spelling such as "$ a/b $" serializes canonically as "frac(a, b)".
                let structure = self.editor.root.clone();
                let display = self.editor.display;
                self.editor.apply(serde_json::from_value::<Action>(value).map_err(|e| e.to_string())?)?;
                let after = typst::write_document_mode(&self.editor.root, "", self.editor.display);
                // A command draft is transient: its half-typed text lives only in
                // the editor until Enter commits or Escape cancels it. Writing it
                // back per keystroke would put unrepresentable Typst (e.g. "$ frac( a $")
                // into the authoritative source and into the undo history.
                let changed = self.editor.root != structure || self.editor.display != display;
                if changed && after != committed && self.editor.pending().is_none() {
                    self.source.replace_range(range.start..range.end, &after);
                    self.last_reparsed=self.syntax.edit(range.start..range.end,&after);
                    self.active = Some(Equation { start: range.start, end: range.start + after.len(), display: self.editor.display });
                    self.revision += 1;
                    self.syntax_revision += 1;
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
            // Every Raw atom gets a source range, whatever the formula's spelling.
            // Gating this on the canonical serialization was why a formula typed
            // with fewer spaces than the writer emits never rendered its fragments.
            let canonical = typst::write_document_mode(&self.editor.root, "", self.editor.display);
            let locator = Locator::new(&self.source, range.start, equation, &canonical);
            annotate(&mut response["view"], &self.editor.root, &locator, &mut raw, &mut HashMap::new(), false, None);
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
        response["formula_definitions"] = json!(self.editor.definitions);
        // The identity the frontend keys a fragment's image and an attachment's placement by
        // (`typformula_core::context`). The page's copy comes from `desktop::project`, and both
        // have to agree or the box would ask the engine for what the page already has.
        if let Some(active)=&self.active {
            if let Some(context)=typformula_core::context::context_in(&self.syntax,&[(active.start,active.end)]) {
                response["formula_context"] = json!(context.digest());
            }
        } response["blocks"] = json!(blocks);
        response["render"] = json!({"source":self.source,"raw":raw,"formulas":formulas}); response
    }
}
/// Source ranges for the Raw fragments of one formula.
///
/// A Raw is drawn by Typst from its own source text, so each one needs a range in
/// the document. The position of a marker in the editor's canonical serialization
/// is only an estimate: the writer puts a space between atoms that the author may
/// have left out, and spells `a/b` as `frac(a, b)`. The estimate is therefore
/// mapped through an alignment of the two strings -- they differ by whitespace in
/// the common case -- and then confirmed by finding the fragment's own text, so a
/// wrong estimate can only cost a fragment its image, never point at other text.
struct Locator<'a> {
    document: &'a str,
    equation: &'a str,
    offset: usize,
    map: Vec<usize>,
}
impl<'a> Locator<'a> {
    fn new(document: &'a str, offset: usize, equation: &'a str, canonical: &str) -> Self {
        Self { document, equation, offset, map: align(canonical, equation) }
    }
    /// The range, confirmed to hold exactly this text in the document.
    fn verified(&self, start: usize, end: usize, text: &str) -> Option<(usize,usize)> {
        self.document.get(start..end).filter(|found| *found == text).map(|_| (start,end))
    }
    /// Range of one fragment whose marker position in `marked` is `position`.
    fn locate(&self, marked: &str, marker: &str, text: &str) -> Option<(usize,usize)> {
        let position = marked.find(marker)?;
        let estimate = self.map.get(position).copied().unwrap_or(0).min(self.equation.len());
        let forward = self.equation.get(estimate..)?.find(text).map(|at| estimate + at);
        let backward = self.equation.get(..estimate)?.rfind(text);
        let start = match (forward, backward) {
            (Some(a), Some(b)) => if estimate.abs_diff(a) <= estimate.abs_diff(b) { a } else { b },
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => return None,
        };
        self.verified(self.offset + start, self.offset + start + text.len(), text)
    }
}
/// Map every byte position of `canonical` to a byte position of `source`.
///
/// Both strings are walked together, step by step; a character only one of them
/// has is skipped when it is whitespace. Where they genuinely diverge the source
/// cursor stops moving, which keeps the map monotone and leaves the estimate
/// short of the truth instead of jumping.
fn align(canonical: &str, source: &str) -> Vec<usize> {
    let mut map = vec![0usize; canonical.len() + 1];
    let (mut i, mut j) = (0usize, 0usize);
    while i < canonical.len() {
        map[i] = j;
        if j < source.len() && canonical.as_bytes()[i] == source.as_bytes()[j] {
            i = step(canonical, i); j = step(source, j);
        } else if whitespace(canonical, i) {
            i = step(canonical, i);
        } else if j < source.len() && whitespace(source, j) {
            j = step(source, j);
        } else {
            i = step(canonical, i);
        }
    }
    map[canonical.len()] = source.len();
    map
}
fn step(text: &str, at: usize) -> usize { at + text[at..].chars().next().map_or(0, char::len_utf8) }
fn whitespace(text: &str, at: usize) -> bool { text[at..].chars().next().is_some_and(char::is_whitespace) }
fn locate_edit(view: &Value, root: &MathData, locator: &Locator, text: &str) -> Option<(usize, usize)> {
    let cursor = serde_json::from_value::<Cursor>(view["edit"].clone()).ok()?;
    let mut copy = root.clone();
    let mut marker = "visualtypstrangemarker".to_string();
    while locator.document.contains(&marker) { marker.push('x'); }
    cell_mut(&mut copy, &cursor.slices)[cursor.pos] = MathAtom::raw(&marker);
    let marked = typst::write_document_mode(&copy, "", locator.equation.starts_with("$ "));
    locator.locate(&marked, &marker, text).or_else(|| {
        let pos=marked.find(&marker)?;
        let estimate=locator.offset+locator.map.get(pos).copied().unwrap_or(0);
        typst::raw_ranges(locator.equation,locator.offset,text).into_iter().min_by_key(|(a,_)|a.abs_diff(estimate))
    })
}

fn has_active_stop(view: &Value) -> bool {
    (view["kind"] == "stop" && view["active"] == true)
        || view["children"].as_array().is_some_and(|children| children.iter().any(has_active_stop))
}

fn annotate(view: &mut Value, root: &MathData, locator: &Locator, raw: &mut Vec<Value>, counts: &mut HashMap<String,usize>, inside: bool, mut call: Option<[usize;2]>) {
    // Locate every fragment, even while the outer call supplies its image. The
    // request list selects non-overlapping outer images or the active call's slots.
    if view["kind"] == "macro" {
        if let Some(text) = view["source_text"].as_str() {
            if let Some((a,b)) = locate_edit(view, root, locator, text) { call = Some([a,b]); }
        }
    }
    let drawn = matches!(view["kind"].as_str(), Some("raw" | "raw_macro"));
    let expanded = view["kind"] == "raw_macro" && has_active_stop(view);
    if drawn {
        let text = view["source_text"].as_str().or(view["text"].as_str()).unwrap_or_default().to_owned();
        let range = locate_edit(view, root, locator, &text).or_else(|| {
            if let Ok([a,b]) = serde_json::from_value::<[usize;2]>(view["source_range"].clone()) {
                return locator.verified(a,b,&text);
            }
            let context = view["definitions"].as_str().unwrap_or_default();
            let origin = view["origin"].as_str().unwrap_or_default();
            let start = context.len();
            if locator.document.get(start..start+origin.len()) != Some(origin) { return None; }
            typst::raw_ranges(origin,start,&text).first().and_then(|(a,b)|locator.verified(*a,*b,&text))
        });
        if let Some((start,end)) = range {
            let instance = if view["kind"] == "raw_macro" && view["edit"].is_null() { call } else { None };
            let base = instance.map_or_else(||format!("{start}:{end}"), |[a,b]|format!("{start}:{end}:{a}:{b}"));
            let occurrence = counts.entry(base.clone()).or_insert(0);
            let id = if instance.is_some() {format!("{base}:{}",*occurrence)} else {base};
            let mut request = json!({"id":id,"start":start,"end":end});
            if let Some(call) = instance {request["call"] = json!(call); request["occurrence"] = json!(*occurrence);}
            view["render_id"] = json!(format!("{id}:0:{}",*occurrence)); *occurrence += 1;
            view["render_request"] = request.clone();
            if !inside && !expanded && !raw.iter().any(|r|r["id"]==id) {raw.push(request);}
        }
    }
    let inside = inside || (drawn && !expanded);
    if let Some(children) = view["children"].as_array_mut() {
        for child in children {annotate(child,root,locator,raw,counts,inside,call);}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_equation_index_follows_every_source_change() {
        let mut doc=Document::default();
        doc.apply(json!({"action":"set_source","source":"$a$ 正文 $ b $\n#let x = 1"})).unwrap();
        assert_eq!(doc.equations(),equations_flat(&doc));
        assert_eq!(doc.equations().len(),2);
        // An edit must never leave a stale index behind.
        doc.apply(json!({"action":"edit_source","start":0,"end":0,"text":"$c$\n"})).unwrap();
        assert_eq!(doc.equations(),equations_flat(&doc));
        assert_eq!(doc.equations().len(),3);
        doc.apply(json!({"action":"edit_source","start":0,"end":4,"text":""})).unwrap();
        assert_eq!(doc.equations(),equations_flat(&doc));
        assert_eq!(doc.equations().len(),2);
        // A structural edit writes the active range back into the source, which
        // changes the tree as well.
        let second=doc.equations()[1].start;
        doc.apply(json!({"action":"activate_formula","start":second})).unwrap();
        doc.apply(json!({"action":"input","text":"z"})).unwrap();
        assert_eq!(doc.equations(),equations_flat(&doc));
        assert_eq!(doc.editor.cursor.pos,1);
        assert_eq!(doc.response()["active_range"]["end"].as_u64().unwrap() as usize,doc.active.as_ref().unwrap().end);
        doc.apply(json!({"action":"set_source","source":"只留 $only$ 一个"})).unwrap();
        assert_eq!(doc.equations(),equations_flat(&doc));
        assert_eq!(doc.equations().len(),1);
        assert_eq!(doc.equation_nodes().len(),1);
    }
}
