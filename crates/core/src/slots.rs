// SPDX-License-Identifier: GPL-2.0-or-later
//! Every `Kind`'s slot schema, declared once.
//!
//! `Kind` carries instance data (how many columns, which script exists); this
//! module says what a node's cells *mean* and how the caret moves between them.
//! The accessors that used to switch on `Kind` with a `_ =>` catch-all now read
//! `Kind::shape()` / `Kind::grammar()`, so a new kind is two exhaustive match arms instead of a hunt
//! for silent fallbacks scattered across `math.rs` and `cursor.rs`.
//!
//! `Decl` also records the three things that are *not* about slots but that
//! every kind has to answer anyway: the frontend's arrangement name (`view`),
//! the Typst spelling (`write`), and which Typst `MathKind`s the kind stands
//! for (`typst`). The last one is the reason the variant names follow Typst's:
//! the two vocabularies are meant to be read side by side, and
//! `the_typst_vocabulary_is_covered_exactly_once` is what keeps the comparison
//! honest.
//!
//! `tests/round_trip.rs` guards the spelling and `tests/caret_navigation.rs`
//! guards the slot schema.

use crate::math::Kind;

/// A slot's meaning inside its parent. Layout and navigation talk about roles,
/// never about `Kind`, so a new kind that reuses existing roles needs no new
/// engine code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// The base an attachment hangs off.
    Base,
    /// Superscript.
    Upper,
    /// Subscript.
    Lower,
    /// Fraction numerator.
    Numerator,
    /// Fraction denominator.
    Denominator,
    /// What a radical takes the root of.
    Radicand,
    /// The degree of a radical.
    Index,
    /// The content a wrapper wraps (delimiters, decorations, text runs).
    Inner,
    /// One cell of a grid or an alignment.
    Cell,
    /// One argument of a macro call.
    Arg,
}

impl Role {
    /// The wire name. The frontend places a child by this name, so a kind that
    /// reuses an existing arrangement is positioned by role rather than by the
    /// order of its cells.
    pub const fn name(self) -> &'static str {
        match self {
            Role::Base => "base", Role::Upper => "upper", Role::Lower => "lower",
            Role::Numerator => "numerator", Role::Denominator => "denominator",
            Role::Radicand => "radicand", Role::Index => "index",
            Role::Inner => "inner", Role::Cell => "cell", Role::Arg => "arg",
        }
    }
}

/// One slot: its role, its size relative to the parent, and whether it may stay
/// empty (an empty slot is drawn as a hole and can hold the caret).
#[derive(Clone, Copy)]
pub struct Slot {
    pub role: Role,
    /// Font scale relative to the parent, in per-mille (`1000` = 100%).
    /// Per-mille rather than `f32` so the whole table stays `const`.
    pub scale: u16,
    pub optional: bool,
}

impl Slot {
    pub const fn full(role: Role) -> Self { Self { role, scale: 1000, optional: false } }
    pub const fn scaled(role: Role, scale: u16) -> Self { Self { role, scale, optional: false } }
    pub const fn blank(role: Role, scale: u16) -> Self { Self { role, scale, optional: true } }
}

/// How many cells the schema describes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arity {
    /// Exactly one cell per slot.
    Exact,
    /// `slots` is a prefix; extra cells repeat the last slot's role.
    Repeat,
}

/// Where the caret lands when it first enters this node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Entry {
    /// Follow the direction to the named role. Roles must be unique in `slots`.
    Role { forward: Role, backward: Role },
    /// First cell forward, last cell backward (used where roles repeat).
    Edge,
    /// Middle row of a grid: first column forward, last column backward.
    GridMiddle,
}

/// Whether left/right walk between this node's cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Horiz {
    /// Never: a fraction and an attachment deliberately block the caret at
    /// their cell boundary (`InsetMathFrac`/`InsetMathScript` do the same).
    Locked,
    /// Linear, but stopping at a column boundary (grid, alignment).
    Column,
    /// Linear.
    Linear,
}

/// How up/down move between this node's cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vertical {
    /// No movement inside this node.
    None,
    /// Swap between two roles. `end_up` puts the caret at the end of the cell
    /// it moves up into (a radical's degree reads before its radicand).
    Swap { up: Role, down: Role, end_up: bool },
    /// Attachment rules: base to script only from the end of the base, and a
    /// script back to the base, which need the live script indices.
    Attach,
    /// Column arithmetic (grid, alignment).
    Column,
}

/// How a node's cells become its Typst spelling.
///
/// Most kinds are a call over their cells, and for those the template states the
/// order those cells are written in — `"frac({0}, {1})"`, `"sqrt({0})"`. A cell
/// order that differs from the spelling would be stated here rather than hidden in
/// a hand-written `c(1), c(0)`; after `Root` was changed to store its cells in the
/// order `root(index, radicand)` writes them, no kind needs that any more.
/// Placeholders are `{0}`, `{1}`… for cells and `{name}` for the node's stored name.
#[derive(Clone, Copy)]
pub enum Write {
    Template(&'static str),
    /// The node's own stored text (`Char`, `Symbol`, `Raw`, `Unknown`).
    OwnText,
    /// One cell's characters, quoted (`Text`).
    Quoted,
    /// One cell's characters with no separator between them (`Number`).
    ///
    /// A separator is what `write_cell` puts between two atoms, and inside a run it
    /// would split the run: `1 2 3` is three numbers to the lexer, `123` is one.
    Run,
    /// A base with optional `^(…)` and `_(…)` (`Script`).
    Attach,
    /// `abs(…)`/`norm(…)` for those two pairs, the literal pair otherwise (`Fenced`).
    Delimited,
    /// A call whose callee is the node's stored name, or the bare name when it
    /// is not a function (`MacroCall`).
    Named,
    /// The spelling is chosen by a stored position rather than by a name: the same
    /// cell is `above` in one node and `below` in another (`Line`).
    Positioned { above: &'static str, below: &'static str },
    /// `mat(…)` over cells grouped by the stored column count (`Table`).
    Matrix,
    /// Rows joined by ` \` + newline, cells by ` & ` (`Multiline`).
    Rows,
    /// A placeholder that is not valid Typst (`Parameter`).
    Marker,
    /// Never written: only reachable inside an expanded macro template.
    TemplateOnly,
}

/// How a node spells itself back into the document. Taken by `Kind`, **not** by
/// shape: the spelling says what the tree holds, and a call spells itself as a
/// call whatever shape it borrows for drawing.
///
/// That is the split `frac(a, b)` makes visible. It is stored as
/// `MacroCall { name: "frac" }`, so its spelling is `Named` — `frac(args)` — and
/// nothing about the `fraction` shape enters it. The shape is what the *slots*
/// and the *drawing* come from.
#[derive(Clone, Copy)]
pub struct Grammar {
    pub write: Write,
}

/// What a node looks like and how the caret moves inside it: its slots and their
/// roles, the arrangement the frontend draws it with, where the caret enters, how
/// it walks, and the spacing class.
///
/// Taken by **shape name**, which is why a configured command can borrow one:
/// `frac(a, b)` is stored as a `MacroCall`, and `config/commands.json` says its
/// shape is `fraction`, so its two cells get the `numerator`/`denominator` roles
/// and the up/down rule of a fraction — exactly as if it were a stored
/// `Kind::Fraction`.
///
/// The name is the *shape* name, not necessarily the wire name: `view_atom` is
/// free to merge several shapes under one wire kind and does (`Sqrt`, `Fenced`,
/// `Accent` and `Line` all travel as `decorated`). `grid`/`aligned`/`script` are
/// likewise shape names whose wire kinds are `table`/`multiline`/`scripts`.
#[derive(Clone, Copy)]
pub struct Shape {
    /// The frontend's layout strategy for this node. `view_atom` reads it; a
    /// kind that needs a second shape for a fallback names that one separately
    /// (`MacroCall`'s collapsed form).
    ///
    /// The arrangement name is deliberately *not* the variant name: it is the
    /// contract with the frontend, and it only has to change when the drawing
    /// changes. The variant names are the contract with Typst's vocabulary and
    /// are recorded in `typst` below.
    ///
    /// It is also the name `config/commands.json` writes, which is what the
    /// `slots.rs` test holds it to — so it has to stay unique per kind and stable
    /// across renames of the variant.
    ///
    /// That makes it the *shape* name, not necessarily the wire name: `view_atom`
    /// is free to merge several shapes under one wire kind, and does — `Sqrt`,
    /// `Fenced`, `Accent` and `Line` all travel as `decorated`, because their
    /// editing is identical and only their drawing differs. The two names agree
    /// for every other kind.
    pub view: &'static str,
    /// The Typst `MathKind` variants this kind stands for, by name. Empty for
    /// the editor-only kinds. Several names mean the editor either keeps those
    /// Typst kinds as one editable node, or keeps one of them as two nodes
    /// (`Sqrt` and `Root` both claim `Radical`).
    pub typst: &'static [&'static str],
    pub slots: &'static [Slot],
    pub arity: Arity,
    pub entry: Entry,
    pub horizontal: Horiz,
    pub vertical: Vertical,
    /// Math spacing class for kinds whose class is fixed. `Char` derives its
    /// class from the character itself, so its entry here is never read.
    pub class: u8,
}

impl Shape {
    /// The role of cell `index`, or `None` when the schema has no such cell.
    pub fn role_at(&self, index: usize) -> Option<Role> {
        match self.arity {
            Arity::Exact => self.slots.get(index).map(|slot| slot.role),
            Arity::Repeat => self.slots.last().map(|last| self.slots.get(index).map_or(last.role, |slot| slot.role)),
        }
    }
    /// The cell holding `role`, if the schema names it.
    pub fn index_of(&self, role: Role) -> Option<usize> {
        self.slots.iter().position(|slot| slot.role == role)
    }
    /// Whether this shape is a radical. The degree is drawn to the **left** of the
    /// radicand, so the radicand is met from its right — which is what makes the caret
    /// land at the end of the cell it is entered from the right.
    pub fn is_radical(&self) -> bool { self.typst == K_RADICAL }
    /// Whether this is the font-variant shape. It is the one shape applied by
    /// substituting codepoints, so it only applies to a body that is a run of
    /// characters (`typst::has_glyph_run`).
    pub fn is_font_variant(&self) -> bool { self.view == "style" }
}

// The command names live in `config/commands.json`, which `build.rs` turns into the
// `COMMANDS` table below. A name maps to the *view* name of the kind it declares, so
// the file reads against `docs/kind-inventory.md` without a lookup. The symbols table
// is generated into the same file and read by `math`.
use crate::math::configured;

/// The entry `config/commands.json` has for a name.
pub fn command_spec(name: &str) -> Option<&'static configured::Command> {
    configured::COMMANDS.iter().find(|command| command.name == name)
}

/// The shape a configured command name borrows, from `config/commands.json`.
///
/// This is the whole "borrow a shape" step: the file names a shape, the table
/// below answers it. A `MacroCall` then draws, navigates and places its cells by
/// that shape while staying a `MacroCall` in the tree.
pub fn configured_shape(name: &str) -> Option<Shape> {
    shape_named(command_spec(name)?.shape)
}

/// The **drawing data** a command name carries, as a descriptor `Kind`.
///
/// Not stored, and no longer consulted for spelling: a configured call writes back
/// through `Grammar`/`Write::Named`, which needs nothing but its own name. What is
/// left here is the data only the name can supply and only the *drawing* reads —
/// `abs` is the `|` pair, `hat` the accent called `hat`, `overline` the line above.
/// `view_atom` reads it; `configured_shape` answers the slots and navigation.
///
/// It goes away when `config/commands.json` carries those view fields itself.
pub fn configured_kind(name: &str) -> Option<Kind> {
    Some(match shape_named(command_spec(name)?.shape)?.view {
        "delim" => match name {
            "abs" => Kind::Fenced { left: "|".into(), right: "|".into() },
            _ => Kind::Fenced { left: "‖".into(), right: "‖".into() },
        },
        "decoration" => Kind::Accent { name: name.to_string() },
        "line" => Kind::Line { above: name == "overline" },
        "style" => Kind::Style { name: name.to_string() },
        "fraction" => Kind::Fraction,
        "sqrt" => Kind::Sqrt,
        "root" => Kind::Root,
        _ => return None,
    })
}

/// Every command name the editor knows, for a completion list.
pub fn command_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = configured::COMMANDS.iter().map(|command| command.name).collect();
    names.sort_unstable(); names.dedup(); names
}

/// The shape a name in `config/commands.json` stands for.
///
/// A command names its shape by the arrangement the frontend draws it with,
/// because that is the name stable enough to write in a file: a `Kind` variant
/// can be renamed (as `Frac` became `Fraction`) without the command changing
/// meaning, and the arrangement is already the editor's contract with the
/// frontend.
///
/// Only the shapes a command can build **from its name alone** are listed. Two
/// are deliberately absent:
///
/// * `grid` is not here. A table's shape is its source's, not its name's — the
///   columns come from how many cells share a row, which only the argument list
///   says — so `parse_atom` builds one from the source it reads (`mat`'s own
///   branch). A placeholder here used to be the answer for any *other* name a
///   config file pointed at `grid`, and writing such a table reached
///   `cells.chunks(0)` and panicked. Refusing the name instead leaves it as source.
/// * `aligned` is not here. `&` and `\\` are not a call at all: they split the
///   math level into rows before any name is looked at.
fn shape_named(view: &str) -> Option<Shape> {
    Some(match view {
        "fraction" => FRACTION_SHAPE,
        "sqrt" => SQRT_SHAPE,
        "root" => ROOT_SHAPE,
        "delim" => DELIM_SHAPE,
        "line" => LINE_SHAPE,
        "decoration" => DECORATION_SHAPE,
        "style" => STYLE_SHAPE,
        _ => return None,
    })
}

/// Command names whose kind the parser builds from the source, not from a name.
///
/// A table's shape is its argument list's, so these names get a branch in `parse_atom`
/// rather than an answer from `shape_named` — which is why `shape_named` refuses
/// `grid` outright: a name with no such branch would be handed a placeholder table and
/// write it with a column count nobody filled in.
///
/// `mat`, `vec` and `cases` are all three of these names. They differ only in how their
/// arguments become rows and what is drawn around them, and both facts live in
/// `config/commands.json` beside the name.
#[cfg(test)]
fn source_built_commands() -> Vec<&'static str> {
    ["mat", "vec", "cases"].to_vec()
}

/// The view name of every kind a command can build, for the test that holds
/// `config/commands.json` to this list.
#[cfg(test)]
fn command_views() -> Vec<&'static str> {
    ["fraction", "sqrt", "root", "delim", "line", "decoration", "style"].to_vec()
}

// The slot arrays are named constants because `&[…]` written inline in the
// table below would be a temporary: a reference to it cannot be `'static`.
const LEAF: &[Slot] = &[];
const TEXT: &[Slot] = &[Slot::full(Role::Inner)];
const ARGS: &[Slot] = &[Slot::full(Role::Arg)];
const CELL: &[Slot] = &[Slot::full(Role::Cell)];
const FRACTION: &[Slot] = &[Slot::scaled(Role::Numerator, 900), Slot::scaled(Role::Denominator, 900)];
const RADICAND: &[Slot] = &[Slot::full(Role::Radicand)];
/// Cells are `[index, radicand]` — the order Typst's own `root(index, radicand)`
/// takes them, and the order they are read in, the degree standing to the left of
/// the radicand.
///
/// This used to be stored reversed, with an `args.swap(0, 1)` at parse time to
/// make it so and a `root({1}, {0})` template to undo it on the way out. Every
/// reader of the tree then had to know about the reversal: the entry roles, the
/// vertical swap, the horizontal pair rule, `∛x`'s own construction, and a
/// `radical` special case in the caret. Storing the source order removes all six.
const ROOT: &[Slot] = &[Slot::scaled(Role::Index, 550), Slot::full(Role::Radicand)];
/// The shape a script's cells always have: base, upper, lower.
const ATTACH: &[Slot] = &[Slot::full(Role::Base), Slot::blank(Role::Upper, 700), Slot::blank(Role::Lower, 700)];

// The `typst` lists of the table below, named for the same reason.
/// A single character, and a named symbol: both resolve to a glyph, which is why
/// they share one `MathKind` rather than one having no counterpart.
const K_GLYPH: &[&str] = &["Glyph"];
/// A run of digits with at most one dot (`math::is_number`). The lexer keeps such
/// a run in one token, so the editor keeps it in one atom.
const K_NUMBER: &[&str] = &["Number"];
/// Source text the editor does not model: a code expression is a `Box`, a
/// `mathml` element is `Mathml`, and anything else the equation carries (a
/// linebreak, for instance) is `External`.
const K_OPAQUE: &[&str] = &["Box", "Mathml", "External"];
const K_TEXT: &[&str] = &["Text"];
const K_FRACTION: &[&str] = &["Fraction"];
const K_RADICAL: &[&str] = &["Radical"];
const K_SCRIPTS: &[&str] = &["Scripts"];
const K_FENCED: &[&str] = &["Fenced"];
const K_TABLE: &[&str] = &["Table"];
const K_MULTILINE: &[&str] = &["Multiline"];
/// `overline` is Typst's `Line` above the base and `underline` its `Line` below;
/// the editor stores the position and writes whichever command spells it.
const K_LINE: &[&str] = &["Line"];
/// The marks `hat` and `cancel` are Typst's `Accent` and `Cancel` — one body with
/// a mark drawn over it, which is the same shape for both. `overbrace`/
/// `underbrace` and the other spreaders resolve to `Accent` too (with a stretched
/// mark), but they are not commands the editor accepts, so they arrive as `Raw`.
const K_ACCENT: &[&str] = &["Accent", "Cancel"];
const K_NONE: &[&str] = &[];

// One `Shape` per arrangement. They are constants rather than inline literals
// because two tables read them: `Kind::shape` (what a stored node looks like) and
// `shape_named` (what a name in `config/commands.json` borrows).
const CHAR_SHAPE: Shape = Shape {
    view: "char", typst: K_GLYPH, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const SYMBOL_SHAPE: Shape = Shape {
    view: "symbol", typst: K_GLYPH, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const NUMBER_SHAPE: Shape = Shape {
    view: "number", typst: K_NUMBER, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const RAW_SHAPE: Shape = Shape {
    view: "raw", typst: K_OPAQUE, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const UNKNOWN_SHAPE: Shape = Shape {
    view: "unknown", typst: K_NONE, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const PARAMETER_SHAPE: Shape = Shape {
    view: "parameter", typst: K_NONE, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const TEXT_SHAPE: Shape = Shape {
    view: "text", typst: K_TEXT, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const MACRO_SHAPE: Shape = Shape {
    view: "macro", typst: K_NONE, slots: ARGS, arity: Arity::Repeat, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const TEMPLATE_CALL_SHAPE: Shape = Shape {
    view: "template-call", typst: K_NONE, slots: ARGS, arity: Arity::Repeat, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const FRACTION_SHAPE: Shape = Shape {
    view: "fraction", typst: K_FRACTION, slots: FRACTION, arity: Arity::Exact,
    entry: Entry::Role { forward: Role::Numerator, backward: Role::Denominator },
    horizontal: Horiz::Locked,
    vertical: Vertical::Swap { up: Role::Numerator, down: Role::Denominator, end_up: false },
    class: 7,
};
const SQRT_SHAPE: Shape = Shape {
    view: "sqrt", typst: K_RADICAL, slots: RADICAND, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const ROOT_SHAPE: Shape = Shape {
    view: "root", typst: K_RADICAL, slots: ROOT, arity: Arity::Exact,
    // Forward enters the degree, which now stands in cell 0 because the cells are
    // stored in the order they are read.
    entry: Entry::Role { forward: Role::Index, backward: Role::Radicand },
    horizontal: Horiz::Linear,
    vertical: Vertical::Swap { up: Role::Index, down: Role::Radicand, end_up: true },
    class: 0,
};
const SCRIPTS_SHAPE: Shape = Shape {
    view: "script", typst: K_SCRIPTS, slots: ATTACH, arity: Arity::Exact,
    entry: Entry::Role { forward: Role::Base, backward: Role::Base },
    horizontal: Horiz::Locked, vertical: Vertical::Attach, class: 0,
};
const DELIM_SHAPE: Shape = Shape {
    view: "delim", typst: K_FENCED, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const GRID_SHAPE: Shape = Shape {
    view: "grid", typst: K_TABLE, slots: CELL, arity: Arity::Repeat, entry: Entry::GridMiddle,
    horizontal: Horiz::Column, vertical: Vertical::Column, class: 7,
};
const ALIGNED_SHAPE: Shape = Shape {
    view: "aligned", typst: K_MULTILINE, slots: CELL, arity: Arity::Repeat, entry: Entry::Edge,
    horizontal: Horiz::Column, vertical: Vertical::Column, class: 0,
};
const DECORATION_SHAPE: Shape = Shape {
    view: "decoration", typst: K_ACCENT, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const LINE_SHAPE: Shape = Shape {
    view: "line", typst: K_LINE, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};
const STYLE_SHAPE: Shape = Shape {
    view: "style", typst: K_GLYPH, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
    horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
};

/// The Typst `MathKind` variants no `Kind` stands for.
///
/// This is the state of the vocabulary alignment, and a test asserts that the
/// set of unclaimed variants is exactly this — so aligning one of them means
/// editing this list, which is where the reason for the rest stays written down.
/// The three entries are three different decisions:
///
/// * `Group` is not missing: a cell of the editor *is* a group of items, so no
///   kind has to stand for it.
/// * `SkewedFraction` is deliberately not modelled. Typst keeps `a/b` and
///   `frac(a, b)` apart; the editor writes both as `frac(a, b)`, and typing `/`
///   opening a fraction directly is the handier behaviour of the two.
/// * `Primes` is the remaining real gap: `x'` is kept as `Raw` source text today.
///   It would only cover the five-and-up case anyway — `PrimesItem`'s own comment
///   says so, and one to four primes are plain glyphs.
///
/// `Cancel` used to be listed here. It is closed by a name in
/// `config/commands.json` rather than by a new `Kind`, because the shape it needs
/// is `Accent`'s own: one body with a mark drawn over it. The engine's stroke,
/// angle and `inverted` parameters are not modelled, exactly as `Accent`'s
/// geometry is not — the node keeps the body and the command name.
///
/// `Sqrt` and `Root` both claiming `Radical` is the opposite decision and is
/// deliberate too: Typst models a square and an nth root as one item with an
/// optional index, the editor keeps two kinds because their slots differ, and
/// merging them would make the caret reach an empty index cell in a square root.
pub const UNMODELLED: &[&str] = &["Group", "Primes", "SkewedFraction"];

/// The spacing class of a single character, in the ported LyX numbering.
///
/// This is textual data, not structure, so it lives beside the table rather
/// than in it. The closing set was spelled `")] }"` with a stray space, which
/// made a typed space a *closing* class. That space is gone: a space is not a
/// closing delimiter.
///
/// It was reachable, not dead: `interpret_char` inserts into a text cell before
/// its `' '` branch, so an inline string really does hold a `Char(' ')`. The
/// one reader is `move_word` (Ctrl+方向键), which groups a run by class — with
/// the stray space a text run alternated 0/5 and each Ctrl+→ advanced exactly
/// one character; without it the run has a single class and Ctrl+→ moves to
/// the end of the run.
pub fn char_class(value: char) -> u8 {
    if "+−-*".contains(value) { 1 }
    else if "=<>≤≥≠≈".contains(value) { 2 }
    else if ",;:".contains(value) { 3 }
    else if "([{".contains(value) { 4 }
    else if ")]}".contains(value) { 5 }
    else { 0 }
}

impl Kind {
    /// What this node looks like and how the caret moves inside it.
    ///
    /// Exhaustive on purpose: the compiler will not accept a new `Kind` until its
    /// slots, its entry rule, its navigation and the Typst construct it models are
    /// all stated.
    ///
    /// Leaves have no cells of their own, so nothing to enter or walk; each is
    /// shown under its own arrangement.
    pub fn shape(&self) -> Shape {
        match self {
            Kind::Char { .. } => CHAR_SHAPE,
            Kind::Symbol { .. } => SYMBOL_SHAPE,
            // A number is a run of digits in one cell, like a text run: the caret
            // can sit between the digits, and the cell is written as one piece so
            // the run stays one token.
            Kind::Number => NUMBER_SHAPE,
            // Whatever the editor does not model structurally is one opaque
            // fragment of Typst source, whichever MathKind it resolves into.
            Kind::Raw { .. } => RAW_SHAPE,
            Kind::Unknown { .. } => UNKNOWN_SHAPE,
            Kind::Parameter { .. } => PARAMETER_SHAPE,
            // A text run holds its characters in one cell, but is written as one
            // quoted string rather than as those characters.
            Kind::Text => TEXT_SHAPE,
            // Macro calls: one cell per argument, entered at the first and left
            // at the last. `delete` and the projection rules are separate
            // obligations and still live with the caller. A call that cannot be
            // expanded shows up as `view::MACRO_COLLAPSED`, not as this view.
            Kind::MacroCall { .. } => MACRO_SHAPE,
            // Template edges exist only inside an expanded macro template.
            Kind::TemplateCall { .. } => TEMPLATE_CALL_SHAPE,
            Kind::Fraction => FRACTION_SHAPE,
            // Typst has one `Radical` with an optional index; the editor keeps
            // the index as a cell of its own, so a square root is a radical
            // whose index cell is empty. Which of the two kinds a node is comes
            // from how many cells it stores, and the two tags below are how the
            // wire tells the frontend to draw a hook or a degree.
            Kind::Sqrt => SQRT_SHAPE,
            Kind::Root => ROOT_SHAPE,
            // Storage is always `[base, upper, lower]`; an empty cell is an
            // attachment the source does not have (`math::script_cell`). Typst's
            // `ScriptsItem` has six attachment fields because it separates
            // limits from scripts and keeps the left ones; which of the two a
            // cell is, is decided by the compiler and asked for separately
            // (`native-adapter`), not stored here.
            Kind::Scripts => SCRIPTS_SHAPE,
            // The delimiters are the two characters the source spelled, not
            // items as in Typst's `FencedItem`, and the one cell is the body
            // between them.
            Kind::Fenced { .. } => DELIM_SHAPE,
            Kind::Table { .. } => GRID_SHAPE,
            Kind::Multiline { .. } => ALIGNED_SHAPE,
            // A mark above or below the base; its name is the callee and the cell
            // is its body. Typst's `AccentItem` derives above/below from the mark
            // itself, so nothing here stores it.
            Kind::Accent { .. } => DECORATION_SHAPE,
            // A rule above or below the base. Only the position is stored, because
            // that is all `LineItem` has.
            Kind::Line { .. } => LINE_SHAPE,
            // A base drawn in a font variant. Its slots are a decoration's — one inner
            // cell, entered at the edge, linear, no vertical move — because the variant
            // changes how the body is *drawn*, not how it is edited. What it does not
            // have is a glyph: Typst substitutes codepoints, and that table is out of the
            // kernel's reach, so the frontend asks the engine (`Shape::typst` records the
            // `Glyph`s it stands for, once the substitution has happened).
            Kind::Style { .. } => STYLE_SHAPE,
        }
    }

    /// How this node spells itself back into the document.
    ///
    /// The second half of the split, and the half that does **not** follow a
    /// borrowed shape: a call is spelled as a call (`Write::Named`), whatever
    /// arrangement it borrows for drawing. `frac(a, b)` is `MacroCall`, so it
    /// writes back through its own name and no `fraction` template is involved.
    pub fn grammar(&self) -> Grammar {
        Grammar { write: match self {
            Kind::Char { .. } | Kind::Symbol { .. } | Kind::Raw { .. } | Kind::Unknown { .. } => Write::OwnText,
            Kind::Number => Write::Run,
            Kind::Text => Write::Quoted,
            Kind::Parameter { .. } => Write::Marker,
            Kind::MacroCall { .. } => Write::Named,
            Kind::TemplateCall { .. } => Write::TemplateOnly,
            Kind::Fraction => Write::Template("frac({0}, {1})"),
            Kind::Sqrt => Write::Template("sqrt({0})"),
            // Stored order is Typst's own `root(index, radicand)`, so the
            // template reads in the same order and no reversal is stated here.
            Kind::Root => Write::Template("root({0}, {1})"),
            Kind::Scripts => Write::Attach,
            Kind::Fenced { .. } => Write::Delimited,
            Kind::Table { .. } => Write::Matrix,
            Kind::Multiline { .. } => Write::Rows,
            Kind::Accent { .. } | Kind::Style { .. } => Write::Template("{name}({0})"),
            Kind::Line { .. } => Write::Positioned { above: "overline({0})", below: "underline({0})" },
        } }
    }
}

/// Views that are not the whole story for their kind.
pub mod view {
    /// A macro call the editor cannot expand. It is the `RawMacro` arrangement: a
    /// known callee drawn as a compiled **image of its own source** while the caret is
    /// outside it, and as its name plus argument slots once the caret enters, because
    /// the kernel has no template to instantiate. `Kind::MacroCall`'s declaration names
    /// the expandable form.
    pub const MACRO_COLLAPSED: &str = "raw_macro";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{MathAtom, MathData};

    /// One representative per `Kind`, with the number of cells it really stores.
    fn representatives() -> Vec<(&'static str, Kind, usize)> {
        vec![
            ("Char", Kind::Char { text: "x".into() }, 0),
            ("Symbol", Kind::Symbol { name: "arrow".into(), glyph: "→".into() }, 0),
            ("Number", Kind::Number, 1),
            ("Raw", Kind::Raw { source: "dif".into() }, 0),
            ("Unknown", Kind::Unknown { name: "fra".into(), saved: MathData::new(), caret: 3, anchor: None, original: None }, 0),
            ("Parameter", Kind::Parameter { index: 0 }, 0),
            ("Text", Kind::Text, 1),
            ("MacroCall", Kind::MacroCall { name: "f".into(), function: true }, 2),
            ("TemplateCall", Kind::TemplateCall { definition: 0 }, 2),
            ("Fraction", Kind::Fraction, 2),
            ("Sqrt", Kind::Sqrt, 1),
            ("Root", Kind::Root, 2),
            ("Scripts", Kind::Scripts, 3),
            ("Fenced", Kind::Fenced { left: "(".into(), right: ")".into() }, 1),
            ("Table", Kind::Table { columns: 2, row_lengths: vec![2, 2], name: "mat".into() }, 4),
            ("Multiline", Kind::Multiline { columns: 2, row_lengths: vec![2, 2] }, 4),
            ("Accent", Kind::Accent { name: "hat".into() }, 1),
            ("Line", Kind::Line { above: true }, 1),
            ("Style", Kind::Style { name: "bold".into() }, 1),
        ]
    }

    #[test]
    fn every_kind_has_a_representative_here() {
        // `Kind::shape` makes a new kind fail to compile; this count is what
        // makes a new kind fail to be *covered* by this file.
        assert_eq!(representatives().len(), 19, "新增 Kind 后请在这里补一条代表实例");
    }

    #[test]
    fn named_roles_exist_in_the_schema_that_names_them() {
        for (label, kind, _) in representatives() {
            let shape = kind.shape();
            if let Entry::Role { forward, backward } = shape.entry {
                assert!(shape.index_of(forward).is_some(), "{label}：入口角色 {forward:?} 不在槽位表里");
                assert!(shape.index_of(backward).is_some(), "{label}：入口角色 {backward:?} 不在槽位表里");
            }
            if let Vertical::Swap { up, down, .. } = shape.vertical {
                assert!(shape.index_of(up).is_some(), "{label}：上移角色 {up:?} 不在槽位表里");
                assert!(shape.index_of(down).is_some(), "{label}：下移角色 {down:?} 不在槽位表里");
            }
        }
    }

    #[test]
    fn the_schema_covers_exactly_the_cells_a_kind_stores() {
        for (label, kind, cells) in representatives() {
            let shape = kind.shape();
            match shape.arity {
                Arity::Exact => {
                    for index in 0..shape.slots.len() {
                        assert!(shape.role_at(index).is_some(), "{label}：第 {index} 格没有角色");
                    }
                    assert!(shape.role_at(shape.slots.len()).is_none(), "{label}：定长图式多出了一格");
                    assert_eq!(shape.slots.len(), cells, "{label}：槽位数与真实格子数不符");
                }
                Arity::Repeat => {
                    assert!(!shape.slots.is_empty(), "{label}：重复图式必须有模式");
                    assert!(cells >= shape.slots.len(), "{label}：重复图式的模式比格子还多");
                    for index in 0..cells.max(8) {
                        assert!(shape.role_at(index).is_some(), "{label}：第 {index} 格没有角色");
                    }
                }
            }
        }
    }

    #[test]
    fn entry_cells_stay_inside_the_cells_they_describe() {
        for (label, kind, cells) in representatives() {
            if cells == 0 { continue; }
            let atom = MathAtom::nest(kind, cells);
            for forward in [true, false] {
                let entry = atom.entry_cell(forward);
                assert!(entry < cells, "{label}：入口格子 {entry} 越界（共 {cells} 格）");
            }
        }
    }

    #[test]
    fn horizontal_neighbours_stay_inside_the_cells_too() {
        for (label, kind, cells) in representatives() {
            if cells == 0 { continue; }
            let atom = MathAtom::nest(kind, cells);
            for index in 0..cells {
                for forward in [true, false] {
                    if let Some(next) = atom.idx_horizontal(index, forward) {
                        assert!(next < cells, "{label}：第 {index} 格向{}走到越界的 {next}", if forward { "右" } else { "左" });
                    }
                }
            }
        }
    }

    #[test]
    fn a_character_class_follows_the_character() {
        for (value, class) in [('+', 1), ('=', 2), (',', 3), ('(', 4), (')', 5), ('x', 0)] {
            assert_eq!(MathAtom::character(value).math_class(), class, "{value:?}");
        }
        // A space is neither an opening nor a closing delimiter.
        assert_eq!(MathAtom::character(' ').math_class(), 0);
        // A fixed class comes from the table, not from the contents.
        assert_eq!(MathAtom::nest(Kind::Fraction, 2).math_class(), 7);
        assert_eq!(MathAtom::nest(Kind::Table { columns: 2, row_lengths: vec![2, 2], name: "mat".into() }, 4).math_class(), 7);
        assert_eq!(MathAtom::nest(Kind::Multiline { columns: 2, row_lengths: vec![2, 2] }, 4).math_class(), 0);
        assert_eq!(MathAtom::nest(Kind::Sqrt, 1).math_class(), 0);
    }

    /// `config/commands.json` is a file, so nothing in the type system holds it to
    /// the kinds it names. These are the checks that do.
    ///
    /// A command whose kind needs the source to know its *shape* is listed here too,
    /// because the file still has to say the name is one the editor knows — it is
    /// `shape_named` that cannot answer it, not the file that is wrong. `mat` is
    /// the only one: its columns are its argument list's.
    #[test]
    fn the_command_file_names_kinds_that_exist_and_are_commands() {
        assert!(!configured::COMMANDS.is_empty(), "config/commands.json 是空的");
        for command in configured::COMMANDS {
            let (name, view) = (command.name, command.shape);
            // Every name must mean *something* to the parser: either this table answers
            // it, or a branch of `parse_atom` builds this **specific name** from the
            // source. The check is on the name, not on the view: `mat` builds a table
            // because `parse_atom` has a branch for `mat`, and that says nothing about
            // whether some other name may claim `grid` — a name that did would reach the
            // writer with a `Table { columns: 0 }` and panic.
            if let Some(shape) = configured_shape(name) {
                assert_eq!(shape.view, view, "{name} 指向 {view}，但建出来的是 {}", shape.view);
                assert!(command_views().contains(&view), "{view} 不是一个命令能建出来的形状");
            } else {
                assert!(source_built_commands().contains(&name),
                        "config/commands.json 里的 {name} 指向 {view}，但解析器既查不到这个名字、也没有从源码建它的分支");
            }
        }
        // The reverse: a kind a command can build has a name unless it is made some
        // other way. `Sqrt` and `Root` share a view family but not a view, and both
        // are named, so the only unnamed ones are the kinds no command declares.
        for view in command_views() {
            assert!(configured::COMMANDS.iter().any(|command| command.shape == view),
                    "{view} 是命令能建的 Kind，却没有命令名");
        }
    }

    #[test]
    fn every_declaration_agrees_with_the_kind_it_describes() {
        // `write_atom` dispatches on the declared `Write`, then reads the data
        // that shape needs and stops with `unreachable!` if the kind has none.
        // This test is what makes those arms unreachable rather than a guess.
        for (label, kind, _) in representatives() {
            let agrees = match kind.grammar().write {
                Write::OwnText => matches!(kind, Kind::Char { .. } | Kind::Symbol { .. } | Kind::Raw { .. } | Kind::Unknown { .. }),
                Write::Marker => matches!(kind, Kind::Parameter { .. }),
                Write::TemplateOnly => matches!(kind, Kind::TemplateCall { .. }),
                Write::Quoted => matches!(kind, Kind::Text),
                Write::Run => matches!(kind, Kind::Number),
                Write::Attach => matches!(kind, Kind::Scripts),
                Write::Delimited => matches!(kind, Kind::Fenced { .. }),
                Write::Matrix => matches!(kind, Kind::Table { .. }),
                Write::Rows => matches!(kind, Kind::Multiline { .. }),
                Write::Named => matches!(kind, Kind::MacroCall { .. }),
                Write::Template(_) => matches!(kind, Kind::Fraction | Kind::Sqrt | Kind::Root | Kind::Accent { .. } | Kind::Style { .. }),
                Write::Positioned { .. } => matches!(kind, Kind::Line { .. }),
            };
            assert!(agrees, "{label}：write 声明与 Kind 不符，写回会走到 unreachable");
        }
    }

    #[test]
    fn a_template_only_names_placeholders_that_exist() {
        for (label, kind, cells) in representatives() {
            assert!(!kind.shape().view.is_empty(), "{label}：没有声明形状名");
            let Write::Template(template) = kind.grammar().write else { continue };
            let mut used = 0;
            let mut rest = template;
            while let Some(open) = rest.find('{') {
                let close = rest[open..].find('}').unwrap_or_else(|| panic!("{label}：模板占位符没有闭合：{template}"));
                let key = &rest[open + 1..open + close];
                if key == "name" {
                    assert!(matches!(kind, Kind::MacroCall { .. } | Kind::Accent { .. } | Kind::Style { .. }), "{label}：模板用了 {{name}}，但这个 Kind 没有名字");
                } else {
                    let index = key.parse::<usize>().unwrap_or_else(|_| panic!("{label}：无法解析的占位符 {{{key}}}"));
                    assert!(index < cells, "{label}：占位符 {{{key}}} 超出 {cells} 个格子");
                    used += 1;
                }
                rest = &rest[open + close + 1..];
            }
            assert!(used > 0, "{label}：写成模板却一个格子都没用到");
        }
    }

    /// The `MathKind` variant names, read out of the vendored Typst source.
    ///
    /// The editor core deliberately does not depend on the compiler (only on
    /// `typst-syntax`), so the two vocabularies cannot be tied together by the
    /// type system from here. The next best thing is to read the file the table
    /// was written against: a variant Typst adds, renames or removes makes these
    /// tests fail, instead of leaving a name in the table that nothing answers.
    fn typst_math_kinds() -> Vec<&'static str> {
        const SOURCE: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../vendor/typst/crates/typst-library/src/math/ir/item.rs"
        ));
        let at = SOURCE.find("pub enum MathKind<").expect("vendored Typst 仍应声明 MathKind");
        let body = &SOURCE[at..];
        let body = &body[body.find('{').expect("枚举声明应有左花括号") + 1..];
        let body = &body[..body.find("\n}").expect("枚举声明应有右花括号")];
        body.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("///"))
            .map(|line| line.split(|c: char| !c.is_alphanumeric()).next().unwrap_or(""))
            .filter(|name| !name.is_empty())
            .collect()
    }

    #[test]
    fn the_math_kinds_are_read_correctly_from_the_vendored_source() {
        // Without this, a broken scan would return an empty (or partial) list
        // and the coverage test below would pass by saying nothing.
        let names = typst_math_kinds();
        assert_eq!(names.len(), 18, "vendored 的 MathKind 变体数变了：{names:?}");
        for expected in [
            "Group", "Multiline", "Radical", "Fenced", "Fraction", "SkewedFraction",
            "Table", "Scripts", "Accent", "Cancel", "Line", "Primes", "Text",
            "Number", "Glyph", "Box", "Mathml", "External",
        ] {
            assert!(names.contains(&expected), "扫描 MathKind 时漏了 {expected}：{names:?}");
        }
    }

    #[test]
    fn the_typst_vocabulary_is_covered_exactly_once() {
        let all = typst_math_kinds();
        let mut claimed: Vec<&str> = vec![];
        for (label, kind, _) in representatives() {
            for name in kind.shape().typst {
                assert!(all.contains(name), "{label}：声明对应 MathKind::{name}，但 vendored 枚举里没有这个变体");
                claimed.push(name);
            }
        }
        let mut unclaimed: Vec<&str> = all.iter().copied().filter(|name| !claimed.contains(name)).collect();
        unclaimed.sort_unstable();
        // This is the alignment checklist: every entry here is a construct the
        // editor keeps as `Raw` source text, or one it does not have to model.
        assert_eq!(unclaimed, UNMODELLED, "未建模的 MathKind 清单变了：请更新 UNMODELLED 并写下每一项的决定");
        // Sharing a variant is a decision, not an accident. `Glyph` is shared
        // because a character and a named symbol really are both glyphs;
        // `Radical` is shared because the editor keeps a square root and an nth
        // root apart while Typst models them as one item.
        let mut sorted = claimed.clone();
        sorted.sort_unstable();
        let mut shared: Vec<&str> = sorted.windows(2).filter(|pair| pair[0] == pair[1]).map(|pair| pair[1]).collect();
        shared.dedup();
        assert_eq!(shared, ["Glyph", "Radical"], "被两个 Kind 共同认领的 MathKind 变了");
    }
}
