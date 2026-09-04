//! `serving/cache-probe` — the substrate measures its OWN prompt-cache reuse.
//!
//! Joel 2026-09-04: "Reliability and repeatability is the name of the game." KV
//! reuse read 0.0 on 99 of 99 turns for hours and the only instrument was a
//! hand-typed curl. This verb is that curl as a repeatable check: send the same
//! prompt twice to one pinned slot with `cache_prompt` on and report what the
//! server says it reused (`timings.cache_n`). A `second_hit_rate` near 1.0 means
//! the server caches; near 0 means the serving configuration (slot, flags,
//! template) defeats reuse before any prompt-shape question arises.

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use crate::inference::llama_server::ServingSnapshot;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/serving/ServingCacheProbeParams.ts")]
pub struct ServingCacheProbeParams {
    /// The slot to pin both requests to (default 0). Use a slot no citizen
    /// holds, or accept evicting her warm prefix for one probe.
    #[ts(optional)]
    pub slot: Option<u32>,
    /// Approximate prompt size in filler words (default 600 → ~800 tokens):
    /// large enough to clear the server's `--cache-reuse` chunk floor.
    #[ts(optional)]
    pub words: Option<u32>,
    /// Also round-trip the slot through `save` → `restore` and re-send: reports
    /// whether a RESTORED prefix is reusable. Measured 2026-09-04 on
    /// llama-server b10751 with a hybrid-cache model: in-slot reuse 11291/11295,
    /// after restore 0/11295 (a full re-prefill) — the restore economy paid
    /// full prefill on every turn while reporting "restore ok in 48 ms".
    #[ts(optional)]
    pub roundtrip: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/serving/ServingCacheProbeResult.ts")]
pub struct ServingCacheProbeResult {
    pub base_url: String,
    pub slot: u32,
    pub prompt_tokens: u32,
    pub first_cache_n: u32,
    pub second_cache_n: u32,
    /// `second_cache_n / prompt_tokens` — the number that should be near 1.0.
    pub second_hit_rate: f64,
    pub first_prompt_ms: f64,
    pub second_prompt_ms: f64,
    /// `reuses` | `no_reuse` — the one-word verdict a dashboard or a test reads.
    pub verdict: String,
    /// With `roundtrip`: tokens reused by the request sent right after
    /// `save` → `restore` (0 = a restored prefix is dead on this server/model).
    #[ts(optional)]
    pub after_restore_cache_n: Option<u32>,
    /// With `roundtrip`: `reuses` | `no_reuse` for the restored prefix.
    #[ts(optional)]
    pub restore_verdict: Option<String>,
}

crate::action_command! {
    /// Send one prompt twice to a pinned slot and report the server's own
    /// cache reuse — the repeatable form of "is KV reuse working at all?".
    pub struct ServingCacheProbe { serving: watch::Receiver<ServingSnapshot> }
    name: "serving/cache-probe",
    access: Privileged,
    params: ServingCacheProbeParams,
    output: ServingCacheProbeResult,
    run(this, _ctx, p) => {
        let snap = this.serving.borrow().clone();
        if !snap.ready || snap.base_url.is_empty() {
            return Err(CommandError::Invalid(
                "serving/cache-probe: no model is being served (serving.ready=false)".into(),
            ));
        }
        let slot = p.slot.unwrap_or(0);
        let words = p.words.unwrap_or(600).clamp(50, 5_000) as usize;
        let filler = "lorem ipsum dolor sit amet ".repeat(words / 5);
        let body = serde_json::json!({
            "model": snap.active_model.clone().unwrap_or_else(|| "served".to_string()),
            "id_slot": slot,
            "cache_prompt": true,
            "max_tokens": 4,
            "temperature": 0,
            "messages": [
                {"role": "system", "content": format!("You are a terse assistant. {filler}")},
                {"role": "user", "content": "Say OK."}
            ]
        });
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| CommandError::Internal(format!("http client: {e}")))?;
        let url = format!("{}/chat/completions", snap.base_url.trim_end_matches('/'));
        let mut timings: Vec<(u32, u32, f64)> = Vec::with_capacity(2);
        for _ in 0..2 {
            let resp = client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| CommandError::Internal(format!("cache-probe request: {e}")))?;
            let v: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| CommandError::Internal(format!("cache-probe response: {e}")))?;
            let t = v.get("timings").cloned().unwrap_or(serde_json::Value::Null);
            let n = |k: &str| t.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0); // unwrap_or: a server without timings reports 0 — the verdict then reads no_reuse, loudly
            timings.push((n("prompt_n") as u32, n("cache_n") as u32, n("prompt_ms")));
        }
        let (first_n, first_cache, first_ms) = timings[0];
        let (second_n, second_cache, second_ms) = timings[1];
        let prompt_tokens = first_n.max(second_n).max(second_cache);
        let rate = if prompt_tokens == 0 { 0.0 } else { second_cache as f64 / prompt_tokens as f64 };
        let (after_restore_cache_n, restore_verdict) = if p.roundtrip.unwrap_or(false) {
            let slot_url = |action: &str| {
                format!("{}/slots/{slot}?action={action}", snap.base_url.trim_end_matches("/v1"))
            };
            let file = format!("cache-probe-{slot}.bin");
            for action in ["save", "restore"] {
                client
                    .post(slot_url(action))
                    .json(&serde_json::json!({ "filename": file }))
                    .send()
                    .await
                    .map_err(|e| CommandError::Internal(format!("cache-probe {action}: {e}")))?;
            }
            let v: serde_json::Value = client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| CommandError::Internal(format!("cache-probe after-restore request: {e}")))?
                .json()
                .await
                .map_err(|e| CommandError::Internal(format!("cache-probe after-restore response: {e}")))?;
            let cached = v
                .get("timings")
                .and_then(|t| t.get("cache_n"))
                .and_then(|x| x.as_f64())
                .unwrap_or(0.0) as u32; // unwrap_or: no timings = no reuse, reported loudly as 0
            let ok = prompt_tokens > 0 && (cached as f64 / prompt_tokens as f64) >= 0.5;
            (Some(cached), Some(if ok { "reuses".to_string() } else { "no_reuse".to_string() }))
        } else {
            (None, None)
        };
        Ok(ServingCacheProbeResult {
            after_restore_cache_n,
            restore_verdict,
            base_url: snap.base_url,
            slot,
            prompt_tokens,
            first_cache_n: first_cache,
            second_cache_n: second_cache,
            second_hit_rate: rate,
            first_prompt_ms: first_ms,
            second_prompt_ms: second_ms,
            verdict: if rate >= 0.5 { "reuses".into() } else { "no_reuse".into() },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: the probe refuses loudly when nothing is served, instead
    // of reporting a fabricated no_reuse against no server.
    #[tokio::test]
    async fn refuses_when_nothing_is_served() {
        let (_tx, rx) = watch::channel(ServingSnapshot::empty());
        let cmd = ServingCacheProbe { serving: rx };
        let err = cmd
            .run(&Ctx::default(), ServingCacheProbeParams::default())
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)), "{err}");
        assert_eq!(ServingCacheProbe::NAME, "serving/cache-probe");
    }
}
