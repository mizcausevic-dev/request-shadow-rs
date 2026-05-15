//! Bounded ring buffer for divergence records.
//!
//! Real production telemetry should ship divergences to your tracing /
//! metrics pipeline. This in-process log is for tests and for operators
//! poking at a live process via an admin endpoint.

use std::collections::VecDeque;

use parking_lot_dummy::Mutex;

use crate::divergence::Divergence;

/// One log entry: the input that produced the divergence + the divergence
/// itself. We don't store the full responses — that's potentially a lot of
/// memory.
#[derive(Clone, Debug)]
pub struct DivergenceEntry {
    /// Sampling / correlation key used by the call.
    pub key: Vec<u8>,
    /// The structured diff.
    pub divergence: Divergence,
}

/// Bounded ring buffer. Drops the oldest entry when full.
#[derive(Debug)]
pub struct DivergenceLog {
    capacity: usize,
    entries: Mutex<VecDeque<DivergenceEntry>>,
}

impl DivergenceLog {
    /// Build a log with the given capacity. `capacity = 0` disables logging.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    /// Push one divergence. No-op when capacity is 0.
    pub fn push(&self, entry: DivergenceEntry) {
        if self.capacity == 0 {
            return;
        }
        let mut g = self.entries.lock();
        if g.len() == self.capacity {
            g.pop_front();
        }
        g.push_back(entry);
    }

    /// Snapshot the current entries oldest-first.
    pub fn snapshot(&self) -> Vec<DivergenceEntry> {
        let g = self.entries.lock();
        g.iter().cloned().collect()
    }

    /// Number of stored entries (≤ capacity).
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    /// Whether the log has any entries.
    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}

impl Default for DivergenceLog {
    fn default() -> Self {
        Self::new(128)
    }
}

// We don't want a parking_lot dep just for this — wrap std::sync::Mutex with
// a poison-discarding API so the call sites stay clean.
mod parking_lot_dummy {
    use std::sync::Mutex as StdMutex;

    #[derive(Debug)]
    pub struct Mutex<T>(StdMutex<T>);

    impl<T> Mutex<T> {
        pub fn new(t: T) -> Self {
            Self(StdMutex::new(t))
        }
        pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }
    }
}
