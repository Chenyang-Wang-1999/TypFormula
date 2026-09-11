// SPDX-License-Identifier: GPL-2.0-or-later
//! Every `Kind`'s slot schema, declared once.
//!
//! `Kind` carries instance data (how many columns, which script exists); this
//! module says what a node's cells *mean* and how the caret moves between them.
//! The accessors that used to switch on `Kind` with a `_ =>` catch-all now read
//! `Kind::decl()`, so a new kind is one exhaustive match arm instead of a hunt
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
    /// Only between two cells (`root`).
    Pair,
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
/// argument order: `"root({1}, {0})"` is where the reverse of Typst's own
/// `root(index, radicand)` becomes visible, instead of hiding in a hand-written
/// `c(1), c(0)` that has to stay in step with the `args.swap(0, 1)` on the parse
/// side. Placeholders are `{0}`, `{1}`… for cells and `{name}` for the node's
/// stored name.
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

/// The schema of one `Kind`.
#[derive(Clone, Copy)]
pub struct Decl {
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
    pub write: Write,
}

impl Decl {
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

/// The kind a Typst call means, from `config/commands.json`.
pub fn command_kind(name: &str) -> Option<Kind> {
    kind_for_view(command_spec(name)?.shape)
}

/// The kind a command name means, **with the data its own name supplies**.
///
/// This is the one place a name becomes a complete shape. Three kinds carry data in
/// their variant that only the name can say — `abs` is the `|` pair, `hat` the accent
/// called `hat`, `overline` the line above — so `kind_for_view` answers them blank and
/// someone has to fill them in. That used to be `parse_atom`, which meant anything
/// *else* asking "what shape is `hat`?" (the view projection, the writer) got the blank
/// and drew an accent with no name. Consolidating it here is what lets a `MacroCall`
/// borrow a shape without the call site having to know how the shape is spelled.
pub fn configured_kind(name: &str) -> Option<Kind> {
    Some(match command_kind(name)? {
        Kind::Accent { .. } => Kind::Accent { name: name.to_string() },
        Kind::Style { .. } => Kind::Style { name: name.to_string() },
        Kind::Line { .. } => Kind::Line { above: name == "overline" },
        Kind::Fenced { .. } => match name {
            "abs" => Kind::Fenced { left: "|".into(), right: "|".into() },
            _ => Kind::Fenced { left: "‖".into(), right: "‖".into() },
        },
        other => other,
    })
}

/// Every command name the editor knows, for a completion list.
pub fn command_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = configured::COMMANDS.iter().map(|command| command.name).collect();
    names.sort_unstable(); names.dedup(); names
}

/// The kind a view name stands for.
///
/// A command names its kind by the arrangement the frontend draws it with, because
/// that is the name stable enough to write in a file: a `Kind` variant can be renamed
/// (as `Frac` became `Fraction`) without the command changing meaning, and `view` is
/// already the editor's contract with the frontend.
///
/// Only the kinds a command can build **from its name alone** are listed. Two kinds
/// are deliberately absent:
///
/// * `grid` is not here. A table's shape is its source's, not its name's — the
///   columns come from how many cells share a row, which only the argument list says
///   — so `parse_atom` builds one from the source it reads (`mat`'s own branch). A
///   placeholder `columns: 0` here used to be the answer for any *other* name a
///   config file pointed at `grid`, and writing such a table reached
///   `cells.chunks(0)` and panicked. Refusing the name instead leaves it as source.
/// * `multiline` is not here. `&` and `\\` are not a call at all: they split the
///   math level into rows before any name is looked at.
///
/// The three kinds that carry data in their own variant are answered with a blank
/// value, which the parser then fills from the source it is reading — the command's
/// name is what says which: `abs` is the `|` pair, `hat` the accent called `hat`,
/// `overline` the line above.
fn kind_for_view(view: &str) -> Option<Kind> {
    Some(match view {
        "fraction" => Kind::Fraction,
        "sqrt" => Kind::Sqrt,
        "root" => Kind::Root,
        "delim" => Kind::Fenced { left: String::new(), right: String::new() },
        "line" => Kind::Line { above: false },
        "decoration" => Kind::Accent { name: String::new() },
        "style" => Kind::Style { name: String::new() },
        _ => return None,
    })
}

/// Command names whose kind the parser builds from the source, not from a name.
///
/// A table's shape is its argument list's, so these names get a branch in `parse_atom`
/// rather than an answer from `kind_for_view` — which is why `kind_for_view` refuses
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
/// Cells are `[radicand, index]`, the reverse of Typst's `root(index, radicand)`.
const ROOT: &[Slot] = &[Slot::full(Role::Radicand), Slot::scaled(Role::Index, 550)];
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
    /// The slot schema. Exhaustive on purpose: the compiler will not accept a
    /// new `Kind` until its cells, its spelling, its entry rule, its navigation
    /// and the Typst construct it models are all stated.
    ///
    /// Key order below is `view`, `typst`, `commands`, `slots`, `arity`, `entry`,
    /// `horizontal`, `vertical`, `class`, `write` — the same order as `Decl`,
    /// so the table reads as one column per obligation.
    pub fn decl(&self) -> Decl {
        match self {
            // Leaves: no cells of their own, so nothing to enter or walk. Each is
            // spelled by its own stored text but shows up under its own view.
            Kind::Char { .. } => Decl {
                view: "char", typst: K_GLYPH, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::OwnText,
            },
            Kind::Symbol { .. } => Decl {
                view: "symbol", typst: K_GLYPH, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::OwnText,
            },
            // A number is a run of digits in one cell, like a text run: the caret
            // can sit between the digits, and the cell is written as one piece so
            // the run stays one token.
            Kind::Number => Decl {
                view: "number", typst: K_NUMBER, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::Run,
            },
            // Whatever the editor does not model structurally is one opaque
            // fragment of Typst source, whichever MathKind it resolves into.
            Kind::Raw { .. } => Decl {
                view: "raw", typst: K_OPAQUE, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::OwnText,
            },
            Kind::Unknown { .. } => Decl {
                view: "unknown", typst: K_NONE, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::OwnText,
            },
            Kind::Parameter { .. } => Decl {
                view: "parameter", typst: K_NONE, slots: LEAF, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::Marker,
            },
            // A text run holds its characters in one cell, but is written as one
            // quoted string rather than as those characters.
            Kind::Text => Decl {
                view: "text", typst: K_TEXT, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::Quoted,
            },
            // Macro calls: one cell per argument, entered at the first and left
            // at the last. `delete` and the projection rules are separate
            // obligations and still live with the caller. A call that cannot be
            // expanded shows up as `view::MACRO_COLLAPSED`, not as this view.
            Kind::MacroCall { .. } => Decl {
                view: "macro", typst: K_NONE, slots: ARGS, arity: Arity::Repeat, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::Named,
            },
            // Template edges exist only inside an expanded macro template.
            Kind::TemplateCall { .. } => Decl {
                view: "template-call", typst: K_NONE, slots: ARGS, arity: Arity::Repeat, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::TemplateOnly,
            },
            Kind::Fraction => Decl {
                view: "fraction", typst: K_FRACTION, slots: FRACTION, arity: Arity::Exact,
                entry: Entry::Role { forward: Role::Numerator, backward: Role::Denominator },
                horizontal: Horiz::Locked,
                vertical: Vertical::Swap { up: Role::Numerator, down: Role::Denominator, end_up: false },
                class: 7, write: Write::Template("frac({0}, {1})"),
            },
            // Typst has one `Radical` with an optional index; the editor keeps
            // the index as a cell of its own, so a square root is a radical
            // whose index cell is empty. Which of the two kinds a node is comes
            // from how many cells it stores, and the two tags below are how the
            // wire tells the frontend to draw a hook or a degree.
            Kind::Sqrt => Decl {
                view: "sqrt", typst: K_RADICAL, slots: RADICAND, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::Template("sqrt({0})"),
            },
            // The stored cells are `[radicand, index]` while Typst writes
            // `root(index, radicand)`. The template below is where that reversal
            // is stated; `parse_atom` swaps the parsed arguments to match it.
            Kind::Root => Decl {
                view: "root", typst: K_RADICAL, slots: ROOT, arity: Arity::Exact,
                entry: Entry::Role { forward: Role::Index, backward: Role::Radicand },
                horizontal: Horiz::Pair,
                vertical: Vertical::Swap { up: Role::Index, down: Role::Radicand, end_up: true },
                class: 0, write: Write::Template("root({1}, {0})"),
            },
            // Storage is always `[base, upper, lower]`; an empty cell is an
            // attachment the source does not have (`math::script_cell`). Typst's
            // `ScriptsItem` has six attachment fields because it separates
            // limits from scripts and keeps the left ones; which of the two a
            // cell is, is decided by the compiler and asked for separately
            // (`native-adapter`), not stored here.
            Kind::Scripts => Decl {
                view: "script", typst: K_SCRIPTS, slots: ATTACH, arity: Arity::Exact,
                entry: Entry::Role { forward: Role::Base, backward: Role::Base },
                horizontal: Horiz::Locked, vertical: Vertical::Attach, class: 0, write: Write::Attach,
            },
            // The delimiters are the two characters the source spelled, not
            // items as in Typst's `FencedItem`, and the one cell is the body
            // between them.
            Kind::Fenced { .. } => Decl {
                view: "delim", typst: K_FENCED, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::Delimited,
            },
            Kind::Table { .. } => Decl {
                view: "grid", typst: K_TABLE, slots: CELL, arity: Arity::Repeat, entry: Entry::GridMiddle,
                horizontal: Horiz::Column, vertical: Vertical::Column, class: 7, write: Write::Matrix,
            },
            Kind::Multiline { .. } => Decl {
                view: "aligned", typst: K_MULTILINE, slots: CELL, arity: Arity::Repeat, entry: Entry::Edge,
                horizontal: Horiz::Column, vertical: Vertical::Column, class: 0, write: Write::Rows,
            },
            // A mark above or below the base; its name is the callee and the cell
            // is its body. Typst's `AccentItem` derives above/below from the mark
            // itself, so nothing here stores it.
            Kind::Accent { .. } => Decl {
                view: "decoration", typst: K_ACCENT, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0, write: Write::Template("{name}({0})"),
            },
            // A rule above or below the base. Only the position is stored, because
            // that is all `LineItem` has; the template pair below spells it.
            Kind::Line { .. } => Decl {
                view: "line", typst: K_LINE, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
                write: Write::Positioned { above: "overline({0})", below: "underline({0})" },
            },
            // A base drawn in a font variant. Its slots are a decoration's — one inner
            // cell, entered at the edge, linear, no vertical move — because the variant
            // changes how the body is *drawn*, not how it is edited. What it does not
            // have is a glyph: Typst substitutes codepoints, and that table is out of the
            // kernel's reach, so the frontend asks the engine (`Decl::typst` records the
            // `Glyph`s it stands for, once the substitution has happened).
            Kind::Style { .. } => Decl {
                view: "style", typst: K_GLYPH, slots: TEXT, arity: Arity::Exact, entry: Entry::Edge,
                horizontal: Horiz::Linear, vertical: Vertical::None, class: 0,
                write: Write::Template("{name}({0})"),
            },
        }
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
        // `Kind::decl` makes a new kind fail to compile; this count is what
        // makes a new kind fail to be *covered* by this file.
        assert_eq!(representatives().len(), 19, "新增 Kind 后请在这里补一条代表实例");
    }

    #[test]
    fn named_roles_exist_in_the_schema_that_names_them() {
        for (label, kind, _) in representatives() {
            let decl = kind.decl();
            if let Entry::Role { forward, backward } = decl.entry {
                assert!(decl.index_of(forward).is_some(), "{label}：入口角色 {forward:?} 不在槽位表里");
                assert!(decl.index_of(backward).is_some(), "{label}：入口角色 {backward:?} 不在槽位表里");
            }
            if let Vertical::Swap { up, down, .. } = decl.vertical {
                assert!(decl.index_of(up).is_some(), "{label}：上移角色 {up:?} 不在槽位表里");
                assert!(decl.index_of(down).is_some(), "{label}：下移角色 {down:?} 不在槽位表里");
            }
        }
    }

    #[test]
    fn the_schema_covers_exactly_the_cells_a_kind_stores() {
        for (label, kind, cells) in representatives() {
            let decl = kind.decl();
            match decl.arity {
                Arity::Exact => {
                    for index in 0..decl.slots.len() {
                        assert!(decl.role_at(index).is_some(), "{label}：第 {index} 格没有角色");
                    }
                    assert!(decl.role_at(decl.slots.len()).is_none(), "{label}：定长图式多出了一格");
                    assert_eq!(decl.slots.len(), cells, "{label}：槽位数与真实格子数不符");
                }
                Arity::Repeat => {
                    assert!(!decl.slots.is_empty(), "{label}：重复图式必须有模式");
                    assert!(cells >= decl.slots.len(), "{label}：重复图式的模式比格子还多");
                    for index in 0..cells.max(8) {
                        assert!(decl.role_at(index).is_some(), "{label}：第 {index} 格没有角色");
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
    /// `kind_for_view` that cannot answer it, not the file that is wrong. `mat` is
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
            if let Some(kind) = command_kind(name) {
                assert_eq!(kind.decl().view, view, "{name} 指向 {view}，但建出来的是 {}", kind.decl().view);
                assert!(command_views().contains(&view), "{view} 不是一个命令能建出来的 Kind");
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
            let agrees = match kind.decl().write {
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
            assert!(!kind.decl().view.is_empty(), "{label}：没有声明视图名");
            let Write::Template(template) = kind.decl().write else { continue };
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
            for name in kind.decl().typst {
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
