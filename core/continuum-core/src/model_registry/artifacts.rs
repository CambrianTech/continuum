//! Local model artifact resolution.
//!
//! The registry owns model identity and artifact hints; this module owns
//! filesystem discovery for those artifacts. Adapters must consume resolved
//! paths from here instead of guessing cache layouts privately.

use super::types::Model;
use std::fs;
use std::path::{Path, PathBuf};

pub fn resolve_model_artifacts(model: &mut Model) {
    model.gguf_local_path = resolve_gguf_for_model(model);
    if let Some(p) = model.mmproj_local_path.take() {
        model.mmproj_local_path = Some(expand_user_path(&p));
    }
    hydrate_artifact_sizes(model);
}

/// Stamp the resolved artifacts' sizes onto the row, ONCE, here — where the paths are
/// discovered. Every residency estimate downstream reads these instead of `stat`ing per
/// call, which is what keeps filesystem I/O off the governor's accounting tick.
///
/// `None` on an unreadable or unresolvable artifact, deliberately: a missing size is
/// "not known", and a consumer that turns that into `0` is the silent-zero defect this
/// exists to prevent. Callers that resolve a path by another route (`attach_local_artifact`
/// after a pull) call this too, so a row never carries a path without its size.
pub fn hydrate_artifact_sizes(model: &mut Model) {
    model.weights_bytes = resolve_gguf_for_model(model)
        .map(|p| total_gguf_bytes(&p))
        .filter(|n| *n > 0);
    // `Some(0)` when the model HAS NO PROJECTOR — that is a real, known fact, not a
    // missing measurement. `None` is reserved for "we could not resolve it", so a
    // consumer can tell "this model holds no projector bytes" apart from "nobody has
    // looked yet". Collapsing both to `None` (and then to 0 downstream) is the same
    // defect as the capacity zero, one field over.
    model.mmproj_bytes = match resolve_mmproj_for_model(model) {
        None => Some(0),
        Some(path) => fs::metadata(path).ok().map(|md| md.len()),
    };
}

/// Total on-disk weight bytes for a GGUF artifact — SUMS ALL SHARDS of a split
/// model. The 2026-08-29 overnight killer: Flash-Next's 28-shard artifact was
/// sized by shard 1 alone (0.69 GB of a 79 GB set), so the planner believed a
/// 176B MoE was a toy model, derived kv_per_token ~40x too small, sized a 46848
/// window past the verified 32k geometry, and every deep solve prefill Metal-
/// OOM'd for six silent hours. A split shard is named `-NNNNN-of-MMMMM.gguf`;
/// sum every sibling that shares the prefix. A single-file GGUF is its own size.
pub fn total_gguf_bytes(first: &std::path::Path) -> u64 {
    let name = first.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let single = || fs::metadata(first).map(|m| m.len()).unwrap_or(0);
    // "-00001-of-00028.gguf" → prefix "…-", suffix "-of-00028.gguf"
    let Some(idx) = name.rfind("-of-") else { return single() };
    if idx < 6 || !name.ends_with(".gguf") { return single(); }
    let (head, _tail) = name.split_at(idx);
    let Some(dash) = head.rfind('-') else { return single() };
    if !head[dash + 1..].chars().all(|c| c.is_ascii_digit()) { return single(); }
    let prefix = &head[..dash + 1];
    let Some(dir) = first.parent() else { return single() };
    let Ok(entries) = fs::read_dir(dir) else { return single() };
    let mut total = 0u64;
    for e in entries.flatten() {
        let f = e.file_name();
        let f = f.to_string_lossy();
        if f.starts_with(prefix) && f.ends_with(".gguf") {
            total += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    if total > 0 { total } else { single() }
}

pub fn resolve_gguf_for_model(model: &Model) -> Option<PathBuf> {
    resolve_gguf(
        &model.id,
        model.gguf_hint.as_deref(),
        model.gguf_local_path.as_deref(),
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
    scan(gguf).unwrap_or(false)
}

/// Find an MTP draft-head GGUF sitting in `dir` — an `mtp-*.gguf` sibling of the
/// model's GGUF. When several quants of the head are present, newest-mtime wins
/// (same tie-break as main-model candidate selection).
fn find_mtp_draft_beside(dir: &Path) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
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

pub fn resolve_local_model_dir_for_model_id(model_id: &str) -> Option<PathBuf> {
    resolve_from_local_model_roots(model_id).and_then(|gguf| gguf.parent().map(Path::to_path_buf))
}

pub fn find_first_local_gguf() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for dir in local_model_roots() {
        collect_ggufs_recursive(&dir, &mut candidates);
    }
    if let Some(cache) = huggingface_cache_root() {
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

fn resolve_gguf(model_id: &str, hint: Option<&str>, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        let expanded = expand_user_path(path);
        if expanded.exists() {
            return Some(expanded);
        }
    }

    if let Some(path) = resolve_from_local_model_roots(model_id) {
        return Some(path);
    }

    if let Some(hint) = hint {
        if let Some(path) = resolve_from_huggingface_hint(hint) {
            return Some(path);
        }
    }

    resolve_from_huggingface_model_id(model_id)
}

fn resolve_from_local_model_roots(model_id: &str) -> Option<PathBuf> {
    for root in local_model_roots() {
        if let Some(dir) = find_model_dir_in_root(model_id, &root) {
            if let Some(gguf) = first_gguf_in_dir(&dir) {
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
    let dir = device_fit_cache_dir(model_id);
    if !dir.is_dir() {
        return None;
    }
    let first_shard = first_gguf_in_dir(&dir)?;
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
pub fn device_fit_cache_dir(model_id: &str) -> PathBuf {
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
    storage_root().join("device-fit").join(slug)
}

fn local_model_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = home_dir_string() {
        roots.push(
            PathBuf::from(&home)
                .join(".continuum")
                .join("genome")
                .join("models"),
        );
    }
    let storage_models = storage_root().join("genome").join("models");
    if !roots.iter().any(|p| p == &storage_models) {
        roots.push(storage_models);
    }
    roots
}

fn storage_root() -> PathBuf {
    if let Ok(storage) = std::env::var("CONTINUUM_STORAGE_PATH") {
        if !storage.trim().is_empty() {
            return PathBuf::from(storage);
        }
    }
    if let Some(home) = home_dir_string() {
        let config_path = PathBuf::from(&home).join(".continuum").join("config.env");
        if let Ok(content) = fs::read_to_string(config_path) {
            for line in content.lines() {
                if let Some(value) = line.trim().strip_prefix("CONTINUUM_STORAGE_PATH=") {
                    let value = value.trim();
                    if !value.is_empty() {
                        return PathBuf::from(value);
                    }
                }
            }
        }
        return PathBuf::from(home).join(".continuum");
    }
    PathBuf::from("/tmp").join(".continuum")
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
        if !path.is_dir() || first_gguf_in_dir(&path).is_none() {
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
    let cache = huggingface_cache_root()?;
    let model_dir = find_hf_model_dir(&cache, &repo_slug)?;
    find_ggufs_under_snapshots(&model_dir)
}

fn resolve_from_huggingface_model_id(model_id: &str) -> Option<PathBuf> {
    let cache = huggingface_cache_root()?;
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

// Merge: BigMama's pub(crate) visibility (install code calls it) + M5's
// config.env HF_HOME read (the cold-storage installer writes it there).
pub(crate) fn huggingface_cache_root() -> Option<PathBuf> {
    // Process env wins (a launcher's explicit override), then config.env — the
    // ONE cross-platform source ([[config-env-single-owner]]): Windows persists
    // user env in the registry and Linux has no equivalent, so the cold-storage
    // installer writes HF_HOME to config.env and the core must honor it here
    // (BigMama 16TB routing, 2026-07-24).
    let hf_home = std::env::var("HF_HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| crate::config_env::read("HF_HOME").filter(|s| !s.trim().is_empty()));
    if let Some(hf_home) = hf_home {
        return Some(PathBuf::from(hf_home).join("hub"));
    }
    Some(
        PathBuf::from(home_dir_string()?)
            .join(".cache")
            .join("huggingface")
            .join("hub"),
    )
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

fn first_gguf_in_dir(dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if is_main_model_gguf(&p) {
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
fn is_main_model_gguf(path: &Path) -> bool {
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
#[cfg(test)]
pub(crate) fn write_empty_gguf(path: &Path) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"GGUF"); // magic
    bytes.extend_from_slice(&3u32.to_le_bytes()); // version
    bytes.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
    bytes.extend_from_slice(&0u64.to_le_bytes()); // metadata_kv_count
    std::fs::write(path, bytes).unwrap();
}

#[cfg(test)]
pub(crate) fn with_test_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let prior_home = std::env::var("HOME").ok();
    let prior_userprofile = std::env::var("USERPROFILE").ok();
    let prior_hf_home = std::env::var("HF_HOME").ok();
    // PIN, NEVER REMOVE — removing a var does not isolate a test, it just moves the
    // read one layer down onto the real machine.
    //
    // Found by BigMama 2026-08-07: three model_registry tests failed ONLY on boxes
    // with cold storage. `remove_var("HF_HOME")` looks hermetic, but
    // `huggingface_cache_root` falls back to `config_env::read("HF_HOME")` — a REAL
    // FILE (`~/.continuum/config.env`) that the cold-storage installer writes with
    // the 16TB path. So the fixture handed the test the operator's actual drive.
    // Same shape for USERPROFILE: `config_env` resolves through `dirs::home_dir()`,
    // which on Windows ignores HOME and uses USERPROFILE — removing it let the
    // lookup escape to the real profile.
    //
    // Pinning both closes it on every platform: process env WINS the resolution
    // chain, so config.env can no longer be consulted at all. HF_HOME is pinned to
    // exactly what the default branch would compute from this home
    // (`<home>/.cache/huggingface`), so behaviour is byte-identical to the intent —
    // only the ambient dependency is gone.
    std::env::set_var("HOME", home);
    std::env::set_var("USERPROFILE", home);
    std::env::set_var("HF_HOME", home.join(".cache").join("huggingface"));
    let result = f();
    if let Some(value) = prior_home {
        std::env::set_var("HOME", value);
    } else {
        std::env::remove_var("HOME");
    }
    if let Some(value) = prior_userprofile {
        std::env::set_var("USERPROFILE", value);
    } else {
        std::env::remove_var("USERPROFILE");
    }
    if let Some(value) = prior_hf_home {
        std::env::set_var("HF_HOME", value);
    } else {
        std::env::remove_var("HF_HOME");
    }
    result
}

#[cfg(test)]
mod tests {
    // what this catches: the embedded-MTP capability signal lying in either
    // direction. A false positive would push `--spec-type draft-mtp` onto a
    // model with no nextn head (llama-server may refuse or misbehave); a false
    // negative silently forfeits the speculation the head was trained for.
    // Synthetic GGUF headers, no real weights — the scan is header-only by
    // design and must stay that way (it runs at model resolution).
    #[test]
    fn embedded_mtp_detection_reads_headers_not_weights() {
        use std::io::Write;
        fn gguf_with_tensor(name: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir().join(format!("mtp-det-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let p = dir.join("model.gguf");
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(b"GGUF").unwrap();
            f.write_all(&3u32.to_le_bytes()).unwrap(); // version
            f.write_all(&1u64.to_le_bytes()).unwrap(); // n_tensors
            f.write_all(&1u64.to_le_bytes()).unwrap(); // n_kv
            // one KV: key "k" -> u32 7
            f.write_all(&1u64.to_le_bytes()).unwrap();
            f.write_all(b"k").unwrap();
            f.write_all(&4u32.to_le_bytes()).unwrap(); // type u32
            f.write_all(&7u32.to_le_bytes()).unwrap();
            // one tensor: name, ndims=1, dim, dtype, offset
            f.write_all(&(name.len() as u64).to_le_bytes()).unwrap();
            f.write_all(name.as_bytes()).unwrap();
            f.write_all(&1u32.to_le_bytes()).unwrap();
            f.write_all(&8u64.to_le_bytes()).unwrap();
            f.write_all(&0u32.to_le_bytes()).unwrap();
            f.write_all(&0u64.to_le_bytes()).unwrap();
            p
        }
        let with = gguf_with_tensor("blk.40.nextn.eh_proj.weight");
        let without = gguf_with_tensor("blk.40.attn_q.weight");
        assert!(super::gguf_has_embedded_mtp(&with), "nextn tensor must be detected");
        assert!(!super::gguf_has_embedded_mtp(&without), "plain model must not");
        assert!(
            !super::gguf_has_embedded_mtp(std::path::Path::new("/nonexistent.gguf")),
            "unreadable file is honestly false, never a panic"
        );
        for p in [with, without] {
            let _ = std::fs::remove_dir_all(p.parent().unwrap());
        }
    }


    use super::*;
    use crate::model_registry::types::{Arch, Capability};
    use std::collections::BTreeSet;

    fn model(id: &str, hint: Option<&str>, explicit: Option<PathBuf>) -> Model {
        Model {
            weights_bytes: None,
            mmproj_bytes: None,
            id: id.to_string(),
            name: None,
            provider: "llamacpp-local".into(),
            arch: Arch::Qwen35,
            context_window: 262144,
            max_output_tokens: 32768,
            tokens_per_second: 33.0,
            capabilities: BTreeSet::from([
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
            ]),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: hint.map(str::to_string),
            hf_source: None,
            gguf_local_path: explicit,
            mmproj_local_path: None,
            chat_template: None,
            multi_party_strategy: Default::default(),
            stop_sequences: Vec::new(),
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
            serving: Default::default(), // test/fixture literal: substrate defaults (text-only main lane, unverified kv-shift)
        }
    }

    // what this catches: #106 live spawn failure 2026-08-02 — pulling Qwen2.5-VL
    // wrote the mmproj AFTER the main weights, mtime-newest candidate selection
    // picked the PROJECTOR as the model, and llama-server died with "unsupported
    // model architecture: 'clip'" (serving fully down until unpin). A `*mmproj*`
    // GGUF must never be main-model candidate no matter its mtime; the
    // mmproj-POSITIVE scan (`find_mmproj_beside`) must still find it.
    #[test]
    fn mmproj_companion_never_wins_main_model_resolution() {
        let snap = tempfile::tempdir().unwrap();
        let model_dir = snap.path().join("snapshots").join("abc123");
        std::fs::create_dir_all(&model_dir).unwrap();
        let main = model_dir.join("Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf");
        write_empty_gguf(&main);
        // Written SECOND → newer mtime, the exact live download ordering.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mmproj = model_dir.join("mmproj-Qwen2.5-VL-7B-Instruct-Q8_0.gguf");
        write_empty_gguf(&mmproj);

        let picked = find_ggufs_under_snapshots(snap.path()).expect("main model resolves");
        assert_eq!(picked, main, "projector must not out-mtime the model");
        let proj = find_mmproj_beside(&model_dir).expect("projector still discoverable");
        assert_eq!(proj, mmproj);
        // A directory holding ONLY a projector resolves no main model — blind
        // beats serving CLIP as a mind.
        std::fs::remove_file(&main).unwrap();
        assert!(find_ggufs_under_snapshots(snap.path()).is_none());
    }

    // what this catches: #440 — the ggml-org Qwen3.8 snapshot ships main + `mtp-*.gguf`
    // draft head + mmproj in ONE dir, and the draft downloads AFTER the main weights
    // (live ordering 2026-08-15: main 02:48, mtp 02:49). Without the mtp exclusion,
    // mtime-newest candidate selection serves the 1.6GB DRAFT HEAD as the 27B model —
    // the exact #106 clip failure shape. The draft-POSITIVE scan
    // (`find_mtp_draft_beside`) must still find it so the spawn can pass
    // `--spec-type draft-mtp`.
    #[test]
    fn mtp_draft_head_never_wins_main_model_resolution_but_resolves_as_draft() {
        let snap = tempfile::tempdir().unwrap();
        let model_dir = snap.path().join("snapshots").join("qwen38");
        std::fs::create_dir_all(&model_dir).unwrap();
        let main = model_dir.join("Qwen3.8-27B-Q4_K_M.gguf");
        write_empty_gguf(&main);
        // Written SECOND → newer mtime, the exact live download ordering.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let draft = model_dir.join("mtp-Qwen3.8-27B-Q4_0.gguf");
        write_empty_gguf(&draft);

        let picked = find_ggufs_under_snapshots(snap.path()).expect("main model resolves");
        assert_eq!(picked, main, "draft head must not out-mtime the model");
        let found = find_mtp_draft_beside(&model_dir).expect("draft head still discoverable");
        assert_eq!(found, draft);
        // A directory with no mtp sibling resolves no draft — the spawn adds no
        // spec-decode flags and serving is byte-identical to pre-#440.
        std::fs::remove_file(&draft).unwrap();
        assert!(find_mtp_draft_beside(&model_dir).is_none());
    }

    // what this catches: tier-1 resolution — an explicitly declared projector resolves
    // (with `~` expansion + existence check) so the serving spawn passes `--mmproj` and
    // the model can SEE; a declared-but-absent projector with no GGUF-sibling either,
    // or no projector at all, resolves to None so a Vision row can't silently claim
    // sight it can't deliver (the spawn's loud-warn path). Runs under an empty test HOME
    // so tier-2's `resolve_gguf` finds nothing — the None cases stay deterministic and
    // don't leak a real on-disk projector from the dev machine's HF cache (cf. #72).
    #[test]
    fn resolves_declared_mmproj_only_when_the_projector_file_exists() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let dir = tempfile::tempdir().unwrap();
            let mmproj = dir.path().join("mmproj-f16.gguf");
            fs::write(&mmproj, b"\0").unwrap();

            let mut m = model("qwen-vl", None, None);
            m.mmproj_local_path = Some(mmproj.clone());
            assert_eq!(
                resolve_mmproj_for_model(&m).as_deref(),
                Some(mmproj.as_path())
            );

            // Declared but not on disk, and no GGUF resolves (empty HOME) → None
            // (serving warns TEXT-ONLY, never fakes sight).
            m.mmproj_local_path = Some(dir.path().join("absent-mmproj.gguf"));
            assert!(resolve_mmproj_for_model(&m).is_none());

            // No projector declared, no GGUF → None.
            m.mmproj_local_path = None;
            assert!(resolve_mmproj_for_model(&m).is_none());
        });
    }

    // what this catches: tier-2 self-provisioning — a VL model's mmproj auto-resolves
    // from the SAME HF cache snapshot as its GGUF, with NO declared-local path. The
    // projector ships beside the GGUF in `*-GGUF` repos, so pulling the model into the
    // HF cache is enough; there is no separate hand-placement step for the projector.
    // This is the resolution asymmetry fix: the mmproj now resolves like the GGUF
    // (local → HF cache), so vision serving is managed, not manually wired.
    #[test]
    fn resolves_mmproj_from_the_gguf_hf_cache_snapshot_when_not_declared() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let snap = home.path().join(
                ".cache/huggingface/hub/models--continuum-ai--qwen3-vl-7b-GGUF/snapshots/abc",
            );
            fs::create_dir_all(&snap).unwrap();
            let gguf = snap.join("qwen3-vl-7b-Q4_K_M.gguf");
            write_empty_gguf(&gguf);
            let mmproj = snap.join("mmproj-qwen3-vl-7b-f16.gguf");
            write_empty_gguf(&mmproj);

            // GGUF resolves from the HF cache via the hint; the projector is NOT declared
            // locally (declared path is stale/absent) but sits beside the GGUF.
            let mut m = model("qwen3-vl-7b", Some("continuum-ai/qwen3-vl-7b-GGUF"), None);
            m.mmproj_local_path = Some(PathBuf::from("~/nonexistent/mmproj.gguf"));
            assert_eq!(
                resolve_mmproj_for_model(&m).as_deref(),
                Some(mmproj.as_path()),
                "mmproj must resolve beside the GGUF in the HF cache snapshot"
            );

            // Tier 1 still wins when a declared projector is actually present.
            m.mmproj_local_path = Some(mmproj.clone());
            assert_eq!(
                resolve_mmproj_for_model(&m).as_deref(),
                Some(mmproj.as_path())
            );
        });
    }

    #[test]
    fn resolves_huggingface_cache_from_hint_when_explicit_path_is_stale() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let cached = home.path().join(
                ".cache/huggingface/hub/models--continuum-ai--qwen3.5-4b-code-forged-GGUF/snapshots/abc",
            );
            fs::create_dir_all(&cached).unwrap();
            let gguf = cached.join("qwen3.5-4b-code-forged-Q4_K_M.gguf");
            write_empty_gguf(&gguf);

            let resolved = resolve_gguf_for_model(&model(
                "continuum-ai/qwen3.5-4b-code-forged-GGUF",
                Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
                Some(PathBuf::from("~/missing/docker/bundle/model.gguf")),
            ));

            assert_eq!(resolved.as_deref(), Some(gguf.as_path()));
        });
    }

    #[test]
    fn explicit_existing_path_wins() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let explicit = home.path().join("models").join("model.gguf");
            fs::create_dir_all(explicit.parent().unwrap()).unwrap();
            write_empty_gguf(&explicit);
            let resolved = resolve_gguf_for_model(&model(
                "continuum-ai/qwen3.5-4b-code-forged-GGUF",
                Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
                Some(PathBuf::from("~/models/model.gguf")),
            ));
            assert_eq!(resolved.as_deref(), Some(explicit.as_path()));
        });
    }

    // what this catches: local model-dir resolution disambiguates by size WITHOUT
    // the deleted `["14b","32b","4b",...]` table or `qwen`/`compacted` family
    // strings. A `19b` request must pick the `19b` dir and reject the `32b` dir
    // purely via token-subset matching — and it must work for a NON-qwen family,
    // proving no hardcoded model knowledge leaked back in.
    #[test]
    fn find_model_dir_disambiguates_size_without_a_hardcoded_table() {
        let root = tempfile::tempdir().unwrap();
        for dir in [
            "qwen3-coder-30b-a3b-compacted-32b",
            "qwen3-coder-30b-a3b-compacted-19b",
            "llama-3-8b-instruct",
        ] {
            let d = root.path().join(dir);
            fs::create_dir_all(&d).unwrap();
            write_empty_gguf(&d.join("model-Q4_K_M.gguf"));
        }

        let resolved = find_model_dir_in_root(
            "Continuum/qwen3-coder-30b-a3b-compacted-19b-256k",
            root.path(),
        );
        assert_eq!(
            resolved.as_deref(),
            Some(
                root.path()
                    .join("qwen3-coder-30b-a3b-compacted-19b")
                    .as_path()
            ),
            "19b request must select the 19b dir, not the 32b sibling"
        );

        // Family-agnostic: a llama request resolves its own dir with zero qwen logic.
        let llama = find_model_dir_in_root("meta/llama-3-8b-instruct", root.path());
        assert_eq!(
            llama.as_deref(),
            Some(root.path().join("llama-3-8b-instruct").as_path())
        );

        // A size the request does not name has no subset dir → no false match.
        let absent = find_model_dir_in_root("Continuum/qwen3-coder-70b", root.path());
        assert_eq!(
            absent, None,
            "no 70b dir exists; must not match a 32b/19b sibling"
        );
    }

    // what this catches: the 2026-08-29 overnight killer — a 28-shard split GGUF
    // sized by shard 1 alone (0.69 of 79 GB), which poisoned kv_per_token and let
    // the planner size a window past the verified geometry (six hours of silent
    // Metal OOMs). Splits sum ALL siblings; singles stay their own size; a
    // non-split name with "-of-" noise falls back to single-file size.
    #[test]
    fn split_gguf_bytes_sum_all_shards() {
        let dir = std::env::temp_dir().join(format!("split-gguf-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let w = |name: &str, len: usize| {
            let p = dir.join(name);
            std::fs::write(&p, vec![0u8; len]).unwrap();
            p
        };
        let s1 = w("m-00001-of-00003.gguf", 100);
        w("m-00002-of-00003.gguf", 200);
        w("m-00003-of-00003.gguf", 300);
        let single = w("solo.gguf", 42);
        w("unrelated-00001-of-00002.bin", 999);
        assert_eq!(total_gguf_bytes(&s1), 600, "split must sum every shard");
        assert_eq!(total_gguf_bytes(&single), 42, "single file is its own size");
        std::fs::remove_dir_all(&dir).ok();
    }
}
