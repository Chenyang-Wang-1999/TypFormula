// SPDX-License-Identifier: GPL-2.0-or-later
// Rust port of the LyX math editing core. See docs/LYX-CREDITS and COPYING.
pub mod cursor;
pub mod math;
pub mod typst;
pub mod view;
pub mod document;
pub mod prewarm;
#[cfg(not(target_arch = "wasm32"))]
mod warmup_service;
#[cfg(not(target_arch = "wasm32"))]
pub mod services;
#[cfg(not(target_arch = "wasm32"))]
pub mod workspace;
#[cfg(not(target_arch = "wasm32"))]
pub mod packages;
#[cfg(not(target_arch = "wasm32"))]
pub mod rpc;
#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
// The JSON bridge is wasm-only, but its buffer ownership rules are worth
// testing on the host too.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) mod wasm;

pub use cursor::{Action, Editor};
