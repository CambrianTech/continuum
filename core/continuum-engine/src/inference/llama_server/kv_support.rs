//! Kv support responsibility of the llama-server process owner.
use super::*;
/// Ask the serving binary, ONCE per process, which KV cache types its build accepts,
/// and record the answer for [`crate::inference::kv_cache_plan`].
///
/// The engine's own `--help` outranks any table in our source — a table can only be a
/// stale guess about someone else's build. Bounded at 10 s like the `--version` and
/// `--list-devices` probes beside it, and every outcome is NAMED: `answered`,
/// `did_not_enumerate` (the flag exists but the build lists no allowed values — we do
/// NOT guess; the backend table decides), `probe_failed`, `probe_timeout`.
///
/// One ask per engine binary that answers, even "it did not say"; a failed or timed-out probe
/// is asked again at the next launch, and a promoted binary afresh (f3ba3b56 item 11).
pub(super) async fn ensure_engine_kv_support_recorded(bin: &str, mut command: tokio::process::Command) {
    static ANSWERED_BY: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);
    if ANSWERED_BY.lock().as_deref() == Some(bin) {
        return;
    }
    command.arg("--help");
    // owned (continuum_process::run_owned): a help probe that hangs is ended with anything it started
    let probe = continuum_process::run_owned(command, Some(Duration::from_secs(10)), "engine-help-probe", std::sync::Arc::default()).await;
    let (outcome, support, error) = match probe {
        Err(e) => ("probe_failed", None, e.to_string()),
        Ok(out) => match (&out.ended, out.capture_note()) {
            (continuum_process::Ended::TimedOut { .. }, _) => ("probe_timeout", None, "no answer in 10 s".to_string()),
            // help text that wasn't read whole can't say which values it lists
            (_, Some(note)) => ("probe_failed", None, note),
            (continuum_process::Ended::Exited(_), None) => {
                let mut text = String::from_utf8_lossy(&out.stdout.bytes).into_owned();
                text.push('\n');
                text.push_str(&String::from_utf8_lossy(&out.stderr.bytes));
                ENGINE_TRAIN_DIR.store(engine_help_lists_train_dir(&text), std::sync::atomic::Ordering::Relaxed);
                match crate::inference::kv_cache_plan::parse_engine_kv_support(&text) {
                    Some(v) => ("answered", Some(v), String::new()),
                    None => ("did_not_enumerate", None, String::new()),
                }
            }
        },
    };
    crate::inference::kv_cache_plan::record_engine_quantized_kv_support(support); // None = unanswered: the backend table decides
    if matches!(outcome, "answered" | "did_not_enumerate") { *ANSWERED_BY.lock() = Some(bin.to_owned()) } else { ENGINE_TRAIN_DIR.store(false, std::sync::atomic::Ordering::Relaxed) }
    continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.kv_cache.engine_support",
        bin = bin,
        outcome = outcome,
        answered = support.is_some(),
        quantized_kv = support.unwrap_or(false), // probe field: read it WITH `answered`
        train_dir = engine_accepts_train_dir(),
        error = %error,
        "what the serving binary says its build accepts for --cache-type-k; unanswered \
         falls through to the backend table, never to a guess"
    );
}
