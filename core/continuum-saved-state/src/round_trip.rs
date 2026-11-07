//! The one medium suite every kind runs: the round trip and its receipt, a reopen after a restart,
//! a write interrupted before its rename, a writer that read a stale generation, and every way a
//! file can disagree with the live world, each answered with its own verdict. A kind's test calls
//! [`every_outcome`] with a sample value instead of writing its own save/load/corrupt test.

use crate::{CorruptCause, Loaded, OwnedFile, SaveError, SavedState, Store};
use sha2::{Digest, Sha256};
use std::fmt::Debug;
use std::path::Path;

/// A new life for every store under `dir`: the process's copies are dropped, so the next load of
/// each key reads the disk. A test calls it after placing a file behind the store (another life's
/// write, a hand edit, a torn write), which a running process never sees until it restarts.
pub fn restart(dir: &Path) {
    crate::memory::restart(dir);
}

/// Run every outcome for kind `K` in the empty directory `dir`. `other` is a contract that differs
/// from `live` (`None` for a kind whose contract is `()`).
pub fn every_outcome<K>(dir: &Path, key: K::Key, live: K::Contract, other: Option<K::Contract>, value: K::Value)
where
    K: SavedState<Medium = OwnedFile>,
    K::Value: PartialEq + Debug + Clone,
{
    let kind = K::KIND;
    let store = Store::<K>::at(dir.join("kind"));
    assert_eq!(store.load(&key, &live), Loaded::Absent, "{kind}: nothing saved reads Absent");
    let first = match store.save(&key, &live, &value) {
        Ok(saved) => saved,
        Err(e) => unreachable!("{kind}: a save lands: {e}"),
    };
    assert_eq!(store.load(&key, &live), Loaded::Matched(value.clone()), "{kind}: a reload reads what was saved");
    let Some(path) = store.path(&key) else { unreachable!("{kind}: a rooted store has a path") };
    let on_disk = std::fs::read(&path).unwrap_or_default(); // unwrap_or_default: an unreadable file fails the digest assert below
    let digest: String = Sha256::digest(&on_disk).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!((first.generation, first.digest.as_str()), (1, digest.as_str()), "{kind}: the receipt names the generation and the bytes it placed");

    // a restart: a new store on the same files reads the value and continues the generations
    restart(dir);
    let reopened = Store::<K>::at(dir.join("kind"));
    assert_eq!(reopened.load_at_generation(&key, &live), (Loaded::Matched(value.clone()), 1), "{kind}: a reopened store reads what the last life saved");

    // a write interrupted after its payload landed and before its rename: the last committed
    // value still reads, and the next save lands over the debris
    let mut staged = match continuum_host::durable::Staging::new_in(&dir.join("kind")) {
        Ok(staged) => staged,
        Err(e) => unreachable!("{kind}: a staging file opens: {e}"),
    };
    assert!(staged.write_all(b"{\"kind\": \"half a wri").is_ok());
    std::mem::forget(staged); // the process died here: no rename, no cleanup
    restart(dir);
    assert_eq!(reopened.load(&key, &live), Loaded::Matched(value.clone()), "{kind}: an interrupted write leaves the committed value");
    assert!(matches!(reopened.save(&key, &live, &value), Ok(saved) if saved.generation == 2), "{kind}: the next save lands after an interrupted one");

    // a writer that read generation 2 loses to one that saved generation 3 meanwhile
    let (_, read) = store.load_at_generation(&key, &live);
    assert!(reopened.save(&key, &live, &value).is_ok());
    assert!(
        matches!(store.save_over(&key, &live, &value, read), Err(SaveError::StaleGeneration { read: 2, found: 3 })),
        "{kind}: a stale-generation writer is refused, never erases the newer save"
    );
    assert!(matches!(store.save_over(&key, &live, &value, 3), Ok(saved) if saved.generation == 4), "{kind}: re-read, it lands");
    let saved: serde_json::Value = match std::fs::read(&path).map(|bytes| serde_json::from_slice(&bytes)) {
        Ok(Ok(doc)) => doc,
        other => unreachable!("{kind}: the store just wrote JSON: {other:?}"),
    };
    // each file below is placed behind the store, as a hand edit or another build's life would
    // have before this one started: `restart` makes the next load read it
    let behind = |bytes: &[u8]| {
        let placed = continuum_host::durable::write_replace(&path, bytes).is_ok();
        restart(dir);
        placed
    };
    let rewrite = |field: &str, to: serde_json::Value| {
        let mut doc = saved.clone();
        doc[field] = to;
        behind(doc.to_string().as_bytes())
    };
    assert!(rewrite("format", (K::FORMAT + 1).into()));
    assert_eq!(store.load(&key, &live), Loaded::Newer { found: K::FORMAT + 1, known: K::FORMAT }, "{kind}: a newer format is never guessed at");
    assert!(rewrite("kind", "another-kind".into()));
    assert_eq!(store.load(&key, &live), Loaded::Corrupt { cause: CorruptCause::OtherKind }, "{kind}");
    assert!(behind(saved["value"].to_string().as_bytes()));
    match K::upgrade(0, saved["value"].clone()) {
        Some(_) => assert_eq!(store.load(&key, &live), Loaded::Upgraded { value: value.clone(), from: 0 }, "{kind}: a file from before the envelope is adopted, said so"),
        None => assert_eq!(store.load(&key, &live), Loaded::Older { found: 0, known: K::FORMAT }, "{kind}: a file from before the envelope is never read as matched"),
    }
    assert!(behind(b"{torn"));
    assert_eq!(store.load(&key, &live), Loaded::Corrupt { cause: CorruptCause::NotJson }, "{kind}: a torn write is Corrupt");
    if let Some(other) = other {
        assert!(store.save(&key, &other, &value).is_ok());
        assert!(matches!(store.load(&key, &live), Loaded::Diverged { .. }), "{kind}: another contract is Diverged");
    }

    // a failed save never latches the store dead: the next save, once the cause is gone, lands
    let blocked = Store::<K>::at(dir.join("blocked"));
    assert!(continuum_host::durable::write_replace(&dir.join("blocked"), b"a file where the directory goes").is_ok());
    assert!(blocked.save(&key, &live, &value).is_err(), "{kind}: a blocked directory fails the save");
    assert!(std::fs::remove_file(dir.join("blocked")).is_ok());
    assert!(blocked.save(&key, &live, &value).is_ok(), "{kind}: the next save lands once the cause is gone");
    assert_eq!(blocked.load(&key, &live), Loaded::Matched(value.clone()));

    let unrooted = Store::<K>::new(None);
    assert_eq!(unrooted.load(&key, &live), Loaded::Unrooted);
    assert!(matches!(unrooted.save(&key, &live, &value), Err(crate::SaveError::Unrooted)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rebuildable, Retention, Singleton};
    use std::time::Duration;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Engine {
        model: String,
        cache_type: String,
    }

    struct Fit;
    impl SavedState for Fit {
        const KIND: &'static str = "fit";
        const FORMAT: u32 = 1;
        const RETENTION: Retention = Retention::PerKey { bound: "one per node" };
        type Key = Singleton;
        type Contract = Engine;
        type Value = Vec<u32>;
        type Medium = OwnedFile;
    }
    impl Rebuildable for Fit {}

    fn never_valid() -> Duration {
        Duration::ZERO
    }
    struct Aged;
    impl SavedState for Aged {
        const KIND: &'static str = "aged";
        const FORMAT: u32 = 1;
        const RETENTION: Retention = Retention::Aged { bound: "one per persona", max_age: never_valid, source: "test: every value already stale" };
        type Key = uuid::Uuid;
        type Contract = ();
        type Value = Vec<u32>;
        type Medium = OwnedFile;
    }

    fn engine(cache_type: &str) -> Engine {
        Engine { model: "m".into(), cache_type: cache_type.into() }
    }

    // what this catches: any arm of the contract answering with another arm's verdict, or the
    // generic round trip itself drifting from the store, for a kind with a struct contract.
    #[test]
    fn every_outcome_holds_for_a_kind_with_a_contract() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        every_outcome::<Fit>(dir.path(), Singleton, engine("q8_0"), Some(engine("f16")), vec![1, 2, 3]);
        Ok(())
    }

    // what this catches: a contract mismatch reported without naming the field that changed, so
    // an owner cannot tell a layout change from a model change.
    #[test]
    fn a_diverged_contract_names_the_field() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = Store::<Fit>::at(dir.path().to_path_buf());
        assert!(store.save(&Singleton, &engine("f16"), &vec![7]).is_ok());
        assert_eq!(store.load(&Singleton, &engine("q8_0")), Loaded::Diverged { what: "cache_type".into() });
        assert_eq!(store.load_rebuildable(&Singleton, &engine("q8_0")), Vec::<u32>::new(), "a rebuildable kind starts empty, never on the other engine's numbers");
        assert_eq!(store.load_rebuildable(&Singleton, &engine("f16")), vec![7]);
        Ok(())
    }

    // what this catches: an aged kind's value read past its measured age, and a key's file read
    // as another key's.
    #[test]
    fn an_aged_value_is_stale_and_a_moved_file_is_another_key() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = Store::<Aged>::at(dir.path().to_path_buf());
        let (a, b) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2));
        assert!(store.save(&a, &(), &vec![1]).is_ok());
        assert!(matches!(store.load(&a, &()), Loaded::Stale { .. }));
        let (Some(from), Some(to)) = (store.path(&a), store.path(&b)) else { return Ok(()) };
        std::fs::rename(from, to)?;
        assert_eq!(store.load(&b, &()), Loaded::Corrupt { cause: CorruptCause::OtherKey });
        Ok(())
    }

    // what this catches (the M5, 2026-11-07, build 839e7a08d: one resident's training-demand file
    // read and probed 1,748 times in 23 minutes): a hot caller's every load reading the disk and
    // logging its verdict. A thousand loads of an absent key, through fresh handles as
    // `TrainingDemand::for_persona` makes them, read the disk once and probe once; a file placed
    // behind the store afterwards is not read, because nothing in this life wrote it.
    #[test]
    fn a_thousand_loads_of_an_absent_key_read_the_disk_once_and_probe_once() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let key = uuid::Uuid::from_u128(0x75ece751);
        for _ in 0..1_000 {
            assert_eq!(Store::<Aged>::at(dir.path().to_path_buf()).load(&key, &()), Loaded::Absent);
            assert_eq!(Store::<Fit>::at(dir.path().to_path_buf()).load(&Singleton, &engine("q8_0")), Loaded::Absent);
        }
        assert_eq!(Store::<Aged>::at(dir.path().to_path_buf()).reads_and_probes(&key), (1, 1), "an aged kind: one read, one probe");
        let fit = Store::<Fit>::at(dir.path().to_path_buf());
        assert_eq!(fit.reads_and_probes(&Singleton), (1, 1), "a kind with a contract: one read, one probe");
        let Some(path) = fit.path(&Singleton) else { unreachable!("a rooted store has a path") };
        std::fs::write(path, b"{ placed behind the store")?;
        assert_eq!(fit.load(&Singleton, &engine("q8_0")), Loaded::Absent, "the copy answers: the disk is not read again");
        Ok(())
    }

    // what this catches: a save that leaves the copy stale (a load after it reading the old
    // verdict, or the disk again), and a contract change hidden behind the copy: the saved value
    // comes back from memory, a load under another contract is still Diverged by field, and each
    // change of verdict is probed once.
    #[test]
    fn a_save_replaces_the_copy_and_a_contract_change_is_still_diverged() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = Store::<Fit>::at(dir.path().to_path_buf());
        assert_eq!(store.load(&Singleton, &engine("f16")), Loaded::Absent);
        assert!(matches!(store.save(&Singleton, &engine("f16"), &vec![7]), Ok(saved) if saved.generation == 1));
        for _ in 0..100 {
            assert_eq!(store.load_at_generation(&Singleton, &engine("f16")), (Loaded::Matched(vec![7]), 1));
        }
        assert_eq!(store.reads_and_probes(&Singleton), (1, 2), "read once (Absent); probed Absent, then Matched after the save");
        assert_eq!(store.load(&Singleton, &engine("q8_0")), Loaded::Diverged { what: "cache_type".into() });
        assert_eq!(store.reads_and_probes(&Singleton), (1, 3), "the contract change is judged from the copy and probed once");
        assert_eq!(store.load_rebuildable(&Singleton, &engine("q8_0")), Vec::<u32>::new());
        assert!(store.remove(&Singleton).is_ok());
        assert_eq!(store.load(&Singleton, &engine("f16")), Loaded::Absent, "a remove leaves the copy Absent");
        assert_eq!(store.reads_and_probes(&Singleton).0, 1, "still one disk read");
        Ok(())
    }

    // what this catches (the retirement that finishes once an operator repairs a torn demand
    // file, card 4545eafd): a file this build cannot trust kept in memory, so a repair made while
    // the core runs is never read and every strict reader keeps refusing until a restart. It is
    // read again on each load, and its verdict is still probed once.
    #[test]
    fn a_file_this_build_cannot_trust_is_read_again_until_it_is_repaired() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = Store::<Fit>::at(dir.path().to_path_buf());
        let Some(path) = store.path(&Singleton) else { unreachable!("a rooted store has a path") };
        std::fs::write(&path, b"{ torn")?;
        for _ in 0..3 {
            assert_eq!(store.load(&Singleton, &engine("f16")), Loaded::Corrupt { cause: CorruptCause::NotJson });
        }
        assert_eq!(store.reads_and_probes(&Singleton), (3, 1), "read each time, probed once");
        std::fs::remove_file(&path)?;
        assert_eq!(store.load(&Singleton, &engine("f16")), Loaded::Absent, "the repair is read at once");
        Ok(())
    }
}
