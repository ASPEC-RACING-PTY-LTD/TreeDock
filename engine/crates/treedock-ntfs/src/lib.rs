// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Pure-Rust NTFS Master File Table parsing for TreeDock's fast scan path.
//!
//! This crate performs no I/O and contains no `unsafe` code, so it can be
//! tested and fuzzed on any platform. Reading the raw volume is done by
//! `treedock-win`.
#![forbid(unsafe_code)]

pub mod index;
pub mod record;

pub use index::{BuildOptions, BuildStats, HardlinkPolicy, MftIndex};
