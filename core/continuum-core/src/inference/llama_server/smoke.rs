//! The decode smoke probe: a real multi-token generation that proves the compute path,
//! judged over both response channels (`counts_upward`, `SmokeVerdict`), with the quick and
//! thinking budgets and the bounded HTTP attempt. `decode_smoke_ok` on the process is the
//! policy over these.

use super::*;

/// Bound for the decode SMOKE-PROBE (one-token generation in [`decode_smoke_ok`]).
/// Unlike a control-plane read, this queues behind the inference slots and runs an
/// actual decode, so it is given more room than [`PROBE_TIMEOUT`] — but still
/// bounded well under the daemon's own budget so a wedged compute path (the very
/// thing we are probing for) resolves to "cannot decode" fast instead of hanging
/// the reconcile. A healthy 14B answers a 1-token request in well under a second.
/// Generous on purpose: the probe shares the lane with LIVE persona traffic, and a
/// 1,238-token co-tenant prefill alone runs ~9s — measured 2026-07-23: the identical
/// probe body took 60s / 24s / 0s across three tries behind normal load. The old 10s
/// budget produced sustained FALSE decode-failures that killed healthy lanes all
/// night (heartbeat → not-ready → kill+respawn → bind race → "crash loop"). A probe's
/// job is truth, not speed — it runs on a slow cadence; let it wait out the queue.
pub(super) const DECODE_SMOKE_TIMEOUT: Duration = Duration::from_secs(75);

/// Minimum completion tokens a healthy lane must produce on the decode smoke-probe.
/// The failure mode this guards is the intermittently-wedged fresh lane that answers
/// EVERY request with ~2 tokens then stops (observed on ephemeral eval lanes: same
/// base+gene generated 82 tok/task on one spawn, 2 tok/task on the next, silently
/// scoring the whole benchmark 0). An HTTP-200-only probe with `max_tokens: 1` cannot
/// tell that lane from a healthy one — so the probe now forces a prompt a healthy model
/// MUST answer with many tokens and asserts it did. `5` sits comfortably above the
/// ~2-token wedge and far below the ~20+ a healthy "count to 20" yields.
// context-budget-exempt: how many tokens a post-spawn smoke decode must produce to prove the lane is alive — a liveness probe, not a budget
pub(super) const MIN_SMOKE_DECODE_TOKENS: u64 = 5;

/// Smoke budget for the quick first attempt. Cheap on purpose: a non-thinking
/// model answers "count to 10" well inside this, and the probe must stay a light
/// passenger on a busy co-tenant lane.
// context-budget-exempt: liveness-probe first-attempt budget — probe cost control, never a serving cap
pub(super) const QUICK_SMOKE_BUDGET: u64 = 24;

/// Smoke budget for the THINK-STARVED retry: a THINKING model that spent the whole
/// quick budget inside its reasoning channel gets one attempt with room to finish
/// thinking AND put the ten digits in the ANSWER channel. Only a proven
/// think-starved outcome (decode ran, visible empty, reasoning non-empty) pays this.
// context-budget-exempt: liveness-probe retry budget for thinking models — room to traverse the reasoning phase, never a serving cap
pub(super) const THINKING_SMOKE_BUDGET: u64 = 768;

/// Does `text` contain a run of three consecutive ascending integers (`… 4 5 6 …`)?
///
/// The decode smoke-probe's assertion. Deliberately tolerant of formatting — separators,
/// preambles and trailing chatter are all fine, because what is being tested is whether the model
/// FOLLOWED a trivial instruction, not whether it formatted nicely. Three in a row is the point:
/// noise can emit a stray digit, but a monotonic run means the model actually understood.
///
/// Pure (str in, bool out) so it is unit-testable without a server.
pub(super) fn counts_upward(text: &str) -> bool {
    let nums: Vec<u64> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    nums.windows(3)
        .any(|w| w[1] == w[0] + 1 && w[2] == w[1] + 1)
}

/// Verdict of one smoke-decode attempt, judged from the response's TWO channels.
///
/// THINK-STARVED (2026-08-15, root cause of a fully dead bench round): a THINKING
/// model spends its whole small smoke budget inside the reasoning channel — the
/// server decodes perfectly (the round's llama-server stderr showed healthy ~30 t/s
/// generations of exactly `max_tokens` on every probe) while `content` comes back
/// EMPTY, so a visible-channel-only judgment reads a working brain as a dead lane,
/// marks serving degraded forever, and every turn then waits on a serving
/// transition that never resolves. The reasoning channel is an ADAPTER FACT — any
/// thinking model (Qwen, DeepSeek, whatever comes next) hits this identically — so
/// the verdict is judged structurally, never by model name.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum SmokeVerdict {
    /// Multi-token decode AND the visible answer followed the instruction.
    Alive,
    /// Wedged, incoherent, or structurally not a chat model.
    Dead,
    /// Decode PROVEN (token floor met) but the whole budget went to private
    /// reasoning and the visible channel is empty — a thinking model mid-thought,
    /// not a dead lane. Retry with a budget that lets it finish and ANSWER.
    ThinkStarved,
}

/// Pure (numbers + strs in, verdict out) so the judgment is unit-testable without
/// a server. The embedding-model-on-the-port incident (BigMama: "uiuiui…" garbage
/// past a count-only gate) still reads `Dead` here: its garbage lands in the
/// VISIBLE channel with no reasoning channel at all.
pub(super) fn smoke_verdict(out_tokens: u64, visible: &str, reasoning_len: usize) -> SmokeVerdict {
    if out_tokens >= MIN_SMOKE_DECODE_TOKENS && counts_upward(visible) {
        return SmokeVerdict::Alive;
    }
    if out_tokens >= MIN_SMOKE_DECODE_TOKENS && visible.trim().is_empty() && reasoning_len > 0 {
        return SmokeVerdict::ThinkStarved;
    }
    SmokeVerdict::Dead
}

/// One smoke-decode HTTP attempt: POST the count-to-ten prompt with `max_tokens`,
/// return `(completion_tokens, visible_content, reasoning_len)` — or `None` when
/// the transport/status/schema failed (each case logged where it is detected).
/// Free fn (client + url in) so [`decode_smoke_ok`]'s policy stays a readable
/// verdict match instead of interleaved I/O.
pub(super) async fn smoke_attempt(
    client: &reqwest::Client,
    v1_url: &str,
    max_tokens: u64,
) -> Option<(u64, String, usize)> {
    let url = format!("{v1_url}/chat/completions");
    let mut body = serde_json::json!({
        // Digits-only instruction so a compliant model spends its budget on the ANSWER,
        // not a preamble — the content assertion needs the answer to actually fit.
        "messages": [{ "role": "user", "content":
            "Reply with only the numbers 1 through 10 separated by single spaces. No other words." }],
        "max_tokens": max_tokens,
        "stream": false,
        "temperature": 0.0,
    });
    // Pin the probe to the SCRATCH slot (Probe class — B2). Unpinned, llama's
    // LCP selection could seat this tiny prompt on a citizen's warm slot and
    // evict her 30k tail to prove the lane decodes — the exact "contend for a
    // slot with the work" the heartbeat's own doc warns about. Best-effort: an
    // uninstalled pool (server not yet probed by any turn) leaves it unpinned.
    if let Some(root) = v1_url.strip_suffix("/v1") {
        if let Some(Some(pool)) = crate::inference::slots::directory().get(root) {
            if let Some(scratch) = pool.scratch_slot() {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("id_slot".to_string(), serde_json::json!(scratch));
                    obj.insert("cache_prompt".to_string(), serde_json::json!(true));
                }
            }
        }
    }
    let resp = match client
        .post(&url)
        .timeout(DECODE_SMOKE_TIMEOUT)
        .json(&body)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp,
        _ => return None,
    };
    let Ok(v) = resp.json::<serde_json::Value>().await else {
        return None;
    };
    // A missing/!u64 `usage.completion_tokens` is NOT "produced 0 tokens" — it is
    // "this server did not tell us". Both fail the smoke test, but only one of them
    // is a wedge; say which, or a schema change reads forever as a dead lane.
    let Some(out_tokens) = v
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|t| t.as_u64())
    else {
        tracing::warn!(
            "smoke decode: response carried no usage.completion_tokens — cannot \
             confirm the compute path (treating as NOT proven, but this is a \
             missing field, not a measured zero)"
        );
        return None;
    };
    // The VISIBLE answer channel. Missing is treated as empty here (not None):
    // paired with a non-empty reasoning channel that shape is the think-starved
    // outcome, which the verdict must SEE rather than abort on.
    let visible = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    // The PRIVATE reasoning channel (llama-server splits `<think>` out when the
    // template opens one). Its length is evidence of where the budget went.
    let reasoning_len = v
        .pointer("/choices/0/message/reasoning_content")
        .and_then(|c| c.as_str())
        .map(str::len)
        .unwrap_or(0);
    Some((out_tokens, visible, reasoning_len))
}
