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

    pub(super) fn observe(self, status: StatusCode, body: &str) {
        if self.record(
            status,
            body,
            crate::inference::llama_server::note_real_decode_failure,
        ) {
            continuum_base::probe!(
                class = "inference.decode.failed",
                kind = "http_engine_fault",
                status = status.as_u16(),
                "managed engine reported fatal decode failure; existing lane health owns recovery"
            );
        }
    }

    fn record(self, status: StatusCode, body: &str, record: impl FnOnce()) -> bool {
        // The fork also uses HTTP 500 for request errors (including context
        // pressure). Only its positive fatal-decode classification is evidence
        // against the engine. Missing, malformed and older responses fail closed.
        let Ok(error) = serde_json::from_str::<serde_json::Value>(body) else {
            return false;
        };
        if error
            .pointer("/error/type")
            .and_then(serde_json::Value::as_str)
            != Some("engine_fault")
        {
            return false;
        }
        let Some((root, sent_generation)) = self.engine else {
            return false;
        };
        // 502/504 are gateway failures; 503 is loading/overload. Only this
        // identified local engine's internal error counts. Check and stamp under
        // the owner's lock; the caller logs after that lock has been released.
        status == StatusCode::INTERNAL_SERVER_ERROR
            && crate::inference::slots::directory().record_for_generation(
                &root,
                sent_generation,
                record,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FATAL: &str = r#"{"error":{"type":"engine_fault","message":"Compute error."}}"#;

    // what this catches: missing HTTP failures and blaming a replacement, remote
    // provider, loading lane or client refusal. This unique endpoint is the only
    // directory state changed; callbacks never touch process-global counters.
    #[tokio::test]
    async fn managed_http_errors_count_without_blaming_other_generations_or_refusals(
    ) -> Result<(), String> {
        let root = format!("http://http-health-fixture-{}", Uuid::new_v4());
        let directory = crate::inference::slots::directory();
        let endpoint = directory.endpoint(&root);
        let mut failures = 0;
        assert!(!HttpGenerationHealth::capture(None).record(
            StatusCode::INTERNAL_SERVER_ERROR,
            FATAL,
            || failures += 1
        ));
        assert!(!HttpGenerationHealth::capture(Some(&root)).record(
            StatusCode::INTERNAL_SERVER_ERROR,
            FATAL,
            || failures += 1
        ));
        let launch = endpoint.transition().await;
        let old = launch.start_generation()?;
        // Context pressure (decode ret=1) and invalid batch (ret=-1) are
        // server_error responses in the fork, not fatal engine evidence.
        // Two refusals must never reach the resident recovery threshold.
        for body in [
            r#"{"error":{"type":"server_error","message":"Context size has been exceeded."}}"#,
            r#"{"error":{"type":"server_error","message":"Invalid input batch."}}"#,
            r#"{"error":{"type":"server_error","message":"Compute error."}}"#,
            r#"{"error":{"message":"engine_fault"}}"#,
            r#"{"error":{"type":null}}"#,
            "not json",
            "",
        ] {
            for _ in 0..2 {
                assert!(!HttpGenerationHealth::capture(Some(&root)).record(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    body,
                    || failures += 1
                ));
            }
        }
        assert_eq!(
            failures, 0,
            "unclassified failures preserve the warm engine"
        );
        for _ in 0..2 {
            assert!(HttpGenerationHealth::capture(Some(&root)).record(
                StatusCode::INTERNAL_SERVER_ERROR,
                FATAL,
                || failures += 1
            ));
        }
        assert_eq!(
            failures, 2,
            "two real failures reach existing recovery threshold"
        );
        for status in [
            StatusCode::OK,
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::NOT_FOUND,
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
        ] {
            assert!(
                !HttpGenerationHealth::capture(Some(&root)).record(status, FATAL, || failures += 1)
            );
        }
        let receipt = HttpGenerationHealth::capture(Some(&root));
        old.observed_exit();
        let _replacement = launch.start_generation()?;
        assert!(!receipt.record(StatusCode::INTERNAL_SERVER_ERROR, FATAL, || failures += 1));
        assert_eq!(
            failures, 2,
            "late response cannot blame a replacement engine"
        );
        Ok(())
    }
}
