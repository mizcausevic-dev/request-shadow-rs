//! # request-shadow
//!
//! Async request mirroring with sampling, divergence detection, and structured
//! response diffs. The SRE primitive for migrations: send the same request to
//! the production service AND a candidate, compare the responses, return the
//! production one to the client while you collect divergence telemetry.
//!
//! ## Why a small crate
//!
//! Every service-mesh has a knob for this — Linkerd shadowing, Istio mirror,
//! AWS App Mesh. Those are great when you own the mesh. They're useless when
//! the migration is in-process (binary library swap, codec change, JSON-vs-
//! protobuf swap, ORM cutover). This crate gives you the same shape as a
//! 30-line Tokio task:
//!
//! ```
//! # use std::sync::Arc;
//! # use request_shadow::{Shadower, ShadowConfig, ResponseRecord, Backend};
//! # use async_trait::async_trait;
//! # #[derive(Clone)]
//! # struct Mock(ResponseRecord);
//! # #[async_trait]
//! # impl Backend for Mock {
//! #     async fn call(&self, _input: &[u8]) -> Result<ResponseRecord, request_shadow::ShadowError> {
//! #         Ok(self.0.clone())
//! #     }
//! # }
//! # async fn demo() -> Result<(), request_shadow::ShadowError> {
//! let primary  = Arc::new(Mock(ResponseRecord::ok(b"prod".to_vec())));
//! let shadow   = Arc::new(Mock(ResponseRecord::ok(b"prod".to_vec())));
//! let shadower = Shadower::new(primary, shadow, ShadowConfig::full_sample());
//!
//! let outcome = shadower.call(b"hello").await?;
//! assert!(outcome.primary.ok);
//! assert!(outcome.divergence.is_none()); // bytes match
//! # Ok(()) }
//! ```
//!
//! ## Pieces
//!
//! - [`Backend`] — the async-trait abstraction the shadower calls. Implement it
//!   over `reqwest::Client` for HTTP, or any in-process call.
//! - [`ResponseRecord`] — what a backend returns: status code, headers, body.
//! - [`ShadowConfig`] — sampling rate (sticky over a key hash), timeout for the
//!   shadow leg, fields to ignore in the diff.
//! - [`Shadower`] — picks whether to mirror based on the sampling key, fires
//!   both calls in a `tokio::join!`, returns a [`ShadowOutcome`].
//! - [`Divergence`] — structured diff: status / headers / body each get their
//!   own bool + summary.
//! - [`DivergenceLog`] — bounded ring buffer so the shadower can hand operators
//!   the last N divergences without unbounded memory growth.
//!
//! ## Composes with
//!
//! - **[reliability-toolkit-rs](https://github.com/mizcausevic-dev/reliability-toolkit-rs)**
//!   — wrap the shadow `Backend` in a [`CircuitBreaker`] so a flaky candidate
//!   never bleeds into the primary path.
//! - **[slo-budget-tracker](https://github.com/mizcausevic-dev/slo-budget-tracker)**
//!   — record every divergence against an SLO so you can answer "is the
//!   candidate good enough to promote?"

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

pub mod backend;
pub mod config;
pub mod divergence;
pub mod error;
pub mod log;
pub mod shadower;

pub use backend::{Backend, ResponseRecord};
pub use config::{IgnoreField, ShadowConfig};
pub use divergence::Divergence;
pub use error::ShadowError;
pub use log::DivergenceLog;
pub use shadower::{ShadowOutcome, Shadower};
