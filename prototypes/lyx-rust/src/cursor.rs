// SPDX-License-Identifier: GPL-2.0-or-later
// Translated branches from upstream/src/Cursor.cpp and
// upstream/src/mathed/InsetMathNest.cpp, InsetMathScript.cpp, InsetMathFrac.cpp.
// Authors of original algorithms are listed in those files and upstream/CREDITS.
use crate::{math::*, typst};
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
    LspCompletions { draft: String, caret: usize, items: Vec<CommandCompletion> },
    Geometry { stops: Vec<StopGeometry> },
    Undo, Redo, Clear,
    AddRow, AddColumn,
}
#[derive(Clone, Debug, Deserialize)]
pub struct StopGeometry { pub cursor: Cursor, pub x: f64, pub y: f64 }
#[derive(Clone)]
struct Snapshot { root: MathData, cursor: Cursor, anchor: Option<Cursor>, definitions: String, display: bool }

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
    failed_previews: HashSet<(String, bool, String)>,
}
impl Default for Editor {
    fn default() -> Self { Self { root: vec![], cursor: Cursor::default(), anchor: None, definitions: String::new(), display: true, message: String::new(), completion_index: 0, revision: 0, geometry: vec![], target_x: None, history: vec![], future: vec![], typing: false, lsp_completions: None, failed_previews: HashSet::new() } }
}
impl Editor {
    fn snapshot(&self) -> Snapshot { Snapshot { root: self.root.clone(), cursor: self.cursor.clone(), anchor: self.anchor.clone(), definitions: self.definitions.clone(), display: self.display } }
    fn restore(&mut self, s: Snapshot) { self.root = s.root; self.cursor = s.cursor; self.anchor = s.anchor; self.definitions = s.definitions; self.display = s.display; }
    pub fn can_undo(&self) -> bool { !self.history.is_empty() }
    pub fn can_redo(&self) -> bool { !self.future.is_empty() }
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
    pub fn completions(&self) -> Vec<String> {
        if self.string_mode() { return vec![]; }
        if let Some(items) = self.current_lsp_completions() { return items.iter().map(|i| i.label.clone()).collect(); }
        let Some(prefix) = self.pending() else { return vec![]; };
        let prefix = prefix.trim();
        if !prefix.chars().all(|c| c.is_alphanumeric() || c == '.') { return vec![]; }
        let registry = typst::macro_registry(&self.definitions);
        let mut names: Vec<_> = COMMANDS.iter().copied().map(str::to_string)
            .chain(registry.entries.iter().filter(|d| d.expandable && !d.shadowed).map(|d| d.name.clone()))
            .filter(|n| n.starts_with(prefix)).collect();
        names.sort(); names.dedup(); names
    }
    pub fn apply(&mut self, action: Action) -> Result<(), String> {
        if let Action::Geometry { stops } = action { self.geometry = stops; return Ok(()); }
        if let Action::PreviewResult { source, definitions, display, failed } = action {
            // Rendering state is transient, independent of source and undo.
            let key = (definitions, display, source);
            if failed { self.failed_previews.insert(key); } else { self.failed_previews.remove(&key); }
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
                    self.root = parsed.root; self.definitions = parsed.definitions;
                    self.cursor = Cursor::default(); self.anchor = None; self.geometry.clear();
                    self.lsp_completions = None;
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
            Action::Geometry { .. } | Action::LspCompletions { .. } | Action::PreviewResult { .. } => unreachable!(),
        }
        if self.root != before.root || self.definitions != before.definitions || self.display != before.display {
            if !is_typing || !self.typing { self.history.push(before); }
            if self.history.len() > 200 { self.history.remove(0); }
            self.future.clear();
        }
        self.typing = is_typing;
        self.revision += 1;
        if !valid(&self.root, &self.cursor) { return Err("光标结构不一致".into()); }
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
            self.plain_insert(MathAtom {kind:Kind::Frac,cells:vec![numerator,vec![]]});
            self.push(pos, 1, false); return;
        }
        if ch == ' ' {
            if self.selection().is_some() { self.anchor = None; } else { self.pop(true); }
            return;
        }
        if ch == '\n' { return; }
        if ch == '^' || ch == '_' { self.script(ch == '^'); return; }
        self.erase_selection();
        self.plain_insert(MathAtom::character(ch));
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
        let name = completion.unwrap_or_else(|| draft.trim().to_string());
        let name = if name == "/" { "frac".to_string() } else { name };
        let template = name.is_empty() || self.factory(&name).is_some();
        let parsed = if !cancel && !template {
            match typst::parse_command(&name, &self.definitions) {
                Ok(doc) => Some(doc),
                Err(error) => { self.message = format!("公式尚未完成：{error}"); return true; }
            }
        } else { None };
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
            }
            let n = data.len(); self.data_mut().splice(pos..pos, data); self.cursor.pos += n;
        } else { self.insert_named(&name, saved); }
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
                // Built-in exact names still create empty, editable LyX slots.
                let exact = self.pending().is_some_and(|s| COMMANDS.contains(&s.trim()) || list.iter().any(|name| name == s.trim()));
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
    fn factory(&self, name: &str) -> Option<MathAtom> {
        let registry = typst::macro_registry(&self.definitions);
        if registry.is_bound(name) {
            let def = registry.get(name).filter(|d| d.expandable)?;
            return Some(MathAtom { kind: Kind::MacroCall { name: name.into(), function: def.function }, cells: vec![vec![]; def.params.len()] });
        }
        Some(match name {
            "frac" => MathAtom::nest(Kind::Frac, 2), "sqrt" => MathAtom::nest(Kind::Sqrt, 1),
            "root" => MathAtom::nest(Kind::Root, 2), "mat" => MathAtom::nest(Kind::Grid { columns: 2 }, 4),
            "abs" => MathAtom::nest(Kind::Delim { left: "|".into(), right: "|".into() }, 1),
            "norm" => MathAtom::nest(Kind::Delim { left: "‖".into(), right: "‖".into() }, 1),
            "overline" | "underline" | "hat" | "vec" => MathAtom::nest(Kind::Decoration { name: name.into() }, 1),
            _ => return None,
        })
    }
    fn nice_insert(&mut self, name: &str) {
        if name == "sup" || name == "sub" { self.script(name == "sup"); return; }
        if name == "()" {
            let saved = self.take_selection(); let pos = self.cursor.pos;
            self.plain_insert(MathAtom { kind: Kind::Delim {left:"(".into(),right:")".into()}, cells:vec![saved] });
            self.push(pos, 0, false); return;
        }
        let saved = self.take_selection(); self.insert_named(name, saved);
    }
    fn insert_named(&mut self, name: &str, saved: MathData) {
        let Some(mut atom) = self.factory(name) else {
            let pos = self.cursor.pos;
            match typst::parse_command(name, &self.definitions) {
                Ok(doc) => {
                    self.definitions = doc.definitions;
                    let n = doc.root.len(); self.data_mut().splice(pos..pos, doc.root); self.cursor.pos += n;
                }
                Err(error) => {
                    let n = saved.len(); self.data_mut().splice(pos..pos, saved); self.cursor.pos += n;
                    self.message = error;
                }
            }
            return;
        };
        let active = atom.active();
        let entry = if active { atom.entry_cell(true) } else { 0 };
        if active { atom.cells[0] = saved; }
        let pos = self.cursor.pos;
        self.plain_insert(atom);
        if active { self.push(pos, entry, false); }
    }
    // InsetMathNest::script: selection becomes SCRIPT CONTENT, not the base.
    fn script(&mut self, up: bool) {
        let saved = self.take_selection();
        let in_nucleus = matches!(self.owner().map(|o| &o.kind), Some(Kind::Script { .. })) && self.cursor.slices.last().unwrap().cell == 0;
        if in_nucleus {
            let idx = self.owner_mut().unwrap().ensure_script(up);
            self.cursor.slices.last_mut().unwrap().cell = idx; self.cursor.pos = 0;
        } else if self.cursor.pos > 0 && matches!(self.data()[self.cursor.pos - 1].kind, Kind::Script { .. }) {
            let pos = self.cursor.pos - 1;
            let idx = self.data_mut()[pos].ensure_script(up);
            self.push(pos, idx, true);
        } else {
            let pos = if self.cursor.pos == 0 { self.plain_insert(MathAtom::script(vec![], up)); 0 }
                else { let p = self.cursor.pos - 1; let old = self.data_mut().remove(p); self.data_mut().insert(p, MathAtom::script(vec![old], up)); p };
            self.push(pos, 1, false);
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
                    if self.failed_previews.contains(&(self.definitions.clone(), self.display, source.clone())) {
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
                let root_back = !forward && matches!(owner.kind, Kind::Root | Kind::Grid { .. } | Kind::Aligned { .. });
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
            match owner.kind {
                Kind::Frac => { let t = usize::from(!up); if idx != t { target = Some(t); } }
                Kind::Root => { let t = usize::from(up); if idx != t { target = Some(t); end = up; } }
                Kind::Script { .. } => {
                    if idx == 0 && self.cursor.pos == self.data().len() { target = owner.script_idx(up); }
                    else if owner.script_idx(true) == Some(idx) && !up || owner.script_idx(false) == Some(idx) && up { target = Some(0); end = true; }
                }
                Kind::Grid { columns } | Kind::Aligned { columns, .. } => {
                    if up { target = idx.checked_sub(columns); }
                    else if idx + columns < owner.cells.len() { target = Some(idx + columns); }
                }
                _ => {},
            }
            if let Some(target) = target {
                self.cursor.slices.last_mut().unwrap().cell = target;
                let candidates: Vec<_> = self.geometry.iter().filter(|s| s.cursor.slices == self.cursor.slices).collect();
                self.cursor.pos = if end { self.data().len() } else if let (Some(x), Some(best)) = (x, candidates.iter().min_by(|a,b| (a.x-x.unwrap_or(0.0)).abs().total_cmp(&(b.x-x.unwrap_or(0.0)).abs()))) {
                    let _ = x; best.cursor.pos
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
            if matches!(self.owner().map(|o| &o.kind), Some(Kind::MacroCall { .. })) {
                self.pop(false); self.anchor = Some(self.cursor.clone()); self.cursor.pos += 1; return;
            }
            let saved = self.data().clone();
            self.pop(false);
            let pos = self.cursor.pos; self.data_mut().splice(pos..pos+1, saved); self.anchor = None; return;
        }
        let pos = self.cursor.pos-1;
        if self.data()[pos].confirm_deletion() { self.anchor = Some(self.cursor.clone()); self.cursor.pos = pos; }
        else { self.data_mut().remove(pos); self.cursor.pos = pos; }
    }
    fn delete(&mut self) {
        if self.pending().is_some() { return; }
        if self.erase_selection() { return; }
        let pos = self.cursor.pos;
        if pos == self.data().len() {
            if let Some(owner) = self.owner() {
                if matches!(owner.kind, Kind::MacroCall { .. }) { return; }
                if owner.cells.len() == 1 && pos == 0 { self.pop(false); let p = self.cursor.pos; self.data_mut().remove(p); }
                else if let Kind::Script { .. } = owner.kind {
                    let idx = self.cursor.slices.last().unwrap().cell;
                    if idx > 0 && pos == 0 { self.owner_mut().unwrap().remove_script(idx); self.cursor.slices.last_mut().unwrap().cell = 0; self.cursor.pos = self.data().len(); }
                }
            }
            return;
        }
        if self.data()[pos].confirm_deletion() { self.anchor = Some(self.cursor.clone()); self.cursor.pos += 1; }
        else { self.data_mut().remove(pos); }
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
                    let columns = match owner.kind { Kind::Grid {columns} | Kind::Aligned {columns,..} => columns, _ => 1 };
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
            Some(Kind::Grid {columns} | Kind::Aligned {columns,..}) => *columns,
            _ => { self.message = "请先进入矩阵或对齐公式的一个格子".into(); return; }
        };
        let old_idx = self.cursor.slices.last().unwrap().cell;
        let grid = self.owner_mut().unwrap();
        if column {
            let rows = grid.cells.len()/columns;
            for row in (0..rows).rev() { grid.cells.insert((row+1)*columns, vec![]); }
            if let Kind::Aligned {columns:count,row_lengths} = &mut grid.kind {
                *count=columns+1; row_lengths.fill(columns+1);
            } else { grid.kind = Kind::Grid { columns: columns+1 }; }
            self.cursor.slices.last_mut().unwrap().cell = old_idx / columns * (columns+1) + old_idx % columns;
        } else {
            grid.cells.extend(vec![vec![]; columns]);
            if let Kind::Aligned {row_lengths,..} = &mut grid.kind { row_lengths.push(columns); }
        }
    }
}
