//! Where minds are preserved across a restart (CONTINUUM-ENGINE 4.16, restore is the norm). The
//! engine's stop is the root suspend: each mind leaves the stream through the pipeline, her blocks are
//! written by the tier (`<persona>.cpb`, raw rows), her token history here (`<persona>.tokens`), and
//! one manifest naming every preserved mind is published last, atomically. The start is the root
//! resume: a mind comes back only from the published manifest, and only after her model, her token
//! count and checksum, and (in the fork) her owner, epoch and memory layout all match.
//!
//! One generation per mind: a suspend replaces her files; nothing here ages them out, because a
//! mind's file is her only copy (Codex on 4.16; the `persona-blocks` class's eviction decision).

use std::io::Write;
use std::path::{Path, PathBuf};

/// One preserved mind, as the manifest names her.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Entry {
    pub persona: uuid::Uuid,
    pub epoch: u64,
    /// Her genes' signature when she was preserved; she resumes under it.
    pub basis: String,
    /// Her token history's length and checksum, verified before she resumes.
    pub tokens: u64,
    pub checksum: u64,
}

/// What a suspend published.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Manifest {
    pub version: u32,
    /// The model every preserved mind ran on: another model resumes nobody.
    pub model: String,
    pub minds: Vec<Entry>,
}

const VERSION: u32 = 1;

/// The directory of preserved minds.
pub(super) struct Store {
    dir: PathBuf,
}

impl Store {
    /// The node's store, under its continuum home.
    pub(super) fn at_home() -> Result<Self, String> {
        let dir = continuum_kernel::paths::continuum_home()?.join("persona-blocks");
        std::fs::create_dir_all(&dir).map_err(|e| format!("persona-blocks {}: {e}", dir.display()))?;
        Ok(Self { dir })
    }

    pub(super) fn blocks(&self, persona: uuid::Uuid) -> PathBuf {
        self.dir.join(format!("{persona}.cpb"))
    }

    fn tokens_path(&self, persona: uuid::Uuid) -> PathBuf {
        self.dir.join(format!("{persona}.tokens"))
    }

    fn manifest_path(&self) -> PathBuf {
        self.dir.join("manifest.json")
    }

    /// Write her token history (through a temporary and a rename) and return its checksum.
    pub(super) fn write_tokens(&self, persona: uuid::Uuid, tokens: &[i32]) -> Result<u64, String> {
        let bytes: Vec<u8> = tokens.iter().flat_map(|token| token.to_le_bytes()).collect();
        replace(&self.tokens_path(persona), &bytes)?;
        Ok(checksum(tokens))
    }

    /// Her token history, if it is exactly what her entry names.
    pub(super) fn read_tokens(&self, entry: &Entry) -> Result<Vec<i32>, String> {
        let path = self.tokens_path(entry.persona);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if bytes.len() % 4 != 0 {
            return Err(format!("{}: {} bytes is not whole token ids", path.display(), bytes.len()));
        }
        let tokens: Vec<i32> = bytes.chunks_exact(4).map(|id| i32::from_le_bytes([id[0], id[1], id[2], id[3]])).collect();
        if tokens.len() as u64 != entry.tokens || checksum(&tokens) != entry.checksum {
            return Err(format!("{}: her token history is not the one her manifest names", path.display()));
        }
        Ok(tokens)
    }

    /// Publish what this suspend preserved: the commit point of a stop.
    pub(super) fn publish(&self, model: &str, minds: Vec<Entry>) -> Result<(), String> {
        let manifest = Manifest { version: VERSION, model: model.to_owned(), minds };
        let json = serde_json::to_vec_pretty(&manifest).map_err(|e| format!("manifest: {e}"))?;
        replace(&self.manifest_path(), &json)
    }

    /// The last published manifest, if any, for `model`; a manifest of another model or version
    /// resumes nobody.
    pub(super) fn published(&self, model: &str) -> Result<Option<Manifest>, String> {
        let path = self.manifest_path();
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok((manifest.version == VERSION && manifest.model == model).then_some(manifest))
    }
}

/// Write `bytes` to `path` through `<path>.tmp` and a rename, so a crash never leaves a torn file.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    file.write_all(bytes).and_then(|()| file.sync_all()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{} -> {}: {e}", tmp.display(), path.display()))
}

/// FNV-1a over her token ids: a token history read back is the one written.
fn checksum(tokens: &[i32]) -> u64 {
    tokens.iter().flat_map(|token| token.to_le_bytes()).fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}
