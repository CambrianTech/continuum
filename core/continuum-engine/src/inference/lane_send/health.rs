//! Pre-stream engine failures feed the existing serving-health owner (7eef3e05).
//! HTTP 500 used to return only to the caller, leaving the lane's failure record empty.
//! Loading, admission and client refusals are not evidence for destroying warm KV.

use reqwest::StatusCode;
use uuid::Uuid;

pub(super) struct HttpGenerationHealth {
    engine: Option<(String, Uuid)>,
}

impl HttpGenerationHealth {
    pub(super) fn capture(root: Option<&str>) -> Self {
        let engine = root.and_then(|root| {
            crate::inference::slots::directory()
                .page_discovery_generation(root)
                .map(|generation| (root.to_owned(), generation))
        });
        Self { engine }
    }

    pub(super) fn observe(self, status: StatusCode) {
        let Some((root, sent_generation)) = self.engine else {
            return;
        };
        let current = crate::inference::slots::directory().page_discovery_generation(&root);
        record_failure(status, Some(sent_generation), current, || {
            crate::inference::llama_server::note_real_decode_failure();
            continuum_base::probe!(
                class = "inference.decode.failed",
                kind = "http_internal_server_error",
                generation = %sent_generation,
                status = status.as_u16(),
                "managed engine rejected generation before streaming; existing lane health owns recovery"
            );
        });
    }
}

fn record_failure(
    status: StatusCode,
    sent: Option<Uuid>,
    current: Option<Uuid>,
    record: impl FnOnce(),
) {
    // 502/504 are gateway failures, 503 is loading/overload; none proves a poisoned
    // compute backend. Only the same identified local engine's internal error counts.
    if status == StatusCode::INTERNAL_SERVER_ERROR && sent.is_some() && sent == current {
        record();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: binding the response to the configured URL (which may
    // differ from the resolved endpoint), or blaming a replacement for old work.
    // Only this test's unique directory entry changes; no live counter is touched.
    #[tokio::test]
    async fn resolved_endpoint_receipt_survives_until_its_engine_is_replaced() -> Result<(), String>
    {
        let root = format!("http://http-health-fixture-{}", Uuid::new_v4());
        let directory = crate::inference::slots::directory();
        let endpoint = directory.endpoint(&root);
        assert!(HttpGenerationHealth::capture(None).engine.is_none());
        assert!(HttpGenerationHealth::capture(Some(&root)).engine.is_none());
        let launch = endpoint.transition().await;
        let old = launch.start_generation()?;
        let receipt = HttpGenerationHealth::capture(Some(&root));
        let Some((captured_root, sent)) = receipt.engine else {
            return Err("managed engine receipt was not captured".into());
        };
        assert_eq!(captured_root, root);
        let mut failures = 0;
        record_failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            Some(sent),
            directory.page_discovery_generation(&root),
            || failures += 1,
        );
        assert_eq!(failures, 1);
        old.observed_exit();
        let _replacement = launch.start_generation()?;
        record_failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            Some(sent),
            directory.page_discovery_generation(&root),
            || failures += 1,
        );
        assert_eq!(
            failures, 1,
            "late response cannot blame a replacement engine"
        );
        Ok(())
    }

    // what this catches: two real pre-stream 500s used to remain invisible to the
    // existing two-failure recovery threshold; unrelated refusals must not reap warm KV.
    #[test]
    fn current_engine_errors_count_but_loading_client_remote_and_old_engine_do_not() {
        let old = Some(Uuid::from_u128(1));
        let new = Some(Uuid::from_u128(2));
        let mut failures = 0;
        for _ in 0..2 {
            record_failure(StatusCode::INTERNAL_SERVER_ERROR, old, old, || {
                failures += 1
            });
        }
        assert_eq!(failures, 2);
        for status in [200, 400, 401, 404, 408, 429, 502, 503, 504] {
            record_failure(
                StatusCode::from_u16(status).expect("fixture status"),
                old,
                old,
                || failures += 1,
            );
        }
        for (sent, current) in [(None, old), (old, new), (old, None)] {
            record_failure(StatusCode::INTERNAL_SERVER_ERROR, sent, current, || {
                failures += 1
            });
        }
        assert_eq!(
            failures, 2,
            "only current local backend errors affect lane health"
        );
    }
}
