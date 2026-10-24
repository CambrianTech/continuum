//! One conformance suite for every `Packable`: implement the trait, run this, inherit the proof.
//! Compiled only for tests and the `conformance` feature. Returns a typed failure naming the
//! case instead of panicking, so a caller's test reports which contract broke.

use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use crate::blob::BlobSource;
use crate::canonical::canonicalize;
use crate::error::{PackError, StoreError};
use crate::id::ContentId;
use crate::manifest::Manifest;
use crate::packable::Packable;
use crate::packer::{commit, save};
use crate::requires::Offers;
use crate::store::{read_verified, FaultMode, FaultStore, Op, RamStore, RefName, Store};
use crate::transport::{verify, TreeStats};
use crate::unpacker::Loader;

/// Two different values of the kind, what loading them needs, and recorded old manifests.
pub struct Sample<T: Packable> {
    pub a: T,
    pub b: T,
    pub offers: Offers,
    pub recorded: Vec<Recorded<T>>,
}

/// A manifest as an older build wrote it, and the value it must migrate to.
pub struct Recorded<T> {
    pub manifest: Vec<u8>,
    pub expected: T,
}

/// Which contract broke, and how.
#[derive(Debug)]
pub struct Failure {
    pub case: &'static str,
    pub detail: String,
}

impl Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "conformance case {}: {}", self.case, self.detail)
    }
}

/// What ran, and the numbers a type-specific test can hold the kind to.
#[derive(Debug, Default)]
pub struct Report {
    pub passed: Vec<&'static str>,
    pub skipped: Vec<(&'static str, &'static str)>,
    /// Store operations one commit of `b` over `a` takes (each was faulted, both modes).
    pub commit_ops: usize,
    /// The shape of `a`'s tree.
    pub tree: TreeStats,
    /// Store reads a load of `a` performs before anything is asked of the value.
    pub gets_at_load: usize,
}

trait Case<T> {
    fn case(self, case: &'static str) -> Result<T, Failure>;
}

impl<T, E: Display> Case<T> for Result<T, E> {
    fn case(self, case: &'static str) -> Result<T, Failure> {
        self.map_err(|e| Failure {
            case,
            detail: e.to_string(),
        })
    }
}

fn require(ok: bool, case: &'static str, detail: impl FnOnce() -> String) -> Result<(), Failure> {
    if ok {
        Ok(())
    } else {
        Err(Failure {
            case,
            detail: detail(),
        })
    }
}

type Fresh<'f> = &'f dyn Fn() -> Result<Arc<dyn Store>, StoreError>;

/// Run every case. `fresh` makes a new, empty store of the medium under test.
pub fn conformance<T: Packable>(
    sample: &Sample<T>,
    cx: &T::Context,
    fresh: Fresh<'_>,
) -> Result<Report, Failure> {
    let mut report = Report::default();
    let name = RefName::new("conformance", T::KIND.name).case("ref name")?;
    let a_id = save(fresh().case("store")?.as_ref(), &sample.a).case("save")?;
    let b_id = save(fresh().case("store")?.as_ref(), &sample.b).case("save")?;

    // round trip: load, re-pack in place and into another store; the id never moves.
    let s = fresh().case("store")?;
    save(s.as_ref(), &sample.a).case("round trip")?;
    let x = Loader::new(s.clone(), sample.offers.clone())
        .load::<T>(&a_id, cx)
        .case("round trip")?;
    let in_place = save(s.as_ref(), &x).case("round trip")?;
    let elsewhere = fresh().case("store")?;
    let moved = save(elsewhere.as_ref(), &x).case("round trip")?;
    require(in_place == a_id && moved == a_id, "round trip", || {
        format!("{a_id} re-packed as {in_place} / {moved}")
    })?;
    verify(elsewhere.as_ref(), &a_id).case("round trip (moved tree verifies)")?;
    report.passed.push("round trip");

    // same value, same id; different value, different id.
    require(a_id != b_id, "same value same id", || {
        format!("a and b share {a_id}")
    })?;
    report.passed.push("same value same id");

    // one root ref flip, last.
    let s = fresh().case("store")?;
    commit(s.as_ref(), &name, &sample.a, None).case("one ref flip")?;
    let f = FaultStore::new(s.clone());
    commit(&f, &name, &sample.b, Some(a_id)).case("one ref flip")?;
    let ops = f.ops().case("one ref flip")?;
    let flips = ops.iter().filter(|o| **o == Op::SetRef).count();
    require(
        flips == 1 && ops.last() == Some(&Op::SetRef),
        "one ref flip",
        || format!("{flips} flips, ops {ops:?}"),
    )?;
    report.commit_ops = ops.len();
    report.passed.push("one ref flip, last");

    // a fault at every operation leaves a loadable root: the old one, or (only when the flip
    // itself completed) the new one.
    for mode in [FaultMode::Before, FaultMode::After] {
        for at in 1..=report.commit_ops as u64 {
            let case = "fault at every op";
            let s = fresh().case(case)?;
            commit(s.as_ref(), &name, &sample.a, None).case(case)?;
            let f = FaultStore::failing_at(s.clone(), at, mode);
            require(
                commit(&f, &name, &sample.b, Some(a_id)).is_err(),
                case,
                || format!("fault {mode:?} at op {at} was not reported"),
            )?;
            let now = s.get_ref(&name).case(case)?;
            let flipped = mode == FaultMode::After && at == report.commit_ops as u64;
            let want = if flipped { b_id } else { a_id };
            require(now == Some(want), case, || {
                format!("fault {mode:?} at op {at}: ref is {now:?}, want {want}")
            })?;
            Loader::new(s.clone(), sample.offers.clone())
                .load::<T>(&want, cx)
                .case(case)?;
            verify(s.as_ref(), &want).case(case)?;
        }
    }
    report.passed.push("fault at every op");

    // a flipped byte anywhere in the tree is caught.
    let s = fresh().case("store")?;
    save(s.as_ref(), &sample.a).case("flipped byte")?;
    let tree = nodes(s.as_ref(), &a_id).case("flipped byte")?;

    // every manifest in the tree is stored as its RFC 8785 bytes and named by their sha256,
    // the id rule forge-alloy's `package` stages carry (one hash, one canonical form).
    for &(node, _) in tree.iter().filter(|(_, is_manifest)| *is_manifest) {
        let case = "id is sha256 of JCS bytes";
        let bytes = read_verified(s.as_ref(), &node).case(case)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).case(case)?;
        let jcs = canonicalize(&value).case(case)?;
        require(jcs == bytes && ContentId::of(&jcs) == node, case, || {
            format!("{node}: stored bytes are not their own JCS form")
        })?;
    }
    report.passed.push("id is sha256 of JCS bytes");

    for (node, _) in tree {
        let f: Arc<dyn Store> = Arc::new(FaultStore::tampering(s.clone(), node));
        let caught = |r: &Result<(), PackError>| matches!(r, Err(PackError::Store(StoreError::DigestMismatch { expected, .. })) if *expected == node);
        let walked = verify(f.as_ref(), &a_id).map(|_| ());
        require(caught(&walked), "flipped byte", || {
            format!("tampered {node}: verify gave {walked:?}")
        })?;
        if node == a_id {
            let loaded = Loader::new(f, sample.offers.clone())
                .load::<T>(&a_id, cx)
                .map(|_| ());
            require(caught(&loaded), "flipped byte", || {
                format!("tampered root: load gave {loaded:?}")
            })?;
        }
    }
    report.passed.push("flipped byte caught");

    // an unmet requirement refuses after reading only the root manifest.
    let root =
        Manifest::decode(&read_verified(s.as_ref(), &a_id).case("requires")?).case("requires")?;
    if root.requires.is_empty() {
        report
            .skipped
            .push(("unmet requires", "the root records no requirement"));
    } else {
        let f = Arc::new(FaultStore::new(s.clone()));
        let r = Loader::new(f.clone(), Offers::none())
            .load::<T>(&a_id, cx)
            .map(|_| ());
        let ops = f.ops().case("requires")?;
        require(
            matches!(r, Err(PackError::UnmetRequirement { .. })) && ops == [Op::Get(a_id)],
            "unmet requires",
            || format!("{r:?} after {ops:?}"),
        )?;
        report
            .passed
            .push("unmet requires refuses before any blob read");
    }

    // a newer schema is refused; every recorded older one migrates to its expected value.
    let mut newer = root.clone();
    newer.schema = T::KIND.schema + 1;
    let newer_id = s
        .put(BlobSource::Bytes(&newer.encode().case("newer schema")?))
        .case("newer schema")?
        .id;
    let r = Loader::new(s.clone(), sample.offers.clone())
        .load::<T>(&newer_id, cx)
        .map(|_| ());
    require(
        matches!(r, Err(PackError::UnknownNewerSchema { .. })),
        "newer schema",
        || format!("{r:?}"),
    )?;
    report.passed.push("newer schema refused");
    if sample.recorded.is_empty() {
        report
            .skipped
            .push(("migration", "no recorded older schema"));
    }
    for rec in &sample.recorded {
        let s = fresh().case("migration")?;
        let old = s
            .put(BlobSource::Bytes(&rec.manifest))
            .case("migration")?
            .id;
        let migrated = Loader::new(s.clone(), sample.offers.clone())
            .load::<T>(&old, cx)
            .case("migration")?;
        let (got, want) = (
            save(s.as_ref(), &migrated).case("migration")?,
            save(s.as_ref(), &rec.expected).case("migration")?,
        );
        require(got == want, "migration", || {
            format!("{old} migrated to {got}, expected {want}")
        })?;
    }
    if !sample.recorded.is_empty() {
        report.passed.push("recorded schemas migrate");
    }

    // every distinct node is stored once, however many parents reference it.
    let ram = RamStore::with_capacity(u64::MAX);
    save(&ram, &sample.a).case("stored once")?;
    report.tree = verify(&ram, &a_id).case("stored once")?;
    let held = ram.blob_count().case("stored once")?;
    require(held as u64 == report.tree.distinct, "stored once", || {
        format!("{held} blobs for {:?}", report.tree)
    })?;
    report.passed.push("shared nodes stored once");

    // what a load reads before the value is used (a lazy kind reads only its root).
    let f = Arc::new(FaultStore::new(s.clone()));
    Loader::new(f.clone(), sample.offers.clone())
        .load::<T>(&a_id, cx)
        .case("lazy load")?;
    report.gets_at_load = f
        .ops()
        .case("lazy load")?
        .iter()
        .filter(|o| matches!(o, Op::Get(_)))
        .count();
    report.passed.push("load measured");
    Ok(report)
}

/// Every distinct node id under `root`, root first, with whether it is a manifest.
fn nodes(store: &dyn Store, root: &ContentId) -> Result<Vec<(ContentId, bool)>, PackError> {
    let mut out = vec![(*root, true)];
    let mut seen = HashSet::from([*root]);
    let mut manifests = vec![*root];
    while let Some(id) = manifests.pop() {
        for (is_child, d) in Manifest::decode(&read_verified(store, &id)?)?.links() {
            if seen.insert(d.digest) {
                out.push((d.digest, is_child));
                if is_child {
                    manifests.push(d.digest);
                }
            }
        }
    }
    Ok(out)
}
