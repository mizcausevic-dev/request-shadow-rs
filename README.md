# request-shadow

[![CI](https://github.com/mizcausevic-dev/request-shadow-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/mizcausevic-dev/request-shadow-rs/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.86%2B-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**Async request mirroring with sampling, divergence detection, and structured response diffs.** The SRE primitive for migrations: send the same request to the production service AND a candidate, compare the responses, return the production one to the client while you collect divergence telemetry.

```rust
use std::sync::Arc;
use request_shadow::{Backend, ResponseRecord, ShadowConfig, Shadower};
# use async_trait::async_trait;
# #[derive(Clone)] struct Mock(ResponseRecord);
# #[async_trait]
# impl Backend for Mock {
#     async fn call(&self, _input: &[u8]) -> Result<ResponseRecord, request_shadow::ShadowError> { Ok(self.0.clone()) }
# }
# async fn demo() -> Result<(), request_shadow::ShadowError> {
let primary  = Arc::new(Mock(ResponseRecord::ok(b"prod".to_vec())));
let shadow   = Arc::new(Mock(ResponseRecord::ok(b"prod".to_vec())));
let shadower = Shadower::new(primary, shadow, ShadowConfig::full_sample());

let outcome = shadower.call(b"hello").await?;
assert!(outcome.primary.ok);
assert!(outcome.divergence.is_none()); // bytes match
# Ok(()) }
```

---

## Why a small crate

Every service mesh has a knob for traffic shadowing — Linkerd, Istio, AWS App Mesh. They're great when you own the mesh. They're useless when the migration is **in-process**: binary library swap, codec change, JSON-vs-protobuf swap, ORM cutover.

This crate gives you the same shape as a 30-line Tokio task:

1. **Backend trait** — abstracts the call. Implement once per transport.
2. **Shadower** — fires both legs concurrently, returns the primary record + an optional divergence.
3. **Divergence** — typed diff: status / headers / body each get their own bool + summary.
4. **Sampling** — sticky on the input bytes (SHA-256 mod 100). The same input always gets the same yes/no for a given rate.
5. **Timeout for the shadow leg only** — never blocks the primary call.

---

## Pieces

| Type | Purpose |
| --- | --- |
| `Backend` | `async fn call(&self, input: &[u8]) -> Result<ResponseRecord, _>`. Implement over `reqwest::Client`, your gRPC client, or anything else. |
| `ResponseRecord` | Backend output: `ok`, `status`, sorted `headers`, opaque `body`. |
| `ShadowConfig` | Sampling rate, shadow timeout, list of fields to ignore in the diff. |
| `Shadower` | The composer. Cheap to clone (both backends are `Arc`). |
| `ShadowOutcome` | What `Shadower::call` returns: `primary`, optional `shadow`, optional `divergence`, plus reason flags. |
| `Divergence` | `status: Option<(u16, u16)>`, `headers: Option<HeaderDiff>`, `body: Option<BodyDiff>`. Each piece is `None` when that aspect matches or was ignored. |
| `DivergenceLog` | Bounded ring buffer of recent divergences for operator inspection. |

---

## Sampling

Set `sample_rate(N)` to mirror N% of requests. Bucketing is sticky over the input bytes:

```rust
use request_shadow::ShadowConfig;
let cfg = ShadowConfig::full_sample().sample_rate(10);
assert_eq!(cfg.should_shadow(b"req-key-1"), cfg.should_shadow(b"req-key-1"));
```

Same key, same answer. Deterministic. No RNG dep.

---

## Composes with

- **[reliability-toolkit-rs](https://github.com/mizcausevic-dev/reliability-toolkit-rs)** — wrap the shadow `Backend` in a [`CircuitBreaker`](https://github.com/mizcausevic-dev/reliability-toolkit-rs#circuitbreaker--closed--open--halfopen) so a flaky candidate never bleeds into the primary path.
- **[slo-budget-tracker](https://github.com/mizcausevic-dev/slo-budget-tracker)** — record every divergence against an SLO so you can answer "is the candidate good enough to promote?"
- **[feature-flag-rs](https://github.com/mizcausevic-dev/feature-flag-rs)** — flip the sampling rate from a remote config push without redeploying.

---

## Run the example

```bash
cargo run --example inproc
```

Builds a primary "v1" backend and a "v2" candidate, fires three requests, prints the primary body + the structured divergence each time.

---

## Bench

```bash
cargo bench
```

The bundled bench times `Divergence::compare` on a 4KB equal body so you can spot regressions in the diff path.

---

## Tests

```bash
cargo test --all-targets
cargo test --doc
cargo clippy --all-targets -- -Dwarnings
cargo fmt --all -- --check
```

CI matrix: `stable`, `beta`, `1.86.0` (MSRV). Eleven async tests cover identical responses, body/status/header divergence, ignore-fields, sampling at 0%, sticky sampling, timeout handling, shadow-backend failures, and the divergence log.

---

## License

MIT. See [LICENSE](LICENSE).
