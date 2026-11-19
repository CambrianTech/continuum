/// Audit instrumentation for JSON body serialization cost.
///
/// "JSON audit pipe 1": measure how many bytes and CPU ms each act spends
/// serializing messages into an OpenAI-format JSON body, so we can quantify
/// the improvement when switching to raw token-ID frames later.

use std::time::Instant;

use serde_json::Value;

/// Wrap a `finish_body` call: measure elapsed time and output byte size,
/// then emit them via tracing as structured events keyed by `act_id`.
///
/// Returns the same `serde_json::Value` that the inner function produces.
pub fn audit_finish_body<F>(
    act_id: &str,
    finish_fn: F,
) -> Value
where
    F: FnOnce(&mut Value) -> Result<(), anyhow::Error>,
{
    let start = Instant::now();

    // Build the body inside an inner scope so we can measure the full span.
    let mut body = Value::Object(serde_json::map::Map::new());
    if let Err(e) = finish_fn(&mut body) {
        tracing::warn!(
            target: "json_audit",
            act = %act_id,
            error = %e,
            phase = "finish_body_failed",
        );
        return body;
    }

    let elapsed_ms = start.elapsed().as_secs_f64() * 1_000.0;

    // Serialize to get the byte size (the same path the HTTP layer uses).
    let json_bytes = match serde_json::to_vec(&body) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(
                target: "json_audit",
                act = %act_id,
                error = %e,
                phase = "serialize_failed",
                elapsed_ms,
            );
            return body;
        }
    };

    let json_bytes_len = json_bytes.len();

    tracing::info!(
        target: "json_audit",
        act = %act_id,
        elapsed_ms = format_args!("{:.3}", elapsed_ms),
        json_bytes = json_bytes_len,
        phase = "finish_body_done",
    );

    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_wraps_a_simple_fn() {
        let mut called = false;
        let result = audit_finish_body("test-1", |v: &mut Value| {
            v["hello"] = serde_json::json!("world");
            called = true;
            Ok(())
        });
        assert!(called);
        assert_eq!(result["hello"], "world");
    }

    #[test]
    fn audit_reports_on_error() {
        let result = audit_finish_body("test-2", |_v: &mut Value| {
            Err(anyhow::anyhow!("boom"))
        });
        // Should return an empty object and log the error.
        assert!(result.is_object());
    }
}
