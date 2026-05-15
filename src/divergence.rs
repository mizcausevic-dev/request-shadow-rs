//! Structured response diff.

use crate::backend::ResponseRecord;
use crate::config::{IgnoreField, ShadowConfig};

/// Per-field divergence summary. None = no diff or field was ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// `Some((primary, shadow))` when the status codes differ.
    pub status: Option<(u16, u16)>,
    /// `Some((added, removed, changed))` header keys.
    pub headers: Option<HeaderDiff>,
    /// `Some((primary_len, shadow_len, prefix_equal_bytes))` when bodies differ.
    pub body: Option<BodyDiff>,
}

/// Differences between two response header maps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderDiff {
    /// Headers present in the shadow but not the primary.
    pub added: Vec<String>,
    /// Headers present in the primary but not the shadow.
    pub removed: Vec<String>,
    /// Headers present in both but with different values.
    pub changed: Vec<String>,
}

/// Differences between two response bodies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyDiff {
    /// Byte length of the primary body.
    pub primary_len: usize,
    /// Byte length of the shadow body.
    pub shadow_len: usize,
    /// Number of leading bytes that match exactly.
    pub prefix_equal_bytes: usize,
}

impl Divergence {
    /// Compute a `Divergence` from two records given the config's ignore list.
    /// Returns `None` when every diffed field matches.
    pub fn compare(
        primary: &ResponseRecord,
        shadow: &ResponseRecord,
        config: &ShadowConfig,
    ) -> Option<Self> {
        let status =
            if config.ignore.contains(&IgnoreField::Status) || primary.status == shadow.status {
                None
            } else {
                Some((primary.status, shadow.status))
            };

        let headers = if config.ignore.contains(&IgnoreField::Headers) {
            None
        } else {
            Self::diff_headers(primary, shadow)
        };

        let body = if config.ignore.contains(&IgnoreField::Body) || primary.body == shadow.body {
            None
        } else {
            Some(BodyDiff {
                primary_len: primary.body.len(),
                shadow_len: shadow.body.len(),
                prefix_equal_bytes: shared_prefix(&primary.body, &shadow.body),
            })
        };

        if status.is_none() && headers.is_none() && body.is_none() {
            None
        } else {
            Some(Self {
                status,
                headers,
                body,
            })
        }
    }

    fn diff_headers(primary: &ResponseRecord, shadow: &ResponseRecord) -> Option<HeaderDiff> {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut changed = Vec::new();

        for (k, v) in &primary.headers {
            match shadow.headers.get(k) {
                None => removed.push(k.clone()),
                Some(sv) if sv != v => changed.push(k.clone()),
                _ => {}
            }
        }
        for k in shadow.headers.keys() {
            if !primary.headers.contains_key(k) {
                added.push(k.clone());
            }
        }
        if added.is_empty() && removed.is_empty() && changed.is_empty() {
            None
        } else {
            Some(HeaderDiff {
                added,
                removed,
                changed,
            })
        }
    }
}

fn shared_prefix(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}
