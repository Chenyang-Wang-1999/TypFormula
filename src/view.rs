// SPDX-License-Identifier: GPL-2.0-or-later
// Build the display tree from editable atoms and their cursor positions.
use crate::{cursor::Editor, math::*, typst};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct View {
    pub kind: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_glyph: Option<String>,
    pub children: Vec<View>,
    pub cursor: Option<Cursor>,
    pub active: bool,
    pub selected: bool,
    pub columns: usize,
    pub edit: Option<Cursor>,
    pub attachment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definitions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    // Where the fragment's own text sits in the document. For a fragment inside a
    // macro template that is the definition's text, which is also what its image
    // is asked for: the call site compiles the template, so the fragment needs no
    // rendering instance of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_range: Option<[usize;2]>,
}
impl View {
    fn new(kind: &str, text: impl Into<String>, children: Vec<View>) -> Self {
        Self { kind: kind.into(), text: text.into(), display_glyph: None, children, cursor: None, active: false, selected: false, columns: 0, edit: None, attachment: None, definitions: None, origin: None, source_range: None }
    }
}
#[derive(Serialize)]
pub struct Response {
    pub revision: u64,
    pub view: View,
    pub source: String,
    pub selected_source: String,
    pub cursor: Cursor,
    pub candidates: Vec<String>,
    pub completion_index: usize,
    pub pending: bool,
    pub undo: bool,
    pub redo: bool,
    pub message: String,
    pub command: Option<crate::cursor::CommandContext>,
    pub definitions: String,
    pub string_mode: bool,
    pub display: bool,
    pub macros: Vec<typst::MacroDefinition>,
}
impl Editor {
    pub fn response(&mut self) -> Response {
        let mut view = self.view_cell(&self.root, Some(&[]), "root");
        let mut candidates = vec![];
        fn collect(view: &View, cursor: &Cursor, out: &mut Vec<Cursor>) {
            if let Some(c) = &view.cursor {
                if c.slices == cursor.slices && c.pos == cursor.pos { out.push(c.clone()); }
            }
            for ch in &view.children { collect(ch, cursor, out); }
        }
        collect(&view, &self.cursor, &mut candidates);
        // Keep the clicked projection, including while its insertion position changes.
        let branch = self.cursor.occurrence.rsplit_once(".p").map(|(s,_)| s);
        if let Some(best) = candidates.iter().find(|c| Some(c.occurrence.rsplit_once(".p").map_or("", |(s,_)| s)) == branch).or(candidates.first()) {
            self.cursor.occurrence = best.occurrence.clone();
        }
        fn activate(view: &mut View, cursor: &Cursor) {
            view.active = view.cursor.as_ref() == Some(cursor);
            for child in &mut view.children { activate(child, cursor); }
        }
        if self.selection().is_none() { activate(&mut view, &self.cursor); }
        let selected_source = self.draft_selection().map(str::to_string).unwrap_or_else(|| self.selection().map(|(a,b)| {
            let selected = cell(&self.root, &self.cursor.slices)[a..b].to_vec();
            if self.text_cell() { typst::write_atom(&MathAtom {kind:Kind::Text,cells:vec![selected]}) }
            else { typst::write_cell(&selected) }
        }).unwrap_or_default());
        Response {
            revision: self.revision, view, source: typst::write_document_mode(&self.root, &self.definitions, self.display), selected_source,
            cursor: self.cursor.clone(), candidates: self.completions(), completion_index: self.completion_index,
            pending: self.pending().is_some(), undo: self.can_undo(), redo: self.can_redo(), message: self.message.clone(),
            command: if self.string_mode() { None } else { self.command_context() }, definitions: self.definitions.clone(), string_mode: self.string_mode(),
            display: self.display,
            macros: typst::macro_registry(&self.definitions).entries.clone(),
        }
    }
    fn view_cell(&self, data: &MathData, path: Option<&[CursorSlice]>, occurrence: &str) -> View {
        let mut children = vec![];
        for p in 0..=data.len() {
            if let Some(path) = path {
                let mut stop = View::new("stop", "", vec![]);
                stop.cursor = Some(Cursor { slices: path.to_vec(), pos: p, occurrence: format!("{occurrence}.p{p}") });
                children.push(stop);
            }
            if let Some(atom) = data.get(p) {
                let mut view = self.view_atom(atom, path, p, &format!("{occurrence}.a{p}"));
                view.selected = path == Some(self.cursor.slices.as_slice()) && self.selection().is_some_and(|(a,b)| a <= p && p < b);
                children.push(view);
            }
        }
        View::new(if data.is_empty() { "empty-cell" } else { "cell" }, "", children)
    }
    fn view_atom(&self, atom: &MathAtom, path: Option<&[CursorSlice]>, pos: usize, occurrence: &str) -> View {
        let child_path = |idx| path.map(|p| { let mut p = p.to_vec(); p.push(CursorSlice { atom: pos, cell: idx }); p });
        if let Kind::MacroCall { name, function } = &atom.kind {
            let registry = typst::macro_registry(&self.definitions);
            let definition = registry.get(name).filter(|d| d.expandable);
            // A call is only bound while its argument count still matches the
            // definition it resolves to. Definitions can be adopted mid-edit (a
            // #let typed inside a formula), which leaves older calls behind, so
            // this is checked here and never assumed.
            let bound = definition.filter(|def| def.params.len() == atom.cells.len());
            if let Some(def) = bound.filter(|_| typst::projection_size(atom, &registry, typst::PROJECTION_LIMIT) <= typst::PROJECTION_LIMIT) {
                let args: Vec<_> = atom.cells.iter().enumerate().map(|(i, arg)| {
                    let mut view = View::new("macro-argument", def.params.get(i).map_or("", String::as_str), vec![self.view_cell(arg, child_path(i).as_deref(), occurrence)]);
                    view.columns = i;
                    view
                }).collect();
                let mut template = self.view_cell(&def.template, None, occurrence);
                self.bind_template(&mut template, def, &registry, &args);
                Self::locate_projection(&mut template, &format!("{occurrence}.macro"));
                return View::new("macro", name, vec![template]);
            }
            // Bounded projection: retain editable call slots for very large
            // expansions and for calls that no longer match their definition.
            let message = if definition.is_some() { "参数个数与定义不符，显示调用与参数" } else { "展开较大，显示调用与参数" };
            let mut children = vec![View::new("symbol", format!("{name}{}", if *function { "(" } else { "" }), vec![])];
            for (i, arg) in atom.cells.iter().enumerate() {
                if i > 0 { children.push(View::new("symbol", ", ", vec![])); }
                children.push(self.view_cell(arg, child_path(i).as_deref(), &format!("{occurrence}.c{i}")));
            }
            if *function { children.push(View::new("symbol", ")", vec![])); }
            return View::new("macro-collapsed", message, children);
        }
        let children: Vec<_> = atom.cells.iter().enumerate().map(|(idx, data)| self.view_cell(data, child_path(idx).as_deref(), &format!("{occurrence}.c{idx}"))).collect();
        match &atom.kind {
            Kind::MacroCall { .. } => unreachable!(),
            Kind::TemplateCall { definition } => { let mut view = View::new("template-call", "", children); view.columns = *definition; view }
            Kind::Parameter { index } => { let mut view = View::new("parameter", "", vec![]); view.columns = *index; view }
            Kind::Char { value } => {
                let mut view = View::new("char", value.to_string(), vec![]);
                // A Char contains exactly one Unicode scalar. Lookup only this
                // character, never its neighbors; editing/source remain Char.
                // The painter bypasses this display hint inside text cells.
                view.display_glyph = symbol(&view.text).map(str::to_string);
                view
            }
            Kind::Symbol { glyph, .. } => View::new("symbol", glyph, vec![]),
            Kind::Raw { source } => {
                let mut view = View::new("raw", source, vec![]);
                view.edit = path.map(|path| Cursor {slices:path.to_vec(),pos,occurrence:format!("{occurrence}.edit")});
                view
            }
            Kind::Unknown { name, caret, anchor, .. } => {
                let a = anchor.unwrap_or(*caret).min(*caret);
                let b = anchor.unwrap_or(*caret).max(*caret);
                let mut parts = vec![];
                if name.is_empty() { parts.push(View::new("draft-placeholder", "⌘", vec![])); }
                for (i,ch) in name.char_indices() {
                    if i == *caret { parts.push(View::new("draft-caret", "", vec![])); }
                    let mut part = View::new("draft-text", ch.to_string(), vec![]);
                    part.selected = a <= i && i < b;
                    parts.push(part);
                }
                if *caret == name.len() { parts.push(View::new("draft-caret", "", vec![])); }
                View::new("unknown", "", parts)
            }
            Kind::Frac => View::new("fraction", "", children),
            Kind::Sqrt => View::new("sqrt", "", children),
            Kind::Root => View::new("root", "", children),
            Kind::Script { .. } => {
                let mut slots = vec![children[0].clone_view()];
                for up in [true, false] { slots.push(atom.script_idx(up).map(|i| children[i].clone_view()).unwrap_or_else(|| View::new("absent", "", vec![]))); }
                let mut view = View::new("script", "", slots);
                fn has_draft(atom: &MathAtom) -> bool {
                    matches!(atom.kind, Kind::Unknown { .. }) || atom.cells.iter().flatten().any(has_draft)
                }
                // Phase one: top-level branches only. Nested math styles must
                // be passed through Typst before this scope can be broadened.
                if path.is_some_and(|p| p.is_empty()) && !has_draft(atom) && !atom.cells[0].is_empty() {
                    view.attachment = Some(typst::write_atom(atom));
                }
                view
            }
            Kind::Text => View::new("text", "", children),
            Kind::Delim { left, right } => View::new("delim", format!("{left}\n{right}"), children),
            Kind::Decoration { name } => View::new("decoration", name, children),
            Kind::Grid { columns } => { let mut v = View::new("grid", "", children); v.columns = *columns; v }
            Kind::Aligned { columns, .. } => { let mut v = View::new("aligned", "", children); v.columns = *columns; v }
        }
    }
    fn bind_template(&self, view: &mut View, def: &typst::MacroDefinition, registry: &typst::MacroRegistry, args: &[View]) {
        self.bind_template_inner(view,def,registry,args,&mut std::collections::HashMap::new());
    }
    fn bind_template_inner(&self, view: &mut View, def: &typst::MacroDefinition, registry: &typst::MacroRegistry, args: &[View], counts: &mut std::collections::HashMap<String,usize>) {
        if view.kind == "parameter" {
            *view = args.get(view.columns).cloned().unwrap_or_else(|| View::new("absent", "", vec![]));
            return;
        }
        if view.kind == "raw" {
            view.definitions = Some(def.context.as_ref().clone()); view.origin = Some(def.source.clone());
            let ranges=typst::definition_raw_ranges(def,&view.text);
            let ordinal=counts.entry(view.text.clone()).or_default();
            view.source_range=ranges.get(*ordinal).map(|(a,b)|[*a,*b]);*ordinal+=1;
        }
        for child in &mut view.children {
            self.bind_template_inner(child, def, registry, args, counts);
        }
        if view.kind == "template-call" {
            let Some(callee) = registry.entries.get(view.columns) else {
                *view = View::new("absent", "", vec![]);
                return;
            };
            let mut template = self.view_cell(&callee.template, None, "");
            self.bind_template(&mut template, callee, registry, &view.children);
            *view = template;
        }
    }
    // Cloned argument projections share canonical cursor paths, but every
    // displayed occurrence needs a unique, stable identity for the caret.
    fn locate_projection(view: &mut View, occurrence: &str) {
        for (i, child) in view.children.iter_mut().enumerate() {
            if let Some(cursor) = &mut child.cursor {
                cursor.occurrence = format!("{occurrence}.p{}", cursor.pos);
            } else {
                Self::locate_projection(child, &format!("{occurrence}.v{i}"));
            }
        }
    }
}
impl View {
    fn clone_view(&self) -> Self { self.clone() }
}
