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
    if !super::genome::needs_asking(&super::genome::ENGINE_PROBES.lock(), bin) {
        return;
    }
    let mut train_dir = false;
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
                train_dir = engine_help_lists_train_dir(&text);
                match crate::inference::kv_cache_plan::parse_engine_kv_support(&text) {
                    Some(v) => ("answered", Some(v), String::new()),
                    None => ("did_not_enumerate", None, String::new()),
                }
            }
        },
    };
    crate::inference::kv_cache_plan::record_engine_quantized_kv_support(support); // None = unanswered: the backend table decides
    // This binary's entry alone, written whole: what it said, or that it did not answer.
    let outcome_of_probe = match matches!(outcome, "answered" | "did_not_enumerate") {
        true => super::genome::ProbeOutcome::Answered { train_dir },
        false => super::genome::ProbeOutcome::Unanswered,
    };
    super::genome::ENGINE_PROBES.lock().insert(bin.to_owned(), outcome_of_probe);
    continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.kv_cache.engine_support",
        bin = bin,
        outcome = outcome,
        answered = support.is_some(),
        quantized_kv = support.unwrap_or(false), // probe field: read it WITH `answered`
        train_dir = super::genome::engine_accepts_train_dir(bin),
        error = %error,
        "what the serving binary says its build accepts for --cache-type-k; unanswered \
         falls through to the backend table, never to a guess"
    );
}

/// The command that runs the engine binary `bin`: its install receipt's (a controlled cwd and
/// environment) when it has one. The one place both the launch and the support probe build it.
pub(super) fn engine_command_of(
    receipt: &Option<crate::inference::engine_install::EngineInstallReceipt>,
    bin: &str,
) -> Result<tokio::process::Command, LlamaServerError> {
    match receipt {
        Some(receipt) => receipt.command().map_err(LlamaServerError::Spawn),
        None => Ok(tokio::process::Command::new(bin)),
    }
}

impl LlamaServerProcess {
    /// Whether the engine binary accepts `--train-dir`, asking it first when it has not been
    /// asked: a core that adopts a live engine decides a gene change in place before any launch of
    /// its own asked (IntelMac 2026-11-07: from=0 to=7 read the unasked flag as no and relaunched
    /// the engine cold). The answer is the binary's own, keyed by it ([`super::genome::train_dir_support`]).
    pub(super) async fn engine_takes_train_dir(&self) -> Result<bool, LlamaServerError> {
        let bin = self.engine_bin()?;
        if let Some(answer) = super::genome::train_dir_support(&bin) {
            return Ok(answer);
        }
        let receipt = crate::inference::engine_install::EngineInstallReceipt::prepare(bin.clone())
            .await
            .map_err(LlamaServerError::Spawn)?;
        ensure_engine_kv_support_recorded(&bin, engine_command_of(&receipt, &bin)?).await;
        Ok(super::genome::engine_accepts_train_dir(&bin))
    }
}
