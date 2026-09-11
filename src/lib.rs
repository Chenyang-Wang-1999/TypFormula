// SPDX-License-Identifier: GPL-2.0-or-later
//! The host half of visual-typst: everything that talks to the outside world.
//!
//! `document` holds one document and one formula session and keeps the *source*
//! authoritative; `desktop` and `rpc` are the two transports (private stdio
//! pipes, never a listening socket); `services`, `packages` and `workspace`
//! reach processes, the network and the file system.
//!
//! The editing kernel is the separate `visual-typst-core` crate. This one may
//! depend on it; it may never depend on this one, which is what makes "change
//! the periphery without opening the kernel" a rule the compiler enforces rather
//! than a convention. See `docs/architecture.md`.
pub mod document;
pub mod services;
pub mod workspace;
pub mod packages;
pub mod rpc;
pub mod desktop;
