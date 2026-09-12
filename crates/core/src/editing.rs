// SPDX-License-Identifier: GPL-2.0-or-later
//! The editing model: where the caret goes, and which cells it can reach.
//!
//! This is the third layer of `docs/editing-model.md`. `Shape` says what a node's
//! box is (its items, its arrangement, its spacing class); this module says how the
//! caret moves inside that box. The two are **separate tables joined by the shape
//! name**, and the direction of the dependency is editing → `Shape`: the rules here
//! *name* roles (`Role::Numerator`), and `Shape` is what declares which roles exist.
//! Reversing it — writing "this cell is editable" into `Shape` — is what this split
//! removes, because reachability is a property of the *instance*, not of the shape
//! (`docs/editing-model.md` §3).
//!
//! The rules are static and keyed by shape name, so a configured command that
//! borrows `fraction` navigates exactly like a stored `Kind::Fraction`.
//!
//! The other half of the editing model is the **binding**: which cell is a hole, and
//! which position of the editable tree it therefore edits. That is per instance, and
//! it is structure rather than a table — a hole is `view::ViewTemplate::Hole`, and the
//! argument view substituted at binding time carries the call site's own `slices`
//! (`docs/editing-model.md` §3.2).
//!
//! `tests/caret_navigation.rs` guards the rules and
//! `every_shape_declares_its_editing_rules` guards the join between the two tables.

use crate::math::MathAtom;
use crate::slots::Role;

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

/// Where the caret goes inside one node, and how it walks between its cells.
///
/// No field here is part of the display contract: the frontend never reads any of
/// them (`Shape.view` is what it dispatches on). They are the editor's own rules
/// for a box it already knows the arrangement of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rules {
    pub entry: Entry,
    pub horizontal: Horiz,
    pub vertical: Vertical,
    /// Whether a **neighbouring** cell met by walking left/right is entered at its
    /// **end** rather than its start.
    ///
    /// A radical reads its degree before its radicand, so the radicand is met from
    /// its right; a grid meets a row from its last column. This used to be asked as
    /// `Shape::is_radical()` plus a `matches!` on two `Kind`s — which is why a
    /// `root(...)` call, stored as a `MacroCall`, silently changed where the caret
    /// landed until the question was moved onto the shape.
    pub back_lands_at_end: bool,
}

/// The rules of every shape with nothing to say: enter at the edge, walk the cells
/// linearly, no vertical move. Leaves and one-cell wrappers are all like this.
const PLAIN: Rules = Rules {
    entry: Entry::Edge,
    horizontal: Horiz::Linear,
    vertical: Vertical::None,
    back_lands_at_end: false,
};

const FRACTION_RULES: Rules = Rules {
    entry: Entry::Role { forward: Role::Numerator, backward: Role::Denominator },
    horizontal: Horiz::Locked,
    vertical: Vertical::Swap { up: Role::Numerator, down: Role::Denominator, end_up: false },
    back_lands_at_end: false,
};
/// Forward enters the degree, which stands in cell 0 because the cells are stored
/// in the order they are read.
const ROOT_RULES: Rules = Rules {
    entry: Entry::Role { forward: Role::Index, backward: Role::Radicand },
    horizontal: Horiz::Linear,
    vertical: Vertical::Swap { up: Role::Index, down: Role::Radicand, end_up: true },
    back_lands_at_end: true,
};
const SCRIPTS_RULES: Rules = Rules {
    entry: Entry::Role { forward: Role::Base, backward: Role::Base },
    horizontal: Horiz::Locked,
    vertical: Vertical::Attach,
    back_lands_at_end: false,
};
const GRID_RULES: Rules = Rules {
    entry: Entry::GridMiddle,
    horizontal: Horiz::Column,
    vertical: Vertical::Column,
    back_lands_at_end: true,
};
const ALIGNED_RULES: Rules = Rules {
    entry: Entry::Edge,
    horizontal: Horiz::Column,
    vertical: Vertical::Column,
    back_lands_at_end: true,
};
/// A bare hook has one cell and nothing to walk to, but it is still a radical: it
/// shares `ROOT_RULES`' reading direction, and its degree cell is what `root` adds.
const SQRT_RULES: Rules = Rules { back_lands_at_end: true, ..PLAIN };

/// The rules of every shape, keyed by the shape name (`Shape::view`).
///
/// Keyed by the **name** rather than by a `Shape` value because that is the join
/// between the two tables: `Shape` is `Copy` and taken by value, so two shapes with
/// the same structure would be one key here and two entries there. The name is also
/// what a command borrows (`config/commands.json` writes it), which is what makes a
/// configured `frac(a, b)` navigate like a stored `Kind::Fraction`.
///
/// `every_shape_declares_its_editing_rules` is what keeps this table and the shape
/// table from drifting apart; without it, a shape with no entry would silently be
/// navigated as `PLAIN`.
const RULES: &[(&str, Rules)] = &[
    ("char", PLAIN),
    ("symbol", PLAIN),
    ("number", PLAIN),
    ("raw", PLAIN),
    ("unknown", PLAIN),
    ("text", PLAIN),
    ("parameter", PLAIN),
    ("macro", PLAIN),
    ("template-call", PLAIN),
    ("delim", PLAIN),
    ("decoration", PLAIN),
    ("line", PLAIN),
    ("style", PLAIN),
    ("sqrt", SQRT_RULES),
    ("fraction", FRACTION_RULES),
    ("root", ROOT_RULES),
    ("script", SCRIPTS_RULES),
    ("grid", GRID_RULES),
    ("aligned", ALIGNED_RULES),
];

/// The editing rules of one shape, by shape name.
///
/// A miss is a claim about a shape that the `Shape` table does not declare — the
/// disagreement `every_shape_declares_its_editing_rules` exists to catch — so it
/// stops here rather than navigating the node by a guess. Same reasoning as
/// `write_atom`'s `unreachable!` arms: a wrong answer is worse than no answer.
pub fn rules(shape: &str) -> Rules {
    RULES.iter().find(|(name, _)| *name == shape).map(|(_, rules)| *rules)
        .unwrap_or_else(|| panic!("形状 {shape} 没有声明编辑规则"))
}

/// The cell the caret enters, walking `forward` into this node.
pub fn entry_cell(atom: &MathAtom, forward: bool) -> usize {
    let shape = atom.shape();
    match rules(shape.view).entry {
        // The roles named here are declared by the same shape's slot list; the
        // alignment test is what holds the two tables to that.
        Entry::Role { forward: f, backward: b } => shape.index_of(if forward { f } else { b }).unwrap_or(0),
        // Saturating rather than `len() - 1`: a leaf has no cell to enter,
        // and the expression this replaces underflowed if one was asked.
        Entry::Edge => if forward { 0 } else { atom.cells.len().saturating_sub(1) },
        Entry::GridMiddle => {
            let columns = atom.columns();
            (atom.cells.len() / columns).saturating_sub(1) / 2 * columns + if forward { 0 } else { columns - 1 }
        }
    }
}

/// The cell reached by walking one step left/right from `index`, if the node allows it.
///
/// `InsetMathFrac` and `InsetMathScript` deliberately do **not** walk their cells on
/// Right, which is what `Horiz::Locked` says.
pub fn idx_horizontal(atom: &MathAtom, index: usize, forward: bool) -> Option<usize> {
    let step = |forward: bool| if forward { (index + 1 < atom.cells.len()).then_some(index + 1) } else { index.checked_sub(1) };
    match rules(atom.shape().view).horizontal {
        Horiz::Locked => None,
        Horiz::Column => {
            let columns = atom.columns();
            if forward && index % columns + 1 == columns || !forward && index % columns == 0 { None } else { step(forward) }
        }
        Horiz::Linear => step(forward),
    }
}

/// Where an up/down move lands: which cell, and whether at its end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Landing {
    pub cell: usize,
    pub end: bool,
}

/// The cell an up/down move from `index` lands in, or `None` to leave the node.
///
/// `at_cell_end` is whether the caret sits at the end of the cell it is in, which
/// the attachment rule needs: a script is only reachable from the end of the base.
/// Everything else about where the caret *ends up* inside that cell is geometry
/// (`Editor::geometry`), which is a measurement rather than a rule.
pub fn vertical(atom: &MathAtom, index: usize, up: bool, at_cell_end: bool) -> Option<Landing> {
    let shape = atom.shape();
    match rules(shape.view).vertical {
        Vertical::None => None,
        Vertical::Swap { up: up_role, down: down_role, end_up } => {
            let target = shape.index_of(if up { up_role } else { down_role })?;
            (index != target).then_some(Landing { cell: target, end: up && end_up })
        }
        // An attachment keeps its own rules: a script is only reachable from the
        // end of the base, and it returns to the base's start.
        Vertical::Attach => {
            if index == 0 && at_cell_end {
                atom.script_idx(up).map(|cell| Landing { cell, end: false })
            } else if atom.script_idx(true) == Some(index) && !up || atom.script_idx(false) == Some(index) && up {
                Some(Landing { cell: 0, end: true })
            } else { None }
        }
        Vertical::Column => {
            let columns = atom.columns();
            if up { index.checked_sub(columns).map(|cell| Landing { cell, end: false }) }
            else if index + columns < atom.cells.len() { Some(Landing { cell: index + columns, end: false }) }
            else { None }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::MathAtom;
    use crate::slots::fixtures::every_shape;

    /// The join between the two tables: every shape has editing rules.
    ///
    /// This is the half of the old `named_roles_exist_in_the_schema_that_names_them`
    /// that says the rule table is *complete*. The other half — that the roles the
    /// rules name exist in the slots that shape declares — is
    /// `named_roles_exist_in_the_schema_that_names_them` below, which now reaches
    /// **across** the two tables instead of reading one struct.
    #[test]
    fn every_shape_declares_its_editing_rules() {
        for (label, shape, _) in every_shape() {
            assert!(RULES.iter().any(|(name, _)| *name == shape.view),
                    "{label}：形状 {} 没有编辑规则，1 会静默按 PLAIN 导航", shape.view);
        }
        // The reverse, so a rule for a shape nothing declares is caught too: it
        // would be a rule the editor can never reach and nobody would notice.
        for (name, _) in RULES {
            assert!(every_shape().iter().any(|(_, shape, _)| shape.view == *name),
                    "编辑规则表里的 {name} 不是任何形状的名字");
        }
    }

    /// The coupling between the two tables: a rule may only name a role the shape
    /// declares.
    ///
    /// This is `slots.rs`'s old `named_roles_exist_in_the_schema_that_names_them`,
    /// continued across the split. It is the only thing standing between "the rules
    /// name roles" and "the slots declare them", so it must not be dropped: a rule
    /// naming a role that does not exist makes `index_of` answer `None`, which the
    /// caret reads as "the cell does not exist" — a navigation rule that silently
    /// stops working.
    #[test]
    fn named_roles_exist_in_the_schema_that_names_them() {
        for (label, shape, _) in every_shape() {
            let rules = rules(shape.view);
            if let Entry::Role { forward, backward } = rules.entry {
                assert!(shape.index_of(forward).is_some(), "{label}：入口角色 {forward:?} 不在槽位表里");
                assert!(shape.index_of(backward).is_some(), "{label}：入口角色 {backward:?} 不在槽位表里");
            }
            if let Vertical::Swap { up, down, .. } = rules.vertical {
                assert!(shape.index_of(up).is_some(), "{label}：上移角色 {up:?} 不在槽位表里");
                assert!(shape.index_of(down).is_some(), "{label}：下移角色 {down:?} 不在槽位表里");
            }
        }
    }

    #[test]
    fn entry_cells_stay_inside_the_cells_they_describe() {
        for (label, kind, cells) in crate::slots::fixtures::representatives() {
            if cells == 0 { continue; }
            let atom = MathAtom::nest(kind, cells);
            for forward in [true, false] {
                let entry = entry_cell(&atom, forward);
                assert!(entry < cells, "{label}：入口格子 {entry} 越界（共 {cells} 格）");
            }
        }
    }

    #[test]
    fn horizontal_neighbours_stay_inside_the_cells_too() {
        for (label, kind, cells) in crate::slots::fixtures::representatives() {
            if cells == 0 { continue; }
            let atom = MathAtom::nest(kind, cells);
            for index in 0..cells {
                for forward in [true, false] {
                    if let Some(next) = idx_horizontal(&atom, index, forward) {
                        assert!(next < cells, "{label}：第 {index} 格向{}走到越界的 {next}", if forward { "右" } else { "左" });
                    }
                }
            }
        }
    }

    /// The vertical rule never leaves the node's own cells, whichever way it is asked.
    #[test]
    fn vertical_landings_stay_inside_the_node() {
        for (label, kind, cells) in crate::slots::fixtures::representatives() {
            if cells == 0 { continue; }
            let atom = MathAtom::nest(kind, cells);
            for index in 0..cells {
                for up in [true, false] {
                    for at_cell_end in [true, false] {
                        if let Some(landing) = vertical(&atom, index, up, at_cell_end) {
                            assert!(landing.cell < cells,
                                    "{label}：第 {index} 格向{}落到越界的 {}（共 {cells} 格）",
                                    if up { "上" } else { "下" }, landing.cell);
                        }
                    }
                }
            }
        }
    }
}
