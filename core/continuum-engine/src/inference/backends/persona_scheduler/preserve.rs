//! The minds an engine preserved across a restart (CONTINUUM-ENGINE 4.16, restore is the norm), as a
//! kind of the one contract for state that outlives a process (`continuum-saved-state`). The envelope
//! gives the atomic save, the generation and digest, and the typed load verdict; the contract
//! (the model) is compared field by field, so another model's minds load as `Diverged`, never as
//! hers. Each mind's blocks are written by the fork beside it (raw rows; the fork checks owner,
//! epoch, memory layout and every row length on read) until the store's `ExternalBlob` medium
//! records them by receipt.
//!
//! **One complete generation at a time** (Codex on #6159). A stop writes every mind's blocks to a
//! file named for that stop's generation (`<persona>.<generation>.cpb`), never over a file an
//! earlier manifest names. Only once every write has landed is the manifest that names them
//! published, and only then are the files it no longer names removed. A stop interrupted between a
//! block write and the manifest leaves the last manifest pairing its own token histories with its
//! own blocks. A mind whose write fails this stop keeps her entry from the last complete generation,
//! so a failed save never drops her.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use continuum_saved_state::{OwnedFile, Retention, SaveError, SavedState, Singleton, Store};

/// Why preserved minds cannot be opened or published. Each leaves the last complete generation
/// standing.
#[derive(Debug, thiserror::Error)]
pub(super) enum StoreError {
    #[error("no continuum home: {0}")]
    Home(String), // continuum_home's own refusal, which is text
    #[error("persona-blocks {}: {source}", dir.display())]
    Dir { dir: PathBuf, source: std::io::Error },
    #[error(transparent)]
    Save(#[from] SaveError),
}

/// One preserved mind: who she is, her genes, her token history (exactly what her blocks hold), and
/// the file her blocks were written to in her generation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct PreservedMind {
    pub persona: uuid::Uuid,
    pub epoch: u64,
    pub basis: String,
    pub held: Vec<i32>,
    /// Her block file's name in the store's directory, written in the generation that named it.
    pub blocks: String,
}

/// A format-1 entry: her blocks were at the fixed `<persona>.cpb`.
#[derive(serde::Deserialize)]
struct PreservedMindV1 {
    persona: uuid::Uuid,
    epoch: u64,
    basis: String,
    held: Vec<i32>,
}

/// What a preserved mind is valid against: the model she ran on.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct ServedModel {
    pub model: String,
}

/// The kind: the minds one engine preserved at its last complete stop.
pub(super) struct PreservedMinds;

impl SavedState for PreservedMinds {
    const KIND: &'static str = "preserved-minds";
    const FORMAT: u32 = 2;
    const RETENTION: Retention = Retention::PerKey { bound: "one per node: the last complete generation, replaced only once a stop's every write landed" };
    type Key = Singleton;
    type Contract = ServedModel;
    type Value = Vec<PreservedMind>;
    type Medium = OwnedFile;

    /// Format 1 named no block file: hers was the fixed `<persona>.cpb`.
    fn upgrade(found: u32, old: serde_json::Value) -> Option<Self::Value> {
        let minds: Vec<PreservedMindV1> = (found == 1).then(|| serde_json::from_value(old).ok()).flatten()?; // disk boundary: the saved-state envelope's format-1 body
        Some(minds.into_iter().map(|m| PreservedMind { blocks: format!("{}.cpb", m.persona), persona: m.persona, epoch: m.epoch, basis: m.basis, held: m.held }).collect())
    }
}

/// One stop's generation: the name its block files carry, fresh at every stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Generation(String);

impl Generation {
    pub(super) fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }

    /// Her block file's name in this generation.
    pub(super) fn file(&self, persona: uuid::Uuid) -> String {
        format!("{persona}.{}.cpb", self.0)
    }
}

/// The manifest a stop publishes: every mind written this generation, and every other mind of the
/// last complete generation, as she was. An entry leaves only when a newer write replaces it: a mind
/// whose write failed, whose resume was refused (a smaller window at boot), or who had no turn before
/// this stop keeps her last entry and her file (nothing this stop wrote has its name). Codex on #6159:
/// a refused restore followed by a stop must never delete her last saved blocks.
pub(super) fn next_manifest(last: &[PreservedMind], written: &[PreservedMind]) -> Vec<PreservedMind> {
    let replaced: HashSet<uuid::Uuid> = written.iter().map(|mind| mind.persona).collect();
    let kept = last.iter().filter(|mind| !replaced.contains(&mind.persona)).cloned();
    written.iter().cloned().chain(kept).collect()
}

/// The block files in `dir` that `manifest` does not name: removable once it is published.
fn unreferenced(dir: &Path, manifest: &[PreservedMind]) -> std::io::Result<Vec<PathBuf>> {
    let named: HashSet<&str> = manifest.iter().map(|mind| mind.blocks.as_str()).collect();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default(); // unwrap_or_default: a non-UTF-8 name is no block file of ours
        let ours = name.ends_with(".cpb") || name.ends_with(".cpb.tmp");
        if ours && !named.contains(name) {
            out.push(path);
        }
    }
    Ok(out)
}

/// Where preserved minds live: the store, the directory of their block files, and the last complete
/// generation's minds (what a mind whose write fails keeps).
pub(super) struct Preserved {
    pub store: Store<PreservedMinds>,
    dir: PathBuf,
    last: Vec<PreservedMind>,
}

impl Preserved {
    /// The node's preserved minds, under its continuum home.
    pub(super) fn at_home() -> Result<Self, StoreError> {
        let dir = continuum_kernel::paths::continuum_home().map_err(StoreError::Home)?.join("persona-blocks");
        Self::at(dir)
    }

    fn at(dir: PathBuf) -> Result<Self, StoreError> {
        if let Err(source) = std::fs::create_dir_all(&dir) {
            return Err(StoreError::Dir { dir, source });
        }
        Ok(Self { store: Store::at(dir.to_path_buf()), dir, last: Vec::new() })
    }

    /// The last complete generation, as loaded at start.
    pub(super) fn remember(&mut self, minds: &[PreservedMind]) {
        self.last = minds.to_vec();
    }

    pub(super) fn last(&self) -> &[PreservedMind] {
        &self.last
    }

    /// The path of a block file named by a manifest or a generation.
    /// Where each mind's live KV file is mapped while she runs (UNIFIED-PAGING.md slice 2), emptied at start:
    /// a file left by a run that ended without a stop holds no restore yet (her written blocks do).
    pub(super) fn live_kv(&self) -> Result<PathBuf, StoreError> {
        let dir = self.dir.join("live");
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|source| StoreError::Dir { dir: dir.clone(), source })?;
        }
        std::fs::create_dir_all(&dir).map_err(|source| StoreError::Dir { dir: dir.clone(), source })?;
        Ok(dir)
    }

    pub(super) fn path(&self, file: &str) -> PathBuf {
        self.dir.join(file)
    }

    /// Publish `manifest` (its every block file already written), then remove the block files it no
    /// longer names. A refused save publishes nothing and removes nothing: the last generation stands.
    /// Once the save returns, `manifest` IS the last complete generation, whatever cleanup does after
    /// (Codex on #6159: a failed directory listing after the commit must not leave `last` stale or
    /// report the save as lost). Returns what cleanup could not do, named, never fatal: the next
    /// publish retries it.
    pub(super) fn publish(&mut self, served: &ServedModel, manifest: &[PreservedMind]) -> Result<Vec<(PathBuf, std::io::Error)>, StoreError> {
        let manifest = manifest.to_vec();
        self.store.save(&Singleton, served, &manifest)?;
        self.last = manifest;
        let unnamed = match unreferenced(&self.dir, &self.last) {
            Ok(unnamed) => unnamed,
            Err(listing) => return Ok(vec![(self.dir.clone(), listing)]),
        };
        let mut kept = Vec::new();
        for path in unnamed {
            if let Err(e) = std::fs::remove_file(&path) {
                kept.push((path, e));
            }
        }
        Ok(kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_saved_state::Loaded;

    fn mind(n: u128, blocks: &str) -> PreservedMind {
        PreservedMind { persona: uuid::Uuid::from_u128(n), epoch: 1, basis: String::new(), held: vec![n as i32], blocks: blocks.to_owned() }
    }

    // what this catches (Codex on #6159): a stop interrupted between a block write and the manifest
    // pairing the old manifest's token histories with newer blocks. The new generation's file never
    // has the old one's name, so the published manifest still names blocks that are its own.
    #[test]
    fn an_interrupted_stop_leaves_the_last_generation_whole() {
        let dir = tempfile::tempdir().expect("test: a temp dir"); // expect: the test's own precondition
        let served = ServedModel { model: "m".into() };
        let mut preserved = Preserved::at(dir.path().to_path_buf()).expect("test: a store"); // expect: the test's own precondition
        let first = Generation::fresh();
        std::fs::write(preserved.path(&first.file(uuid::Uuid::from_u128(1))), b"generation one").expect("test: write"); // expect: the test's own precondition
        let published = vec![mind(1, &first.file(uuid::Uuid::from_u128(1)))];
        preserved.publish(&served, &published).expect("test: publish"); // expect: the test's own precondition

        // the next stop writes her blocks, then the process ends before its manifest
        let second = Generation::fresh();
        assert_ne!(first.file(uuid::Uuid::from_u128(1)), second.file(uuid::Uuid::from_u128(1)), "a new generation never overwrites a named file");
        std::fs::write(preserved.path(&second.file(uuid::Uuid::from_u128(1))), b"generation two").expect("test: write"); // expect: the test's own precondition

        let reopened = Preserved::at(dir.path().to_path_buf()).expect("test: reopen"); // expect: the test's own precondition
        let loaded = reopened.store.load(&Singleton, &served);
        assert!(matches!(loaded, Loaded::Matched(_)), "the last manifest loads: {}", loaded.describe());
        let Loaded::Matched(loaded) = loaded else { return };
        assert_eq!(loaded, published, "the last complete generation, unchanged");
        assert_eq!(std::fs::read(reopened.path(&loaded[0].blocks)).ok().as_deref(), Some(&b"generation one"[..]), "with its own blocks");
    }

    // what this catches (Codex on #6159): a resident dropped from the next manifest because this stop
    // did not write her (her write failed, her resume was refused, she had no turn). Only a newer
    // write replaces her entry.
    #[test]
    fn a_mind_not_written_this_stop_keeps_her_last_complete_entry() {
        let last = vec![mind(1, "a.old.cpb"), mind(2, "b.old.cpb"), mind(3, "c.old.cpb")];
        let next = next_manifest(&last, &[mind(1, "a.new.cpb")]);
        assert_eq!(next, vec![mind(1, "a.new.cpb"), mind(2, "b.old.cpb"), mind(3, "c.old.cpb")], "written: new; the rest: as they were");
    }

    // what this catches (Codex on #6159, the highest-risk path): a refused restore followed by a stop
    // deleting her last saved blocks. Saved at one boot, refused at the next (a smaller window), a
    // stop before her next turn, then a boot that fits her: her original history and blocks resume.
    #[test]
    fn a_refused_restore_then_a_stop_keeps_her_history_and_blocks() {
        let dir = tempfile::tempdir().expect("test: a temp dir"); // expect: the test's own precondition
        let served = ServedModel { model: "m".into() };
        let her = uuid::Uuid::from_u128(1);
        let mut saved_at = Preserved::at(dir.path().to_path_buf()).expect("test: a store"); // expect: the test's own precondition
        let file = Generation::fresh().file(her);
        std::fs::write(saved_at.path(&file), b"her rows at the large window").expect("test: write"); // expect: the test's own precondition
        let original = vec![PreservedMind { persona: her, epoch: 1, basis: String::new(), held: vec![1, 2, 3], blocks: file.clone() }];
        saved_at.publish(&served, &original).expect("test: publish"); // expect: the test's own precondition

        // the smaller boot: her manifest loads, her read is refused, so she never enters the warm
        // table; the stop before her next turn writes nobody
        let mut refused_at = Preserved::at(dir.path().to_path_buf()).expect("test: reopen"); // expect: the test's own precondition
        let loaded = refused_at.store.load(&Singleton, &served);
        assert!(matches!(loaded, Loaded::Matched(_)), "her manifest loads: {}", loaded.describe());
        let Loaded::Matched(loaded) = loaded else { return };
        refused_at.remember(&loaded);
        let manifest = next_manifest(refused_at.last(), &[]);
        let unremoved = refused_at.publish(&served, &manifest).expect("test: publish"); // expect: the test's own precondition
        assert!(unremoved.is_empty(), "{unremoved:?}");

        // the boot that fits her
        let fits = Preserved::at(dir.path().to_path_buf()).expect("test: reopen"); // expect: the test's own precondition
        let back = fits.store.load(&Singleton, &served);
        assert!(matches!(back, Loaded::Matched(_)), "her manifest loads: {}", back.describe());
        let Loaded::Matched(back) = back else { return };
        assert_eq!(back, original, "her original history and file");
        assert_eq!(std::fs::read(fits.path(&back[0].blocks)).ok().as_deref(), Some(&b"her rows at the large window"[..]), "her blocks, never deleted");
    }

    // what this catches (Codex on #6159): a committed manifest reported as lost, and `last` left
    // stale, when cleanup cannot list the directory after the save. The new generation is
    // authoritative from the commit; the listing failure comes back as cleanup.
    #[test]
    fn a_cleanup_failure_after_the_commit_leaves_the_new_generation_authoritative() {
        let dir = tempfile::tempdir().expect("test: a temp dir"); // expect: the test's own precondition
        let mut preserved = Preserved::at(dir.path().to_path_buf()).expect("test: a store"); // expect: the test's own precondition
        preserved.dir = dir.path().join("listing-fails-here");
        let manifest = vec![mind(1, "a.g2.cpb")];
        let cleanup = preserved.publish(&ServedModel { model: "m".into() }, &manifest).expect("test: the save committed"); // expect: the test's own precondition
        assert_eq!(cleanup.len(), 1, "the listing failure is reported as cleanup: {cleanup:?}");
        assert_eq!(preserved.last(), manifest.as_slice(), "last is the committed generation");
    }

    // what this catches: publishing removing a file the manifest names, or leaving one it no longer
    // names to fill the disk. After a publish only the named block files remain.
    #[test]
    fn a_publish_removes_only_what_it_no_longer_names() {
        let dir = tempfile::tempdir().expect("test: a temp dir"); // expect: the test's own precondition
        let mut preserved = Preserved::at(dir.path().to_path_buf()).expect("test: a store"); // expect: the test's own precondition
        for file in ["a.g1.cpb", "a.g2.cpb", "b.g1.cpb", "b.g2.cpb.tmp"] {
            std::fs::write(preserved.path(file), b"rows").expect("test: write"); // expect: the test's own precondition
        }
        let kept = preserved.publish(&ServedModel { model: "m".into() }, &[mind(1, "a.g2.cpb"), mind(2, "b.g1.cpb")]).expect("test: publish"); // expect: the test's own precondition
        assert!(kept.is_empty(), "{kept:?}");
        let mut left: Vec<String> = std::fs::read_dir(dir.path()).expect("test: list").filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.contains(".cpb")).collect(); // expect: the test's own precondition
        left.sort();
        assert_eq!(left, vec!["a.g2.cpb".to_owned(), "b.g1.cpb".to_owned()]);
    }

    // what this catches: the format change dropping every preserved mind at the first start after
    // it. A format-1 manifest upgrades with her blocks at the fixed name it used.
    #[test]
    fn a_format_one_manifest_upgrades_to_its_fixed_block_names() {
        let id = uuid::Uuid::from_u128(7);
        let old = serde_json::json!([{ "persona": id, "epoch": 3, "basis": "", "held": [1, 2] }]);
        let upgraded = PreservedMinds::upgrade(1, old).expect("test: format 1 upgrades"); // expect: the test's own precondition
        assert_eq!(upgraded, vec![PreservedMind { persona: id, epoch: 3, basis: String::new(), held: vec![1, 2], blocks: format!("{id}.cpb") }]);
    }
}
