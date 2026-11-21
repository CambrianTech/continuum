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
/// Accepts the pre-built base body (from `build_base_body`) so it measures
/// only the finish step — the same pattern the production path uses.
/// Returns the original Result from `finish_fn`, plus logs audit data.
pub fn audit_finish_body<F, R>(
    act_id: &str,
    body: &mut Value,
    finish_fn: F,
) -> R
where
    F: FnOnce(&mut Value) -> R,
{
    let start = Instant::now();

    let result = finish_fn(body);

    let elapsed_ms = start.elapsed().as_secs_f64() * 1_000.0;

    // Serialize to get the byte size (the same path the HTTP layer uses).
    let json_bytes_len = match serde_json::to_vec(body) {
        Ok(bytes) => bytes.len() as i64,
        Err(_) => -1, // serialization failed — report negative as sentinel
    };

    tracing::info!(
        target: "json_audit",
        act = %act_id,
        elapsed_ms = format_args!("{:.3}", elapsed_ms),
        json_bytes = json_bytes_len,
        phase = "finish_body_done",
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_wraps_a_simple_fn() {
        let mut body = serde_json::json!({});
        let result: Result<(), anyhow::Error> = audit_finish_body("test-1", &mut body, |v: &mut Value| {
            v["hello"] = serde_json::json!("world");
            Ok(())
        });
        assert_eq!(result, Ok(()));
        assert_eq!(body["hello"], "world");
    }

    #[test]
    fn audit_returns_result_on_error() {
        let mut body = serde_json::json!({});
        let result: Result<(), anyhow::Error> = audit_finish_body("test-2", &mut body, |_v: &mut Value| {
            Err(anyhow::anyhow!("boom"))
        });
        assert!(result.is_err());
    }
}
