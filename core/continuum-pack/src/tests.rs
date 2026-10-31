//! The contract, proven on two outlier kinds: a tiny manifest-only struct with a recorded old
//! schema, and a three-level tree with multi-MB blobs, a shared child and a hand-written leaf.
//! Both run the generic conformance suite on RAM and on disk; the rest pins what is specific
//! to each shape.
#![cfg(test)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::conformance::{conformance, Recorded, Report, Sample};
use crate::*;

/// A manifest-only kind on schema 2; schema 1 called `name` `label`.
#[derive(Packable, Debug, PartialEq)]
#[pack(kind = "test.tiny", schema = 2, migrate = TINY_MIGRATIONS)]
struct Tiny {
    name: String,
    count: u32,
    #[pack(requires)]
    model: Requirement,
}

const TINY_MIGRATIONS: &[Migration] = &[Migration {
    from: 1,
    apply: label_became_name,
}];

fn label_became_name(m: &mut Manifest) -> Result<(), PackError> {
    let v = m.fields.remove("label").ok_or(PackError::MissingPart {
        part: "label".into(),
    })?;
    m.fields.insert("name".into(), v);
    Ok(())
}

/// Exactly what a schema-1 build wrote (canonical, recorded byte for byte).
const TINY_V1: &str = r#"{"fields":{"count":7,"label":"old"},"format":1,"kind":"test.tiny","parts":{},"requires":{"model":{"model":"sha256:1111111111111111111111111111111111111111111111111111111111111111"}},"schema":1}"#;

fn model() -> ContentId {
    ContentId::from_digest([0x11; 32])
}

fn tiny(name: &str, count: u32) -> Tiny {
    Tiny {
        name: name.into(),
        count,
        model: Requirement::Model(model()),
    }
}

/// A typed requirement field, the way a real kind names its base model.
#[derive(Debug, Clone, PartialEq)]
struct ModelId(ContentId);

impl Required for ModelId {
    fn to_requirement(&self) -> Requirement {
        Requirement::Model(self.0)
    }

    fn from_requirement(part: &str, r: &Requirement) -> Result<Self, PackError> {
        match r {
            Requirement::Model(id) => Ok(Self(*id)),
            other => Err(PackError::RequirementShape {
                part: part.into(),
                found: other.clone(),
            }),
        }
    }
}

#[derive(Packable, Debug)]
#[pack(kind = "test.root", schema = 1)]
struct Root {
    title: String,
    #[pack(blob = "application/octet-stream")]
    weights: Blob,
    #[pack(child)]
    primary: Handle<Mid>,
    #[pack(child)]
    parents: Vec<Handle<Mid>>,
    #[pack(requires)]
    base: ModelId,
}

#[derive(Packable, Debug)]
#[pack(kind = "test.mid", schema = 1)]
struct Mid {
    #[pack(blob = "application/vnd.test.page")]
    page: Blob,
    #[pack(child)]
    leaf: Handle<Leaf>,
}

/// Hand-written, to prove the trait and the derive meet in one tree.
#[derive(Debug, PartialEq)]
struct Leaf {
    note: String,
    tags: Vec<String>,
}

impl Packable for Leaf {
    const KIND: Kind = Kind::new("test.leaf", 1);
    type Context = ();

    fn pack(&self, p: &mut Packer<'_>) -> Result<(), PackError> {
        p.field("note", &self.note)?;
        p.field("tags", &self.tags)
    }

    fn unpack(u: &mut Unpacker<'_>, _cx: &()) -> Result<Self, PackError> {
        Ok(Self {
            note: u.field("note")?,
            tags: u.field("tags")?,
        })
    }
}

fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn tree(title: &str, note: &str) -> Root {
    let leaf = Handle::new(Leaf {
        note: note.into(),
        tags: vec!["a".into(), "b".into()],
    });
    let m1 = Handle::new(Mid {
        page: Blob::from_bytes(noise(1 << 20, 2)),
        leaf: leaf.clone(),
    });
    let m2 = Handle::new(Mid {
        page: Blob::from_bytes(noise(64 << 10, 3)),
        leaf,
    });
    Root {
        title: title.into(),
        weights: Blob::from_bytes(noise(3 << 20, 1)),
        primary: m1.clone(),
        parents: vec![m1, m2],
        base: ModelId(model()),
    }
}

fn ram() -> Result<Arc<dyn Store>, StoreError> {
    Ok(Arc::new(RamStore::with_capacity(u64::MAX)))
}

/// A fresh DiskStore per call, each in its own directory under one temp root.
fn disks(root: &tempfile::TempDir) -> impl Fn() -> Result<Arc<dyn Store>, StoreError> + '_ {
    let n = AtomicU64::new(0);
    move || {
        Ok(Arc::new(DiskStore::open(
            root.path()
                .join(n.fetch_add(1, Ordering::Relaxed).to_string()),
        )?) as Arc<dyn Store>)
    }
}

fn run<T: Packable<Context = ()>>(sample: &Sample<T>) -> (Report, Report) {
    let dir = tempfile::tempdir().unwrap();
    let on_ram = conformance(sample, &(), &ram).unwrap_or_else(|f| panic!("RAM: {f}"));
    let on_disk = conformance(sample, &(), &disks(&dir)).unwrap_or_else(|f| panic!("disk: {f}"));
    eprintln!(
        "{} on RAM: {on_ram:?}\n{} on disk: {on_disk:?}",
        T::KIND.name,
        T::KIND.name
    );
    (on_ram, on_disk)
}

// what this catches: the whole generic contract on the smallest kind, on both stores: round
// trip, identity, one ref flip, a fault before/after every op, a flipped byte, an unmet
// requirement read nothing but the root, a newer schema refused, and the RECORDED schema-1
// manifest migrating to the schema-2 value.
#[test]
fn a_manifest_only_kind_conforms() {
    let sample = Sample {
        a: tiny("a", 1),
        b: tiny("b", 2),
        offers: Offers::none().with(Requirement::Model(model())),
        recorded: vec![Recorded {
            manifest: TINY_V1.as_bytes().to_vec(),
            expected: tiny("old", 7),
        }],
    };
    let (r, d) = run(&sample);
    for report in [&r, &d] {
        assert!(
            report.skipped.is_empty(),
            "every case ran: {:?}",
            report.skipped
        );
        assert_eq!(
            (report.tree.distinct, report.gets_at_load),
            (1, 1),
            "one manifest, one read"
        );
        assert_eq!(report.commit_ops, 2, "manifest put + ref flip");
    }
    // The recorded manifest is canonical: this build writes those exact bytes for schema 1,
    // and its id is the one forge-alloy's shared vector pins (test-vectors/jcs.json
    // `contentIds`), so a `package` stage in an alloy names it by the same hash.
    let m = Manifest::decode(TINY_V1.as_bytes()).unwrap();
    assert_eq!(m.encode().unwrap(), TINY_V1.as_bytes());
    assert_eq!(
        ContentId::of(TINY_V1.as_bytes()).to_string(),
        "sha256:631135120fe75465ac31c0957e2acb6e38771cde30c6e67af767defa3d4857ae"
    );
}

// what this catches: the contract on a deep, shared, multi-MB tree, plus its shape: a load
// reads only the root manifest; a shared Mid and a shared Leaf are stored once though
// referenced twice; and a whole three-level tree commits by its one root flip.
#[test]
fn a_nested_tree_with_blobs_and_shared_children_conforms() {
    let sample = Sample {
        a: tree("a", "leaf"),
        b: tree("b", "leaf"),
        offers: Offers::none().with(Requirement::Model(model())),
        recorded: vec![],
    };
    let (r, d) = run(&sample);
    for report in [&r, &d] {
        // root, weights, m1, m1.page, m2, m2.page, leaf: 7 nodes behind 9 references.
        assert_eq!(
            (report.tree.distinct, report.tree.references),
            (7, 9),
            "{:?}",
            report.tree
        );
        assert_eq!(
            report.gets_at_load, 1,
            "a lazy kind reads only its root manifest"
        );
    }
}

// what this catches: laziness end to end (a handle reads its child only on load, once, and a
// blob reads nothing until asked), and that loaded bytes equal what was packed.
#[test]
fn handles_and_blobs_load_on_first_use_only() {
    let s = Arc::new(FaultStore::new(ram().unwrap()));
    let original = tree("t", "leaf");
    let id = save(&*s, &original).unwrap();
    let loader = Loader::new(s.clone(), Offers::none().with(Requirement::Model(model())));
    let before = s.ops().unwrap().len();
    let root: Root = loader.load(&id, &()).unwrap();
    let gets = |from: usize| {
        s.ops().unwrap()[from..]
            .iter()
            .filter(|o| matches!(o, Op::Get(_)))
            .count()
    };
    assert_eq!(gets(before), 1);
    assert!(!root.primary.is_loaded() && !root.weights.is_resident());
    let mark = s.ops().unwrap().len();
    let mid = root.primary.load(&()).unwrap();
    let again = root.primary.load(&()).unwrap();
    assert!(Arc::ptr_eq(&mid, &again));
    assert_eq!(
        gets(mark),
        1,
        "one read for the child, none for the second load"
    );
    assert_eq!(mid.leaf.load(&()).unwrap().note, "leaf");
    assert_eq!(
        root.weights.read_all().unwrap(),
        original.weights.read_all().unwrap()
    );
    assert_eq!(
        root.weights.read_range_unverified(10, 5).unwrap(),
        original.weights.read_all().unwrap().slice(10..15)
    );
    assert_eq!(root.title, "t");
    assert_eq!(root.base, ModelId(model()));
}

// what this catches: the Merkle property (a change one level deep changes the root id) and
// that a flipped byte in a real file on disk is caught at the read that uses it, while the
// rest of the tree still loads.
#[test]
fn a_deep_change_moves_the_root_and_disk_corruption_is_caught_where_read() {
    let ram_store = RamStore::with_capacity(u64::MAX);
    assert_ne!(
        save(&ram_store, &tree("t", "x")).unwrap(),
        save(&ram_store, &tree("t", "y")).unwrap()
    );

    let dir = tempfile::tempdir().unwrap();
    let disk = Arc::new(DiskStore::open(dir.path()).unwrap());
    let original = tree("t", "leaf");
    let id = save(&*disk, &original).unwrap();
    let page = original.primary.load(&()).unwrap().page.content_id();
    let path = disk.blob_path(&page);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[1234] ^= 0x80;
    std::fs::write(&path, bytes).unwrap();
    let root: Root = Loader::new(disk, Offers::none().with(Requirement::Model(model())))
        .load(&id, &())
        .unwrap();
    let mid = root.primary.load(&()).unwrap();
    assert!(
        matches!(mid.page.read_all(), Err(StoreError::DigestMismatch { expected, .. }) if expected == page)
    );
    assert!(
        root.weights.read_all().is_ok(),
        "the untouched blob still verifies"
    );
}

// what this catches: refusals that must be typed, never a degraded result: a secret is never
// stored in plaintext before sealing exists; a package loaded as the wrong kind; a missing
// ref; a stale compare-and-swap at the package level.
#[test]
fn refusals_are_typed() {
    struct Diary;
    impl Packable for Diary {
        const KIND: Kind = Kind::new("test.diary", 1);
        type Context = ();
        fn pack(&self, p: &mut Packer<'_>) -> Result<(), PackError> {
            p.secret(
                "entry",
                "hers alone",
                &Audience {
                    keys: Default::default(),
                },
            )
        }
        fn unpack(_: &mut Unpacker<'_>, _: &()) -> Result<Self, PackError> {
            Ok(Diary)
        }
    }
    let s = RamStore::with_capacity(u64::MAX);
    assert!(matches!(
        save(&s, &Diary),
        Err(PackError::SealingUnavailable { .. })
    ));
    assert_eq!(s.blob_count().unwrap(), 0, "nothing was written");

    let s: Arc<dyn Store> = ram().unwrap();
    let name = RefName::new("test", "tiny").unwrap();
    let id = commit(s.as_ref(), &name, &tiny("a", 1), None).unwrap();
    let loader = Loader::new(s.clone(), Offers::none().with(Requirement::Model(model())));
    assert!(matches!(
        loader.load::<Leaf>(&id, &()),
        Err(PackError::KindMismatch { .. })
    ));
    assert!(matches!(
        loader.load_ref::<Tiny>(&RefName::new("test", "none").unwrap(), &()),
        Err(PackError::RefAbsent(_))
    ));
    assert!(matches!(
        commit(s.as_ref(), &name, &tiny("b", 2), None),
        Err(PackError::Store(StoreError::RefConflict { .. }))
    ));
    assert_eq!(loader.load_ref::<Tiny>(&name, &()).unwrap(), tiny("a", 1));
}

// what this catches: the cost the design's budget rests on (SHA-256 throughput on this
// machine, and a whole DiskStore put of the same blob including fsync). Run with
// `cargo test -p continuum-pack --release -- --ignored --nocapture throughput`.
#[test]
#[ignore = "a measurement, not a check; run in release"]
fn hashing_throughput() {
    let data = noise(512 << 20, 9);
    let mb = data.len() as f64 / (1024.0 * 1024.0);
    let best = |f: &dyn Fn()| {
        (0..3)
            .map(|_| {
                let t = std::time::Instant::now();
                f();
                t.elapsed().as_secs_f64()
            })
            .fold(f64::MAX, f64::min)
    };
    let hash = best(&|| {
        std::hint::black_box(ContentId::of(&data));
    });
    let dir = tempfile::tempdir().unwrap();
    let n = std::cell::Cell::new(0u32);
    let put = best(&|| {
        n.set(n.get() + 1);
        let s = DiskStore::open(dir.path().join(n.get().to_string())).unwrap();
        std::hint::black_box(s.put(BlobSource::Bytes(&data)).unwrap());
        std::fs::remove_dir_all(s.root()).unwrap();
    });
    eprintln!("sha256: {:.0} MB/s ({mb:.0} MB in {hash:.3} s); DiskStore put incl. fsync: {:.0} MB/s ({put:.3} s)", mb / hash, mb / put);
}
