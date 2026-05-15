//! The async backend abstraction.

use std::collections::BTreeMap;

use async_trait::async_trait;
use bytes::Bytes;

use crate::error::ShadowError;

/// What both legs of a shadow call return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseRecord {
    /// Whether the call completed normally. Backends that handle their own
    /// errors should leave this `true` and put any error payload in `body`.
    pub ok: bool,
    /// HTTP-ish status code (or 0 for non-HTTP calls). Diffed unless the
    /// caller adds it to [`crate::config::IgnoreField::Status`].
    pub status: u16,
    /// Sorted-by-key headers map. Diffed unless [`crate::config::IgnoreField::Headers`].
    pub headers: BTreeMap<String, String>,
    /// Response body, opaque.
    pub body: Bytes,
}

impl ResponseRecord {
    /// Construct a success record from a body — handy in tests.
    pub fn ok(body: Vec<u8>) -> Self {
        Self {
            ok: true,
            status: 200,
            headers: BTreeMap::new(),
            body: Bytes::from(body),
        }
    }

    /// Construct a failure record.
    pub fn err(status: u16, body: Vec<u8>) -> Self {
        Self {
            ok: false,
            status,
            headers: BTreeMap::new(),
            body: Bytes::from(body),
        }
    }

    /// Convenience: set a header. Returns `self` for chaining.
    #[must_use]
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }
}

/// The async surface the shadower drives. Implement once per transport.
#[async_trait]
pub trait Backend: Send + Sync {
    /// Issue a call. The input is opaque so this trait works for HTTP bodies,
    /// gRPC bytes, raw JSON, MessagePack — anything that fits in a `&[u8]`.
    async fn call(&self, input: &[u8]) -> Result<ResponseRecord, ShadowError>;
}
