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
        if self.record(
            status,
            crate::inference::llama_server::note_real_decode_failure,
        ) {
            continuum_base::probe!(
                class = "inference.decode.failed",
                kind = "http_internal_server_error",
                status = status.as_u16(),
                "managed engine rejected generation before streaming; existing lane health owns recovery"
            );
        }
    }

    fn record(self, status: StatusCode, record: impl FnOnce()) -> bool {
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
        assert!(!HttpGenerationHealth::capture(None)
            .record(StatusCode::INTERNAL_SERVER_ERROR, || failures += 1));
        assert!(!HttpGenerationHealth::capture(Some(&root))
            .record(StatusCode::INTERNAL_SERVER_ERROR, || failures += 1));
        let launch = endpoint.transition().await;
        let old = launch.start_generation()?;
        for _ in 0..2 {
            assert!(HttpGenerationHealth::capture(Some(&root))
                .record(StatusCode::INTERNAL_SERVER_ERROR, || failures += 1));
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
            assert!(!HttpGenerationHealth::capture(Some(&root)).record(status, || failures += 1));
        }
        let receipt = HttpGenerationHealth::capture(Some(&root));
        old.observed_exit();
        let _replacement = launch.start_generation()?;
        assert!(!receipt.record(StatusCode::INTERNAL_SERVER_ERROR, || failures += 1));
        assert_eq!(
            failures, 2,
            "late response cannot blame a replacement engine"
        );
        Ok(())
    }
}
