// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! TreeDock engine core: platform-neutral tree store, scanning engine,
//! queries, layouts and persistence. See MASTER_PLAN.md §3.4.

pub mod export;
pub mod fmt;
pub mod query;
pub mod rows;
pub mod scan;
pub mod snapshot;
pub mod time;
pub mod tree;
pub mod treemap;
pub mod wtf8;

pub use tree::{NONE, NodeId, Tree, TreeMeta};
