// SPDX-License-Identifier: GPL-2.0-or-later
// Translated branches from upstream/src/Cursor.cpp and
// upstream/src/mathed/InsetMathNest.cpp, InsetMathScript.cpp, InsetMathFrac.cpp.
// Authors of original algorithms are listed in docs/LYX-CREDITS.
use crate::{math::*, slots::{self, Vertical}, typst};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CommandCompletion { pub label: String, pub replacement: String, pub caret: usize }
#[derive(Serialize)]
pub struct CommandContext { pub source: String, pub start: usize, pub end: usize, pub caret: usize, pub draft: String, pub draft_caret: usize }

#[derive(Clone, Deserialize, Debug)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    State,
    SetDisplay { display: bool },
    SetDefinitions { definitions: String },
    Input { text: String },
    Key { key: String, #[serde(default)] shift: bool, #[serde(default)] ctrl: bool },
    Insert { name: String },
    Click { cursor: Cursor, #[serde(default)] shift: bool },
    Paste { text: String },
    Import { source: String },
    Complete { name: String },
    EditSource { cursor: Cursor, source: String },
    PreviewResult { source: String, definitions: String, display: bool, failed: bool },
    // The desktop learns the fate of every fragment of one render pass at once.
    PreviewResults { sources: Vec<String>, definitions: String, display: bool, failed: bool },
    LspCompletions { draft: String, caret: usize, items: Vec<CommandCompletion> },
    Geometry { stops: Vec<StopGeometry> },
    Undo, Redo, Clear,
    AddRow, AddColumn,
}
#[derive(Clone, Debug, Deserialize)]
pub struct StopGeometry { pub cursor: Cursor, pub x: f64, pub y: f64 }
// Only a hint about which Raws are worth opening their source for; dropping the
// whole set at the limit is harmless.
const FAILED_PREVIEW_LIMIT: usize = 256;#[derive(Clone)]
pub(crate) struct Snapshot { root: MathData, cursor: Cursor, anchor: Option<Cursor>, definitions: String, display: bool }

pub struct Editor {
    pub root: MathData,
    pub cursor: Cursor,
    pub anchor: Option<Cursor>,
    pub definitions: String,
    pub display: bool,
    pub message: String,
    pub completion_index: usize,
    pub revision: u64,
    pub geometry: Vec<StopGeometry>,
    target_x: Option<f64>,
    history: Vec<Snapshot>,
    future: Vec<Snapshot>,
    typing: bool,
    lsp_completions: Option<(String, usize, Vec<CommandCompletion>)>,
    // Which Raw fragments of the active formula have no rendered image. A verdict
    // belongs to one definition context, and a change discards the set: it stays
    // proportional to a single formula instead of to a session of prefixes.
    failed_previews: HashSet<String>,
    failed_context: Option<(String, bool)>,
}
impl Default for Editor {
    fn default() -> Self { Self { root: vec![], cursor: Cursor::default(), anchor: None, definitions: String::new(), display: true, message: String::new(), completion_index: 0, revision: 0, geometry: vec![], target_x: None, history: vec![], future: vec![], typing: false, lsp_completions: None, failed_previews: HashSet::new(), failed_context: None } }
}
/// The shape a new matrix starts with, which is what `mat` builds when it is typed
/// with no arguments: one row of two columns, the same as `\mat` + Enter has always
/// given. The full size a *repeating* node ends up at is the author's to grow.
const NEW_MATRIX_COLUMNS: usize = 2;
const NEW_MATRIX_ROWS: usize = 2;

/// Give a node built from a bare command name the slots its spelling promises.
///
/// `$frac()$` is a fraction with no arguments and `$mat()$` a table with none — both
/// are exactly what the source says. The editor's `frac` and `mat` mean the other
/// thing: slots to type into, which is what the factory used to build. A node the
/// parser already gave cells to is left alone, so `frac(a, b)` is never grown.
///
/// The count comes from the **shape**, so there is no second list of arities:
/// the `fraction` shape has two slots, `decoration` one, and a grid repeats its
/// one-cell pattern up to the default matrix the editor has always built. A leaf
/// has no slots, so it is left empty.
fn fill_command_cells(atom: &mut MathAtom) {
    if !atom.cells.is_empty() { return; }
    let shape = atom.shape();
    let want = match shape.arity {
        slots::Arity::Exact => shape.slots.len(),
        slots::Arity::Repeat => NEW_MATRIX_COLUMNS * NEW_MATRIX_ROWS,
    };
    for _ in 0..want { atom.cells.push(vec![]); }
}

impl Editor {
    pub(crate) fn snapshot(&self) -> Snapshot { Snapshot { root: self.root.clone(), cursor: self.cursor.clone(), anchor: self.anchor.clone(), definitions: self.definitions.clone(), display: self.display } }
    pub(crate) fn restore(&mut self, s: Snapshot) { self.root = s.root; self.cursor = s.cursor; self.anchor = s.anchor; self.definitions = s.definitions; self.display = s.display; }
    pub fn can_undo(&self) -> bool { !self.history.is_empty() }
    pub fn can_redo(&self) -> bool { !self.future.is_empty() }
    // Adopting a definition set always replaces the whole projected tree, so the
    // cursor cannot survive it: the structural paths it points at are re-derived.
    fn adopt(&mut self, parsed: typst::Parsed) {
        self.root = parsed.root; self.definitions = parsed.definitions;
        self.cursor = Cursor::default(); self.anchor = None; self.geometry.clear();
        self.lsp_completions = None;
    }
    // Reclassify the current cell against the current definitions. A MacroCall
    // must never outlive the arity it was parsed with.
    fn rebind(&mut self) -> Result<(), String> {
        let parsed = typst::parse_document(&typst::write_document_mode(&self.root, &self.definitions, self.display))?;
        self.adopt(parsed);
        Ok(())
    }
    // A #let confirmed inside a formula arrives mid-edit. Route it through the
    // same reparse as an explicit definition update instead of leaving the cell
    // parsed against the previous registry.
    fn refresh_definitions(&mut self, previous: &str) {
        if self.definitions == previous { return; }
        if let Err(error) = self.rebind() { self.message = error; }
    }
    fn data(&self) -> &MathData { cell(&self.root, &self.cursor.slices) }
    fn data_mut(&mut self) -> &mut MathData { cell_mut(&mut self.root, &self.cursor.slices) }
    fn owner(&self) -> Option<&MathAtom> {
        let last = self.cursor.slices.last()?;
        Some(&cell(&self.root, &self.cursor.slices[..self.cursor.slices.len()-1])[last.atom])
    }
    fn owner_mut(&mut self) -> Option<&mut MathAtom> {
        let last = self.cursor.slices.last()?.clone();
        let path = self.cursor.slices[..self.cursor.slices.len()-1].to_vec();
        Some(&mut cell_mut(&mut self.root, &path)[last.atom])
    }
    pub fn pending(&self) -> Option<&str> {
        let atom = self.cursor.pos.checked_sub(1).and_then(|p| self.data().get(p))?;
        if let Kind::Unknown { name, .. } = &atom.kind { Some(name) } else { None }
    }
    // A draft opened from a Raw fragment carries that fragment as its source, and
    // its spelling is the authority once the draft is committed.
    fn editing_source(&self) -> bool {
        self.cursor.pos.checked_sub(1).and_then(|p| self.data().get(p))
            .is_some_and(|atom| matches!(&atom.kind, Kind::Unknown { original: Some(_), .. }))
    }
    // Rendering state is transient and independent of source and undo: it only
    // decides whether a Raw opens its source on a boundary key.
    fn record_preview(&mut self, definitions: &str, display: bool, sources: impl IntoIterator<Item=String>, failed: bool) {
        if self.failed_context.as_ref().is_none_or(|(context,shown)| context != definitions || *shown != display) {
            self.failed_previews.clear();
            self.failed_context = Some((definitions.to_owned(), display));
        }
        for source in sources {
            if failed {
                if self.failed_previews.len() >= FAILED_PREVIEW_LIMIT { self.failed_previews.clear(); }
                self.failed_previews.insert(source);
            } else { self.failed_previews.remove(&source); }
        }
    }
    fn preview_failed(&self, source: &str) -> bool {
        self.failed_context.as_ref().is_some_and(|(context,shown)| context == &self.definitions && *shown == self.display)
            && self.failed_previews.contains(source)
    }
    pub fn completions(&self) -> Vec<String> {
        if self.string_mode() { return vec![]; }
        if let Some(items) = self.current_lsp_completions() { return items.iter().map(|i| i.label.clone()).collect(); }
        let Some(prefix) = self.pending() else { return vec![]; };
        let prefix = prefix.trim();
        if !prefix.chars().all(|c| c.is_alphanumeric() || c == '.') { return vec![]; }
        let registry = typst::macro_registry(&self.definitions);
        let mut names: Vec<_> = slots::command_names().into_iter().map(str::to_string)
            .chain(registry.entries.iter().filter(|d| d.expandable && !d.shadowed).map(|d| d.name.clone()))
            .filter(|n| n.starts_with(prefix)).collect();
        names.sort(); names.dedup(); names
    }
    pub fn apply(&mut self, action: Action) -> Result<(), String> {
        if let Action::Geometry { stops } = action {
            // Geometry is measured by a frontend and can be stale after an undo
            // or a definition change. It is a hint for the caret, never a
            // cursor source, so drop every stop that is not valid right now.
            self.geometry = stops.into_iter().filter(|stop| valid(&self.root, &stop.cursor)).collect();
            return Ok(());
        }
        if let Action::PreviewResult { source, definitions, display, failed } = action {
            self.record_preview(&definitions, display, [source], failed);
            return Ok(());
        }
        if let Action::PreviewResults { sources, definitions, display, failed } = action {
            self.record_preview(&definitions, display, sources, failed);
            return Ok(());
        }
        if let Action::LspCompletions { draft, caret, items } = action {
            if self.pending() == Some(draft.as_str()) && self.draft_caret() == Some(caret) {
                self.lsp_completions = Some((draft, caret, items)); self.completion_index = 0;
            }
            return Ok(());
        }
        let before = self.snapshot();
        self.message.clear();
        let is_typing = matches!(&action, Action::Input { text } if !text.contains([' ', '\n']) );
        match action {
            Action::State => return Ok(()),
            Action::SetDisplay { display } => { self.display = display; }
            Action::SetDefinitions { definitions } => {
                if self.pending().is_some() { return Err("请先确认或取消公式中的命令草稿，再应用宏定义。".into()); }
                typst::validate_definitions(&definitions)?;
                // Reclassify calls only when the definition context changes.
                // Serialize first, so removing/changing a definition cannot lose arguments.
                if definitions != self.definitions {
                    let parsed = typst::parse_document(&typst::write_document_mode(&self.root, &definitions, self.display))?;
                    self.adopt(parsed);
                }
            }
            Action::Undo => {
                if let Some(s) = self.history.pop() { self.future.push(before); self.restore(s); }
                self.typing = false; self.revision += 1; return Ok(());
            }
            Action::Redo => {
                if let Some(s) = self.future.pop() { self.history.push(before); self.restore(s); }
                self.typing = false; self.revision += 1; return Ok(());
            }
            Action::Input { text } => { for ch in text.chars() { self.interpret_char(ch); } }
            Action::Key { key, shift, ctrl } => self.key(&key, shift, ctrl),
            Action::Insert { name } => { if self.pending().is_some() { self.fill_command(name); } else { self.nice_insert(&name); } }
            Action::Complete { name } => { self.accept_completion(&name); }
            Action::EditSource { cursor, source } => {
                let caret = source.len(); self.open_source(cursor, source, caret);
            }
            Action::Click { cursor, shift } => {
                if self.pending().is_some() { self.message = "请按 Enter 确认命令，或 Esc 取消".into(); }
                else if valid(&self.root, &cursor) {
                    if shift { self.anchor.get_or_insert(self.cursor.clone()); } else { self.anchor = None; }
                    self.cursor = cursor; self.reduce_selection(); self.target_x = None;
                }
            }
            Action::Paste { text } => {
                if self.pending().is_none() { self.begin_command(); }
                for c in text.chars() { self.interpret_char(c); }
            }
            Action::Import { source } => {
                let parsed = typst::parse_document(&source)?;
                self.root = parsed.root; self.definitions = parsed.definitions; self.display = parsed.display; self.cursor = Cursor::default(); self.anchor = None;
            }
            Action::Clear => { self.root.clear(); self.cursor = Cursor::default(); self.anchor = None; }
            Action::AddRow => self.grow_grid(false),
            Action::AddColumn => self.grow_grid(true),
            Action::Geometry { .. } | Action::LspCompletions { .. } | Action::PreviewResult { .. } | Action::PreviewResults { .. } => unreachable!(),
        }
        let typed_before = self.typing;
        let mut pushed = false;
        let mut future_before = None;
        if self.root != before.root || self.definitions != before.definitions || self.display != before.display {
            if !is_typing || !self.typing { self.history.push(before.clone()); pushed = true; }
            future_before = Some(std::mem::take(&mut self.future));
        }
        self.typing = is_typing;
        self.revision += 1;
        if !valid(&self.root, &self.cursor) {
            // A rejected step must not leave a cursor whose next keystroke would
            // index a cell that no longer exists: drop the step, its history
            // entry and its effects on the redo stack.
            if pushed { self.history.pop(); }
            if let Some(future) = future_before { self.future = future; }
            self.restore(before);
            self.typing = typed_before;
            return Err("光标结构不一致".into());
        }
        if pushed && self.history.len() > 200 { self.history.remove(0); }
        Ok(())
    }
    // CutAndPaste reduceSelectionToOneCell: crossing a nest selects its atom.
    fn reduce_selection(&mut self) {
        let Some(anchor) = &mut self.anchor else { return; };
        if anchor.slices == self.cursor.slices { return; }
        let common = anchor.slices.iter().zip(&self.cursor.slices).take_while(|(a,b)| a == b).count();
        let a = anchor.slices.get(common).map(|s| s.atom).unwrap_or(anchor.pos);
        let b = self.cursor.slices.get(common).map(|s| s.atom).unwrap_or(self.cursor.pos);
        let left = a.min(b);
        let right = if a == b || self.cursor.slices.len() > common || anchor.slices.len() > common { a.max(b) + 1 } else { a.max(b) };
        let slices = self.cursor.slices[..common].to_vec();
        // One side may name an atom while the other names a position, so the
        // widened end can pass the cell it lives in. A selection never may.
        let length = cell(&self.root, &slices).len();
        let right = right.min(length);
        *anchor = Cursor { slices: slices.clone(), pos: left, occurrence: String::new() };
        self.cursor = Cursor { slices, pos: right, occurrence: String::new() };
    }
    pub fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor.as_ref()?;
        if a.slices != self.cursor.slices || a.pos == self.cursor.pos { return None; }
        Some((a.pos.min(self.cursor.pos), a.pos.max(self.cursor.pos)))
    }
    fn take_selection(&mut self) -> MathData {
        self.reduce_selection();
        let Some((a,b)) = self.selection() else { self.anchor = None; return vec![]; };
        let data = self.data_mut().drain(a..b).collect(); self.cursor.pos = a; self.anchor = None; data
    }
    fn erase_selection(&mut self) -> bool { let selected = self.selection().is_some(); self.take_selection(); selected }
    fn plain_insert(&mut self, atom: MathAtom) { let pos = self.cursor.pos; self.data_mut().insert(pos, atom); self.cursor.pos += 1; }
    fn push(&mut self, atom: usize, idx: usize, end: bool) {
        self.cursor.slices.push(CursorSlice { atom, cell: idx });
        self.cursor.pos = if end { self.data().len() } else { 0 };
    }
    fn pop(&mut self, forward: bool) -> bool {
        if let Some(slice) = self.cursor.slices.pop() {
            self.cursor.pos = slice.atom + usize::from(forward); true
        } else { self.message = "已到公式边界".into(); false }
    }
    // Typst command drafts keep punctuation and whitespace until explicit Enter.
    fn interpret_char(&mut self, ch: char) {
        self.target_x = None;
        if self.pending().is_some() {
            let pos = self.cursor.pos - 1;
            if let Kind::Unknown { name, caret, anchor, .. } = &mut self.data_mut()[pos].kind {
                if let Some(a) = anchor.take() { let start = a.min(*caret); name.replace_range(start..a.max(*caret), ""); *caret = start; }
                name.insert(*caret, ch); *caret += ch.len_utf8();
            }
            self.completion_index = 0; return;
        }
        if self.text_cell() {
            if ch == '"' || ch == '\n' { self.anchor = None; self.pop(true); return; }
            self.erase_selection(); self.plain_insert(MathAtom::character(ch)); return;
        }
        if self.number_cell() {
            // A run holds digits, so a digit goes in at the caret and anything else
            // ends the run there. That is the same character handling a text run
            // gets, minus the freedom to hold something that is not a digit.
            if ch.is_ascii_digit() { self.erase_selection(); self.plain_insert(MathAtom::character(ch)); return; }
            self.leave_number();
        }
        if ch == '"' {
            let saved = self.take_selection(); let pos = self.cursor.pos;
            self.plain_insert(MathAtom { kind: Kind::Text, cells: vec![saved] });
            self.push(pos, 0, true); return;
        }
        if ch == '\\' {
            self.begin_command(); return;
        }
        if ch == '/' {
            let mut numerator = self.take_selection();
            if numerator.is_empty() && self.cursor.pos > 0 {
                self.cursor.pos -= 1; let pos = self.cursor.pos;
                numerator.push(self.data_mut().remove(pos));
            }
            let pos = self.cursor.pos;
            self.plain_insert(MathAtom {kind:Kind::Fraction,cells:vec![numerator,vec![]]});
            self.push(pos, 1, false); return;
        }
        if ch == ' ' {
            if self.selection().is_some() { self.anchor = None; } else { self.pop(true); }
            return;
        }
        if ch == '\n' { return; }
        if ch == '^' || ch == '_' { self.script(ch == '^'); return; }
        self.erase_selection();
        if ch.is_ascii_digit() { self.insert_digit(ch); return; }
        self.plain_insert(MathAtom::character(ch));
    }
    /// A digit joins the number run beside the caret instead of becoming loose.
    ///
    /// The lexer groups a run into one token, so the tree agrees with the source
    /// only while the run stays one `Number`. Both sides are as close, so the left
    /// one wins; either way the caret ends up *inside* the run, right after the
    /// digit that was just typed, which is what makes `|456` plus `9` read `9456`
    /// with the caret between the `9` and the `4` rather than in front of the `9`.
    fn insert_digit(&mut self, ch: char) {
        let pos = self.cursor.pos;
        if pos > 0 && matches!(self.data()[pos - 1].kind, Kind::Number) { self.push(pos - 1, 0, true); }
        else if matches!(self.data().get(pos).map(|a| &a.kind), Some(Kind::Number)) { self.push(pos, 0, false); }
        else {
            let at = self.cursor.pos;
            self.plain_insert(MathAtom::number(""));
            self.push(at, 0, true);
        }
        self.plain_insert(MathAtom::character(ch));
    }
    /// Ends the run the caret is in by splitting it, then leaves for the parent.
    ///
    /// Called for anything that is not a digit. Splitting keeps the caret where the
    /// user put it and keeps both halves valid runs, so the character that ended the
    /// run is handled at the formula level with its usual meaning (`/` opens a
    /// fraction, `^` an attachment, `"` a text run).
    fn leave_number(&mut self) {
        let Some(slice) = self.cursor.slices.last().cloned() else { return };
        let digits = self.data().clone();
        let (left, right) = digits.split_at(self.cursor.pos);
        let mut replacement = vec![];
        if !left.is_empty() { replacement.push(MathAtom { kind: Kind::Number, cells: vec![left.to_vec()] }); }
        if !right.is_empty() { replacement.push(MathAtom { kind: Kind::Number, cells: vec![right.to_vec()] }); }
        let caret = slice.atom + usize::from(!left.is_empty());
        self.cursor.slices.pop();
        self.data_mut().splice(slice.atom..slice.atom + 1, replacement);
        self.cursor.pos = caret;
    }
    /// A run with no digits left is removed, never written as an empty atom.
    ///
    /// `write_cell` puts one separator between two atoms, so an empty run would
    /// write a stray separator and read back as a different tree.
    fn dissolve_empty_run(&mut self) {
        if !self.data().is_empty() || !matches!(self.owner().map(|o| &o.kind), Some(Kind::Number)) { return; }
        self.pop(false);
        let at = self.cursor.pos;
        self.data_mut().remove(at);
    }
    fn begin_command(&mut self) {
        let saved = self.take_selection();
        self.plain_insert(MathAtom { kind: Kind::Unknown { name: String::new(), saved, caret: 0, anchor: None, original: None }, cells: vec![] });
        self.lsp_completions = None; self.completion_index = 0;
    }
    // Shared source-block entry for the edit button and horizontal navigation.
    fn open_source(&mut self, cursor: Cursor, source: String, caret: usize) {
        if self.pending().is_some() || !valid(&self.root, &cursor) || !source.is_char_boundary(caret) { return; }
        if !cell(&self.root, &cursor.slices).get(cursor.pos).is_some_and(|a| matches!(&a.kind, Kind::Raw {source:s} if s == &source)) { return; }
        self.cursor = cursor; self.anchor = None; let pos = self.cursor.pos;
        self.data_mut()[pos] = MathAtom {kind:Kind::Unknown {name:source.clone(),saved:vec![],caret,anchor:None,original:Some(source)},cells:vec![]};
        self.cursor.pos += 1; self.lsp_completions = None; self.completion_index = 0;
    }
    // Cursor::macroModeClose. The backslash is an input gesture, never serialized.
    fn close_command(&mut self, cancel: bool, completion: Option<String>) -> bool {
        let Some(draft) = self.pending() else { return false; };
        // Asked before the draft is removed: an unparseable edit of a Raw fragment
        // is committed as that fragment's source instead of being refused.
        let from_source = self.editing_source();
        let previous_definitions = self.definitions.clone();
        let name = completion.clone().unwrap_or_else(|| draft.trim().to_string());
        let name = if name == "/" { "frac".to_string() } else { name };
        // A command *name* typed on its own is read as the call it spells, so `\frac`
        // + Enter builds a fraction rather than an identifier called `frac`. The test
        // is `trim`ped but the draft may not carry anything else: `frac ` is a name
        // with a space, while `frac(a, b)` and `alpha + beta` are bodies and are
        // parsed exactly as written — which is what keeps a half-typed body
        // (`frac(a, b`) an error instead of a silent loss. A completion replaces the
        // whole draft with a name, so it counts as one.
        let bare_name = (completion.is_some() || draft.trim() == name || draft.trim() == "/")
            && self.is_callable(&name);
        let parsed = if cancel || name.is_empty() { None } else {
            let text = if bare_name { typst::parse_command_invocation(&name, &self.definitions) }
                else { typst::parse_command(&name, &self.definitions) };            match text {
                Ok(doc) => Some(doc),
                Err(error) => { self.message = format!("公式尚未完成：{error}"); return true; }
            }
        };
        let pos = self.cursor.pos - 1;
        let atom = self.data_mut().remove(pos);
        self.cursor.pos = pos;
        let Kind::Unknown { saved, original, .. } = atom.kind else { unreachable!() };
        self.lsp_completions = None;
        if cancel {
            if let Some(source) = original { self.plain_insert(MathAtom::raw(source)); }
            else { let n=saved.len(); self.data_mut().splice(pos..pos,saved); self.cursor.pos+=n; }
            return true;
        }
        if name.is_empty() { return true; }
        if let Some(doc) = parsed {
            let mut data = doc.root;
            self.definitions = doc.definitions;
            if self.text_cell() {
                let text = data.iter().map(|a| if matches!(a.kind, Kind::Text) {
                    a.cells[0].iter().map(typst::write_atom).collect::<String>()
                } else { typst::write_atom(a) }).collect::<Vec<_>>().join(" ");
                data = text.chars().map(MathAtom::character).collect();
                let n = data.len(); self.data_mut().splice(pos..pos, data); self.cursor.pos += n;
            } else {
                // A node the parser built has to be *writable*, and that is the one
                // thing the parser cannot know: `$frac()$` really resolves to a
                // fraction with no cells, so writing it back would emit `frac()`
                // instead of the two slots the editor's `frac` means — and worse,
                // `write_atom` would put the template's own `{0}` into the document.
                // Filling the cells is what the removed factory did, kept as the one
                // place a parsed answer becomes an editable node.
                //
                // It runs for every draft, not only a bare name: `frac()` and
                // `frac(a, b)` parse to the same kind, and only the first needs cells
                // added. `fill_command_cells` leaves a node that already has them
                // alone, so this is safe for both.
                for atom in &mut data { fill_command_cells(atom); }
                let n = data.len(); self.data_mut().splice(pos..pos, data); self.cursor.pos += n;
                // A lone command node is entered at its first slot, the way a
                // factory-built one used to be, so `\frac` + Enter leaves the caret
                // in the numerator. A longer draft leaves the caret after it.
                if bare_name && n == 1 && !self.data()[pos].cells.is_empty() {
                    let entry = self.data()[pos].entry_cell(true);
                    self.push(pos, entry, false);
                }
            }
        } else if from_source {
            // The draft was opened from a Raw fragment and does not parse as a
            // formula: keep the edited text as that fragment's source, because the
            // source is authoritative and refusing would lose the repair.
            self.plain_insert(MathAtom::raw(name));
        } else { self.insert_named(&name, saved); }
        self.refresh_definitions(&previous_definitions);
        true
    }
    fn fill_command(&mut self, replacement: String) {
        if self.pending().is_none() { return; }
        let pos = self.cursor.pos - 1;
        if let Kind::Unknown { name, caret, anchor, .. } = &mut self.data_mut()[pos].kind {
            *caret = replacement.len(); *name = replacement; *anchor = None;
        }
        self.completion_index = 0;
    }
    fn draft_caret(&self) -> Option<usize> {
        let p = self.cursor.pos.checked_sub(1)?;
        if let Kind::Unknown { caret, .. } = self.data().get(p)?.kind { Some(caret) } else { None }
    }
    fn current_lsp_completions(&self) -> Option<&Vec<CommandCompletion>> {
        let (draft, caret, items) = self.lsp_completions.as_ref()?;
        (self.pending() == Some(draft.as_str()) && self.draft_caret() == Some(*caret)).then_some(items)
    }
    fn accept_completion(&mut self, label: &str) {
        let item = self.current_lsp_completions().and_then(|items| items.iter().find(|i| i.label == label)).cloned();
        if let Some(item) = item {
            if item.caret <= item.replacement.len() && item.replacement.is_char_boundary(item.caret) {
                self.fill_command(item.replacement);
                let p = self.cursor.pos-1;
                if let Kind::Unknown { caret, .. } = &mut self.data_mut()[p].kind { *caret = item.caret; }
            }
        } else { self.fill_command(label.into()); }
        self.lsp_completions = None;
    }
    pub fn command_context(&self) -> Option<CommandContext> {
        let draft = self.pending()?.to_string(); let caret = self.draft_caret()?;
        let mut root = self.root.clone();
        // Find the draft insertion point in the serialized source.
        let mut marker = "visualtypstdraftmarker".to_string();
        let source_before = typst::write_document_mode(&root, &self.definitions, self.display);
        while source_before.contains(&marker) { marker.push('x'); }
        cell_mut(&mut root, &self.cursor.slices)[self.cursor.pos-1] = MathAtom::raw(&marker);
        let mut source = typst::write_document_mode(&root, &self.definitions, self.display);
        let start = source.find(&marker)?; source.replace_range(start..start+marker.len(), &draft);
        Some(CommandContext { source, start, end: start+draft.len(), caret: start+caret, draft, draft_caret: caret })
    }
    pub(crate) fn text_cell(&self) -> bool { matches!(self.owner().map(|o| &o.kind),Some(Kind::Text)) }
    /// Whether the caret is inside a number run, whose cell takes digits and
    /// nothing else. Unlike a text run it does **not** trap the arrow keys: a run
    /// is something the reader passes through, so `key` leaves the boundaries to
    /// the ordinary cell handling and left/right walk out of it.
    fn number_cell(&self) -> bool { matches!(self.owner().map(|o| &o.kind), Some(Kind::Number)) }
    fn quoted_draft(&self) -> bool {
        let Some(name) = self.pending() else { return false; };
        let mut quoted = false; let mut escaped = false;
        for ch in name[..self.draft_caret().unwrap()].chars() {
            if escaped { escaped=false; } else if ch == '\\' { escaped=true; } else if ch == '"' { quoted=!quoted; }
        }
        quoted
    }
    pub fn string_mode(&self) -> bool { self.text_cell() || self.quoted_draft() }
    pub fn draft_selection(&self) -> Option<&str> {
        let pos = self.cursor.pos.checked_sub(1)?;
        if let Kind::Unknown { name, caret, anchor: Some(a), .. } = &self.data()[pos].kind {
            return Some(&name[(*a).min(*caret)..(*a).max(*caret)]);
        }
        None
    }
    fn draft_key(&mut self, key: &str, shift: bool, ctrl: bool) {
        let list = self.completions();
        match key {
            "Enter" => {
                if self.quoted_draft() { self.interpret_char('"'); return; }
                // Built-in exact names still create empty, editable LyX slots; a
                // macro name comes from the completion list instead.
                let exact = self.pending().map(str::trim).is_some_and(|s| self.is_command_name(s))
                    || list.iter().any(|name| Some(name.as_str()) == self.pending().map(str::trim));
                let single_name = self.pending().is_some_and(|s| !s.trim().is_empty() && s.trim().chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_'));
                if !exact && single_name {
                    if let Some(label) = list.get(self.completion_index) { self.accept_completion(label); }
                }
                self.close_command(false, None); return;
            }
            "Escape" => { self.close_command(true, None); return; }
            "ArrowDown" | "ArrowUp" => {
                if !list.is_empty() { self.completion_index = (self.completion_index + if key == "ArrowDown" { 1 } else { list.len()-1 }) % list.len(); }
                return;
            }
            "Tab" => {
                if self.current_lsp_completions().is_some() { if let Some(label) = list.get(self.completion_index) { self.accept_completion(label); } return; }
                if let Some(first) = list.first() {
                    let common = first.chars().enumerate().take_while(|(i,c)| list.iter().all(|s| s.chars().nth(*i) == Some(*c))).map(|(_,c)| c).collect();
                    self.fill_command(common);
                }
                return;
            }
            " " => { if !ctrl { self.interpret_char(' '); } return; }
            _ => {}
        }
        let pos = self.cursor.pos - 1;
        let Kind::Unknown { name, caret, anchor, .. } = &mut self.data_mut()[pos].kind else { unreachable!() };
        if ctrl && key.eq_ignore_ascii_case("a") { *anchor = Some(0); *caret = name.len(); return; }
        let left = |s: &str, p: usize| s[..p].char_indices().next_back().map_or(0, |(i,_)| i);
        let right = |s: &str, p: usize| p + s[p..].chars().next().map_or(0, char::len_utf8);
        match key {
            "ArrowLeft" | "ArrowRight" | "Home" | "End" => {
                let old = *caret;
                let forward = key == "ArrowRight";
                let mut next = match key {
                    "Home" => 0, "End" => name.len(),
                    _ if !shift && anchor.is_some_and(|a| a != old) => if forward { old.max(anchor.unwrap()) } else { old.min(anchor.unwrap()) },
                    _ if forward => right(name, old), _ => left(name, old),
                };
                if ctrl && matches!(key, "ArrowLeft" | "ArrowRight") {
                    if forward { while next < name.len() && !name[next..].starts_with(char::is_whitespace) { next = right(name, next); } }
                    else { while next > 0 && !name[..next].ends_with(char::is_whitespace) { next = left(name, next); } }
                }
                if shift { anchor.get_or_insert(old); } else { *anchor = None; }
                *caret = next;
            }
            "Backspace" | "Delete" => {
                let (a,b) = if let Some(a) = anchor.take().filter(|a| *a != *caret) { (a.min(*caret), a.max(*caret)) }
                    else if key == "Backspace" { (left(name, *caret), *caret) } else { (*caret, right(name, *caret)) };
                name.replace_range(a..b, ""); *caret = a;
            }
            _ => {}
        }
        self.completion_index = 0;
    }
    /// Was this draft a command name on its own? Only used to decide whether a
    /// half-typed name may be replaced by the highlighted completion.
    fn is_command_name(&self, name: &str) -> bool {
        slots::command_names().contains(&name)
    }
    /// Whether a name on its own should be read as the call it spells.
    ///
    /// True for a built-in command and for a macro the definitions bind, because both
    /// name a node the editor can put slots in; a variable or an unknown name is left
    /// as the text it is.
    fn is_callable(&self, name: &str) -> bool {
        self.is_command_name(name)
            || typst::macro_registry(&self.definitions).get(name).is_some_and(|def| def.expandable)
    }
    fn nice_insert(&mut self, name: &str) {
        if name == "sup" || name == "sub" { self.script(name == "sup"); return; }
        if name == "()" {
            let saved = self.take_selection(); let pos = self.cursor.pos;
            self.plain_insert(MathAtom { kind: Kind::Fenced {left:"(".into(),right:")".into()}, cells:vec![saved] });
            self.push(pos, 0, false); return;
        }
        let saved = self.take_selection(); self.insert_named(name, saved);
    }
    /// Put a command in, by handing its spelling to the parser.
    ///
    /// There is no second opinion here about what a command builds: `parse_command`
    /// reads `frac`/`sqrt`/`overline`/`hat`/`mat` … through the same table the
    /// document goes through, so a command typed in the editor and the same text
    /// read from the file cannot disagree.
    ///
    /// One thing is filled in behind the parser's back, and only one: a node whose
    /// own spelling promises a placeholder needs that cell to exist, because
    /// `$frac()$` really resolves to a fraction with no cells and writing it back
    /// would emit `frac()` instead of the empty slots the author asked for. See
    /// `fill_command_cells`.
    fn insert_named(&mut self, name: &str, saved: MathData) {
        let pos = self.cursor.pos;
        // A command with no arguments of its own is read as the call it spells, so
        // `mat` builds a matrix rather than an identifier called `mat`.
        let invocation = typst::parse_command_invocation(name, &self.definitions);
        match invocation {
            Ok(doc) => {
                let previous = self.definitions.clone();
                self.definitions = doc.definitions;
                let mut data = doc.root;
                for atom in &mut data { fill_command_cells(atom); }
                // The caret enters the node when it has a first slot to sit in, which
                // is what makes a fresh `\frac` land in the numerator whether the
                // parser supplied the cells (`frac`) or `fill_command_cells` did.
                let active = data.len() == 1 && data[0].active();
                if active { data[0].cells[0] = saved; }
                let n = data.len();
                self.data_mut().splice(pos..pos, data);
                self.cursor.pos += if active { 1 } else { n };
                if active {
                    let entry = self.data()[pos].entry_cell(true);
                    self.push(pos, entry, false);
                }
                self.refresh_definitions(&previous);
            }
            Err(error) => {
                let n = saved.len(); self.data_mut().splice(pos..pos, saved); self.cursor.pos += n;
                self.message = error;
            }
        }
    }
    // InsetMathNest::script: selection becomes SCRIPT CONTENT, not the base.
    fn script(&mut self, up: bool) {
        let saved = self.take_selection();
        let in_nucleus = matches!(self.owner().map(|o| &o.kind), Some(Kind::Scripts)) && self.cursor.slices.last().unwrap().cell == 0;
        if in_nucleus {
            // The storage is fixed, so entering an attachment is just naming its
            // cell rather than creating it.
            self.cursor.slices.last_mut().unwrap().cell = script_cell(up); self.cursor.pos = 0;
        } else if self.cursor.pos > 0 && matches!(self.data()[self.cursor.pos - 1].kind, Kind::Scripts) {
            let pos = self.cursor.pos - 1;
            self.push(pos, script_cell(up), true);
        } else {
            let pos = if self.cursor.pos == 0 { self.plain_insert(MathAtom::script(vec![])); 0 }
                else { let p = self.cursor.pos - 1; let old = self.data_mut().remove(p); self.data_mut().insert(p, MathAtom::script(vec![old])); p };
            self.push(pos, script_cell(up), false);
        }
        let pos = self.cursor.pos; let count = saved.len(); self.data_mut().splice(pos..pos, saved); self.cursor.pos += count;
    }
    fn move_horizontal(&mut self, forward: bool, shift: bool) {
        if self.pending().is_some() { return; }
        self.target_x = None;
        if shift { self.anchor.get_or_insert(self.cursor.clone()); } else { self.anchor = None; }
        let len = self.data().len();
        if forward && self.cursor.pos < len || !forward && self.cursor.pos > 0 {
            let p = if forward { self.cursor.pos } else { self.cursor.pos - 1 };
            if !shift {
                if let Kind::Raw {source} = &self.data()[p].kind {
                    if self.preview_failed(source) {
                        let source = source.clone();
                        let caret = if forward { 0 } else { source.len() };
                        self.open_source(Cursor {pos:p,..self.cursor.clone()}, source, caret); return;
                    }
                }
            }
            if !shift && self.data()[p].active() {
                let idx = self.data()[p].entry_cell(forward);
                self.push(p, idx, !forward);
            } else if forward { self.cursor.pos += 1; } else { self.cursor.pos -= 1; }
        } else if let Some(owner) = self.owner() {
            let idx = self.cursor.slices.last().unwrap().cell;
            if !shift && let Some(next) = owner.idx_horizontal(idx, forward) {
                // Which shapes land at the END of the cell they are entered backward
                // into: a radical (its degree is drawn to the left, so the radicand is
                // met from its right) and a grid (a row is met from its last column).
                // The radical is asked of the **shape**, because a `root(...)` call is
                // stored as a `MacroCall` and only the command file says it is a radical
                // at all — testing `Kind::Root` here silently changed where the caret landed.
                let radical = owner.command_shape().is_some_and(|descriptor| descriptor.shape().is_radical());
                let root_back = !forward && (radical || matches!(owner.kind, Kind::Table { .. } | Kind::Multiline { .. }));
                self.cursor.slices.last_mut().unwrap().cell = next; self.cursor.pos = if root_back { self.data().len() } else { 0 };
            } else { self.pop(forward); }
        }
        if shift { self.reduce_selection(); }
    }
    // InsetMathNest::LFUN_CELL_FORWARD/BACKWARD wraps multi-cell insets.
    fn tab(&mut self, forward: bool) {
        if self.pending().is_some() { return; }
        self.anchor = None; self.target_x = None;
        let Some(owner) = self.owner() else { self.message = "当前已在公式最外层".into(); return; };
        let n = owner.cells.len();
        let first = owner.entry_cell(true);
        if n == 1 { self.pop(forward); }
        else {
            let s = self.cursor.slices.last_mut().unwrap(); s.cell = if forward { if s.cell + 1 == n { first } else { s.cell+1 } } else { (s.cell + n - 1) % n }; self.cursor.pos = 0;
        }
    }
    // Cursor::mathForward/mathBackward(word=true), grouping by math class.
    fn move_word(&mut self, forward: bool, shift: bool) {
        if self.pending().is_some() { return; }
        if forward && self.cursor.pos == self.data().len() || !forward && self.cursor.pos == 0 { self.move_horizontal(forward, shift); return; }
        if shift { self.anchor.get_or_insert(self.cursor.clone()); } else { self.anchor = None; }
        self.target_x = None;
        if forward {
            let mc = self.data()[self.cursor.pos].math_class();
            while self.cursor.pos < self.data().len() && self.data()[self.cursor.pos].math_class() == mc { self.cursor.pos += 1; }
            if self.cursor.pos < self.data().len() {
                let next = self.data()[self.cursor.pos].math_class();
                if (1..=3).contains(&next) { while self.cursor.pos < self.data().len() && self.data()[self.cursor.pos].math_class() == next { self.cursor.pos += 1; } }
            }
        } else {
            let mc = self.data()[self.cursor.pos-1].math_class();
            while self.cursor.pos > 0 && self.data()[self.cursor.pos-1].math_class() == mc { self.cursor.pos -= 1; }
            if self.cursor.pos > 0 && (1..=3).contains(&mc) {
                let next = self.data()[self.cursor.pos-1].math_class();
                while self.cursor.pos > 0 && self.data()[self.cursor.pos-1].math_class() == next { self.cursor.pos -= 1; }
            }
        }
    }
    fn vertical(&mut self, up: bool, shift: bool) {
        if self.pending().is_some() { return; }
        if shift { self.anchor.get_or_insert(self.cursor.clone()); } else { self.anchor = None; }
        let original = self.cursor.clone();
        let x = self.target_x.or_else(|| self.geometry.iter().find(|s| s.cursor == original).map(|s| s.x));
        self.target_x = x;
        loop {
            let Some(owner) = self.owner() else { self.cursor = original; break; };
            let idx = self.cursor.slices.last().unwrap().cell;
            let mut target = None;
            let mut end = false;
            let shape = owner.shape();
            match shape.vertical {
                Vertical::Swap { up: up_role, down: down_role, end_up } => {
                    if let Some(t) = shape.index_of(if up { up_role } else { down_role }) {
                        if idx != t { target = Some(t); end = up && end_up; }
                    }
                }
                // An attachment keeps its own rules: a script is only reachable
                // from the end of the base, and it returns to the base's start.
                Vertical::Attach => {
                    if idx == 0 && self.cursor.pos == self.data().len() { target = owner.script_idx(up); }
                    else if owner.script_idx(true) == Some(idx) && !up || owner.script_idx(false) == Some(idx) && up { target = Some(0); end = true; }
                }
                Vertical::Column => {
                    let columns = owner.columns();
                    if up { target = idx.checked_sub(columns); }
                    else if idx + columns < owner.cells.len() { target = Some(idx + columns); }
                }
                Vertical::None => {},
            }
            if let Some(target) = target {
                self.cursor.slices.last_mut().unwrap().cell = target;
                let candidates: Vec<_> = self.geometry.iter().filter(|s| s.cursor.slices == self.cursor.slices).collect();
                self.cursor.pos = if end { self.data().len() } else if let (Some(x), Some(best)) = (x, candidates.iter().min_by(|a,b| (a.x-x.unwrap_or(0.0)).abs().total_cmp(&(b.x-x.unwrap_or(0.0)).abs()))) {
                    // The stop's own position is a measurement hint; clamp it to
                    // the cell it was measured in.
                    let _ = x; best.cursor.pos.min(self.data().len())
                } else { original.pos.min(self.data().len()) };
                break;
            }
            if !self.pop(false) { self.cursor = original; break; }
        }
        if shift { self.reduce_selection(); }
    }
    // Cursor::backspace / pullArg: at cell start, dissolve the parent and retain
    // ONLY the current argument (not all sibling cells).
    fn backspace(&mut self) {
        if self.pending().is_some() { self.draft_key("Backspace", false, false); return; }
        if self.erase_selection() { return; }
        if self.cursor.pos == 0 {
            if self.cursor.slices.is_empty() { return; }
            if self.owner().is_some_and(MathAtom::is_macro) {
                self.pop(false); self.anchor = Some(self.cursor.clone()); self.cursor.pos += 1; return;
            }
            let saved = self.data().clone();
            self.pop(false);
            let pos = self.cursor.pos; self.data_mut().splice(pos..pos+1, saved); self.anchor = None; return;
        }
        let pos = self.cursor.pos-1;
        if self.data()[pos].confirm_deletion() { self.anchor = Some(self.cursor.clone()); self.cursor.pos = pos; }
        else { self.data_mut().remove(pos); self.cursor.pos = pos; self.dissolve_empty_run(); }
    }
    fn delete(&mut self) {
        if self.pending().is_some() { return; }
        if self.erase_selection() { return; }
        let pos = self.cursor.pos;
        if pos == self.data().len() {
            if let Some(owner) = self.owner() {
                if owner.is_macro() { return; }
                if owner.cells.len() == 1 && pos == 0 { self.pop(false); let p = self.cursor.pos; self.data_mut().remove(p); }
                else if let Kind::Scripts = owner.kind {
                    let idx = self.cursor.slices.last().unwrap().cell;
                    if idx > 0 && pos == 0 { self.owner_mut().unwrap().remove_script(idx); self.cursor.slices.last_mut().unwrap().cell = 0; self.cursor.pos = self.data().len(); }
                }
            }
            return;
        }
        if self.data()[pos].confirm_deletion() { self.anchor = Some(self.cursor.clone()); self.cursor.pos += 1; }
        else { self.data_mut().remove(pos); self.dissolve_empty_run(); }
    }
    fn key(&mut self, key: &str, shift: bool, ctrl: bool) {
        if self.pending().is_some() { self.draft_key(key, shift, ctrl); return; }
        if self.text_cell() {
            match key {
                "Enter" => { self.anchor=None; self.pop(true); return; }
                "ArrowLeft" if self.cursor.pos == 0 => return,
                "ArrowRight" if self.cursor.pos == self.data().len() => return,
                "ArrowUp" | "ArrowDown" | "Tab" => return,
                "Home" | "End" => {
                    if shift { self.anchor.get_or_insert(self.cursor.clone()); } else {self.anchor=None;}
                    self.cursor.pos=if key=="Home" {0} else {self.data().len()};return;
                }
                _ => {}
            }
        }
        if ctrl && key.eq_ignore_ascii_case("a") {
            self.anchor = Some(Cursor { pos: 0, ..self.cursor.clone() }); self.cursor.pos = self.data().len(); return;
        }
        if ctrl && key == " " { if self.pending().is_none() { self.interpret_char('\\'); } return; }
        match key {
            "ArrowLeft" | "ArrowRight" if ctrl => self.move_word(key == "ArrowRight", shift),
            "ArrowLeft" => self.move_horizontal(false, shift), "ArrowRight" => self.move_horizontal(true, shift),
            "ArrowUp" => self.vertical(true, shift), "ArrowDown" => self.vertical(false, shift),
            "Tab" => self.tab(!shift), " " => self.interpret_char(' '),
            "Backspace" => self.backspace(), "Delete" => self.delete(),
            "Escape" => if self.selection().is_some() { self.anchor = None; } else { self.pop(true); },
            "Home" | "End" => {
                if shift { self.anchor.get_or_insert(self.cursor.clone()); } else { self.anchor = None; }
                let end = key == "End";
                if end && self.cursor.pos != self.data().len() { self.cursor.pos = self.data().len(); }
                else if !end && self.cursor.pos != 0 { self.cursor.pos = 0; }
                else if let Some(owner) = self.owner() {
                    let idx = self.cursor.slices.last().unwrap().cell;
                    let last = owner.cells.len()-1;
                    let columns = match &owner.kind { Kind::Table {columns,..} | Kind::Multiline {columns,..} => *columns, _ => 1 };
                    let target = if !end && idx % columns != 0 { Some(idx - idx % columns) }
                        else if end && idx % columns + 1 != columns { Some(idx - idx % columns + columns - 1) }
                        else if !end && idx != 0 { Some(0) }
                        else if end && idx != last { Some(last) } else { None };
                    if let Some(target) = target { self.cursor.slices.last_mut().unwrap().cell = target; self.cursor.pos = if end { self.data().len() } else { 0 }; }
                    else { self.pop(end); }
                }
                self.target_x = None;
            }
            // LyX ignores ordinary newlines in a simple math hull.
            "Enter" => {},
            _ => {},
        }
    }
    fn grow_grid(&mut self, column: bool) {
        if self.pending().is_some() { self.message = "请先按 Enter 确认命令".into(); return; }
        let columns = match self.owner().map(|a| &a.kind) {
            Some(Kind::Table {columns,..} | Kind::Multiline {columns,..}) => *columns,
            _ => { self.message = "请先进入矩阵或对齐公式的一个格子".into(); return; }
        };
        let old_idx = self.cursor.slices.last().unwrap().cell;
        let grid = self.owner_mut().unwrap();
        if column {
            let rows = grid.cells.len()/columns;
            for row in (0..rows).rev() { grid.cells.insert((row+1)*columns, vec![]); }
            if let Kind::Multiline {columns:count,row_lengths} = &mut grid.kind {
                *count=columns+1; row_lengths.fill(columns+1);
            } else if let Kind::Table { columns: count, row_lengths, name } = &mut grid.kind {
                // Growing a table's columns has to keep the command that built it — a
                // widened `vec` is still written `vec(…)` — and it fills every row, the
                // way it already did for an alignment: asking for a column asks for it
                // in all of them.
                let _ = name;
                *count = columns+1; row_lengths.fill(columns+1);
            }
            self.cursor.slices.last_mut().unwrap().cell = old_idx / columns * (columns+1) + old_idx % columns;
        } else {
            grid.cells.extend(vec![vec![]; columns]);
            match &mut grid.kind {
                Kind::Multiline { row_lengths, .. } | Kind::Table { row_lengths, .. } => row_lengths.push(columns),
                _ => {}
            }
        }
    }
}
