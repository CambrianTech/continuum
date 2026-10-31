//! Local model artifact resolution.
//!
//! The registry owns model identity and artifact hints; this module owns
//! filesystem discovery for those artifacts. Adapters must consume resolved
//! paths from here instead of guessing cache layouts privately.

use super::types::Model;
use std::fs;
use std::path::{Path, PathBuf};

pub mod expert_placement;

pub fn resolve_model_artifacts(model: &mut Model) {
    model.gguf_local_path = resolve_gguf_for_model(model);
    if let Some(p) = model.mmproj_local_path.take() {
        model.mmproj_local_path = Some(expand_user_path(&p));
    }
    // Hydrate artifact sizes from resolved paths
    if let Some(ref gguf) = model.gguf_local_path {
        if let Ok(meta) = fs::metadata(gguf) {
            model.weights_bytes = meta.len().try_into().ok();
        }
    }
    if let Some(ref mmproj) = model.mmproj_local_path {
        if let Ok(meta) = fs::metadata(mmproj) {
            model.mmproj_bytes = meta.len().try_into().ok();
        }
    }
}

/// Resolve a GGUF for `model`, respecting its explicit [`Model::active_quant_name`]
/// (a named file preference that pins which quant in a multi-quant snapshot directory
/// is the active one). When the field is absent, falls back to mtime-newest — the
/// historical behaviour.
pub fn resolve_gguf_for_model(model: &Model) -> Option<PathBuf> {
    resolve_gguf(
        &model.id,
        model.gguf_hint.as_deref(),
        model.gguf_local_path.as_deref(),
        model.active_quant_name.as_deref(),
    )
}

pub fn resolve_gguf_for_model_id(model_id: &str) -> Option<PathBuf> {
    if let Some(registry) = crate::model_registry::try_global() {
        if let Some(model) = registry.model(model_id) {
            return resolve_gguf_for_model(model);
        }
    }
    resolve_gguf(model_id, None, None)
}

/// Resolve a vision/audio model's multimodal projector (mmproj) GGUF for serving.
/// Two tiers, in order — an explicitly declared path first, then the SAME cache the
/// GGUF resolves from:
///
/// 1. **Declared local path** — the row's `mmproj_local_path`, with `~` expanded, when
///    it's present on disk. Operator/row placement is honored first.
/// 2. **Beside the resolved GGUF** — the projector ships in the same `*-GGUF` repo
///    snapshot as the model (bartowski-style layout: `model-Q4_K_M.gguf` +
///    `mmproj-model-f16.gguf` side by side). So we resolve the GGUF the normal way
///    (local root → HF cache) and look for a `*mmproj*.gguf` sibling in its directory.
///
/// Tier 2 is what makes vision serving self-provisioning: pulling the model pulls its
/// projector into the HF cache, and the mmproj resolves from there exactly like the
/// GGUF does — no hand-placed path, no manual step. This mirrors
/// [`resolve_gguf_for_model`]; a projector must never require a placement the GGUF
/// doesn't ([[fallbacks-are-illegal-fail-loud]]).
///
/// `None` when the row declares no projector AND none sits beside the GGUF. A VL model
/// needs this to SEE: `llama-server --mmproj <path>` loads the vision encoder so image
/// content parts are tokenized. Without it the server serves text but silently ignores
/// images — so the serving spawn treats a Vision-capable row with an unresolved
/// projector as a LOUD warning (blind, not a fabricated sight), never a silent
/// capability lie.
pub fn resolve_mmproj_for_model(model: &Model) -> Option<PathBuf> {
    // Tier 1: an explicitly declared projector that's actually on disk.
    if let Some(declared) = model.mmproj_local_path.as_deref() {
        let expanded = expand_user_path(declared);
        if expanded.exists() {
            return Some(expanded);
        }
    }
    // Tier 2: the projector sibling in the GGUF's resolved snapshot dir.
    let gguf = resolve_gguf_for_model(model)?;
    find_mmproj_beside(gguf.parent()?)
}

/// Find a multimodal projector GGUF sitting in `dir` — a `*mmproj*.gguf` sibling of
/// the model's GGUF (the layout every `*-GGUF` vision repo snapshot uses). First match
/// wins; a snapshot ships one projector.
fn find_mmproj_beside(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        (name.contains("mmproj") && name.ends_with(".gguf")).then_some(path)
    })
}

/// Resolve a model's native-MTP speculative-decode draft head for serving
/// (`llama-server --spec-type draft-mtp --spec-draft-model <this>`).
///
/// Convention (ggml-org, e.g. Qwen3.8-27B): repos whose architecture bakes in
/// multi-token-prediction heads ship the head as a sibling `mtp-<Model>-<quant>.gguf`
/// beside the main weights, so it lands in the same snapshot dir the normal GGUF
/// resolution finds. Artifact presence IS the capability signal — the exact pattern
/// [`resolve_mmproj_for_model`] established: no draft file → `None` → the spawn adds
/// no flags and serving is byte-identical to before this seam existed.
pub fn resolve_mtp_draft_for_model(model: &Model) -> Option<PathBuf> {
    let gguf = resolve_gguf_for_model(model)?;
    find_mtp_draft_beside(gguf.parent()?)
}

/// Does this GGUF carry an EMBEDDED multi-token-prediction head — `nextn`
/// tensors baked into the main weights (the DeepSeek/Ornith layout), as opposed
/// to the sibling `mtp-*.gguf` convention above?
///
/// Detection = a header-only scan of tensor NAMES (never a weight load): the
/// GGUF tensor-info section sits after the KV metadata, so this reads a few MB
/// of names at most. Verified live 2026-08-28 on Ornith-1.5-35B-Q4_K_M: 4
/// `blk.40.nextn.*` tensors, and `--spec-type draft-mtp` (no sidecar) engaged
/// with **91% draft acceptance** (draft_n=90, accepted=82) on the scratch A/B.
///
/// CAUTION, measured the same day: acceptance is a MODEL property, throughput
/// is a BACKEND property. On CPU the same 91%-acceptance run decoded SLOWER
/// than the no-spec control (30.6 vs 48.6 tok/s — batch verification costs
/// more than sequential decode for a 3B-active MoE on CPU). The Metal receipt
/// decides whether the flag ships on; presence alone is NOT the capability
/// signal for embedded MTP the way it is for the sidecar.
pub fn gguf_has_embedded_mtp(gguf: &Path) -> bool {
    fn scan(gguf: &Path) -> Option<bool> {
        use std::io::Read;
        let mut f = std::fs::File::open(gguf).ok()?;
        let mut hdr = [0u8; 24];
        f.read_exact(&mut hdr).ok()?;
        if &hdr[0..4] != b"GGUF" {
            return Some(false);
        }
        let n_tensors = u64::from_le_bytes(hdr[8..16].try_into().ok()?);
        let n_kv = u64::from_le_bytes(hdr[16..24].try_into().ok()?);
        let mut rdr = std::io::BufReader::new(f);
        fn read_u32(r: &mut impl Read) -> Option<u32> {
            let mut b = [0u8; 4];
            r.read_exact(&mut b).ok()?;
            Some(u32::from_le_bytes(b))
        }
        fn read_u64(r: &mut impl Read) -> Option<u64> {
            let mut b = [0u8; 8];
            r.read_exact(&mut b).ok()?;
            Some(u64::from_le_bytes(b))
        }
        fn read_str(r: &mut impl Read) -> Option<String> {
            let n = read_u64(r)? as usize;
            if n > 1 << 20 {
                return None; // a >1MB "string" means a corrupt header — bail
            }
            let mut b = vec![0u8; n];
            r.read_exact(&mut b).ok()?;
            Some(String::from_utf8_lossy(&b).into_owned())
        }
        fn skip_val(r: &mut impl Read, t: u32) -> Option<()> {
            match t {
                0 | 1 | 7 => std::io::copy(&mut r.take(1), &mut std::io::sink()).ok().map(|_| ()),
                2 | 3 => std::io::copy(&mut r.take(2), &mut std::io::sink()).ok().map(|_| ()),
                4 | 5 | 6 => std::io::copy(&mut r.take(4), &mut std::io::sink()).ok().map(|_| ()),
                10 | 11 | 12 => std::io::copy(&mut r.take(8), &mut std::io::sink()).ok().map(|_| ()),
                8 => read_str(r).map(|_| ()),
                9 => {
                    let et = read_u32(r)?;
                    let n = read_u64(r)?;
                    for _ in 0..n {
                        skip_val(r, et)?;
                    }
                    Some(())
                }
                _ => None,
            }
        }
        for _ in 0..n_kv {
            read_str(&mut rdr)?;
            let t = read_u32(&mut rdr)?;
            skip_val(&mut rdr, t)?;
        }
        for _ in 0..n_tensors {
            let name = read_str(&mut rdr)?;
            if name.contains(".nextn.") {
                return Some(true);
            }
            let nd = read_u32(&mut rdr)? as u64;
            std::io::copy(&mut (&mut rdr).take(8 * nd + 12), &mut std::io::sink()).ok()?;
        }
        Some(false)
    }
    match scan(gguf) {
        Some(found) => found,
        None => {
            // An unreadable or malformed header is not "no embedded head": name it.
            continuum_base::probe!(
                class = "serving.mtp.embedded_scan_failed",
                path = %gguf.display(),
                "the GGUF header could not be scanned for embedded MTP heads: serving without a draft"
            );
            false
        }
    }
}

/// Find an MTP draft-head GGUF sitting in `dir` — an `mtp-*.gguf` sibling of the
/// model's GGUF. When several quants of the head are present, newest-mtime wins
/// (same tie-break as main-model candidate selection).
fn find_mtp_draft_beside(dir: &Path) -> Option<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            // A directory we cannot list is not "no draft shipped": speculative decode is
            // lost either way, but only this one is a defect (card eaeead64).
            continuum_base::probe!(
                class = "serving.mtp.draft_lookup_failed",
                dir = %dir.display(),
                error = %error,
                "the model's directory could not be listed for an MTP draft head: serving without one"
            );
            return None;
        }
    };
    let candidates: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_ascii_lowercase();
            (name.starts_with("mtp-") && name.ends_with(".gguf")).then_some(path)
        })
        .collect();
    pick_best_candidate(candidates)
}

/// Resolve a canonical model id to the HF safetensors repo id of its
/// *trainable* form (`Model::hf_source`). The training lane (`mlx_lm.lora
/// --model`) and the forge custodian's HF→PEFT→GGUF convert both need the
/// safetensors base, NOT the serving GGUF — this is the one bridge from the
/// canonical id (which serving/eval resolve to a GGUF) to the HF cache.
///
/// Fails loud (no fallback) when the id has no registry row or the row
/// declares no `hf_source`: a missing trainable base is a real precondition
/// gap the caller must fix (add the field to the row), never silently
/// reinterpreted as "the id is already an HF repo".
pub fn resolve_hf_source_for_model_id(model_id: &str) -> Result<String, String> {
    let registry = crate::model_registry::try_global().ok_or_else(|| {
        format!("cannot resolve hf_source for '{model_id}': model registry not initialized")
    })?;
    let model = registry.model(model_id).ok_or_else(|| {
        format!(
            "cannot resolve hf_source for '{model_id}': no such model row in the registry \
             — training/convert require a canonical registry id, not an arbitrary string"
        )
    })?;
    model.hf_source.clone().ok_or_else(|| {
        format!(
            "model '{model_id}' has no hf_source — it declares a serving GGUF but no \
             trainable HF safetensors base; add `hf_source` to its registry row before \
             training or converting against it"
        )
    })
}

/// Whether a request naming `requested` as its base names the model `served_id` serves: the
/// served row's own id, or the trainable HF base that row declares (`Model::hf_source`). One
/// row is two spellings: the catalog id that serves it and the HF repo it trains from, and a
/// training request carrying the second still means the first (card e8a15ab0: Kimi's restored
/// request named `Qwen/Qwen3.8-27B`, the lane served `ggml-org/Qwen3.8-27B-GGUF`, and the
/// in-engine trainer was filtered out for a separate trainer that needed the whole weights).
/// Anchored at the SERVED row, so an HF repo two rows share never picks a row for the caller.
pub fn names_the_same_base(served_id: &str, requested: &str) -> bool {
    names_the_same_base_in(crate::model_registry::try_global(), served_id, requested)
}

fn names_the_same_base_in(registry: Option<&crate::model_registry::Registry>, served_id: &str, requested: &str) -> bool {
    served_id == requested
        || registry
            .and_then(|registry| registry.model(served_id))
            .and_then(|row| row.hf_source.as_deref())
            .is_some_and(|hf| hf == requested)
}

pub fn resolve_local_model_dir_for_model_id(model_id: &str) -> Option<PathBuf> {
    resolve_from_local_model_roots(model_id).and_then(|gguf| gguf.parent().map(Path::to_path_buf))
}

pub fn find_first_local_gguf() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for dir in local_model_roots() {
        collect_ggufs_recursive(&dir, &mut candidates);
    }
    if let Some(cache) = crate::paths::huggingface::huggingface_cache_root() {
        collect_ggufs_recursive(&cache, &mut candidates);
    }
    pick_best_candidate(candidates)
}

pub fn expand_user_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    let home = home_dir_string();
    if let Some(home) = home {
        if let Some(rest) = s.strip_prefix("~/") {
            return PathBuf::from(format!("{home}/{rest}"));
        }
        if s == "~" {
            return PathBuf::from(home);
        }
        if let Some(rest) = s.strip_prefix("$HOME/") {
            return PathBuf::from(format!("{home}/{rest}"));
        }
        if let Some(rest) = s.strip_prefix("%USERPROFILE%/") {
            return PathBuf::from(format!("{home}/{rest}"));
        }
        if let Some(rest) = s.strip_prefix("%USERPROFILE%\\") {
            return PathBuf::from(format!("{home}\\{rest}"));
        }
    }
    p.to_path_buf()
}

/// Resolve a GGUF for `model`, respecting its explicit [`Model::active_quant_name`]
/// (a named file preference that pins which quant in a multi-quant snapshot directory
/// is the active one). When the field is absent, falls back to mtime-newest — the
/// historical behaviour.
pub fn resolve_gguf(
    model_id: &str,
    hint: Option<&str>,
    explicit: Option<&Path>,
    active_quant_name: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path) = explicit {
        let expanded = expand_user_path(path);
        if expanded.exists() {
            return Some(expanded);
        }
    }

    if let Some(path) = resolve_from_local_model_roots(model_id, active_quant_name) {
        return Some(path);
    }

    if let Some(hint) = hint {
        if let Some(path) = resolve_from_huggingface_hint(hint) {
            return Some(path);
        }
    }

    resolve_from_huggingface_model_id(model_id)
}

fn resolve_from_local_model_roots(
    model_id: &str,
    active_quant_name: Option<&str>,
) -> Option<PathBuf> {
    for root in local_model_roots() {
        if let Some(dir) = find_model_dir_in_root(model_id, &root) {
            if let Some(gguf) = first_gguf_in_dir(&dir, active_quant_name) {
                return Some(gguf);
            }
        }
    }
    None
}

/// Resolve a CACHED device-fit resident-override for `(model_id, usable_bytes)`.
/// The device-fit foundry (`tools/moe-fit`) writes a precision-shrunk RESIDENT
/// (non-expert) GGUF into a per-model cache dir plus a `resident-bytes` sidecar;
/// this looks it up and returns it ONLY when its resident tier fits the caller's
/// usable VRAM. Generation / HF discovery is #35 — absent a cached artifact this
/// returns `None` and the caller falls to `Unfittable` (loud), never a hardcoded
/// path. [[device-fit-repeatable-primitive]]
pub fn resolve_device_fit_override(
    model_id: &str,
    usable_bytes: u64,
) -> Option<crate::capacity::device_fit::ResidentOverride> {
    let dir = device_fit_cache_dir(model_id)?;
    if !dir.is_dir() {
        return None;
    }
    let first_shard = first_gguf_in_dir(&dir, None)?;
    // The foundry records the resident byte total the shrink produced, so the plan
    // verifies fit without loading the GGUF. Missing sidecar = not our foundry's
    // artifact → refuse rather than guess its size ([[no-masking-fallbacks-my-style-tell]]).
    let resident_bytes: u64 = fs::read_to_string(dir.join("resident-bytes"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    (resident_bytes <= usable_bytes).then_some(crate::capacity::device_fit::ResidentOverride {
        path: first_shard,
        resident_bytes,
    })
}

/// Per-user cache dir a device-fit override for `model_id` lives in:
/// `<storage_root>/device-fit/<normalized id>/`. A convention (mirrors
/// [`local_model_roots`]), never a hardcoded operator path. Public because the
/// division actuator discovers its `*.resident.json` tier manifests here.
pub fn device_fit_cache_dir(model_id: &str) -> Option<PathBuf> {
    let slug: String = model_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    Some(storage_root()?.join("device-fit").join(slug))
}

fn local_model_roots() -> Vec<PathBuf> {
    // The home's model dir, then the configured storage tier's when it is elsewhere.
    let layouts = [
        crate::paths::ContinuumLayout::home().ok(),
        crate::paths::configured_storage_root().map(crate::paths::ContinuumLayout::at),
    ];
    let mut out: Vec<PathBuf> = Vec::new();
    for models in layouts.into_iter().flatten().map(|layout| layout.genome_models()) {
        if !out.contains(&models) {
            out.push(models);
        }
    }
    out
}

/// Where large derived data lives: the configured storage tier, else the continuum home
/// (both resolved by `paths`, the one owner). `None` with neither: there is no root to
/// write to, never a guessed `/tmp`.
fn storage_root() -> Option<PathBuf> {
    crate::paths::configured_storage_root().or_else(|| crate::paths::continuum_home().ok())
}

/// Split a model or directory name into normalized identity tokens: lowercase,
/// alphanumeric runs. Purely structural — carries NO model-family knowledge, no
/// size table, no hardcoded names. `"Qwen3-Coder-30B-A3B-compacted-19b"` →
/// `["qwen3","coder","30b","a3b","compacted","19b"]`.
fn identity_tokens(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Resolve which local directory under `root` holds the artifact for `model_id`.
///
/// Model-agnostic by construction: a directory qualifies when every token in its
/// name is also a token of the requested model id (the dir name is an
/// abbreviation of the full identity). Among qualifying dirs, the one sharing the
/// most tokens wins — the most specific match. Size disambiguation falls out for
/// free: a dir carrying a size token the request does NOT name (e.g. `32b` when
/// the request is a `19b` model) is not a subset, so it's rejected — without any
/// hardcoded `["14b","32b",...]` table or family (`qwen`/`compacted`) string.
fn find_model_dir_in_root(model_id: &str, root: &Path) -> Option<PathBuf> {
    if !root.exists() {
        return None;
    }

    let repo_name = model_id.split('/').next_back()?;
    let wanted: std::collections::HashSet<String> =
        identity_tokens(repo_name).into_iter().collect();
    if wanted.is_empty() {
        return None;
    }

    let mut best: Option<(usize, PathBuf)> = None;
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() || first_gguf_in_dir(&path, None).is_none() {
            continue;
        }
        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let have = identity_tokens(dir_name);
        // Every token of the directory name must be a token of the requested id.
        // An extra token in the dir (a different size, a different family) breaks
        // the subset and disqualifies it. Empty dir names never qualify.
        if have.is_empty() || !have.iter().all(|t| wanted.contains(t)) {
            continue;
        }
        let overlap = have.len();
        if best
            .as_ref()
            .is_none_or(|(best_overlap, _)| overlap > *best_overlap)
        {
            best = Some((overlap, path));
        }
    }
    best.map(|(_, path)| path)
}

fn resolve_from_huggingface_hint(hint: &str) -> Option<PathBuf> {
    let repo_slug = hf_repo_slug(hint)?;
    let cache = crate::paths::huggingface::huggingface_cache_root()?;
    let model_dir = find_hf_model_dir(&cache, &repo_slug)?;
    find_ggufs_under_snapshots(&model_dir)
}

fn resolve_from_huggingface_model_id(model_id: &str) -> Option<PathBuf> {
    let cache = crate::paths::huggingface::huggingface_cache_root()?;
    let wanted = model_id.to_lowercase().replace('/', "--");
    let mut candidates = Vec::new();
    for entry in fs::read_dir(cache).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with("models--") && name.contains(&wanted) {
            if let Some(gguf) = find_ggufs_under_snapshots(&entry.path()) {
                candidates.push(gguf);
            }
        }
    }
    pick_best_candidate(candidates)
}

fn hf_repo_slug(hint: &str) -> Option<String> {
    let trimmed = hint
        .strip_prefix("huggingface.co/")
        .unwrap_or(hint)
        .split(':')
        .next()?
        .trim_matches('/');
    let parts: Vec<&str> = trimmed.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    Some(format!(
        "{}--{}",
        parts[parts.len() - 2],
        parts[parts.len() - 1]
    ))
}

fn find_hf_model_dir(cache: &Path, repo_slug: &str) -> Option<PathBuf> {
    let wanted = format!("models--{}", repo_slug).to_lowercase();
    for entry in fs::read_dir(cache).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name == wanted {
            return Some(entry.path());
        }
    }
    None
}

fn find_ggufs_under_snapshots(model_dir: &Path) -> Option<PathBuf> {
    let snapshots = model_dir.join("snapshots");
    let mut candidates = Vec::new();
    for snap in fs::read_dir(snapshots).ok()?.flatten() {
        let Ok(files) = fs::read_dir(snap.path()) else {
            continue;
        };
        for file in files.flatten() {
            let p = file.path();
            if is_main_model_gguf(&p) {
                candidates.push(p);
            }
        }
    }
    pick_best_candidate(candidates)
}

fn collect_ggufs_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_ggufs_recursive(&p, out);
        } else if is_main_model_gguf(&p) {
            out.push(p);
        }
    }
}

fn first_gguf_in_dir(dir: &Path, active_quant_name: Option<&str>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if is_main_model_gguf(&p) {
            // Filter by active quant name when specified (multi-quant snapshot dirs)
            if let Some(qn) = active_quant_name {
                let fname = p.file_name().and_then(|n| n.to_string_lossy()).unwrap_or_default();
                if !fname.contains(qn) {
                    continue;
                }
            }
            candidates.push(p);
        }
    }
    pick_best_candidate(candidates)
}

fn pick_best_candidate(mut candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.sort_by(|a, b| {
        let ma = fs::metadata(a).and_then(|m| m.modified()).ok();
        let mb = fs::metadata(b).and_then(|m| m.modified()).ok();
        mb.cmp(&ma).then_with(|| a.cmp(b))
    });
    candidates.into_iter().next()
}

fn is_gguf(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
}

/// A GGUF eligible to be THE model — excludes multimodal-projector
/// companions (`*mmproj*.gguf`), which are CLIP-architecture sidecars that
/// llama-server loads via `--mmproj`, never `-m`. Glass-boxed 2026-08-02
/// (#106): pulling Qwen2.5-VL wrote the mmproj AFTER the main weights, so
/// mtime-newest candidate selection picked the projector as the model and
/// every VL spawn died with "unsupported model architecture: 'clip'".
/// Also excludes `mtp-*.gguf` MTP draft heads (the ggml-org Qwen3.8 layout):
/// same failure shape — the draft downloads AFTER the main weights, so
/// mtime-newest would serve the 1.6GB head as the 27B model. Draft heads load
/// via `--spec-draft-model`, never `-m`.
/// ONE predicate for every main-model collector; [`find_mmproj_beside`] and
/// [`find_mtp_draft_beside`] remain the sidecar-POSITIVE scans.
pub(crate) fn is_main_model_gguf(path: &Path) -> bool {
    if !is_gguf(path) {
        return false;
    }
    path.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            !name.contains("mmproj") && !name.starts_with("mtp-")
        })
}

fn home_dir_string() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
}

/// Write a minimal but STRUCTURALLY VALID empty GGUF (magic + v3 header +
/// zero tensors + zero metadata) at `path`. The canonical stand-in for "a
/// model is present here" in resolution tests: the registry hydrates every
/// resolved GGUF's header at load (param count, arch, context, sensory caps),
/// so a fixture standing in for a present model MUST be a parseable GGUF —
/// `b"gguf"` is a lie that fails loud the moment hydration reads it (and,
/// because `HOME` is a process-global the resolver reads, that lie leaks into
/// any concurrently-running catalog-load test). One writer, one truth.
#[cfg(any(test, feature = "test-fixtures"))]
pub fn write_empty_gguf(path: &Path) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"GGUF"); // magic
    bytes.extend_from_slice(&3u32.to_le_bytes()); // version
    bytes.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
    bytes.extend_from_slice(&0u64.to_le_bytes()); // metadata_kv_count
    std::fs::write(path, bytes).unwrap();
}

#[cfg(test)]
pub(crate) fn with_test_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    // ONE lock for the whole crate (`crate::paths::test_home`, #4082): the private ENV_LOCK
    // that used to live here isolated model_registry tests from each other and from
    // nobody else — a citizen-layer clone under another test's home was torn down
    // mid-write. The pin-never-remove reasoning (BigMama, 2026-08-07: HF_HOME falls
    // back to config.env; USERPROFILE on Windows) lives in paths::test_home now.
    crate::paths::test_home::with_test_home(home, f)
}

#[cfg(test)]
mod tests;
