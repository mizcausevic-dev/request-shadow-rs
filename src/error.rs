//! Crate-wide error type.

use thiserror::Error;

/// Anything that can go wrong inside the crate.
#[derive(Debug, Error)]
pub enum ShadowError {
    /// The wrapped backend returned an error.
    #[error("backend failure: {0}")]
    Backend(String),

    /// The shadow leg timed out. The primary call is unaffected.
    #[error("shadow leg timed out")]
    ShadowTimeout,
}
