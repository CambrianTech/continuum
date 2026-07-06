//! Owning the model catalog — since unsloth is gone, WE decide which GGUF a repo offers
//! and which quantization fits THIS machine. This module is the pure selection heart:
//! given the files a repo publishes + a VRAM budget, pick the highest-fidelity quant
//! that fits, or fail loud that the machine can't host the model. The HF API query that
//! lists a repo's files feeds these candidates (next slice); keeping the *policy* pure
//! makes the hardware-fit decision testable without a network.
//!
//! This is the convergence point with the lane governor: "which quant to fetch" IS
//! "which base model to serve" ([[model-fit-is-the-priority-single-machine-first]],
//! [[governor-settles-on-measured-throughput]]) — one decision, made from measured VRAM,
//! never a hardcoded `Q4_K_M`.

/// One GGUF file a repo publishes — the filename (which encodes the quant) and the size
/// that determines whether it fits in VRAM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufCandidate {
    pub filename: String,
    pub size_bytes: u64,
}

impl GgufCandidate {
    pub fn new(filename: impl Into<String>, size_bytes: u64) -> Self {
        Self { filename: filename.into(), size_bytes }
    }

    /// The quant label parsed from the filename, e.g. "Q4_K_M" (None if unparseable).
    pub fn quant(&self) -> Option<String> {
        parse_quant(&self.filename)
    }

    /// True for an actual quantization (Q… / IQ…), false for raw float weights
    /// (F16/F32/BF16). Float GGUFs are ~2× a near-lossless Q8 for inference — on a
    /// shared misfit pool that's memory stolen from other personas + KV + render, so
    /// they're a last resort, not the preferred pick.
    pub fn is_quantized(&self) -> bool {
        matches!(self.quant().as_deref(), Some(q) if q.starts_with('Q') || q.starts_with("IQ"))
    }

    /// Auxiliary GGUFs that are NOT the main weights: the vision projector (`mmproj-…`)
    /// and multi-part splits (`…-00002-of-00003.gguf`) — the main-weight selection must
    /// skip these, they're fetched alongside the chosen quant, not instead of it.
    pub fn is_auxiliary(&self) -> bool {
        let lower = self.filename.to_lowercase();
        lower.contains("mmproj") || lower.contains("-of-")
    }
}

/// Parse the quant label out of a GGUF filename: the last hyphen/dot-delimited token
/// that looks like a quant tag (`Q4_K_M`, `IQ3_XXS`, `F16`, `BF16`, `Q8_0`). Returns the
/// tag verbatim (upper-cased), or None if the name carries no recognizable quant.
pub fn parse_quant(filename: &str) -> Option<String> {
    let stem = filename.strip_suffix(".gguf").unwrap_or(filename);
    // Tokens are separated by '-' or '.'; scan right-to-left so `…-Instruct-Q4_K_M`
    // returns the quant, not an earlier token.
    for token in stem.rsplit(['-', '.']) {
        if looks_like_quant(token) {
            return Some(token.to_uppercase());
        }
    }
    None
}

fn looks_like_quant(token: &str) -> bool {
    let up = token.to_uppercase();
    // Float weights.
    if matches!(up.as_str(), "F16" | "F32" | "BF16") {
        return true;
    }
    // Integer quants: Q… or IQ… followed by a digit (Q4_K_M, IQ3_XXS, Q8_0).
    let rest = up.strip_prefix("IQ").or_else(|| up.strip_prefix('Q'));
    matches!(rest, Some(r) if r.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

/// Pick the highest-fidelity main-weight GGUF that FITS `vram_budget_bytes`: among the
/// non-auxiliary candidates whose size ≤ budget, the LARGEST (bigger quant ≈ bigger file
/// ≈ higher fidelity). Returns None when NONE fit — a hard truth about this machine, to
/// be surfaced (fail loud), never silently downgraded past what exists or oversized past
/// what fits.
pub fn select_best_fit(candidates: &[GgufCandidate], vram_budget_bytes: u64) -> Option<&GgufCandidate> {
    select_for_demand(candidates, vram_budget_bytes, QualityTarget::Balanced)
}

/// Pick the main-weight GGUF for a demand level. `Balanced` prefers the largest QUANTIZED
/// tier that fits (near-lossless, leaves the pool for others), falling to float only if
/// no quant fits. `Maximum` takes the single largest that fits, raw float included — the
/// gas pedal. None when nothing fits (fail loud).
pub fn select_for_demand(
    candidates: &[GgufCandidate],
    vram_budget_bytes: u64,
    target: QualityTarget,
) -> Option<&GgufCandidate> {
    // Largest that fits, deterministic tie-break by name — over a given candidate set.
    let largest = |quantized_only: bool| {
        candidates
            .iter()
            .filter(|c| !c.is_auxiliary() && c.size_bytes <= vram_budget_bytes)
            .filter(|c| !quantized_only || c.is_quantized())
            .max_by(|a, b| {
                a.size_bytes
                    .cmp(&b.size_bytes)
                    .then_with(|| b.filename.cmp(&a.filename))
            })
    };
    match target {
        // Share the box: near-lossless quantized, float only if no quant fits.
        QualityTarget::Balanced => largest(true).or_else(|| largest(false)),
        // Floor it: the biggest thing that fits, F16 included.
        QualityTarget::Maximum => largest(false),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("hf api error for {repo}: {source}")]
    Http {
        repo: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("no quant of {repo} fits {} MiB of VRAM — this machine can't host it", budget >> 20)]
    NoneFit { repo: String, budget: u64 },
}

/// The resolved decision of what to download for a model repo on this machine: the exact
/// file URL, its name, size, and quant. Produced cheaply (one API call, no download) so
/// the hardware-fit choice is inspectable before committing to a multi-GB fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFetchPlan {
    pub url: String,
    pub filename: String,
    pub size_bytes: u64,
    pub quant: Option<String>,
}

/// Normalize a `gguf_hint` to a bare `org/name` HF repo id (strip scheme + host + slashes).
pub fn normalize_repo(hint: &str) -> String {
    hint.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("huggingface.co/")
        .trim_matches('/')
        .to_string()
}

/// The direct download URL for one file in a repo (main branch).
pub fn resolve_file_url(repo: &str, filename: &str) -> String {
    format!(
        "https://huggingface.co/{}/resolve/main/{filename}",
        normalize_repo(repo)
    )
}

#[derive(serde::Deserialize)]
struct HfTreeEntry {
    #[serde(rename = "type")]
    entry_type: String,
    path: String,
    #[serde(default)]
    size: u64,
}

/// Query the HF tree API for a repo's GGUF files + their real sizes — WE are the catalog
/// now, so this is how we learn what quants exist to choose among. Returns every `.gguf`
/// (main weights + auxiliaries; `select_best_fit` filters auxiliaries at pick time).
pub async fn list_repo_ggufs(
    client: &reqwest::Client,
    repo: &str,
) -> Result<Vec<GgufCandidate>, CatalogError> {
    let repo = normalize_repo(repo);
    let url = format!("https://huggingface.co/api/models/{repo}/tree/main?recursive=true");
    let entries: Vec<HfTreeEntry> = client
        .get(&url)
        .header("user-agent", "continuum-provisioner")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|source| CatalogError::Http { repo: repo.clone(), source })?
        .json()
        .await
        .map_err(|source| CatalogError::Http { repo: repo.clone(), source })?;
    Ok(entries
        .into_iter()
        .filter(|e| e.entry_type == "file" && e.path.to_lowercase().ends_with(".gguf"))
        .map(|e| GgufCandidate::new(e.path, e.size))
        .collect())
}

/// How hard to push THIS request — the gas pedal. Selection is dynamic, not a fixed
/// conservative policy ([[misfit-grid-is-a-distributed-moe]], "dynamic base model up or
/// down").
/// - `Balanced` (default): the machine is shared — many personas, a live call. Leave
///   headroom for the others + render; prefer near-lossless quantized weights.
/// - `Maximum`: floor it. The teacher that trains the others, a substantial iOS build,
///   "I need the BEST coder" — hand this one request nearly the whole machine and the
///   highest fidelity that fits, raw F16 included. If we can run it, we should.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QualityTarget {
    #[default]
    Balanced,
    Maximum,
}

/// The weights budget for a demand level: `Balanced` leaves headroom for other personas
/// + render; `Maximum` hands this request nearly the whole machine (a small OS reserve).
pub fn budget_for_demand(total_bytes: u64, target: QualityTarget) -> u64 {
    match target {
        QualityTarget::Balanced => model_budget_from_total(total_bytes),
        // Press the gas: the whole box minus a thin OS reserve. One demanding ask can
        // take everything, because that's the point of being able to run it at all.
        QualityTarget::Maximum => total_bytes.saturating_sub(2 * (1 << 30)),
    }
}

/// The model-weight budget (bytes) derivable from a machine's total memory — the pure
/// policy, so the caller passes `SystemResourceMonitor::memory().total` (the ONE resource
/// authority — never a parallel probe) and gets a conservative weights budget. Reserves
/// headroom for the OS + Bevy render + LiveKit, then leaves a share for the KV cache /
/// activations. On Mac unified memory this IS the GPU pool; the governor's measured VRAM
/// refines it on discrete GPUs. Conservative on purpose — a quant that fits beats an OOM.
pub fn model_budget_from_total(total_bytes: u64) -> u64 {
    const OS_RENDER_RESERVE: u64 = 4 * (1 << 30); // OS + Bevy render + LiveKit encode
    let usable = total_bytes.saturating_sub(OS_RENDER_RESERVE);
    (usable as f64 * 0.70) as u64 // the rest for KV cache + activations at runtime
}

/// Resolve WHAT to download for `repo` on a machine with `total_memory_bytes` at demand
/// `target`: derive the budget (shared vs floored), query the repo, pick the best quant
/// that fits, and return the download plan. Fails loud (`NoneFit`) when nothing fits —
/// this machine can't host the model at this demand, don't pretend otherwise. Cheap (one
/// API call, no download); the fetch is the proven Downloader.
pub async fn plan_model_fetch(
    client: &reqwest::Client,
    repo: &str,
    total_memory_bytes: u64,
    target: QualityTarget,
) -> Result<ModelFetchPlan, CatalogError> {
    let budget = budget_for_demand(total_memory_bytes, target);
    let ggufs = list_repo_ggufs(client, repo).await?;
    let pick = select_for_demand(&ggufs, budget, target).ok_or_else(|| CatalogError::NoneFit {
        repo: normalize_repo(repo),
        budget,
    })?;
    Ok(ModelFetchPlan {
        url: resolve_file_url(repo, &pick.filename),
        filename: pick.filename.clone(),
        size_bytes: pick.size_bytes,
        quant: pick.quant(),
    })
}

/// A capability served by a LADDER of model sizes (coder 7B → 14B → 32B). The gas pedal
/// climbs it: `Balanced` serves the everyday default size; `Maximum` serves the LARGEST
/// model whose best quant fits this machine — a bigger brain, not just a bigger quant of
/// the same one. This is "dynamic base model up or down" made concrete: the teacher that
/// trains the others reaches for the 32B when the box can hold it.
#[derive(Debug, Clone)]
pub struct ModelFamily {
    pub name: &'static str,
    /// Repos ordered SMALLEST → largest capability.
    pub ladder: &'static [&'static str],
    /// Index into `ladder` of the shared-default (Balanced) size.
    pub default_idx: usize,
}

impl ModelFamily {
    /// The Qwen2.5-Coder ladder — the everyday coder + the teacher.
    pub fn coder() -> Self {
        Self {
            name: "qwen2.5-coder",
            ladder: &[
                "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF",
                "bartowski/Qwen2.5-Coder-14B-Instruct-GGUF",
                "bartowski/Qwen2.5-Coder-32B-Instruct-GGUF",
            ],
            default_idx: 1, // 14B is the everyday size
        }
    }
}

/// Plan the fetch for a family at a demand. `Balanced` serves the default size (share the
/// box). `Maximum` climbs the ladder TOP-DOWN and returns the largest model whose best
/// quant fits — pressing the gas escalates the brain, not just the precision. Fails loud
/// only if nothing on the ladder fits at all.
pub async fn plan_family_fetch(
    client: &reqwest::Client,
    family: &ModelFamily,
    total_memory_bytes: u64,
    target: QualityTarget,
) -> Result<ModelFetchPlan, CatalogError> {
    match target {
        QualityTarget::Balanced => {
            plan_model_fetch(client, family.ladder[family.default_idx], total_memory_bytes, target).await
        }
        QualityTarget::Maximum => {
            let mut last_err = None;
            for repo in family.ladder.iter().rev() {
                match plan_model_fetch(client, repo, total_memory_bytes, target).await {
                    Ok(plan) => return Ok(plan),
                    Err(e) => last_err = Some(e),
                }
            }
            Err(last_err.unwrap_or(CatalogError::NoneFit {
                repo: family.name.to_string(),
                budget: budget_for_demand(total_memory_bytes, target),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: quant parsing pulls the tag from real GGUF names (right-most
    // token wins over model-name noise) and rejects names with no quant.
    #[test]
    fn parse_quant_reads_the_tag() {
        assert_eq!(
            parse_quant("Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf").as_deref(),
            Some("Q4_K_M")
        );
        assert_eq!(parse_quant("model-IQ3_XXS.gguf").as_deref(), Some("IQ3_XXS"));
        assert_eq!(parse_quant("weights.F16.gguf").as_deref(), Some("F16"));
        assert_eq!(parse_quant("some-random-model.gguf"), None);
    }

    // what this catches: THE model-fit decision — pick the largest quant that fits VRAM,
    // return None (fail loud) when the smallest still overflows, and never pick an
    // auxiliary (mmproj / split part) as the main weights.
    #[test]
    fn select_best_fit_is_largest_that_fits_or_none() {
        let files = vec![
            GgufCandidate::new("m-Q3_K_M.gguf", 6_000),
            GgufCandidate::new("m-Q4_K_M.gguf", 8_000),
            GgufCandidate::new("m-Q8_0.gguf", 15_000),
            GgufCandidate::new("m-f16.gguf", 30_000),     // raw float — last resort
            GgufCandidate::new("mmproj-f16.gguf", 1_000), // auxiliary — never the main pick
        ];
        // 10k budget → Q4 (8k) is the largest quant that fits.
        assert_eq!(select_best_fit(&files, 10_000).unwrap().filename, "m-Q4_K_M.gguf");
        // 40k budget → Q8 (15k), NOT the larger F16 (30k): prefer quantized, don't burn
        // the pool on raw float weights.
        assert_eq!(select_best_fit(&files, 40_000).unwrap().filename, "m-Q8_0.gguf");
        // 5k budget → nothing fits (Q3 is 6k). Fail loud, don't grab the 1k mmproj.
        assert!(select_best_fit(&files, 5_000).is_none());

        // F16-only repo: float is the last resort, used when no quant exists.
        let float_only = vec![GgufCandidate::new("m-f16.gguf", 10_000)];
        assert_eq!(select_best_fit(&float_only, 20_000).unwrap().filename, "m-f16.gguf");
    }

    // what this catches: the gguf_hint → repo → file-URL derivation (host/scheme stripped,
    // resolve/main path correct) — the wiring the fetch executor uses to turn a chosen
    // quant into a download.
    #[test]
    fn repo_and_url_derivation() {
        assert_eq!(normalize_repo("https://huggingface.co/bartowski/Foo-GGUF"), "bartowski/Foo-GGUF");
        assert_eq!(normalize_repo("bartowski/Foo-GGUF/"), "bartowski/Foo-GGUF");
        assert_eq!(
            resolve_file_url("huggingface.co/bartowski/Foo-GGUF", "Foo-Q4_K_M.gguf"),
            "https://huggingface.co/bartowski/Foo-GGUF/resolve/main/Foo-Q4_K_M.gguf"
        );
    }

    // what this catches: LIVE — the real HF tree API yields this repo's GGUF quants with
    // real sizes, and the model-fit pick for a 16 GB budget actually fits. Network-gated;
    // run: `cargo test -p continuum-core -- --ignored list_repo_ggufs_live`.
    #[tokio::test]
    #[ignore]
    async fn list_repo_ggufs_live_reads_real_quants() {
        let client = reqwest::Client::new();
        let ggufs = list_repo_ggufs(&client, "bartowski/Qwen2.5-Coder-14B-Instruct-GGUF")
            .await
            .expect("real HF query");
        assert!(ggufs.len() > 3, "repo publishes multiple quants");
        assert!(ggufs.iter().all(|g| g.size_bytes > 0), "each gguf has a real size");
        let budget = 16u64 * (1 << 30); // 16 GiB VRAM
        let pick = select_best_fit(&ggufs, budget).expect("something fits 16 GiB");
        assert!(pick.size_bytes <= budget);
        println!(
            "16 GiB VRAM → {} ({} MiB, quant {:?}) out of {} quants",
            pick.filename,
            pick.size_bytes >> 20,
            pick.quant(),
            ggufs.len()
        );
    }

    // what this catches: LIVE — the full resolution (query → select → URL) yields a real
    // downloadable plan for a fitting budget, and fails LOUD (NoneFit) when the budget is
    // too small for any quant. Run: `cargo test -p continuum-core -- --ignored plan_model_fetch_live`.
    #[tokio::test]
    #[ignore]
    async fn plan_model_fetch_live_resolves_and_fails_loud() {
        let client = reqwest::Client::new();
        let repo = "bartowski/Qwen2.5-Coder-14B-Instruct-GGUF";
        // Fits: a real plan with a resolve/main URL for the chosen quant.
        let plan = plan_model_fetch(&client, repo, 32 * (1 << 30), QualityTarget::Balanced)
            .await
            .unwrap();
        assert!(plan.url.contains("/resolve/main/"), "downloadable URL");
        assert!(plan.url.ends_with(&plan.filename));
        println!("plan: {} ({} MiB)", plan.url, plan.size_bytes >> 20);
        // Doesn't fit: a 2 GiB machine → budget 0 → fail loud, not a silent tiny pick.
        let err = plan_model_fetch(&client, repo, 2 * (1 << 30), QualityTarget::Balanced)
            .await
            .unwrap_err();
        assert!(matches!(err, CatalogError::NoneFit { .. }));
    }

    // what this catches: the budget policy reserves headroom + scales with the machine —
    // a big box gets a big budget, an 8 GB toy gets a small one, and a machine at/below
    // the reserve gets 0 (fetch nothing local, lean remote), never a negative underflow.
    #[test]
    fn model_budget_reserves_and_scales() {
        assert_eq!(model_budget_from_total(96 * (1 << 30)), (92 * (1 << 30)) * 7 / 10);
        // 8 GiB toy: (8-4)*0.7 = 2.8 GiB — small, but a real budget.
        assert!(model_budget_from_total(8 * (1 << 30)) < 3 * (1 << 30));
        assert!(model_budget_from_total(8 * (1 << 30)) > 2 * (1 << 30));
        // At/under the reserve → 0, not underflow.
        assert_eq!(model_budget_from_total(2 * (1 << 30)), 0);
    }

    // what this catches: the gas pedal — with the same candidates + budget, Balanced
    // shares the box (near-lossless Q8) while Maximum floors it (raw F16); and Maximum's
    // budget hands over nearly the whole machine vs Balanced's reserved share.
    #[test]
    fn maximum_floors_it_balanced_shares() {
        let files = vec![
            GgufCandidate::new("m-Q8_0.gguf", 15_000),
            GgufCandidate::new("m-f16.gguf", 30_000),
        ];
        assert_eq!(
            select_for_demand(&files, 40_000, QualityTarget::Balanced).unwrap().filename,
            "m-Q8_0.gguf"
        );
        assert_eq!(
            select_for_demand(&files, 40_000, QualityTarget::Maximum).unwrap().filename,
            "m-f16.gguf"
        );
        let total = 64u64 * (1 << 30);
        assert!(
            budget_for_demand(total, QualityTarget::Maximum)
                > budget_for_demand(total, QualityTarget::Balanced)
        );
    }

    // what this catches: LIVE misfit-hardware proof — THIS machine's real memory → budget
    // → the quant of coder-14b it would actually fetch. The whole point: the same code
    // picks Q8 on a big box and a small quant on a toy. Run:
    // `cargo test -p continuum-core -- --ignored this_machine_model_fit`.
    #[tokio::test]
    #[ignore]
    async fn this_machine_model_fit_for_coder_14b() {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total = sys.total_memory();
        let client = reqwest::Client::new();
        let repo = "bartowski/Qwen2.5-Coder-14B-Instruct-GGUF";
        println!("this machine: total {} MiB", total >> 20);
        for target in [QualityTarget::Balanced, QualityTarget::Maximum] {
            let budget = budget_for_demand(total, target);
            match plan_model_fetch(&client, repo, total, target).await {
                Ok(p) => println!(
                    "  {:?} (budget {} MiB) → {} ({} MiB, {:?})",
                    target,
                    budget >> 20,
                    p.filename,
                    p.size_bytes >> 20,
                    p.quant
                ),
                Err(e) => println!("  {target:?} → {e}"),
            }
        }
    }

    // what this catches: LIVE — the gas pedal climbs the SIZE ladder, not just the quant.
    // Balanced serves the everyday 14B; Maximum reaches for the biggest coder this machine
    // can hold (the teacher's brain). Run: `-- --ignored this_machine_climbs_the_coder_ladder`.
    #[tokio::test]
    #[ignore]
    async fn this_machine_climbs_the_coder_ladder() {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total = sys.total_memory();
        let client = reqwest::Client::new();
        let fam = ModelFamily::coder();
        println!("this machine: total {} MiB — coder family {:?}", total >> 20, fam.ladder);
        for target in [QualityTarget::Balanced, QualityTarget::Maximum] {
            match plan_family_fetch(&client, &fam, total, target).await {
                Ok(p) => println!("  {:?} → {} ({} MiB, {:?})", target, p.filename, p.size_bytes >> 20, p.quant),
                Err(e) => println!("  {target:?} → {e}"),
            }
        }
    }
}
