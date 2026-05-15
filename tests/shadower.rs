use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use request_shadow::{Backend, IgnoreField, ResponseRecord, ShadowConfig, ShadowError, Shadower};

#[derive(Clone)]
struct Fixed(ResponseRecord);

#[async_trait]
impl Backend for Fixed {
    async fn call(&self, _input: &[u8]) -> Result<ResponseRecord, ShadowError> {
        Ok(self.0.clone())
    }
}

#[derive(Clone)]
struct Counting {
    inner: Arc<AtomicU32>,
    response: ResponseRecord,
}

#[async_trait]
impl Backend for Counting {
    async fn call(&self, _input: &[u8]) -> Result<ResponseRecord, ShadowError> {
        self.inner.fetch_add(1, Ordering::SeqCst);
        Ok(self.response.clone())
    }
}

#[derive(Clone)]
struct Slow {
    delay: Duration,
    response: ResponseRecord,
}

#[async_trait]
impl Backend for Slow {
    async fn call(&self, _input: &[u8]) -> Result<ResponseRecord, ShadowError> {
        tokio::time::sleep(self.delay).await;
        Ok(self.response.clone())
    }
}

#[derive(Clone)]
struct Failing(String);

#[async_trait]
impl Backend for Failing {
    async fn call(&self, _input: &[u8]) -> Result<ResponseRecord, ShadowError> {
        Err(ShadowError::Backend(self.0.clone()))
    }
}

#[tokio::test]
async fn identical_responses_have_no_divergence() {
    let primary = Arc::new(Fixed(ResponseRecord::ok(b"hello".to_vec())));
    let shadow = Arc::new(Fixed(ResponseRecord::ok(b"hello".to_vec())));
    let s = Shadower::new(primary, shadow, ShadowConfig::full_sample());

    let outcome = s.call(b"req-1").await.unwrap();
    assert!(outcome.primary.ok);
    assert!(outcome.divergence.is_none());
    assert!(outcome.shadow.is_some());
    assert!(!outcome.skipped_by_sampler);
}

#[tokio::test]
async fn body_difference_is_flagged_with_prefix_length() {
    let primary = Arc::new(Fixed(ResponseRecord::ok(b"hello world".to_vec())));
    let shadow = Arc::new(Fixed(ResponseRecord::ok(b"hello earth".to_vec())));
    let s = Shadower::new(primary, shadow, ShadowConfig::full_sample());

    let outcome = s.call(b"req-2").await.unwrap();
    let div = outcome.divergence.expect("body differs");
    let body = div.body.expect("body diff present");
    assert_eq!(body.primary_len, 11);
    assert_eq!(body.shadow_len, 11);
    // "hello " is the shared prefix.
    assert_eq!(body.prefix_equal_bytes, 6);
}

#[tokio::test]
async fn status_difference_is_flagged() {
    let primary = Arc::new(Fixed(ResponseRecord::ok(b"x".to_vec())));
    let shadow = Arc::new(Fixed(ResponseRecord::err(500, b"x".to_vec())));
    let s = Shadower::new(primary, shadow, ShadowConfig::full_sample());

    let div = s.call(b"req-3").await.unwrap().divergence.unwrap();
    assert_eq!(div.status, Some((200, 500)));
}

#[tokio::test]
async fn header_difference_is_flagged() {
    let primary = Arc::new(Fixed(
        ResponseRecord::ok(b"x".to_vec()).with_header("x-server", "old"),
    ));
    let shadow = Arc::new(Fixed(
        ResponseRecord::ok(b"x".to_vec())
            .with_header("x-server", "new")
            .with_header("x-extra", "yes"),
    ));
    let s = Shadower::new(primary, shadow, ShadowConfig::full_sample());
    let div = s.call(b"req-4").await.unwrap().divergence.unwrap();
    let headers = div.headers.unwrap();
    assert!(headers.changed.contains(&"x-server".to_string()));
    assert!(headers.added.contains(&"x-extra".to_string()));
}

#[tokio::test]
async fn ignore_body_silences_body_diff() {
    let primary = Arc::new(Fixed(ResponseRecord::ok(b"a".to_vec())));
    let shadow = Arc::new(Fixed(ResponseRecord::ok(b"b".to_vec())));
    let cfg = ShadowConfig::full_sample().ignore(IgnoreField::Body);
    let s = Shadower::new(primary, shadow, cfg);
    let outcome = s.call(b"req-5").await.unwrap();
    assert!(outcome.divergence.is_none());
}

#[tokio::test]
async fn sampling_at_zero_never_mirrors() {
    let counter = Arc::new(AtomicU32::new(0));
    let primary = Arc::new(Fixed(ResponseRecord::ok(b"x".to_vec())));
    let shadow = Arc::new(Counting {
        inner: counter.clone(),
        response: ResponseRecord::ok(b"x".to_vec()),
    });
    let cfg = ShadowConfig::full_sample().sample_rate(0);
    let s = Shadower::new(primary, shadow, cfg);

    for i in 0..50 {
        let outcome = s.call(format!("req-{i}").as_bytes()).await.unwrap();
        assert!(outcome.skipped_by_sampler);
        assert!(outcome.shadow.is_none());
    }
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn sampling_is_sticky_by_key() {
    let cfg = ShadowConfig::full_sample().sample_rate(50);
    // Same key always lands in the same bucket.
    let first = cfg.should_shadow(b"my-key");
    for _ in 0..50 {
        assert_eq!(cfg.should_shadow(b"my-key"), first);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shadow_timeout_doesnt_block_primary_and_marks_failure() {
    let primary = Arc::new(Fixed(ResponseRecord::ok(b"prod".to_vec())));
    let shadow = Arc::new(Slow {
        delay: Duration::from_millis(300),
        response: ResponseRecord::ok(b"prod".to_vec()),
    });
    let cfg = ShadowConfig::full_sample().shadow_timeout(Duration::from_millis(50));
    let s = Shadower::new(primary, shadow, cfg);

    let outcome = s.call(b"req").await.unwrap();
    assert!(outcome.primary.ok);
    assert!(outcome.shadow.is_none());
    assert_eq!(outcome.shadow_failed.as_deref(), Some("timeout"));
    // Body / status / headers all None because shadow didn't return.
    assert!(outcome.divergence.is_none());
}

#[tokio::test]
async fn shadow_backend_error_marks_failure_without_breaking_primary() {
    let primary = Arc::new(Fixed(ResponseRecord::ok(b"prod".to_vec())));
    let shadow = Arc::new(Failing("downstream 5xx".to_string()));
    let s = Shadower::new(primary, shadow, ShadowConfig::full_sample());

    let outcome = s.call(b"req").await.unwrap();
    assert!(outcome.primary.ok);
    assert!(outcome.shadow.is_none());
    assert!(outcome.shadow_failed.is_some());
}

#[tokio::test]
async fn divergence_log_records_recent_entries() {
    let primary = Arc::new(Fixed(ResponseRecord::ok(b"a".to_vec())));
    let shadow = Arc::new(Fixed(ResponseRecord::ok(b"b".to_vec())));
    let s = Shadower::new(primary, shadow, ShadowConfig::full_sample());

    for i in 0..5 {
        s.call(format!("req-{i}").as_bytes()).await.unwrap();
    }
    assert_eq!(s.divergence_count(), 5);
    let snap = s.divergences();
    assert_eq!(snap.len(), 5);
}
