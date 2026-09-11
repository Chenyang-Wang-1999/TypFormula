// SPDX-License-Identifier: GPL-2.0-or-later
// Ports MathData/MathAtom and InsetMathNest/Script cell conventions.
// Original authors: Alejandro Aguilar Sierra, André Pönitz,
// Lars Gullik Bjønnes, Stefan Schimanski. See docs/LYX-CREDITS.
use crate::slots::{self, char_class, Decl, Entry, Horiz};
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

// The symbol table and the command table are both generated from `config/`, by
// `build.rs`, into one file. `symbols::SYMBOLS` is a sorted source→glyph map;
// `commands::COMMANDS` maps a command name to a kind's view name (see `slots`).
pub mod configured { include!(concat!(env!("OUT_DIR"), "/config.rs")); }

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
/// * a *split* kind is one whose one Typst construct the editor models with more
///   than one node (`Sqrt` and `Root` against `Radical`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    /// One character, held as a grapheme cluster: Typst's `Glyph`.
    ///
    /// `text` is a `String` rather than a `char` because a reader's "character"
    /// and a Unicode scalar are not the same thing: `é` may be one scalar or two,
    /// and an emoji cluster is often several. The lexer keeps one cluster in one
    /// token, and `GlyphItem` holds one cluster too, so keeping one here is what
    /// stops the writer from putting a separator *inside* a character.
    Char { text: String },
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
    /// A grid of cells: Typst's `Table`.
    ///
    /// `columns` is the one piece of instance data no name and no config file can
    /// supply — it comes from how the argument list was punctuated (`mat(a, b; c, d)`
    /// is two columns, `mat(a; b; c)` is one), and a flat cell list no longer says.
    /// That is why this kind is *stored* while `Fraction`/`Sqrt`/`Accent`/`Line` are
    /// only shape descriptors.
    ///
    /// `row_lengths` records how many cells each row really had before the flat list
    /// was padded to `columns`, exactly as `Multiline` does. Rows need not be equal:
    /// the editor used to refuse a ragged `mat(a, b; c)` as "not a table Typst can lay
    /// out", which is simply untrue — measured against the engine it is two rows of two
    /// and one, `43.008pt` tall, the same as `mat(1, 2; 3, 4)`. Without this the writer
    /// would have to guess where a row ends, and trimming trailing empty cells would
    /// turn `mat(a, ; c, d)` — two columns, one deliberately blank — into the ragged
    /// `mat(a; c, d)`.
    ///
    /// `name` is the command that built it, because the shape does not imply the
    /// spelling: `vec(a, b)` and `mat(a; b)` lay out to the same table and must still
    /// be written back as what they were.
    Table { columns: usize, row_lengths: Vec<usize>, name: String },
    // `columns` and `row_lengths` are the flat encoding of Typst's
    // `MultilineItem::rows`: the cells are one flat list, padded to `columns`.
    Multiline { columns: usize, row_lengths: Vec<usize> },
    /// A base with a mark above or below: Typst's `Accent`.
    ///
    /// The name is the command that built it (`hat`, `vec`), which is what the
    /// writer needs; the engine's `AccentItem` carries the resolved mark item
    /// instead, and derives above/below from the mark's own Unicode class, so the
    /// position is not stored here.
    Accent { name: String },
    /// A base with a rule above or below: Typst's `Line`.
    ///
    /// `LineItem` holds nothing but the position, so neither does this: the
    /// spelling (`overline` or `underline`) follows from it.
    Line { above: bool },
    /// A base drawn in a font variant — `bold`, `upright`, `bb`, `frak`: one body cell
    /// plus the name of the variant.
    ///
    /// **The glyphs are not here.** Typst applies a variant by *substituting codepoints*
    /// (`resolve.rs` passes `codex`'s `to_style(c, …)` output into the item's text), and
    /// that table lives in a crate the kernel cannot reach. So this kind stores the same
    /// thing `Accent` stores — the name — and the frontend asks the engine for the
    /// substituted glyphs. Measured: `bold(A)` and a literal `𝐀` are the same width
    /// (0.869), `bold(alpha)` and `𝛂` share height and baseline (0.460/0.452).
    Style { name: String },
    Unknown { name: String, saved: MathData, caret: usize, anchor: Option<usize>, original: Option<String> },
}

impl MathAtom {
    /// One character typed by the editor. A single scalar is one cluster.
    pub fn character(value: char) -> Self { Self { kind: Kind::Char { text: value.to_string() }, cells: vec![] } }
    /// One character read from source, as the lexer grouped it.
    pub fn glyph(text: &str) -> Self {
        debug_assert_eq!(text.graphemes(true).count(), 1, "一个 Char 只能装一个字形簇：{text:?}");
        Self { kind: Kind::Char { text: text.to_string() }, cells: vec![] }
    }
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
    /// The shape a configured command call borrows, if it names one.
    ///
    /// `Kind::MacroCall` is where every name-driven construct is heading — `frac`,
    /// `sqrt`, `hat`, `overline` and the rest are calls whose shape only
    /// `config/commands.json` knows. The node stores the *name*; the shape is looked
    /// up, never stored, so changing the file cannot leave a node holding a stale one.
    pub fn command_shape(&self) -> Option<Kind> {
        match &self.kind {
            Kind::MacroCall { name, .. } => slots::configured_kind(name),
            _ => None,
        }
    }
    /// Whether this call's cells are a **macro's parameters** rather than a shape's
    /// slots.
    ///
    /// A configured command borrows a shape, so its cells have to behave like that
    /// shape's cells: `frac(a, b)` pulls an argument out on backspace exactly as it did
    /// when it was a `Kind::Fraction`. A `#let` macro's cells are its parameters and
    /// cannot be dissolved — pulling one out would leave a call the definition no
    /// longer matches. Both are `Kind::MacroCall` in the tree, which is why this has to
    /// be asked rather than tested for with a `matches!`.
    pub fn is_macro(&self) -> bool {
        matches!(self.kind, Kind::MacroCall { .. }) && self.command_shape().is_none()
    }
    /// The slot schema of this atom's kind. See `crate::slots`.
    ///
    /// A configured call takes the schema of the shape it names, because editing is a
    /// property of the shape: `sqrt(x)` and the `sqrt` node a `∛` produces answer the
    /// same slots, reach their cell the same way, and take the same class. A call
    /// whose name the file does not know keeps `MacroCall`'s own schema — one linear
    /// cell per argument.
    pub fn decl(&self) -> Decl {
        self.command_shape().map_or_else(|| self.kind.decl(), |shape| shape.decl())
    }
    /// The column count of a table or an alignment. Never zero: only a table
    /// with at least one column is ever built, and both navigation and layout
    /// divide by this.
    pub fn columns(&self) -> usize {
        match self.kind { Kind::Table { columns, .. } | Kind::Multiline { columns, .. } => columns.max(1), _ => 1 }
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
        // answered before the table is consulted. For a cluster the first scalar
        // decides, the way `GlyphItem` reads `default_math_class` off it.
        if let Kind::Char { text } = &self.kind { return text.chars().next().map_or(0, char_class); }
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
