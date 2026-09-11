// SPDX-License-Identifier: GPL-2.0-or-later
// Build the display tree from editable atoms and their cursor positions.
use crate::{cursor::Editor, math::*, typst};
use crate::slots::{view::MACRO_COLLAPSED, Role};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct View {
    pub kind: String,
    pub text: String,
    // Which slot of its parent this node fills. `kind` names the arrangement
    // this node is laid out by; `role` names its position in the parent's
    // arrangement, so a future kind that reuses an arrangement is placed by role
    // and needs no frontend change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_glyph: Option<String>,
    pub children: Vec<View>,
    pub cursor: Option<Cursor>,
    pub active: bool,
    pub selected: bool,
    pub columns: usize,
    // Which decoration this node is, for the view kinds whose drawing is chosen
    // by a name rather than by the shape: `decorated` (radical, delimiter pair,
    // accent, rule) and `root`. The vocabulary is a contract with the frontend,
    // which dispatches on it, so a marker the frontend does not know is a
    // frontend/backend mismatch rather than data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    // The font variant a `style` node is drawn in (`bold`, `upright`, …). The glyphs
    // that variant produces are **not** on the wire: Typst applies a variant by
    // substituting codepoints through a table the kernel cannot reach, so the frontend
    // asks the engine for them and stamps the answer on (`_glyph`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_name: Option<String>,
    // A table's delimiters, spelled left-then-right (`"()"`, `"| |"` minus the
    // space). The engine's `MatElem` defaults to a parenthesis pair, and the
    // frontend used to draw square brackets for every table; carrying the pair
    // is what lets the two agree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    // Whether the cells are grouped the way `mat` spells them (comma per column,
    // semicolon per row) rather than one argument per row (`vec`, `cases`).
    pub is_mat: bool,
    // How many cells each row really has, before the flat list was padded to
    // `columns`. `mat(a, b; c)` and `a & b \ c` both pad a short row, so without this
    // the frontend would draw a cell the source does not have.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub row_lengths: Vec<usize>,
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
        Self { kind: kind.into(), text: text.into(), role: None, display_glyph: None, children, cursor: None, active: false, selected: false, columns: 0, marker: None, style_name: None, border: None, is_mat: false, row_lengths: vec![], edit: None, attachment: None, definitions: None, origin: None, source_range: None }
    }
    /// One `decorated` node: a single inner cell with a decoration whose drawing
    /// the frontend chooses by `marker`. `sqrt`, a delimiter pair, an accent and
    /// a rule share this kind because their *editing* is identical (one cell,
    /// linear, entered at the edge, no vertical move) — only the drawing differs.
    fn decorated(marker: impl Into<String>, text: impl Into<String>, children: Vec<View>) -> Self {
        let mut view = Self::new("decorated", text, children);
        view.marker = Some(marker.into());
        view
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
        // A configured command call has no shape of its own: the command file says
        // which shape it draws as, and the call's cells *are* that shape's slots. So
        // the projection runs on the shape and reads the call's cells — the one place
        // a `MacroCall` is not drawn as a call.
        let shape = atom.command_shape();
        let kind = shape.as_ref().unwrap_or(&atom.kind);
        // The layout strategy is declared per kind (`slots::Decl::view`); only
        // the fields that a strategy reads are filled in below.
        let view_kind = kind.decl().view;
        if shape.is_none() && let Kind::MacroCall { name, function } = &atom.kind {
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
                return View::new(view_kind, name, vec![template]);
            }
            // Bounded projection: retain editable call slots for very large
            // expansions and for calls that no longer match their definition.
            //
            // `text` is the **call's own spelling**, not a message. The document is what
            // the engine typesets this call to, so the frontend asks for that fragment's
            // image and draws it while the caret is *outside*; entering the node swaps to
            // the name and argument slots below. The message that used to sit here
            // ("展开较大" / "参数个数与定义不符") was read by nobody — the frontend only ever
            // laid out the children — so the field was free to become the source, which is
            // what `document::annotate` and the image pipeline need to find it by.
            let mut children = vec![View::new("symbol", format!("{name}{}", if *function { "(" } else { "" }), vec![])];
            for (i, arg) in atom.cells.iter().enumerate() {
                if i > 0 { children.push(View::new("symbol", ", ", vec![])); }
                children.push(self.view_cell(arg, child_path(i).as_deref(), &format!("{occurrence}.c{i}")));
            }
            if *function { children.push(View::new("symbol", ")", vec![])); }
            let mut view = View::new(MACRO_COLLAPSED, typst::write_atom(atom), children);
            view.edit = path.map(|path| Cursor { slices: path.to_vec(), pos, occurrence: format!("{occurrence}.edit") });
            return view;
        }
        // Every cell is a slot of this node, and its role comes from the node's
        // own declaration, so the frontend can place it without counting cells.
        let children: Vec<_> = atom.cells.iter().enumerate().map(|(idx, data)| {
            let mut child = self.view_cell(data, child_path(idx).as_deref(), &format!("{occurrence}.c{idx}"));
            child.role = kind.decl().role_at(idx).map(Role::name);
            child
        }).collect();
        match kind {
            Kind::MacroCall { .. } => unreachable!(),
            Kind::TemplateCall { definition } => { let mut view = View::new(view_kind, "", children); view.columns = *definition; view }
            Kind::Parameter { index } => { let mut view = View::new(view_kind, "", vec![]); view.columns = *index; view }
            Kind::Char { text } => {
                let mut view = View::new(view_kind, text, vec![]);
                // A Char is one grapheme cluster, the unit the lexer and the engine
                // both keep together. Lookup only this character's text, never its
                // neighbors; editing/source remain Char. The painter bypasses this
                // display hint inside text cells.
                view.display_glyph = symbol(&view.text).map(str::to_string);
                view
            }
            Kind::Symbol { glyph, .. } => View::new(view_kind, glyph, vec![]),
            // One cell holding the run's characters, like a text run: the frontend
            // draws the cell and the caret can sit between the digits.
            Kind::Number => View::new(view_kind, "", children),
            Kind::Raw { source } => {
                let mut view = View::new(view_kind, source, vec![]);
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
                View::new(view_kind, "", parts)
            }
            Kind::Fraction => { let mut v = View::new("fraction", "", children); v.marker = Some("-".into()); v }
            Kind::Sqrt => View::decorated("radical", "", children),
            Kind::Root => { let mut v = View::new("root", "", children); v.marker = Some("radical".into()); v }
            Kind::Scripts => {
                let mut slots = vec![children[0].clone_view()];
                for up in [true, false] {
                    let index = script_cell(up);
                    let mut slot = match atom.script_idx(up) { Some(i) => children[i].clone_view(), None => View::new("absent", "", vec![]) };
                    // An empty attachment is a slot too, so it carries the role
                    // of the cell it stands in for.
                    slot.role = atom.decl().role_at(index).map(Role::name);
                    slots.push(slot);
                }
                let mut view = View::new("scripts", "", slots);
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
            // The delimiter characters stay in `text` (left, newline, right, as the
            // frontend has always read them); `marker` says only *which* decoration
            // this is, so the frontend dispatches on one name instead of on the shape.
            Kind::Fenced { left, right } => View::decorated("delim", format!("{left}\n{right}"), children),
            Kind::Accent { name } => View::decorated(name.clone(), "", children),
            // `LineItem` stores the position and nothing else, so the marker is the
            // position spelled the way the frontend draws it.
            Kind::Line { above } => View::decorated(if *above { "overline" } else { "underline" }, "", children),
            // A font variant. `style_name` is the command that made it, and `text` is the
            // **call's spelling** (`bold(upright(a))`) — the expression whose substituted
            // glyphs are wanted, so nothing has to be reassembled from the children. The
            // glyphs themselves are not here: the kernel cannot reach the table that
            // produces them, so the frontend reads them out of its own cache when it lays
            // the view out, and draws the call itself until it has them.
            Kind::Style { name } => {
                let mut v = View::new("style", typst::write_atom(atom), children);
                v.style_name = Some(name.clone());
                // The cursor locates this call for the *frontend*, not for `annotate`:
                // a variant is never drawn from an image, so it holds no source range.
                // Its children are what an edit of the body goes through.
                v.edit = path.map(|path| Cursor { slices: path.to_vec(), pos, occurrence: format!("{occurrence}.edit") });
                v
            }
            Kind::Table { columns, row_lengths, name } => {
                let mut v = View::new("table", "", children);
                v.columns = *columns;
                v.row_lengths = row_lengths.clone();
                // The delimiters and the row convention belong to the *command*, not
                // to the table it builds: `mat` is centred inside parentheses, `cases`
                // is a left brace, `vec` is a column vector — three tables from one
                // shape, told apart only by the name they were written with.
                let spec = crate::slots::command_spec(name);
                v.border = spec.and_then(|spec| spec.border).map(str::to_string);
                v.is_mat = spec.and_then(|spec| spec.rows) == Some("mat");
                v
            }
            Kind::Multiline { columns, row_lengths } => {
                let mut v = View::new("multiline", "", children);
                v.columns = *columns;
                v.row_lengths = row_lengths.clone();
                v
            }
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
