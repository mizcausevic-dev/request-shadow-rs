//! Shadow configuration.

use std::time::Duration;

use sha2::{Digest, Sha256};

/// Diff field a caller wants to ignore.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IgnoreField {
    /// Don't flag a status-code difference.
    Status,
    /// Don't flag a header difference.
    Headers,
    /// Don't flag a body difference.
    Body,
}

/// Knobs the shadower needs.
#[derive(Clone, Debug)]
pub struct ShadowConfig {
    /// Percentage of requests that get mirrored. `0..=100`. Default 100.
    sample_rate: u32,
    /// Max time the shadow leg is allowed. After this we drop it and flag a
    /// `ShadowTimeout` on the outcome — never blocks the primary.
    pub shadow_timeout: Duration,
    /// Fields to skip in the divergence check.
    pub ignore: Vec<IgnoreField>,
}

impl ShadowConfig {
    /// Mirror every request, default timeout 2s.
    pub fn full_sample() -> Self {
        Self {
            sample_rate: 100,
            shadow_timeout: Duration::from_secs(2),
            ignore: Vec::new(),
        }
    }

    /// Set sampling rate as a percentage (`0..=100`). Values >100 are clamped.
    #[must_use]
    pub fn sample_rate(mut self, percent: u32) -> Self {
        self.sample_rate = percent.min(100);
        self
    }

    /// Override the shadow timeout.
    #[must_use]
    pub fn shadow_timeout(mut self, d: Duration) -> Self {
        self.shadow_timeout = d;
        self
    }

    /// Add a field to skip in the divergence diff.
    #[must_use]
    pub fn ignore(mut self, field: IgnoreField) -> Self {
        self.ignore.push(field);
        self
    }

    /// Configured sample rate (0..=100).
    pub fn sample_rate_percent(&self) -> u32 {
        self.sample_rate
    }

    /// Decide whether this request should be mirrored. Bucketing is sticky on
    /// the `key` — the same key always gets the same yes/no for a given
    /// `sample_rate`. SHA-256 (mod 100) is cheap and avoids an RNG dep.
    pub fn should_shadow(&self, key: &[u8]) -> bool {
        if self.sample_rate >= 100 {
            return true;
        }
        if self.sample_rate == 0 {
            return false;
        }
        let mut hasher = Sha256::new();
        hasher.update(key);
        let digest = hasher.finalize();
        let bucket = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 100;
        bucket < self.sample_rate
    }
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self::full_sample()
    }
}
