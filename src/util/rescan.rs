//! Singleton rescan infrastructure.
//!
//! Guarantees at most one rescan runs at any moment.  Duplicate requests
//! are ignored.  Progress is reported via Server-Sent Events (SSE).

use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag: `true` while a rescan task is in progress.
static RESCAN_RUNNING: AtomicBool = AtomicBool::new(false);

/// RAII guard that sets `RESCAN_RUNNING` to `true` on creation and
/// resets it to `false` on drop.
pub struct RescanGuard;

impl RescanGuard {
    /// Try to acquire the rescan lock.
    ///
    /// Returns `Some(guard)` if no rescan is running.
    /// Returns `None` if a rescan is already in progress.
    pub fn try_acquire() -> Option<Self> {
        if RESCAN_RUNNING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Some(RescanGuard)
        } else {
            None
        }
    }
}

impl Drop for RescanGuard {
    fn drop(&mut self) {
        RESCAN_RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Returns `true` if a rescan is currently in progress.
pub fn is_running() -> bool {
    RESCAN_RUNNING.load(Ordering::SeqCst)
}

