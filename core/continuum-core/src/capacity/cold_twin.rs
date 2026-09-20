//! Verified cold-twin detection — the "safe to drop from NVMe" primitive for the
//! storage serving-tier governor (`docs/architecture/STORAGE-SERVING-TIER-GOVERNOR.md`).
//!
//! When the NVMe hot-serving tier is over budget, the governor's `evict_at_least`
//! migrates the coldest FROZEN/DUPLICATE artifact to the Cold drive. A frozen GGUF
//! whose identical twin ALREADY exists on cold storage is a pure duplicate — dropping
//! its NVMe copy loses no data. But "identical" must be VERIFIED, never assumed from a
//! path: dropping a 662 GB model on a bad guess is the failure this guards
//! ([[no-masking-fallbacks-my-style-tell]]). The verdict here is the gate an eviction
//! pool consults before it drops anything.
//!
//! Verification tiers (cheap → strong), the caller picks the floor:
//!   * **structural** — same shard count, same names, same per-shard sizes. Catches a
//!     partial/truncated copy or a different quant. The default floor (what a human
//!     `dir` comparison does, but mechanical).
//!   * **content** (caller's job, not here) — a header-magic check or a hash. This
//!     module returns the candidate; the caller escalates if it wants stronger proof.
//!
//! Pure over its inputs (the shard lists); the fs scan that produces them is a thin
//! separate step so the match logic is unit-testable without touching disk.

use std::path::{Path, PathBuf};

/// One shard's identity for twin comparison: file name + byte length. Deliberately
/// NOT the full path — a twin lives under a different root by definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardStat {
    pub name: String,
    pub size: u64,
}

/// The shard set of one artifact (a multi-file GGUF), sorted by name for order-stable
/// comparison. Produced by [`scan_shards`]; compared by [`is_structural_twin`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactShards {
    pub dir: PathBuf,
    pub shards: Vec<ShardStat>,
}

impl ArtifactShards {
    /// Total bytes across all shards — the reclaim that dropping this copy frees.
    pub fn total_bytes(&self) -> u64 {
        self.shards.iter().map(|s| s.size).sum()
    }
}

/// STRUCTURAL twin test (the default safe-to-drop floor): the two artifacts have the
/// same shard count, and each shard matches its counterpart by NAME and SIZE. Order is
/// normalized (both sorted by name). An empty artifact never twins anything (dropping
/// on "both empty" is a bug, not a duplicate). Pure — no I/O.
pub fn is_structural_twin(nvme: &ArtifactShards, cold: &ArtifactShards) -> bool {
    if nvme.shards.is_empty() || nvme.shards.len() != cold.shards.len() {
        return false;
    }
    nvme.shards
        .iter()
        .zip(cold.shards.iter())
        .all(|(a, b)| a.name == b.name && a.size == b.size && a.size > 0)
}

/// Scan `dir` for shards with `ext` (e.g. `"gguf"`), returning name+size sorted by
/// name. Missing dir / no matches → empty shard list (never twins). Thin I/O layer
/// over the pure [`is_structural_twin`].
pub fn scan_shards(dir: &Path, ext: &str) -> ArtifactShards {
    let mut shards: Vec<ShardStat> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some(ext) {
                return None;
            }
            let name = path.file_name()?.to_str()?.to_string();
            let size = e.metadata().ok()?.len();
            Some(ShardStat { name, size })
        })
        .collect();
    shards.sort_by(|a, b| a.name.cmp(&b.name));
    ArtifactShards {
        dir: dir.to_path_buf(),
        shards,
    }
}

/// Find a VERIFIED structural twin of `nvme_artifact` under any of `cold_roots` (the
/// Cold-drive artifact roots). Returns the cold twin's dir when found — the signal that
/// the NVMe copy is a safe-to-drop duplicate. `None` = no verified twin → the governor
/// must NOT drop the NVMe copy (migrate the bytes instead, or keep it).
///
/// A cold root is scanned one level deep for a subdir whose shards structurally twin the
/// NVMe artifact (the model's own dir name may differ across drives, so match on shard
/// identity, not the dir name).
pub fn find_cold_twin(
    nvme_artifact: &ArtifactShards,
    cold_roots: &[PathBuf],
    ext: &str,
) -> Option<PathBuf> {
    if nvme_artifact.shards.is_empty() {
        return None;
    }
    for root in cold_roots {
        // the root itself might hold the shards…
        let here = scan_shards(root, ext);
        if is_structural_twin(nvme_artifact, &here) {
            return Some(root.clone());
        }
        // …or one of its immediate subdirs (Steam-library-style per-model dirs).
        if let Ok(entries) = std::fs::read_dir(root) {
            for sub in entries.flatten() {
                let p = sub.path();
                if p.is_dir() {
                    let cand = scan_shards(&p, ext);
                    if is_structural_twin(nvme_artifact, &cand) {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(shards: &[(&str, u64)]) -> ArtifactShards {
        ArtifactShards {
            dir: PathBuf::from("x"),
            shards: shards
                .iter()
                .map(|(n, s)| ShardStat {
                    name: n.to_string(),
                    size: *s,
                })
                .collect(),
        }
    }

    // what this catches: identical shard sets (name+size) verify as twins — the K3
    // case (16 shards, matching sizes) → safe to reclaim the NVMe copy.
    #[test]
    fn identical_shards_are_a_twin() {
        let a = art(&[("k3-00001.gguf", 41 * 1024), ("k3-00002.gguf", 45 * 1024)]);
        let b = art(&[("k3-00001.gguf", 41 * 1024), ("k3-00002.gguf", 45 * 1024)]);
        assert!(is_structural_twin(&a, &b));
    }

    // what this catches: a TRUNCATED/partial cold copy (one shard short) is NOT a twin
    // — never drop the NVMe copy against an incomplete backup.
    #[test]
    fn missing_shard_is_not_a_twin() {
        let a = art(&[("k3-00001.gguf", 41 * 1024), ("k3-00002.gguf", 45 * 1024)]);
        let b = art(&[("k3-00001.gguf", 41 * 1024)]);
        assert!(!is_structural_twin(&a, &b));
    }

    // what this catches: same names but a DIFFERENT size (different quant, or a
    // corrupt/partial shard) is NOT a twin — size is load-bearing.
    #[test]
    fn size_mismatch_is_not_a_twin() {
        let a = art(&[("k3-00001.gguf", 41 * 1024)]);
        let b = art(&[("k3-00001.gguf", 20 * 1024)]);
        assert!(!is_structural_twin(&a, &b));
    }

    // what this catches: an empty artifact never twins anything — dropping on
    // "both empty" would be a bug, not a verified duplicate.
    #[test]
    fn empty_never_twins() {
        assert!(!is_structural_twin(&art(&[]), &art(&[])));
        assert!(!is_structural_twin(&art(&[("a.gguf", 10)]), &art(&[])));
    }

    // what this catches: a zero-byte shard (the interrupted-write case, like the
    // container's L11=0) never counts as a matching shard.
    #[test]
    fn zero_byte_shard_is_not_a_match() {
        let a = art(&[("a.gguf", 0)]);
        let b = art(&[("a.gguf", 0)]);
        assert!(!is_structural_twin(&a, &b));
    }
}
