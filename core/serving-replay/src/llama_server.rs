//! Today's engine behind the seam: llama-server over its HTTP API. An act renders with
//! the engine's own chat template (`/apply-template`) and runs as `/completion` pinned to
//! the mind's slot with `cache_prompt`; paging is `/slots/{id}?action=save|restore`, with
//! the checkpoint subset the live core saves (continuum-engine `turn_admission::page_body`).

use crate::engine_process::EngineProcess;
use crate::http::Endpoint;
use crate::seated::{RestoredPage, SavedPage, SlotEngine};
use crate::target::{ActOutcome, ActRequest, EngineStats, TargetError};

/// The live core's page body (continuum-engine `turn_admission::page_body`), so a replayed page
/// costs what a live one does: the head, the checkpoint at the view end and those made for the
/// prompt's end. A replayed act names no view end (`/completion` takes a rendered prompt, not
/// messages), so its page carries the head and the prompt-end checkpoints.
fn page_body(page: &str) -> serde_json::Value {
    serde_json::json!({"filename": page, "keep_view_checkpoints": true, "keep_newest_checkpoints": 0})
}

pub struct LlamaServer {
    endpoint: Endpoint,
    seats: u32,
    process: Option<EngineProcess>,
    relaunches: u32,
    seed: u64,
}

impl LlamaServer {
    /// Drive an engine this replay launched: relaunches are observed and counted.
    pub fn launched(endpoint: Endpoint, seats: u32, process: EngineProcess, seed: u64) -> Self {
        Self { endpoint, seats, process: Some(process), relaunches: 0, seed }
    }

    /// Drive an engine someone else runs: relaunches cannot be observed.
    pub fn attached(endpoint: Endpoint, seats: u32, seed: u64) -> Self {
        Self { endpoint, seats, process: None, relaunches: 0, seed }
    }

    pub fn ready_ms(&self) -> Option<f64> {
        self.process.as_ref().map(|p| p.ready_ms)
    }

    /// The engine's chat template applied to the act: one renderer, the engine's.
    fn render(&self, request: &ActRequest) -> Result<String, TargetError> {
        let mut messages = Vec::with_capacity(request.messages.len() + 1);
        if let Some(system) = &request.system {
            messages.push(serde_json::json!({"role": "system", "content": system}));
        }
        messages.extend(request.messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})));
        let mut body = serde_json::json!({ "messages": messages });
        if !request.tools.is_empty() {
            body["tools"] = serde_json::Value::Array(request.tools.clone());
        }
        let rendered = self.endpoint.post_ok("/apply-template", &body)?;
        rendered["prompt"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| TargetError(format!("/apply-template returned no prompt: {rendered}")))
    }
}

fn number(v: &serde_json::Value, what: &str) -> Result<f64, TargetError> {
    v.as_f64().ok_or_else(|| TargetError(format!("engine answer lacks {what}")))
}

impl SlotEngine for LlamaServer {
    fn seats(&self) -> u32 {
        self.seats
    }

    fn revive(&mut self) -> Result<bool, TargetError> {
        let Some(process) = self.process.as_mut() else {
            return Ok(false);
        };
        let relaunched = process.revive(&self.endpoint)?;
        self.relaunches += u32::from(relaunched);
        Ok(relaunched)
    }

    fn save(&mut self, seat: u32, page: &str) -> Result<SavedPage, TargetError> {
        let r = self.endpoint.post_ok(&format!("/slots/{seat}?action=save"), &page_body(page))?;
        Ok(SavedPage {
            tokens: number(&r["n_saved"], "n_saved")? as u64,
            bytes: number(&r["n_written"], "n_written")? as u64,
            ms: number(&r["timings"]["save_ms"], "timings.save_ms")?,
        })
    }

    fn restore(&mut self, seat: u32, page: &str) -> Result<RestoredPage, TargetError> {
        let r = self.endpoint.post_ok(&format!("/slots/{seat}?action=restore"), &serde_json::json!({"filename": page}))?;
        Ok(RestoredPage {
            tokens: number(&r["n_restored"], "n_restored")? as u64,
            ms: number(&r["timings"]["restore_ms"], "timings.restore_ms")?,
        })
    }

    fn act(&mut self, seat: u32, request: &ActRequest) -> Result<ActOutcome, TargetError> {
        let prompt = self.render(request)?;
        let body = serde_json::json!({
            "prompt": prompt,
            "id_slot": seat,
            "cache_prompt": true,
            "n_predict": request.n_predict,
            "temperature": 0.0,
            "seed": self.seed,
        });
        let r = self.endpoint.post_ok("/completion", &body)?;
        if r["id_slot"].as_u64() != Some(u64::from(seat)) {
            return Err(TargetError(format!("act pinned to slot {seat} ran on slot {}", r["id_slot"])));
        }
        let t = &r["timings"];
        let cached = number(&t["cache_n"], "timings.cache_n")? as u64;
        let fresh = number(&t["prompt_n"], "timings.prompt_n")? as u64;
        Ok(ActOutcome {
            prompt_tokens: r["tokens_evaluated"].as_u64().unwrap_or(cached + fresh), // unwrap_or: older engines omit it; cached + fresh is the same count
            cached_tokens: cached,
            fresh_tokens: fresh,
            generated_tokens: number(&t["predicted_n"], "timings.predicted_n")? as u64,
            prompt_ms: number(&t["prompt_ms"], "timings.prompt_ms")?,
            generate_ms: number(&t["predicted_ms"], "timings.predicted_ms")?,
        })
    }

    fn stats(&mut self) -> EngineStats {
        EngineStats {
            engine: "llama-server".into(),
            seats: self.seats,
            relaunches: self.process.as_ref().map(|_| self.relaunches),
            argv: self.process.as_ref().map(|p| p.argv().to_vec()).unwrap_or_default(), // unwrap_or_default: an attached engine's command line is not ours to know
        }
    }
}
