// SPDX-License-Identifier: GPL-2.0-or-later
// Ports MathData/MathAtom and InsetMathNest/Script cell conventions.
// Original authors: Alejandro Aguilar Sierra, André Pönitz,
// Lars Gullik Bjønnes, Stefan Schimanski. See docs/LYX-CREDITS.
use crate::slots::{char_class, Decl, Entry, Horiz};
use serde::{Deserialize, Serialize};

mod configured { include!(concat!(env!("OUT_DIR"), "/symbols.rs")); }

pub type MathData = Vec<MathAtom>;

pub fn is_operator(ch: char) -> bool { "+-=<>!*:|~".contains(ch) }
/// Which cell of a `Kind::Scripts` holds a given attachment.
///
/// The storage is always `[base, upper, lower]`, even when one of them is empty,
/// so this is a constant rather than a lookup that depends on which scripts
/// exist — that dependence is what `cell_1_is_up` used to encode.
pub const fn script_cell(up: bool) -> usize { if up { 1 } else { 2 } }

/// Whether a run of characters is one `Kind::Number`.
///
/// This is the engine's rule, not the lexer's, and the two differ on purpose:
///
/// * the lexer starts a run at any `char::is_numeric` and takes one tentatively
///   eaten dot (`Lexer::math_text`), so `²3` is one token;
/// * `resolve_text` turns a text run into a `NumberItem` only when every
///   character is an ASCII digit or a dot, there is at most one dot, and there is
///   at least one digit (`resolve.rs:302-308`).
///
/// Following the engine is what keeps this kind 1:1 with `MathKind::Number`:
/// `²3` lexes as one run but resolves to a `TextItem`, so it must not become a
/// `Number` here.
pub fn is_number(text: &str) -> bool {
    let mut digits = 0;
    let mut dots = 0;
    for value in text.chars() {
        if value == '.' { dots += 1; }
        else if value.is_ascii_digit() { digits += 1; }
        else { return false; }    }
    digits > 0 && dots <= 1
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MathAtom {
    pub kind: Kind,
    pub cells: Vec<MathData>,
}

/// The editable model of one math node.
///
/// The variant names follow Typst's `MathKind`
/// (`vendor/typst/crates/typst-library/src/math/ir/item.rs`) wherever the two
/// describe the same construct, so the vocabularies can be compared item by
/// item. Where they differ, `Kind::decl`'s `typst` field says so in the open
/// instead of leaving the reader to guess:
///
/// * an *editor-only* kind has no `MathKind` (`MacroCall`, `TemplateCall`,
///   `Parameter`, `Unknown`): it exists for editing, not for layout;
/// * an *opaque* kind stands in for several `MathKind`s the editor keeps as
///   source text (`Raw`, and `Fenced`'s delimiters are strings, not items as in
///   `FencedItem`);
/// * a *split* pair is one construct modelled as two kinds, or two constructs
///   modelled as one (`Sqrt`/`Root` against `Radical`; `Decoration` against
///   `Accent` and `Line`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Char { value: char },
    Symbol { name: String, glyph: String },
    /// A run of digits with at most one dot: Typst's `Number`.
    ///
    /// One cell holding the run's characters, the way `Text` holds a string's.
    /// The caret can sit *between* the digits, which a leaf could not express, and
    /// the writer writes the cell as one piece — a separator inside a run would
    /// split it into several numbers (`math::is_number` decides what a run is).
    Number,
    Raw { source: String },
    MacroCall { name: String, function: bool },
    // Template-only edge to an earlier definition version in the registry.
    TemplateCall { definition: usize },
    // Template-only reference. Arguments live exclusively in MacroCall.cells.
    Parameter { index: usize },
    Text,
    Fraction,
    Sqrt,
    Root,
    Scripts,
    Fenced { left: String, right: String },
    Table { columns: usize },
    // `columns` and `row_lengths` are the flat encoding of Typst's
    // `MultilineItem::rows`: the cells are one flat list, padded to `columns`.
    Multiline { columns: usize, row_lengths: Vec<usize> },
    Decoration { name: String },
    Unknown { name: String, saved: MathData, caret: usize, anchor: Option<usize>, original: Option<String> },
}

impl MathAtom {
    pub fn character(value: char) -> Self { Self { kind: Kind::Char { value }, cells: vec![] } }
    /// One run of digits, from its spelling. `is_number` decides what may be a run.
    pub fn number(text: &str) -> Self {
        Self { kind: Kind::Number, cells: vec![text.chars().map(MathAtom::character).collect()] }
    }
    pub fn raw(source: impl Into<String>) -> Self { Self { kind: Kind::Raw { source: source.into() }, cells: vec![] } }
    pub fn from_source(source: impl Into<String>) -> Self {
        let source = source.into();
        if let Some(glyph) = symbol(&source) {
            Self { kind: Kind::Symbol { name: source, glyph: glyph.into() }, cells: vec![] }
        } else { Self::raw(source) }
    }
    pub fn nest(kind: Kind, cells: usize) -> Self { Self { kind, cells: vec![vec![]; cells] } }
    pub fn script(base: MathData) -> Self {
        Self { kind: Kind::Scripts, cells: vec![base, vec![], vec![]] }
    }
    pub fn active(&self) -> bool { !self.cells.is_empty() }
    /// The slot schema of this atom's kind. See `crate::slots`.
    pub fn decl(&self) -> Decl { self.kind.decl() }
    /// The column count of a table or an alignment. Never zero: only a table
    /// with at least one column is ever built, and both navigation and layout
    /// divide by this.
    pub fn columns(&self) -> usize {
        match self.kind { Kind::Table { columns } | Kind::Multiline { columns, .. } => columns.max(1), _ => 1 }
    }
    pub fn entry_cell(&self, forward: bool) -> usize {
        let decl = self.decl();
        match decl.entry {
            // The roles named here are present in the same declaration.
            Entry::Role { forward: f, backward: b } => decl.index_of(if forward { f } else { b }).unwrap_or(0),
            // Saturating rather than `len() - 1`: a leaf has no cell to enter,
            // and the expression this replaces underflowed if one was asked.
            Entry::Edge => if forward { 0 } else { self.cells.len().saturating_sub(1) },
            Entry::GridMiddle => {
                let columns = self.columns();
                (self.cells.len() / columns).saturating_sub(1) / 2 * columns + if forward { 0 } else { columns - 1 }
            }
        }
    }
    pub fn confirm_deletion(&self) -> bool { self.active() }
    pub fn math_class(&self) -> u8 {
        // A character's class follows the character, not the kind, so it is
        // answered before the table is consulted.
        if let Kind::Char { value } = self.kind { return char_class(value); }
        self.decl().class
    }
    // InsetMathScript::idxOfScript, ensure, removeScript (same cell ordering).
    /// The cell holding an attachment, or `None` when that cell is empty.
    ///
    /// An empty cell means "no such script", which is what the callers test for:
    /// up/down does not enter an attachment that does not exist, and writing the
    /// node back omits it.
    pub fn script_idx(&self, up: bool) -> Option<usize> {
        if !matches!(self.kind, Kind::Scripts) { return None; }
        let index = script_cell(up);
        self.cells.get(index).filter(|cell| !cell.is_empty()).map(|_| index)
    }
    /// Empties one attachment. The cell itself stays, so the storage keeps its
    /// fixed `[base, upper, lower]` shape.
    pub fn remove_script(&mut self, idx: usize) {
        if let Some(cell) = self.cells.get_mut(idx) { cell.clear(); }
    }
    // InsetMathFrac and InsetMathScript deliberately DO NOT walk cells on Right.
    pub fn idx_horizontal(&self, idx: usize, forward: bool) -> Option<usize> {
        let step = |forward: bool| if forward { (idx + 1 < self.cells.len()).then_some(idx + 1) } else { idx.checked_sub(1) };
        match self.decl().horizontal {
            Horiz::Locked => None,
            Horiz::Pair => if forward { (idx == 1).then_some(0) } else { (idx == 0).then_some(1) },
            Horiz::Column => {
                let columns = self.columns();
                if forward && idx % columns + 1 == columns || !forward && idx % columns == 0 { None } else { step(forward) }
            }
            Horiz::Linear => step(forward),
        }
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

#[cfg(test)]
mod tests {
    use super::is_number;

    /// `is_number` has to be pinned directly, because round-tripping cannot see a
    /// wrong rule: the writer spells a `Number` as its own text and the parser
    /// applies the same rule, so a `Number(".5")` no one should have built still
    /// reads back perfectly. The rule's source is the engine, not us, so the test
    /// states the engine's rule.
    #[test]
    fn only_ascii_digits_with_at_most_one_dot_are_a_number() {
        for text in ["1", "12", "123", "1.5", "0.", ".5", "123.456"] {
            assert!(is_number(text), "{text} 应当是 Number");
        }
        for text in ["", ".", "..", "1.2.3", "²3", "٣", "1e3", "x", "-1", "1 2", "1.5.6"] {
            assert!(!is_number(text), "{text} 不应当是 Number");
        }
    }
}
