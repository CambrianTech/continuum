//! The one medium suite every kind runs: the round trip and its receipt, a reopen after a restart,
//! a write interrupted before its rename, a writer that read a stale generation, and every way a
//! file can disagree with the live world, each answered with its own verdict. A kind's test calls
//! [`every_outcome`] with a sample value instead of writing its own save/load/corrupt test.

use crate::{CorruptCause, Loaded, OwnedFile, SaveError, SavedState, Store};
use sha2::{Digest, Sha256};
use std::fmt::Debug;
use std::path::Path;

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
    let rewrite = |field: &str, to: serde_json::Value| {
        let mut doc = saved.clone();
        doc[field] = to;
        continuum_host::durable::write_replace(&path, doc.to_string().as_bytes()).is_ok()
    };
    assert!(rewrite("format", (K::FORMAT + 1).into()));
    assert_eq!(store.load(&key, &live), Loaded::Newer { found: K::FORMAT + 1, known: K::FORMAT }, "{kind}: a newer format is never guessed at");
    assert!(rewrite("kind", "another-kind".into()));
    assert_eq!(store.load(&key, &live), Loaded::Corrupt { cause: CorruptCause::OtherKind }, "{kind}");
    assert!(continuum_host::durable::write_replace(&path, saved["value"].to_string().as_bytes()).is_ok());
    match K::upgrade(0, saved["value"].clone()) {
        Some(_) => assert_eq!(store.load(&key, &live), Loaded::Upgraded { value: value.clone(), from: 0 }, "{kind}: a file from before the envelope is adopted, said so"),
        None => assert_eq!(store.load(&key, &live), Loaded::Older { found: 0, known: K::FORMAT }, "{kind}: a file from before the envelope is never read as matched"),
    }
    assert!(continuum_host::durable::write_replace(&path, b"{torn").is_ok());
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
}
