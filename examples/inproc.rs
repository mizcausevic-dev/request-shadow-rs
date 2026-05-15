//! `cargo run --example inproc`
//!
//! In-process shadow: primary backend returns a "v1" payload, the candidate
//! returns "v2" — the shadower yields the v1 primary AND a structured
//! divergence so you can see what the migration is doing differently.

use std::sync::Arc;

use async_trait::async_trait;
use request_shadow::{Backend, ResponseRecord, ShadowConfig, ShadowError, Shadower};

#[derive(Clone)]
struct V1;
#[async_trait]
impl Backend for V1 {
    async fn call(&self, _input: &[u8]) -> Result<ResponseRecord, ShadowError> {
        Ok(
            ResponseRecord::ok(br#"{"user_id":"u-42","plan":"pro"}"#.to_vec())
                .with_header("content-type", "application/json"),
        )
    }
}

#[derive(Clone)]
struct V2;
#[async_trait]
impl Backend for V2 {
    async fn call(&self, _input: &[u8]) -> Result<ResponseRecord, ShadowError> {
        Ok(
            ResponseRecord::ok(br#"{"user_id":"u-42","plan":"pro","tier":"gold"}"#.to_vec())
                .with_header("content-type", "application/json")
                .with_header("x-version", "2"),
        )
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shadower = Shadower::new(Arc::new(V1), Arc::new(V2), ShadowConfig::full_sample());

    for i in 0..3 {
        let outcome = shadower.call(format!("req-{i}").as_bytes()).await?;
        println!("\n--- request {i} ---");
        println!(
            "primary body: {}",
            String::from_utf8_lossy(&outcome.primary.body)
        );
        if let Some(div) = outcome.divergence {
            println!("divergence:");
            if let Some((p, s)) = div.status {
                println!("  status: {p} vs {s}");
            }
            if let Some(h) = div.headers {
                println!(
                    "  headers added={:?} removed={:?} changed={:?}",
                    h.added, h.removed, h.changed
                );
            }
            if let Some(b) = div.body {
                println!(
                    "  body lens primary={} shadow={} shared_prefix={}",
                    b.primary_len, b.shadow_len, b.prefix_equal_bytes
                );
            }
        } else {
            println!("no divergence");
        }
    }

    Ok(())
}
