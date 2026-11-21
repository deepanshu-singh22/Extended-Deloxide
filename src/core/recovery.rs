//! Cooperative Deadlock Recovery (Deloxide-SR Novelty 2)
//!
//! Provides policy-driven victim selection (`MinLocksHeld`, `YoungestThread`)
//! and `DeadlockError` for cooperative rollback via `Mutex::lock_recoverable()`.

use crate::core::types::{LockId, ThreadId};
use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

/// Policy used to select a victim thread when a deadlock cycle is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecoveryPolicy {
    /// Select the thread holding the fewest locks (minimizes rollback work).
    MinLocksHeld = 0,
    /// Select the most recently spawned thread (highest ThreadId).
    YoungestThread = 1,
}

static ACTIVE_POLICY: AtomicU8 = AtomicU8::new(RecoveryPolicy::MinLocksHeld as u8);

/// Set the global deadlock recovery policy.
pub fn set_recovery_policy(policy: RecoveryPolicy) {
    ACTIVE_POLICY.store(policy as u8, Ordering::SeqCst);
}

/// Get the currently active deadlock recovery policy.
pub fn get_recovery_policy() -> RecoveryPolicy {
    match ACTIVE_POLICY.load(Ordering::SeqCst) {
        1 => RecoveryPolicy::YoungestThread,
        _ => RecoveryPolicy::MinLocksHeld,
    }
}

/// Error returned by `Mutex::lock_recoverable()` when acquiring the lock
/// would complete a deadlock cycle.
#[derive(Debug, Clone)]
pub struct DeadlockError {
    /// The thread whose acquisition attempt was aborted to break the cycle.
    pub aborted_thread: ThreadId,
    /// The lock that could not be acquired without deadlocking.
    pub lock_id: LockId,
    /// The threads participating in the detected deadlock cycle.
    pub cycle: Vec<ThreadId>,
    /// The victim selected according to the active `RecoveryPolicy`.
    pub policy_victim: ThreadId,
    /// The policy active at the time of recovery.
    pub policy: RecoveryPolicy,
}

impl fmt::Display for DeadlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Deadlock avoided on lock {} for thread {} (cycle: {:?}, policy: {:?}, victim: {})",
            self.lock_id, self.aborted_thread, self.cycle, self.policy, self.policy_victim
        )
    }
}

impl std::error::Error for DeadlockError {}
