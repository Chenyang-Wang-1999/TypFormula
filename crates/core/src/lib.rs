// SPDX-License-Identifier: GPL-2.0-or-later
//! The editing kernel: the model, its declarations, and the Typst spelling.
//!
//! This crate is the half a frontend is written against. It knows nothing about
//! transports, processes, files or the network. Its dependencies are `typst-syntax`,
//! serde/serde_json and unicode-segmentation, with no dependency on the host. That is the
//! point of it being a separate crate rather than a module: `pub(crate)` cannot
//! stop a kernel file from reaching into a host file, a crate boundary can.
//!
//! What lives here:
//!
//! * `math` — `MathData`/`MathAtom`/`Kind`: the editable tree.
//! * `slots` — two exhaustive tables per `Kind`: the box it draws (`Shape`) and
//!   the way it spells itself (`Grammar`).
//! * `editing` — the editing model: where the caret enters, how it walks, and
//!   which cells it can reach.
//! * `typst` — parsing Typst source into that tree, and writing it back.
//! * `context` — the part of a document a piece of source still depends on.
//! * `cursor` — `Editor`: every editing action, in terms of the tree.
//! * `view` — the tree as the frontend receives it.
//!
//! What lives in the host crate: `document` (source is authoritative), `desktop`
//! and `rpc` (transports), `services`/`packages`/`workspace` (process, network,
//! paths). See `docs/architecture.md`.
pub mod context;
pub mod cursor;
pub mod editing;
pub mod math;
pub mod slots;
pub mod typst;
pub mod view;

pub use cursor::{Action, Editor};
