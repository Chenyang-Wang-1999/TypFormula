// SPDX-License-Identifier: GPL-2.0-or-later
// Ports MathData/MathAtom and InsetMathNest/Script cell conventions.
// Original authors: Alejandro Aguilar Sierra, André Pönitz,
// Lars Gullik Bjønnes, Stefan Schimanski. See docs/LYX-CREDITS.
use serde::{Deserialize, Serialize};

mod configured { include!(concat!(env!("OUT_DIR"), "/symbols.rs")); }

pub type MathData = Vec<MathAtom>;

pub fn is_operator(ch: char) -> bool { "+-=<>!*:|~".contains(ch) }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MathAtom {
    pub kind: Kind,
    pub cells: Vec<MathData>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Char { value: char },
    Symbol { name: String, glyph: String },
    Raw { source: String },
    MacroCall { name: String, function: bool },
    // Template-only edge to an earlier definition version in the registry.
    TemplateCall { definition: usize },
    // Template-only reference. Arguments live exclusively in MacroCall.cells.
    Parameter { index: usize },
    Text,
    Frac,
    Sqrt,
    Root,
    Script { cell_1_is_up: bool },
    Delim { left: String, right: String },
    Grid { columns: usize },
    Aligned { columns: usize, row_lengths: Vec<usize> },
    Decoration { name: String },
    Unknown { name: String, saved: MathData, caret: usize, anchor: Option<usize>, original: Option<String> },
}

impl MathAtom {
    pub fn character(value: char) -> Self { Self { kind: Kind::Char { value }, cells: vec![] } }
    pub fn raw(source: impl Into<String>) -> Self { Self { kind: Kind::Raw { source: source.into() }, cells: vec![] } }
    pub fn from_source(source: impl Into<String>) -> Self {
        let source = source.into();
        if let Some(glyph) = symbol(&source) {
            Self { kind: Kind::Symbol { name: source, glyph: glyph.into() }, cells: vec![] }
        } else { Self::raw(source) }
    }
    pub fn nest(kind: Kind, cells: usize) -> Self { Self { kind, cells: vec![vec![]; cells] } }
    pub fn script(base: MathData, up: bool) -> Self {
        Self { kind: Kind::Script { cell_1_is_up: up }, cells: vec![base, vec![]] }
    }
    pub fn active(&self) -> bool { !self.cells.is_empty() }
    pub fn entry_cell(&self, forward: bool) -> usize {
        match self.kind {
            Kind::Root => usize::from(forward), Kind::Script { .. } => 0,
            Kind::Grid { columns } => ((self.cells.len()/columns-1)/2)*columns + if forward {0} else {columns-1},
            Kind::Aligned { .. } => if forward { 0 } else { self.cells.len()-1 },
            _ => if forward { 0 } else { self.cells.len()-1 }
        }
    }
    pub fn confirm_deletion(&self) -> bool { self.active() }
    pub fn math_class(&self) -> u8 {
        match &self.kind {
            Kind::Char { value } if "+−-*".contains(*value) => 1,
            Kind::Char { value } if "=<>≤≥≠≈".contains(*value) => 2,
            Kind::Char { value } if ",;:".contains(*value) => 3,
            Kind::Char { value } if "([{".contains(*value) => 4,
            Kind::Char { value } if ")] }".contains(*value) => 5,
            Kind::Frac | Kind::Grid { .. } => 7,
            _ => 0,
        }
    }
    // InsetMathScript::idxOfScript, ensure, removeScript (same cell ordering).
    pub fn script_idx(&self, up: bool) -> Option<usize> {
        let Kind::Script { cell_1_is_up } = self.kind else { return None; };
        match self.cells.len() { 3 => Some(if up { 1 } else { 2 }), 2 if cell_1_is_up == up => Some(1), _ => None }
    }
    pub fn ensure_script(&mut self, up: bool) -> usize {
        let Kind::Script { ref mut cell_1_is_up } = self.kind else { unreachable!() };
        if self.cells.len() == 1 {
            self.cells.push(vec![]); *cell_1_is_up = up;
        } else if self.cells.len() == 2 && *cell_1_is_up != up {
            if up { let down = std::mem::take(&mut self.cells[1]); self.cells.push(down); }
            else { self.cells.push(vec![]); }
        }
        self.script_idx(up).unwrap()
    }
    pub fn remove_script(&mut self, idx: usize) {
        let Kind::Script { ref mut cell_1_is_up } = self.kind else { return; };
        if self.cells.len() == 3 { *cell_1_is_up = idx != 1; }
        if idx > 0 && idx < self.cells.len() { self.cells.remove(idx); }
    }
    // InsetMathFrac and InsetMathScript deliberately DO NOT walk cells on Right.
    pub fn idx_horizontal(&self, idx: usize, forward: bool) -> Option<usize> {
        if matches!(self.kind, Kind::Frac | Kind::Script { .. }) { return None; }
        if matches!(self.kind, Kind::Root) { return if forward && idx == 1 { Some(0) } else if !forward && idx == 0 { Some(1) } else { None }; }
        if let Kind::Grid { columns } | Kind::Aligned { columns, .. } = self.kind {
            if forward && idx % columns + 1 == columns || !forward && idx % columns == 0 { return None; }
        }
        if forward { (idx + 1 < self.cells.len()).then_some(idx + 1) }
        else { idx.checked_sub(1) }
    }
}

// Each slice points at the owning atom in the parent cell, then its cell index.
// The final position is between atoms, never a byte offset in serialized Typst.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CursorSlice { pub atom: usize, pub cell: usize }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Cursor {
    pub slices: Vec<CursorSlice>,
    pub pos: usize,
    #[serde(default)]
    pub occurrence: String,
}
pub fn cell<'a>(root: &'a MathData, slices: &[CursorSlice]) -> &'a MathData {
    let mut data = root;
    for slice in slices { data = &data[slice.atom].cells[slice.cell]; }
    data
}
pub fn cell_mut<'a>(root: &'a mut MathData, slices: &[CursorSlice]) -> &'a mut MathData {
    let mut data = root;
    for slice in slices { data = &mut data[slice.atom].cells[slice.cell]; }
    data
}
pub fn valid(root: &MathData, cur: &Cursor) -> bool {
    let mut data = root;
    for slice in &cur.slices {
        let Some(next) = data.get(slice.atom).and_then(|atom| atom.cells.get(slice.cell)) else { return false; };
        data = next;
    }
    cur.pos <= data.len()
}

pub fn symbol(name: &str) -> Option<&'static str> {
    configured::SYMBOLS.binary_search_by(|(source,_)| source.cmp(&name))
        .ok().map(|i| configured::SYMBOLS[i].1)
}
pub const COMMANDS: &[&str] = &["frac", "sqrt", "root", "mat", "abs", "norm", "overline", "underline", "hat", "vec"];
