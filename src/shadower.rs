//! [`Shadower`] — the main type. Fires primary + shadow legs concurrently and
//! returns the primary response plus an optional divergence.

use std::sync::Arc;

use tokio::time::timeout;

use crate::backend::{Backend, ResponseRecord};
use crate::config::ShadowConfig;
use crate::divergence::Divergence;
use crate::error::ShadowError;
use crate::log::{DivergenceEntry, DivergenceLog};

/// Result of a shadow call. The client *always* sees `primary` — even if the
/// shadow leg failed or timed out, only telemetry is affected.
#[derive(Debug)]
pub struct ShadowOutcome {
    /// The primary response. This is what the calling code should return.
    pub primary: ResponseRecord,
    /// The shadow response, if it succeeded inside the configured timeout.
    pub shadow: Option<ResponseRecord>,
    /// Structured diff. `None` when bodies/status/headers all match (modulo
    /// the config's ignore list) or when the shadow didn't run.
    pub divergence: Option<Divergence>,
    /// True when the request would have been mirrored but the sampler said no.
    pub skipped_by_sampler: bool,
    /// True when the shadow leg timed out / errored.
    pub shadow_failed: Option<String>,
}

/// The shadower. Cheap to clone — both backends are `Arc`-shaped.
#[derive(Clone)]
pub struct Shadower {
    primary: Arc<dyn Backend>,
    shadow: Arc<dyn Backend>,
    config: ShadowConfig,
    log: Arc<DivergenceLog>,
}

impl Shadower {
    /// Build a shadower with the default ring-buffer-backed [`DivergenceLog`].
    pub fn new(primary: Arc<dyn Backend>, shadow: Arc<dyn Backend>, config: ShadowConfig) -> Self {
        Self {
            primary,
            shadow,
            config,
            log: Arc::new(DivergenceLog::default()),
        }
    }

    /// Override the log (e.g. with a custom capacity).
    #[must_use]
    pub fn with_log(mut self, log: Arc<DivergenceLog>) -> Self {
        self.log = log;
        self
    }

    /// Snapshot the divergence log.
    pub fn divergences(&self) -> Vec<DivergenceEntry> {
        self.log.snapshot()
    }

    /// Issue a call. Both legs run concurrently. The shadow's deadline is the
    /// `ShadowConfig::shadow_timeout`; expiry never blocks the primary.
    pub async fn call(&self, input: &[u8]) -> Result<ShadowOutcome, ShadowError> {
        let should_shadow = self.config.should_shadow(input);

        if !should_shadow {
            let primary = self.primary.call(input).await?;
            return Ok(ShadowOutcome {
                primary,
                shadow: None,
                divergence: None,
                skipped_by_sampler: true,
                shadow_failed: None,
            });
        }

        let primary_fut = self.primary.call(input);
        let shadow_fut = timeout(self.config.shadow_timeout, self.shadow.call(input));

        let (primary_res, shadow_res) = tokio::join!(primary_fut, shadow_fut);
        let primary = primary_res?;

        let (shadow, shadow_failed) = match shadow_res {
            Ok(Ok(resp)) => (Some(resp), None),
            Ok(Err(err)) => (None, Some(err.to_string())),
            Err(_) => (None, Some("timeout".to_string())),
        };

        let divergence = match &shadow {
            Some(s) => Divergence::compare(&primary, s, &self.config),
            None => None,
        };

        if let Some(d) = &divergence {
            self.log.push(DivergenceEntry {
                key: input.to_vec(),
                divergence: d.clone(),
            });
        }

        Ok(ShadowOutcome {
            primary,
            shadow,
            divergence,
            skipped_by_sampler: false,
            shadow_failed,
        })
    }

    /// Internal — count entries (used by tests).
    pub fn divergence_count(&self) -> usize {
        self.log.len()
    }
}
