//! Smoke responsibility of the llama-server process owner.
use super::*;
/// The probe: count upward over this range. The prompt and the expected answer are both
/// rendered from it, so what the probe asks and what it budgets for are one fact.
const SMOKE_COUNT: std::ops::RangeInclusive<u64> = 1..=10;

/// The question the probe asks. Digits only, so a compliant model spends its decode on the
/// ANSWER, not a preamble.
pub(super) fn smoke_prompt() -> String {
    format!(
        "Reply with only the numbers {} through {} separated by single spaces. No other words.",
        SMOKE_COUNT.start(),
        SMOKE_COUNT.end()
    )
}

/// The answer a compliant model gives: what the quick attempt's cap is measured from.
pub(super) fn smoke_answer() -> String {
    SMOKE_COUNT
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// How far one smoke attempt may decode. Never a number someone picked: either the engine's
/// own token count of the answer the probe asks for, or the bound the engine already applies.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum SmokeCap {
    /// Exactly the answer: `max_tokens` = the served model's token count of [`smoke_answer`],
    /// read from the engine's `/tokenize` (our fork is the contract). The request also asks
    /// the engine for no reasoning phase, so a healthy lane's whole decode IS the answer. A
    /// forced-thinking template whose close tag the engine must inject loses those few
    /// tokens off the answer's tail; [`counts_upward`] needs three, not ten.
    Answer(u64),
    /// No `max_tokens`: the lane's own per-request bound (`--n-predict`, one slot window,
    /// set at launch) and [`DECODE_SMOKE_TIMEOUT`] bound it. Only a lane whose template
    /// thinks even when asked not to pays this: it decodes until its reasoning ends and it
    /// answers (EOS), the shortest a healthy thinking model can prove itself in.
    EngineBound,
}

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
    /// The visible answer followed the instruction (three ascending integers cannot come
    /// out of a ~2-token wedge, so the content proves the decode).
    Alive,
    /// Wedged, incoherent, or structurally not a chat model.
    Dead,
    /// Decode PROVEN (it ran to the attempt's cap) but the whole cap went to private
    /// reasoning and the visible channel is empty: a thinking model mid-thought, not a
    /// dead lane. Retry at [`SmokeCap::EngineBound`] so it can finish and ANSWER.
    ThinkStarved,
}

/// Pure (numbers + strs in, verdict out) so the judgment is unit-testable without
/// a server. The embedding-model-on-the-port incident (BigMama: "uiuiui…" garbage
/// past a count-only gate) still reads `Dead` here: its garbage lands in the
/// VISIBLE channel with no reasoning channel at all.
///
/// No token floor: the healthy arm's content (a monotonic run) is the proof a ~2-token
/// wedge cannot forge, and the think-starved arm's proof is structural, "the decode ran to
/// THIS attempt's cap inside the reasoning channel". An attempt at the engine's own bound
/// that still ends with no answer has nothing further to try, so it reads `Dead`.
pub(super) fn smoke_verdict(
    cap: SmokeCap,
    out_tokens: u64,
    visible: &str,
    reasoning_len: usize,
) -> SmokeVerdict {
    if counts_upward(visible) {
        return SmokeVerdict::Alive;
    }
    let ran_to_cap = match cap {
        SmokeCap::Answer(max_tokens) => out_tokens >= max_tokens,
        SmokeCap::EngineBound => false,
    };
    if ran_to_cap && visible.trim().is_empty() && reasoning_len > 0 {
        return SmokeVerdict::ThinkStarved;
    }
    SmokeVerdict::Dead
}

/// Where a synthetic smoke decode may run without destroying a mind's KV.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum SmokeSeat {
    /// The pool's reserved scratch slot: the probe pins there.
    Scratch(u32),
    /// No slot holds anything (a freshly launched server): any slot is free to prove decode on.
    Empty,
    /// Every slot the probe could land on holds someone's tokens: a synthetic decode would
    /// replace them (card 9e3350bd on the 5090; the M5 2026-10-25, where the reconciler's
    /// adoption probe ran every ensure tick on an adopted engine and wiped a resident's slot
    /// each time, so her next turn re-prefilled from 0 and her next page save captured the
    /// probe). Liveness is then the lane's own deliveries, which the heartbeat reads.
    Occupied,
}

/// PURE: the seat for a smoke decode, from the pool's scratch slot and the server's own
/// `/slots` account. A slot holds something when it is processing or kept a prompt.
pub(super) fn smoke_seat(scratch: Option<u32>, slots: &serde_json::Value) -> SmokeSeat {
    if let Some(slot) = scratch {
        return SmokeSeat::Scratch(slot);
    }
    let holds = |s: &serde_json::Value| {
        s.get("is_processing").and_then(|p| p.as_bool()) == Some(true)
            || s.get("n_prompt_tokens")
                .and_then(|n| n.as_u64())
                .is_some_and(|n| n > 0)
    };
    match slots.as_array() {
        Some(all) if !all.iter().any(holds) => SmokeSeat::Empty,
        _ => SmokeSeat::Occupied,
    }
}

/// The scratch slot of the pool installed for `v1_url`'s server, if it has one. A server whose
/// pool was never probed or has none has no scratch slot to aim the smoke at.
pub(super) fn scratch_slot_for(v1_url: &str) -> Option<u32> {
    use continuum_base::probed::Probed;
    let root = v1_url.strip_suffix("/v1")?;
    match crate::inference::slots::directory().get(root) {
        Probed::Known(pool) => pool.scratch_slot(),
        Probed::NotAsked | Probed::Absent(_) => None,
    }
}

/// The engine's own token count of [`smoke_answer`]: the quick attempt's cap. POSTs the
/// served model's `/tokenize` (an HTTP-thread call: no slot, no decode, so the control-plane
/// timeout). `None` when the transport, status or schema failed: a lane that cannot
/// tokenize cannot be proven.
pub(super) async fn smoke_answer_tokens(client: &reqwest::Client, root: &str) -> Option<u64> {
    let resp = match client
        .post(format!("{root}/tokenize"))
        .timeout(PROBE_TIMEOUT)
        .json(&serde_json::json!({ "content": smoke_answer() }))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp,
        _ => return None,
    };
    let Ok(v) = resp.json::<serde_json::Value>().await else {
        return None;
    };
    token_count_of(&v)
}

/// PURE: the token count in a `/tokenize` response (`{"tokens": [..]}`). An empty list is
/// no answer at all, so it reads `None` rather than a zero cap.
pub(super) fn token_count_of(v: &serde_json::Value) -> Option<u64> {
    let n = v.get("tokens")?.as_array()?.len();
    u64::try_from(n).ok().filter(|n| *n > 0)
}

/// PURE: the smoke request body at `cap`, pinned to `scratch` when the pool has one.
/// Every attempt asks the engine for no reasoning phase (`enable_thinking: false` for a
/// template that honours it, `reasoning_budget_tokens: 0` for one whose thinking the
/// engine can close itself): a liveness probe needs an answer, not a thought.
pub(super) fn smoke_body(cap: SmokeCap, scratch: Option<u32>) -> serde_json::Value {
    let mut body = serde_json::json!({
        "messages": [{ "role": "user", "content": smoke_prompt() }],
        "chat_template_kwargs": { "enable_thinking": false },
        "reasoning_budget_tokens": 0,
        "stream": false,
        "temperature": 0.0,
    });
    if let Some(obj) = body.as_object_mut() {
        if let SmokeCap::Answer(max_tokens) = cap {
            obj.insert("max_tokens".to_string(), serde_json::json!(max_tokens));
        }
        // Pin the probe to the SCRATCH slot (Probe class — B2). Unpinned, llama's
        // LCP selection could seat this tiny prompt on a citizen's warm slot and
        // evict her 30k tail to prove the lane decodes — the exact "contend for a
        // slot with the work" the heartbeat's own doc warns about. Best-effort: an
        // uninstalled pool (server not yet probed by any turn) leaves it unpinned.
        if let Some(scratch) = scratch {
            obj.insert("id_slot".to_string(), serde_json::json!(scratch));
            obj.insert("cache_prompt".to_string(), serde_json::json!(true));
        }
    }
    body
}

/// One smoke-decode HTTP attempt: POST the count-to-ten prompt at `cap`,
/// return `(completion_tokens, visible_content, reasoning_len)` — or `None` when
/// the transport/status/schema failed (each case logged where it is detected).
/// Free fn (client + url in) so [`decode_smoke_ok`]'s policy stays a readable
/// verdict match instead of interleaved I/O.
pub(super) async fn smoke_attempt(
    client: &reqwest::Client,
    v1_url: &str,
    cap: SmokeCap,
) -> Option<(u64, String, usize)> {
    let url = format!("{v1_url}/chat/completions");
    let body = smoke_body(cap, scratch_slot_for(v1_url));
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
        tracing::warn!(target: "continuum_core::inference::llama_server", "smoke decode: response carried no usage.completion_tokens — cannot \
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
        .unwrap_or("") // absent content = an empty visible channel, judged by the verdict
        .to_string();
    // The PRIVATE reasoning channel (llama-server splits `<think>` out when the
    // template opens one). Its length is evidence of where the budget went.
    let reasoning_len = v
        .pointer("/choices/0/message/reasoning_content")
        .and_then(|c| c.as_str())
        .map(str::len)
        .unwrap_or(0); // absent reasoning channel = no reasoning spent
    Some((out_tokens, visible, reasoning_len))
}

/// One attempt at `cap`, judged: `None` when the attempt itself failed (transport, status,
/// schema), else the verdict of what it decoded.
pub(super) async fn smoke_attempt_verdict(
    client: &reqwest::Client,
    v1_url: &str,
    cap: SmokeCap,
) -> Option<SmokeVerdict> {
    let (out_tokens, visible, reasoning_len) = smoke_attempt(client, v1_url, cap).await?;
    Some(smoke_verdict(cap, out_tokens, &visible, reasoning_len))
}
