// SPDX-License-Identifier: GPL-2.0-or-later
// Build the display tree from editable atoms and their cursor positions, and store a
// definition's body as the display tree it expands into.
use crate::{cursor::Editor, math::*, slots::{self, view::MACRO_COLLAPSED, Role}, typst::{self, MacroDefinition, MacroRegistry}};
use serde::Serialize;
use std::collections::HashMap;

/// Everything about one display node that is a fact about **the node**.
///
/// `View` is the wire node: the structure below plus the session looking at it (a
/// caret, a selection, an edit point, a source range). Splitting the two is what lets
/// a macro template be **stored as a display tree** (`ViewTemplate`) with no session
/// in it at all: a template is a fact about the document, not about who is editing it,
/// so "a stored template carries no cursor" is a property of the type rather than a
/// rule someone has to remember.
///
/// The session fields are not here, and the *bind-time* ones are not either:
/// `definitions`/`origin`/`source_range` say where a fragment of a **template** lives
/// in the definition text, so they are written by the binder, which is the only place
/// that knows which definition a fragment came from.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewNode {
    pub kind: String,
    pub text: String,
    pub role: Option<&'static str>,
    pub display_glyph: Option<String>,
    /// Which decoration this node is, for the view kinds whose drawing is chosen
    /// by a name rather than by the shape: `decorated` (radical, delimiter pair,
    /// accent, rule) and `root`. The vocabulary is a contract with the frontend,
    /// which dispatches on it, so a marker the frontend does not know is a
    /// frontend/backend mismatch rather than data.
    pub marker: Option<String>,
    /// The font variant a `style` node is drawn in (`bold`, `upright`, …). The glyphs
    /// that variant produces are **not** on the wire: Typst applies a variant by
    /// substituting codepoints through a table the kernel cannot reach, so the frontend
    /// asks the engine for them and stamps the answer on (`_glyph`).
    pub style_name: Option<String>,
    /// A table's delimiters, spelled left-then-right (`"()"`, `"| |"` minus the
    /// space). The engine's `MatElem` defaults to a parenthesis pair, and the
    /// frontend used to draw square brackets for every table; carrying the pair
    /// is what lets the two agree.
    pub border: Option<String>,
    /// Whether the cells are grouped the way `mat` spells them (comma per column,
    /// semicolon per row) rather than one argument per row (`vec`, `cases`).
    pub is_mat: bool,
    /// The column count of a table or an alignment. While a template is being built it
    /// also carries the instance data of the two editor-only kinds — a hole's argument
    /// index and an edge's definition index — because they are projected as nodes
    /// before `ViewTemplate::of` turns them into variants.
    pub columns: usize,
    /// How many cells each row really has, before the flat list was padded to
    /// `columns`. `mat(a, b; c)` and `a & b \ c` both pad a short row, so without this
    /// the frontend would draw a cell the source does not have.
    pub row_lengths: Vec<usize>,
    pub children: Vec<ViewTemplate>,
}

/// A macro template, **stored as the display tree it expands into**.
///
/// A definition's body is projected once, at registration, and kept in this form; a
/// call site is then that material with the arguments bound into its holes. Nothing
/// here is a spelling of the definition: the template is never written back, because
/// what gets written back is the *call* (`Write::Named`), which is what makes
/// "expansion is only visual" a fact about the grammar rather than a discipline.
///
/// The two editor-only nodes are **variants**, not view kinds. That is the whole
/// point of the type: a hole cannot reach the frontend, because the display tree has
/// no variant for one — `docs/editing-model.md` §6.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewTemplate {
    /// A display node: drawn, and possibly a parent.
    Node(ViewNode),
    /// A parameter hole: the argument view is substituted here, and nothing else is.
    Hole { index: usize },
    /// An edge to an earlier definition: embed that definition's template with these
    /// cells as its arguments. The cells are bound in the **caller's** context first,
    /// which is how a nested expansion keeps each hole pointing at the call site it
    /// really came from — no side table of origins is needed.
    Edge { definition: usize, cells: Vec<ViewTemplate> },
}

impl ViewTemplate {
    /// The template of a definition the editor does not expand.
    ///
    /// It is what an **empty cell** projects to, not a special case: `view_cell(&[])`
    /// has always answered `empty-cell`, and a definition whose body never became a
    /// template is exactly a definition with no material. Nothing reads it — only an
    /// expandable definition is ever bound, and only expandable definitions are named
    /// by `ViewTemplate::Edge` — so it exists to keep the field total rather than to
    /// be drawn.
    pub fn empty() -> Self {
        Self::Node(ViewNode {
            kind: "empty-cell".into(), text: String::new(), role: None, display_glyph: None,
            marker: None, style_name: None, border: None, is_mat: false, columns: 0,
            row_lengths: vec![], children: vec![],
        })
    }
    /// Turn one session-free projection into a stored template.
    ///
    /// This is the **one** place the two editor-only view kinds are recognised, and
    /// they are recognised where a node is *read back into the model* — the same kind
    /// of step as the parser reading source into atoms. Everything downstream
    /// (`ViewTemplate::Hole`, `ViewTemplate::Edge`) is structure, so no later pass can
    /// mistake a hole for a node to draw.
    ///
    /// A variant written around a hole projects as `raw_macro` here — the projection
    /// cannot borrow a body-dependent shape without a body — and that is left alone: the
    /// stored template is a *description* of what the definition expands to, and where it
    /// cannot say, the honest description is the arrangement the projection produced. The
    /// call site re-projects the bound body instead (`Projector::material`), which settles
    /// it exactly.
    ///
    /// The session fields are dropped rather than copied, which is exactly what they
    /// are: the projection that builds a template is run with no session, so there is
    /// nothing to drop. The `debug_assert` is what keeps that true if the projection
    /// ever learns to fill one of them without a caret.
    fn of(view: &View) -> Self {
        debug_assert!(
            view.cursor.is_none() && view.edit.is_none() && view.attachment.is_none() && !view.active && !view.selected,
            "模板是用无会话的投影建的，不该带上光标或编辑点：{}", view.kind,
        );
        match view.kind.as_str() {
            // A hole carries which argument it stands for in `columns`, the field the
            // editor-only kinds have always used for their own instance data.
            "parameter" => return Self::Hole { index: view.columns },
            "template-call" => return Self::Edge {
                definition: view.columns,
                cells: view.children.iter().map(Self::of).collect(),
            },
            _ => {}
        }
        Self::Node(ViewNode {
            kind: view.kind.clone(), text: view.text.clone(), role: view.role,
            display_glyph: view.display_glyph.clone(), marker: view.marker.clone(),
            style_name: view.style_name.clone(), border: view.border.clone(), is_mat: view.is_mat,
            columns: view.columns, row_lengths: view.row_lengths.clone(),
            children: view.children.iter().map(Self::of).collect(),
        })
    }
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    pub is_mat: bool,
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
    // is asked for. Template calls additionally carry invocation metadata in the
    // host response, so different actual arguments select different frames.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_range: Option<[usize;2]>,
    /// Definition spelling used for location, independent of the bound display text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_text: Option<String>,
}
impl View {
    fn new(kind: &str, text: impl Into<String>, children: Vec<View>) -> Self {
        Self { kind: kind.into(), text: text.into(), role: None, display_glyph: None, children, cursor: None, active: false, selected: false, columns: 0, marker: None, style_name: None, border: None, is_mat: false, row_lengths: vec![], edit: None, attachment: None, definitions: None, origin: None, source_range: None, source_text: None }
    }
    /// One display node, from the structure a template stored for it.
    fn of(node: &ViewNode, children: Vec<View>) -> Self {
        Self { kind: node.kind.clone(), text: node.text.clone(), role: node.role, display_glyph: node.display_glyph.clone(),
               children, cursor: None, active: false, selected: false, columns: node.columns, marker: node.marker.clone(),
               style_name: node.style_name.clone(), border: node.border.clone(), is_mat: node.is_mat,
               row_lengths: node.row_lengths.clone(), edit: None, attachment: None, definitions: None, origin: None, source_range: None, source_text: None }
    }
    /// A cell that holds nothing: the answer for a hole whose argument is missing.
    fn absent() -> Self { Self::new("absent", "", vec![]) }
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
        // The projection is scoped: it borrows the definition registry and the caret,
        // and the caret is touched again below (`self.cursor.occurrence` is rewritten
        // once the view exists).
        let registry = typst::macro_registry(&self.definitions);
        let mut view = {
            let projector = Projector {
                registry: &registry,
                session: Some(Session { slices: &self.cursor.slices, selection: self.selection() }),
            };
            projector.view_cell(&self.root, Some(&[]), "root", true)
        };
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
}

/// One projection's session state: where the caret is and what is selected **right now**.
///
/// Passing `None` instead is how a *template* view is built. At registration there is no
/// caret, no selection, no history and no geometry, so there is nothing to pass — which
/// is what makes "a stored template carries no cursor" a fact about the call rather than
/// a rule someone has to remember.
#[derive(Clone, Copy)]
pub struct Session<'a> {
    pub slices: &'a [CursorSlice],
    pub selection: Option<(usize, usize)>,
}

/// The projection: an editable tree (or a template tree) in, a display tree out.
///
/// Reads only the macro registry plus an optional [`Session`], and deliberately holds
/// no `Editor`. Two things follow from that, and both are `docs/editing-model.md`:
///
/// * registration can build a template's display tree, because building one needs no
///   caret, no selection, no history and no geometry — there is nothing to pass, so
///   "a stored template carries no session" is a fact about the call rather than a
///   rule someone has to remember;
/// * the whole document's registry is looked up **once** per projection rather than
///   once per call node, which is what `Editor::response` used to do.
pub struct Projector<'a> {
    pub registry: &'a MacroRegistry,
    pub session: Option<Session<'a>>,
}

impl Projector<'_> {
    /// The caret's path, when there is a session.
    fn slices(&self) -> &[CursorSlice] {
        self.session.map_or(&[][..], |session| session.slices)
    }
    /// The selection, when there is a session.
    fn selection(&self) -> Option<(usize, usize)> {
        self.session.and_then(|session| session.selection)
    }
    fn view_cell(&self, data: &MathData, path: Option<&[CursorSlice]>, occurrence: &str, attachment_level: bool) -> View {
        let mut children = vec![];
        for p in 0..=data.len() {
            if let Some(path) = path {
                let mut stop = View::new("stop", "", vec![]);
                stop.cursor = Some(Cursor { slices: path.to_vec(), pos: p, occurrence: format!("{occurrence}.p{p}") });
                children.push(stop);
            }
            if let Some(atom) = data.get(p) {
                let mut view = self.view_atom(atom, path, p, &format!("{occurrence}.a{p}"), attachment_level);
                view.selected = path == Some(self.slices()) && self.selection().is_some_and(|(a,b)| a <= p && p < b);
                children.push(view);
            }
        }
        View::new(if data.is_empty() { "empty-cell" } else { "cell" }, "", children)
    }
    fn view_atom(&self, atom: &MathAtom, path: Option<&[CursorSlice]>, pos: usize, occurrence: &str, attachment_level: bool) -> View {
        let child_path = |idx| path.map(|p| { let mut p = p.to_vec(); p.push(CursorSlice { atom: pos, cell: idx }); p });
        let shape = atom.shape();
        // The layout strategy is declared per shape (`slots::Shape::view`); only
        // the fields that a strategy reads are filled in below.
        let view_kind = shape.view;
        if let Kind::MacroCall { name, function } = &atom.kind {
            // A configured call is drawn from the data its *name* supplies plus the
            // shape it names — no `Kind` of its own. This is the one place a
            // `MacroCall` is not drawn as a call.
            //
            // The gate is `command_shape`, **not** `configured_draw`: the shape lookup
            // is what answers `None` for a font variant whose body is not a glyph run,
            // and that answer is what sends `bold(frac(a, b))` down the `raw_macro` path
            // below instead of building a `style` node it could never fill.
            let draw = atom.command_shape().and_then(|_| crate::slots::configured_draw(name));
            if let Some(draw) = draw {
                let children = atom.cells.iter().enumerate().map(|(idx, data)| {
                    let mut child = self.view_cell(data, child_path(idx).as_deref(), &format!("{occurrence}.c{idx}"), attachment_level && matches!(atom.kind, Kind::Multiline {..}));
                    child.role = shape.role_at(idx).map(Role::name);
                    child
                }).collect();
                return Self::view_configured(atom, name, draw, children, path, pos, occurrence);
            }
            let registry = self.registry;
            let definition = registry.get(name).filter(|d| d.expandable);
            // A call is only bound while its argument count still matches the
            // definition it resolves to. Definitions can be adopted mid-edit (a
            // #let typed inside a formula), which leaves older calls behind, so
            // this is checked here and never assumed.
            let bound = definition.filter(|def| def.params.len() == atom.cells.len());
            if let Some(def) = bound.filter(|_| typst::projection_size(atom, self.registry, typst::PROJECTION_LIMIT) <= typst::PROJECTION_LIMIT) {
                let args: Vec<_> = atom.cells.iter().enumerate().map(|(i, arg)| {
                    let mut view = View::new("macro-argument", def.params.get(i).map_or("", String::as_str), vec![self.view_cell(arg, child_path(i).as_deref(), occurrence, false)]);
                    view.columns = i;
                    view
                }).collect();
                let mut template = self.expand(def, &args, occurrence);
                Self::locate_projection(&mut template, &format!("{occurrence}.macro"));
                let mut view = View::new(view_kind, name, vec![template]);
                view.edit = path.map(|path| Cursor { slices:path.to_vec(), pos, occurrence:format!("{occurrence}.edit") });
                view.source_text = Some(typst::write_atom(atom));
                return view;
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
                children.push(self.view_cell(arg, child_path(i).as_deref(), &format!("{occurrence}.c{i}"), false));
            }
            if *function { children.push(View::new("symbol", ")", vec![])); }
            let mut view = View::new(MACRO_COLLAPSED, typst::write_atom(atom), children);
            view.edit = path.map(|path| Cursor { slices: path.to_vec(), pos, occurrence: format!("{occurrence}.edit") });
            return view;
        }
        // Every cell is a slot of this node, and its role comes from the node's
        // own shape, so the frontend can place it without counting cells.
        let children: Vec<_> = atom.cells.iter().enumerate().map(|(idx, data)| {
            let mut child = self.view_cell(data, child_path(idx).as_deref(), &format!("{occurrence}.c{idx}"), attachment_level && matches!(atom.kind, Kind::Multiline {..}));
            child.role = shape.role_at(idx).map(Role::name);
            child
        }).collect();
        match &atom.kind {
            Kind::MacroCall { .. } => unreachable!(),
            Kind::TemplateCall { definition } => { let mut view = View::new(view_kind, "", children); view.columns = *definition; view }
            Kind::Parameter { index, .. } => { let mut view = View::new(view_kind, "", vec![]); view.columns = *index; view }
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
            Kind::Scripts => {
                let mut slots = vec![children[0].clone_view()];
                for up in [true, false] {
                    let index = script_cell(up);
                    // Optional empty scripts are absent only when they are not
                    // being edited. Creating a^ moves the caret into an empty
                    // upper cell, whose placeholder and stop must remain visible.
                    let editing_empty = child_path(index).as_deref()
                        .is_some_and(|p| self.session.is_some() && p == self.slices());
                    let mut slot = if atom.script_idx(up).is_some() || editing_empty {
                        children[index].clone_view()
                    } else {View::new("absent", "", vec![])};
                    // An empty attachment is a slot too, so it carries the role
                    // of the cell it stands in for.
                    slot.role = atom.shape().role_at(index).map(Role::name);
                    slots.push(slot);
                }
                let mut view = View::new("scripts", "", slots);
                fn has_draft(atom: &MathAtom) -> bool {
                    matches!(atom.kind, Kind::Unknown { .. }) || atom.cells.iter().flatten().any(has_draft)
                }
                // Alignment cells keep their equation's math style; fractions,
                // scripts and other nests still require their own engine context.
                if attachment_level && path.is_some() && !has_draft(atom) && !atom.cells[0].is_empty() {
                    view.attachment = Some(typst::write_atom(atom));
                }
                view
            }
            Kind::Text => View::new("text", "", children),
            // The delimiter characters stay in `text` (left, newline, right, as the
            // frontend has always read them); `marker` says only *which* decoration
            // this is, so the frontend dispatches on one name instead of on the shape.
            Kind::Fenced { left, right } => View::decorated("delim", format!("{left}\n{right}"), children),
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
    /// A call drawn by the shape its name declares.
    ///
    /// This replaces what used to be five `Kind` variants (`Sqrt`, `Root`, `Accent`,
    /// `Line`, `Style`) that no source code could ever store: they existed so that a
    /// configured call had a `Kind` to borrow. The arrangement comes from the shape,
    /// the mark or the delimiter pair from the config entry, and where the command's
    /// own name *is* the data (`hat`'s mark, `bold`'s variant) it is read straight off
    /// the call.
    ///
    /// The wire kinds are spelled out rather than taken from `Shape::view`, because
    /// the two disagree for most of these shapes — `sqrt`, `delim`, `decoration` and
    /// `line` all travel as `decorated`, told apart by `marker`.
    fn view_configured(atom: &MathAtom, name: &str, draw: slots::Draw, children: Vec<View>, path: Option<&[CursorSlice]>, pos: usize, occurrence: &str) -> View {
        match draw {
            slots::Draw::Fraction => { let mut view = View::new("fraction", "", children); view.marker = Some("-".into()); view }
            // A bare hook is a `decorated`; a hook with a degree is its own arrangement,
            // because the frontend lays the two out differently.
            slots::Draw::Radical { degree: false } => View::decorated("radical", "", children),
            slots::Draw::Radical { degree: true } => { let mut view = View::new("root", "", children); view.marker = Some("radical".into()); view }
            // A delimited pair: the characters stay in `text` (left, newline, right, as
            // the frontend has always read them); `marker` says only *which* decoration
            // this is, so the frontend dispatches on one name instead of on the shape.
            slots::Draw::Delim(text) => View::decorated("delim", text, children),
            // The mark is the command's own name, which is why the config needs no
            // field for it: `hat` is drawn with the mark `hat`.
            slots::Draw::Mark => View::decorated(name, "", children),
            slots::Draw::Rule { above } => View::decorated(if above { "overline" } else { "underline" }, "", children),
            slots::Draw::Variant => {
                // A font variant. `style_name` is the command that made it, and `text` is
                // the **call's spelling** (`bold(upright(a))`) — the expression whose
                // substituted glyphs are wanted, so nothing has to be reassembled from the
                // children. The glyphs themselves are not here: the kernel cannot reach the
                // table that produces them, so the frontend reads them out of its own cache
                // when it lays the view out, and draws the call itself until it has them.
                let mut view = View::new("style", typst::write_atom(atom), children);
                view.style_name = Some(name.to_string());
                // The cursor locates this call for the *frontend*, not for `annotate`:
                // a variant is never drawn from an image, so it holds no source range.
                // Its children are what an edit of the body goes through.
                view.edit = path.map(|path| Cursor { slices: path.to_vec(), pos, occurrence: format!("{occurrence}.edit") });
                view
            }
        }
    }
    /// Bind a stored node's children, leaving the node itself as stored.
    fn bind_children(&self, node: &ViewNode, def: &MacroDefinition, args: &[View], occurrence: &str, counts: &mut HashMap<String, usize>) -> View {
        let children = node.children.iter().enumerate()
            .map(|(i, child)| self.material(child, def, args, &format!("{occurrence}.v{i}"), counts))
            .collect();
        let mut view = View::of(node, children);
        // A fragment of a template has no `edit` cursor — that is the session's, and there
        // is no session in a definition — so where it sits is said by the definition's own
        // range instead: the fragment's image is asked for at the text the **definition**
        // writes, and the call site is what compiles it (`document::annotate` reads these
        // three fields).
        if matches!(node.kind.as_str(), "raw" | "raw_macro") {
            view.definitions = Some(def.context.as_ref().clone()); view.origin = Some(def.source.clone());
            let ranges=typst::definition_raw_ranges(def,&view.text);
            let ordinal=counts.entry(view.text.clone()).or_default();
            view.source_range=ranges.get(*ordinal).map(|(a,b)|[*a,*b]);*ordinal+=1;
            view.source_text = Some(node.text.clone());
            if let Some([a,b])=view.source_range {
                view.source_text=typst::definition_prefix(def).get(a..b).map(str::to_string);
            }
        }
        view
    }
    /// Expand one call: the definition's stored template in, this call's display
    /// tree out.
    ///
    /// A **total** function of the template and the arguments. That is the whole
    /// construction behind `docs/editing-model.md` §6:
    ///
    /// * a hole has no display of its own — it *is* the argument view, so the caret
    ///   inside it points at the call site, because that is the view the argument
    ///   projection brought along;
    /// * material that has no hole in it is material, and was projected with no
    ///   cursor, so it carries no `stop` and the caret cannot reach it;
    /// * nothing here tests a view *kind* to decide any of that, because `Hole` and
    ///   `Edge` are variants of the stored type rather than names a node might have.
    fn expand(&self, def: &MacroDefinition, args: &[View], occurrence: &str) -> View {
        // One ordinal table per level, counting how many times this level's text
        // spells the same `Raw`: one definition that writes the same fragment twice
        // has two ranges, and the occurrence is what tells them apart.
        self.material(&def.template, def, args, occurrence, &mut HashMap::new())
    }
    fn material(&self, template: &ViewTemplate, def: &MacroDefinition, args: &[View], occurrence: &str, counts: &mut HashMap<String, usize>) -> View {
        match template {
            ViewTemplate::Hole { index } => args.get(*index).cloned().unwrap_or_else(View::absent),
            // The arguments of a nested call are the **caller's** material, so they are
            // bound in the caller's context first; only then do they become the callee's
            // arguments. Doing it the other way round would leave an inner hole pointing
            // at the outer definition, which is the bug the side table of origins that
            // `docs/editing-model.md` §8 rejects was meant to paper over.
            ViewTemplate::Edge { definition, cells } => {
                let Some(callee) = self.registry.entries.get(*definition) else { return View::absent() };
                let args: Vec<View> = cells.iter().enumerate()
                    .map(|(i, cell)| self.material(cell, def, args, &format!("{occurrence}.c{i}"), counts))
                    .collect();
                self.expand(callee, &args, occurrence)
            }
            // A call the registration could not shape: either one that has no shape at
            // all (an unknown callee, a macro call that is not being expanded), or a font
            // variant whose body was a **hole** — and a hole is not a character, so the
            // registration had nothing to decide with.
            //
            // Bind child Views first. Their structure determines whether a font
            // variant now has a glyph run; their cursor identities must survive.
            ViewTemplate::Node(node) if node.kind == MACRO_COLLAPSED => {
                let mut view = self.bind_children(node, def, args, occurrence, counts);
                // Bind the actual child Views, including their editable argument
                // cursors. Re-parsing their text would erase those identities.
                view.text = view.children.iter().filter_map(View::spelling).collect();
                let name = node.text.split('(').next().unwrap_or_default();
                let cells: Vec<_> = view.children.iter().filter(|v| matches!(v.kind.as_str(), "cell" | "empty-cell")).cloned().collect();
                if slots::configured_shape(name).is_some_and(|s| s.is_font_variant())
                    && !cells.is_empty() && cells.iter().all(View::glyph_run) {
                    view.kind = "style".into(); view.style_name = Some(name.into());
                    view.children = cells;
                    for child in &mut view.children { child.role = Some("inner"); }
                }
                view
            }
            ViewTemplate::Node(node) => self.bind_children(node, def, args, occurrence, counts),
        }
    }
    /// Cloned argument projections share canonical cursor paths, but every
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

/// Project a definition's body into the display tree the registry stores.
///
/// Called from `typst::analyze_macros`, which is why this module and that one refer to
/// each other: the registry's job is to know what a definition *expands to*, and what
/// it expands to is a display tree. Passing `session: None` is not a mode — there is no
/// session at registration, so there is nothing else it could be.
pub fn template_tree(template: &MathData, registry: &MacroRegistry) -> ViewTemplate {
    ViewTemplate::of(&Projector { registry, session: None }.view_cell(template, None, "", false))
}
impl View {
    fn glyph_run(&self) -> bool {
        match self.kind.as_str() {
            "char" | "symbol" | "number" | "text" | "stop" => true,
            "cell" | "empty-cell" | "macro-argument" | "style" => self.children.iter().all(View::glyph_run),
            _ => false,
        }
    }
    fn clone_view(&self) -> Self { self.clone() }
    /// The Typst spelling of a **projected** view, as the document has it.
    ///
    /// The projection merges view kinds (`sqrt`, `delim`, `decoration` and `line` all
    /// travel as `decorated`) and moves data into fields (`display_glyph`, `border`,
    /// `marker`), so not every node can be written back out. This answers for the ones an
    /// argument can be made of: the leaves carry their own text, a cell is its children
    /// joined the way `write_cell` joins them, and the structured nodes spell themselves
    /// the way `write_atom` does.
    ///
    /// It exists for one caller — re-reading a macro template's collapsed body once a
    /// call site has bound it — and it is deliberately **conservative**: anything it
    /// cannot spell answers `None`, which leaves that node as the collapsed image it
    /// already was rather than inventing source nobody wrote.
    pub fn spelling(&self) -> Option<String> {
        match self.kind.as_str() {
            "char" | "symbol" | "raw" => Some(self.text.clone()),
            // A run of digits and a quoted string are one cell of characters, written
            // without separators and with quotes respectively.
            "number" => Some(self.children.iter().filter_map(View::spelling).collect()),
            "text" => serde_json::to_string(&self.children.iter().filter_map(View::spelling).collect::<String>()).ok(),
            "cell" | "empty-cell" => {
                let parts: Vec<String> = self.children.iter().filter_map(View::spelling).collect();
                if parts.is_empty() { Some("\"\"".into()) } else { Some(parts.join(" ")) }
            }
            // `frac(a, b)` — the shape merges into one wire kind, and the spelling is the
            // call the writer would emit.
            "fraction" => Some(format!("frac({}, {})", self.cell_spelling(0)?, self.cell_spelling(1)?)),
            // A radical's two spellings differ by whether a degree is drawn.
            "decorated" if self.marker.as_deref() == Some("radical") => Some(format!("sqrt({})", self.cell_spelling(0)?)),
            "root" => Some(format!("root({}, {})", self.cell_spelling(0)?, self.cell_spelling(1)?)),
            "decorated" if self.marker.as_deref() == Some("delim") => {
                let (left, right) = self.text.split_once('\n')?;
                Some(format!("{left}{}{right}", self.cell_spelling(0)?))
            }
            // A mark's own name is the command that wrote it (`hat`, `cancel`).
            "decorated" if self.marker.as_deref() == Some("overline") => Some(format!("overline({})", self.cell_spelling(0)?)),
            "decorated" if self.marker.as_deref() == Some("underline") => Some(format!("underline({})", self.cell_spelling(0)?)),
            "decorated" => Some(format!("{}({})", self.marker.as_deref()?, self.cell_spelling(0)?)),
            // A font variant keeps the command and the body; `text` is already the call.
            "style" => Some(self.text.clone()),
            // A call the kernel cannot shape: `text` is its own source, holes and all.
            "raw_macro" => Some(self.text.clone()),
            "macro" => self.source_text.clone(),
            // A macro's argument is a cell by another name.
            "macro-argument" => self.children.first().and_then(View::spelling),
            _ => None,
        }
    }
    /// The spelling of one child, as a call argument: `write_cell` of an empty cell is the
    /// quoted empty string, which is how a text run spells "nothing here".
    fn cell_spelling(&self, index: usize) -> Option<String> {
        self.children.get(index).and_then(View::spelling)
    }
}

impl View {
}
