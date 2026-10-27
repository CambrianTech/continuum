//! Genes paged between the engine's device memory and host RAM (fork CambrianTech/llama.cpp#72):
//! the mechanism `genome::gene_residency` drives. Which genes are LOADED at all stays the serving
//! target's (`converge_genome`); this moves a loaded one between the device and host RAM without
//! touching its id, so slot adapter lists and the KV computed under it stay valid.

use super::configuration::{serving_root, PROBE_TIMEOUT};

/// One loaded gene as the engine lists it (`GET /lora-adapters`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneSeat {
    pub id: i64,
    pub path: String,
    /// `false` while paged out to host RAM.
    pub on_device: bool,
    /// Its bytes where it is now; `None` from an engine that predates residency reporting.
    pub n_bytes: Option<u64>,
}

/// What one page_out / page_in did.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneMove {
    pub id: i64,
    pub on_device: bool,
    pub n_bytes: u64,
    /// Bytes copied; 0 when it was already there.
    pub n_moved: u64,
    pub t_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GenePageError {
    /// The engine predates gene paging (`/props` lora_page_version).
    #[error("the engine has no gene paging route")]
    NoRoute,
    /// A turn in flight applies it: retry when it finishes.
    #[error("a slot is applying the gene")]
    Busy,
    #[error("the engine refused: {0}")]
    Refused(String),
    #[error("the engine did not answer: {0}")]
    Unreachable(String),
}

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// The engine's loaded genes with where each is and its bytes. An engine that predates
/// residency reporting lists its genes as on the device (where it keeps every one).
pub async fn gene_seats() -> Result<Vec<GeneSeat>, GenePageError> {
    let url = format!("{}/lora-adapters", serving_root());
    let resp = client()
        .get(&url)
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map_err(|e| GenePageError::Unreachable(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(GenePageError::Unreachable(format!("/lora-adapters status {}", resp.status())));
    }
    let body: Vec<serde_json::Value> = resp.json().await.map_err(|e| GenePageError::Unreachable(e.to_string()))?;
    Ok(body.iter().filter_map(seat_of).collect())
}

/// PURE: one catalog entry. `None` for an entry without an id or path (never a guessed one).
pub(crate) fn seat_of(entry: &serde_json::Value) -> Option<GeneSeat> {
    Some(GeneSeat {
        id: entry.get("id")?.as_i64()?,
        path: entry.get("path")?.as_str()?.to_string(),
        on_device: entry.get("resident").and_then(|r| r.as_str()).map_or(true, |r| r == "device"), // an engine without the field keeps every gene on the device
        n_bytes: entry.get("n_bytes").and_then(|b| b.as_u64()),
    })
}

/// `POST /lora-adapters/page_in` (`to_device`) or `/page_out` for gene `id`.
pub async fn page_gene(id: i64, to_device: bool) -> Result<GeneMove, GenePageError> {
    let verb = if to_device { "page_in" } else { "page_out" };
    let url = format!("{}/lora-adapters/{verb}", serving_root());
    // PROBE_TIMEOUT (4 s): a page measured 2-5 ms for 50 MB on the M5 (fork #72), three
    // orders under it; a page that takes seconds is a fault to surface, not to wait on.
    let resp = client()
        .post(&url)
        .json(&serde_json::json!({ "id": id }))
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map_err(|e| GenePageError::Unreachable(e.to_string()))?;
    match resp.status().as_u16() {
        200..=299 => {
            let body: serde_json::Value = resp.json().await.map_err(|e| GenePageError::Unreachable(e.to_string()))?;
            move_of(&body).ok_or_else(|| GenePageError::Unreachable(format!("/lora-adapters/{verb} answered {body}")))
        }
        404 | 405 => Err(GenePageError::NoRoute),
        503 => Err(GenePageError::Busy),
        status => Err(GenePageError::Refused(format!(
            "/lora-adapters/{verb} status {status}: {}",
            resp.text().await.unwrap_or_default() // unwrap_or_default: the status alone still names the refusal
        ))),
    }
}

/// PURE: a page answer.
pub(crate) fn move_of(body: &serde_json::Value) -> Option<GeneMove> {
    Some(GeneMove {
        id: body.get("id")?.as_i64()?,
        on_device: body.get("resident")?.as_str()? == "device",
        n_bytes: body.get("n_bytes")?.as_u64()?,
        n_moved: body.get("n_moved")?.as_u64()?,
        t_ms: body.get("t_ms")?.as_f64()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the fork's answer shapes (#72) read wrong, e.g. a host-resident gene
    // read as on the device (the residency owner would never page it back in), or an engine
    // older than residency reporting read as having nothing loaded.
    #[test]
    fn catalog_entries_and_page_answers_read_as_the_fork_writes_them() {
        let host = serde_json::json!({"id": 3, "path": "/t/g.gguf", "scale": 0.0, "resident": "host", "n_bytes": 50373120});
        assert_eq!(seat_of(&host), Some(GeneSeat { id: 3, path: "/t/g.gguf".into(), on_device: false, n_bytes: Some(50373120) }));
        let older = serde_json::json!({"id": 0, "path": "/t/a.gguf", "scale": 0.0});
        assert_eq!(seat_of(&older).map(|s| (s.on_device, s.n_bytes)), Some((true, None)));
        assert_eq!(seat_of(&serde_json::json!({"path": "/t/a.gguf"})), None);
        let moved = serde_json::json!({"success": true, "id": 3, "path": "/t/g.gguf", "resident": "device", "n_bytes": 9, "n_moved": 9, "t_ms": 4.6});
        assert_eq!(move_of(&moved), Some(GeneMove { id: 3, on_device: true, n_bytes: 9, n_moved: 9, t_ms: 4.6 }));
    }
}
