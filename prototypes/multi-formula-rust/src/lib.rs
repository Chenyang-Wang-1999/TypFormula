//! Typst-backed document engine and source-linked editing projections.
pub mod document;
pub mod engine;
pub mod macros;
pub mod projection;
pub use document::{Diagnostic, Document, Error, Formula, NodeId, NodeRecord};
pub use macros::{EnvironmentId, MacroIndex};
pub use projection::{Projection, ProjectionKind, Slot};
