//! The dream stage's engine calls (docs/architecture/ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md §9.6):
//! a mind's shadow adapter made fresh, one served reply learned into it, and the shadow saved as
//! an immutable snapshot. Learning is a small step on the same loaded base between her turns,
//! paced by the engine's duty share; a step the engine defers while a slot is busy comes back
//! with when to retry, and is never dropped. Never a job, a dataset or a second model.

use super::configuration::{serving_root, PROBE_TIMEOUT};

/// What a learn step did.
#[derive(Debug, Clone, PartialEq)]
pub struct Learned {
    pub loss: f64,
    pub n_labels: i32,
    pub t_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DreamError {
    /// The engine predates the route: the lane takes no dream steps.
    #[error("the engine has no {0} route")]
    NoRoute(&'static str),
    /// Learning yields to a busy slot: retry after this long, with the same inputs.
    #[error("the engine deferred the step for {retry_after_ms} ms")]
    Deferred { retry_after_ms: u64 },
    #[error("the engine refused: {0}")]
    Refused(String),
    #[error("the engine did not answer: {0}")]
    Unreachable(String),
}

/// A mind's shadow: the adapter her steps train, at her gene's shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowShape {
    pub rank: u32,
    pub alpha: f32,
    pub targets: Vec<String>,
    pub top_layers: u32,
    pub seed: u32,
}

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// `POST /lora-adapters/fresh`: a fresh shadow named `name` (zero-B, so the model's output is
/// unchanged until the first step), loaded dormant. Returns its adapter id.
pub async fn fresh_shadow(name: &str, shape: &ShadowShape) -> Result<i64, DreamError> {
    let body = serde_json::json!({
        "name": name,
        "rank": shape.rank,
        "alpha": shape.alpha,
        "targets": shape.targets.join(","),
        "top_layers": shape.top_layers,
        "seed": shape.seed,
    });
    let answer = post("/lora-adapters/fresh", &body, Some(PROBE_TIMEOUT)).await?;
    answer.get("id").and_then(|id| id.as_i64()).ok_or_else(|| DreamError::Unreachable(format!("/lora-adapters/fresh answered {answer}")))
}

/// `POST /lora-adapters/load`: her existing shadow file `name` loaded dormant (after an engine
/// restart, which forgets loaded adapters but not the file). Returns its adapter id.
pub async fn load_shadow(name: &str) -> Result<i64, DreamError> {
    let answer = post("/lora-adapters/load", &serde_json::json!({ "name": name }), Some(PROBE_TIMEOUT)).await?;
    answer.get("id").and_then(|id| id.as_i64()).ok_or_else(|| DreamError::Unreachable(format!("/lora-adapters/load answered {answer}")))
}

/// `POST /learn`: one served reply learned into `lora_id`. `messages` are the conversation she was
/// served, its last message the reply to learn (her own, or the reviewed correction); `tools` the
/// tools she was offered, so the tokens are the tokens she was served (§9.1).
pub async fn learn(lora_id: i64, messages: &serde_json::Value, tools: Option<&serde_json::Value>, lr: f32) -> Result<Learned, DreamError> {
    let mut body = serde_json::json!({ "messages": messages, "lora_id": lora_id, "lr": lr });
    if let Some(tools) = tools {
        body["tools"] = tools.clone();
    }
    // No timeout: a step takes what the engine's duty share gives it, and a busy lane answers
    // at once with a deferral rather than holding the call.
    let answer = post("/learn", &body, None).await?;
    learned_of(&answer)
}

/// `POST /lora-adapters/save`: the loaded adapter `lora_id` written as the immutable snapshot `name`.
pub async fn save_shadow(lora_id: i64, name: &str) -> Result<(), DreamError> {
    post("/lora-adapters/save", &serde_json::json!({ "id": lora_id, "name": name }), Some(PROBE_TIMEOUT)).await.map(|_| ())
}

async fn post(route: &'static str, body: &serde_json::Value, timeout: Option<std::time::Duration>) -> Result<serde_json::Value, DreamError> {
    let mut request = client().post(format!("{}{route}", serving_root())).json(body);
    if let Some(timeout) = timeout {
        request = request.timeout(timeout);
    }
    let resp = request.send().await.map_err(|e| DreamError::Unreachable(e.to_string()))?;
    match resp.status().as_u16() {
        200..=299 => resp.json().await.map_err(|e| DreamError::Unreachable(e.to_string())),
        404 | 405 => Err(DreamError::NoRoute(route)),
        status => Err(DreamError::Refused(format!(
            "{route} status {status}: {}",
            resp.text().await.unwrap_or_default() // unwrap_or_default: the status alone still names the refusal
        ))),
    }
}

/// PURE: a learn answer: a step taken, a deferral, or a refusal with the engine's reason.
pub(crate) fn learned_of(answer: &serde_json::Value) -> Result<Learned, DreamError> {
    let outcome = answer.get("outcome").unwrap_or(answer); // the outcome object, whether or not the route wraps it
    if outcome.get("deferred").and_then(|d| d.as_bool()) == Some(true) {
        return Err(DreamError::Deferred {
            retry_after_ms: outcome.get("retry_after_ms").and_then(|r| r.as_u64()).unwrap_or(0), // unwrap_or: a deferral without a time retries at the next step
        });
    }
    if outcome.get("ok").and_then(|ok| ok.as_bool()) != Some(true) {
        let why = outcome.get("failure").and_then(|f| f.as_str()).unwrap_or("the engine named no reason"); // unwrap_or: a refusal still refuses
        return Err(DreamError::Refused(why.to_owned()));
    }
    Ok(Learned {
        loss: outcome.get("loss").and_then(|l| l.as_f64()).unwrap_or(0.0), // unwrap_or: an older engine reports no loss
        n_labels: outcome.get("n_labels").and_then(|n| n.as_i64()).map_or(0, |n| n as i32), // map_or: an older engine reports no label count
        t_ms: outcome.get("t_ms").and_then(|t| t.as_f64()).unwrap_or(0.0), // unwrap_or: an older engine reports no time
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a deferral read as a failure (the experience dropped) or as a step taken.
    #[test]
    fn a_learn_answer_is_a_step_a_deferral_or_a_refusal() {
        let step = learned_of(&serde_json::json!({ "ok": true, "loss": 1.25, "n_labels": 40, "t_ms": 812.0 }));
        assert_eq!(step, Ok(Learned { loss: 1.25, n_labels: 40, t_ms: 812.0 }));
        let wrapped = learned_of(&serde_json::json!({ "outcome": { "ok": true, "loss": 0.5, "n_labels": 3, "t_ms": 9.0 } }));
        assert_eq!(wrapped.map(|l| l.n_labels), Ok(3));
        let deferred = learned_of(&serde_json::json!({ "deferred": true, "retry_after_ms": 1500 }));
        assert_eq!(deferred, Err(DreamError::Deferred { retry_after_ms: 1500 }));
        let refused = learned_of(&serde_json::json!({ "ok": false, "failure": "no loaded adapter 3 to train" }));
        assert_eq!(refused, Err(DreamError::Refused("no loaded adapter 3 to train".into())));
    }
}
