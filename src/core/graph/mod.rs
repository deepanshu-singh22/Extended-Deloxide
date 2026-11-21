//! Graph module for deadlock detection
//!
//! This module contains graph implementations used for deadlock detection:
//! - Wait-for graph: tracks which threads are waiting for which other threads
//! - Lock order graph: tracks the order in which locks are acquired (optional feature)

#[cfg(feature = "lock-order-graph")]
pub mod lock_order_graph;
pub mod wait_for_graph;

#[cfg(feature = "lock-order-graph")]
pub use lock_order_graph::LockOrderGraph;
#[allow(unused_imports)]
pub use wait_for_graph::WaitForGraph;

pub mod sharded_wfg;
pub use sharded_wfg::ShardedWaitForGraph;
