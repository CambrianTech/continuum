//! The engine-train cleaner as a SAGE component (card 6ad3f0c5): its own worker, its own timeline.
//! Every learning seat writes a fresh shadow (`<bucket stem>-<seat id>.gguf`) into the engine's
//! `--train-dir`. The trainer only says what happened; the cleaner removes what nothing holds:
//!
//! - it hears `trainer/settled` (one event per page-out: the seat's shadow, what it learned and
//!   whether it published). A seat that learned nothing and published nothing leaves its shadow to go;
//! - it watches `trainer/seated` (the shadows live seats hold, as state) so a sweep never takes one;
//! - it watches `resources/disk` (the disk monitor's frames). When the volume holding the train dir
//!   reaches High it sweeps, oldest first, every shadow no seat holds.
//!
//! What must survive, always: a shadow a generation was published from (its `<shadow>.checkpoint`
//! sibling), a shadow a live seat holds, every staged gene (`gene-*.gguf`), every checkpoint dir, and
//! anything outside the train dir. File work runs off the worker (`Ctx::off`); a sweep is said on
//! `trainer/cleaned`. On the IntelMac (2026-11-23) the leftovers reached 11,130 files, 46 GB.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use continuum_sage::component::{Component, Ctx, Key, Urgency, Wiring};
use continuum_sage::dataflow::{Event, Snapshot, TopicName};
use continuum_sage::Qos;

use continuum_kernel::system_resources::disk_pressure::DiskPressureLevel;
use continuum_kernel::system_resources::disk_relief::{Volumes, DISK};
use continuum_kernel::system_resources::drives::drive_holding;

/// One page-out, said by the trainer: the seat's shadow and what came of it.
pub const SETTLED: TopicName = TopicName("trainer/settled");
/// The shadows live seats hold, as state.
pub const SEATED: TopicName = TopicName("trainer/seated");

/// A seat's page-out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled {
    pub shadow: PathBuf,
    pub learned: u32,
    pub published: bool,
}

/// The shadows live seats hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seated(pub BTreeSet<PathBuf>);

/// What became of one shadow.
#[derive(Debug)]
pub enum Released {
    Removed(u64),
    /// Kept, and why.
    Kept(&'static str),
    Failed(std::io::Error),
}

/// PURE: whether `name` is a seat's shadow (`<32 hex>-<64 hex>-<32 hex>.gguf`): never a gene, a
/// checkpoint dir or anything else in the train dir.
pub fn is_shadow(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".gguf") else { return false };
    let parts: Vec<&str> = stem.split('-').collect();
    matches!(parts.as_slice(), [persona, signature, seat] if [(persona, 32), (signature, 64), (seat, 32)].iter().all(|(p, len)| p.len() == *len && p.bytes().all(|b| b.is_ascii_hexdigit())))
}

/// A shadow's key: its file name, unique by its seat id. A seat's shadow is held and released by
/// name, never by the path the engine reported, which may spell the train dir differently (relative,
/// through a symlink, a `\\?\` prefix on Windows; Fable on #6240).
pub fn shadow_name(shadow: &Path) -> Option<std::ffi::OsString> {
    shadow.file_name().map(std::ffi::OsStr::to_os_string)
}

/// Remove the shadow named `name` from `train_dir` when it is a seat's shadow, no seat holds it,
/// and no generation was published from it.
pub fn release(name: &std::ffi::OsStr, train_dir: &Path, held: &BTreeSet<std::ffi::OsString>) -> Released {
    if !name.to_str().is_some_and(is_shadow) {
        return Released::Kept("not a seat's shadow");
    }
    if held.contains(name) {
        return Released::Kept("a live seat holds it");
    }
    let shadow = train_dir.join(name);
    let mut generation = shadow.as_os_str().to_owned();
    generation.push(".checkpoint");
    if Path::new(&generation).exists() {
        return Released::Kept("a generation was published from it");
    }
    let bytes = match std::fs::metadata(&shadow) {
        Ok(meta) => meta.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Released::Removed(0),
        Err(error) => return Released::Failed(error),
    };
    match std::fs::remove_file(&shadow) {
        Ok(()) => Released::Removed(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Released::Removed(0),
        Err(error) => Released::Failed(error),
    }
}

/// What a sweep may take: only a shadow it has evidence is dead. A shadow written before this boot
/// belongs to no live seat; a shadow written since is taken only once its seat was heard to settle.
/// A seat that starts during a sweep, or between the engine writing its shadow and the trainer saying
/// it is held, is in neither, so a sweep never unlinks a shadow a live seat trains on.
pub struct Sweepable<'a> {
    pub booted: std::time::SystemTime,
    pub settled: &'a BTreeSet<std::ffi::OsString>,
}

impl Sweepable<'_> {
    fn admits(&self, name: &std::ffi::OsStr, written: std::time::SystemTime) -> bool {
        written < self.booted || self.settled.contains(name)
    }
}

/// What a sweep freed, and the shadows it removed (their names leave the cleaner's settled set).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Swept {
    pub freed: u64,
    pub removed: Vec<std::ffi::OsString>,
}

/// Every sweepable shadow in `train_dir` no seat holds, oldest first, released until `want` bytes
/// are freed.
pub fn sweep(train_dir: &Path, held: &BTreeSet<std::ffi::OsString>, sweepable: &Sweepable<'_>, want: u64) -> Swept {
    let Ok(entries) = std::fs::read_dir(train_dir) else { return Swept::default() };
    let mut shadows: Vec<(std::time::SystemTime, std::ffi::OsString)> = entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(is_shadow))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.file_name())))
        .filter(|(written, name)| sweepable.admits(name, *written))
        .collect();
    shadows.sort();
    let mut swept = Swept::default();
    for (_, name) in shadows {
        if swept.freed >= want {
            break;
        }
        if let Released::Removed(bytes) = release(&name, train_dir, held) {
            swept.freed = swept.freed.saturating_add(bytes);
            swept.removed.push(name);
        }
    }
    swept
}

/// What wakes the cleaner.
pub enum Wake {
    Settled(Arc<Settled>),
    Seated(Arc<Seated>),
    Disk(Arc<Volumes>),
    /// File work it handed off, finished.
    Done { what: &'static str, freed: u64, removed: Vec<std::ffi::OsString> },
}

impl From<Event<Settled>> for Wake {
    fn from(event: Event<Settled>) -> Self {
        Wake::Settled(event.value)
    }
}

impl From<Snapshot<Seated>> for Wake {
    fn from(snapshot: Snapshot<Seated>) -> Self {
        Wake::Seated(snapshot.value)
    }
}

impl From<Snapshot<Volumes>> for Wake {
    fn from(snapshot: Snapshot<Volumes>) -> Self {
        Wake::Disk(snapshot.value)
    }
}

/// The cleaner of one train dir.
pub struct Cleaner {
    train_dir: PathBuf,
    /// The shadows live seats hold, by name.
    held: Arc<BTreeSet<std::ffi::OsString>>,
    /// Shadows whose seats settled and kept them (they learned or published): a sweep may take them.
    settled: Arc<BTreeSet<std::ffi::OsString>>,
    /// When this cleaner started: a shadow written before it belongs to no live seat.
    booted: std::time::SystemTime,
    /// A sweep is running off the worker; disk frames meanwhile wait for the next.
    sweeping: bool,
}

impl Cleaner {
    /// The cleaner of `train_dir`. `booted` is when this process began, before anything in it could
    /// seat a learner: a shadow written before then belongs to no live seat.
    pub fn of(train_dir: PathBuf, booted: std::time::SystemTime) -> Self {
        Self { train_dir, held: Arc::default(), settled: Arc::default(), booted, sweeping: false }
    }
}

impl Component for Cleaner {
    type Input = Wake;
    const NAME: &'static str = "engine-train-cleaner";
    const QOS: Qos = Qos::Background;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.hear::<Settled>(SETTLED, std::num::NonZeroUsize::MIN.saturating_add(63)); // a window of 64 page-outs: more than any burst of seats one pass settles
        wiring.watch::<Seated>(SEATED, Key::ONE);
        wiring.watch::<Volumes>(DISK, Key::ONE);
    }

    async fn handle(&mut self, wake: Wake, ctx: &mut Ctx<Self>) {
        match wake {
            Wake::Seated(seated) => self.held = Arc::new(seated.0.iter().filter_map(|shadow| shadow_name(shadow)).collect()),
            Wake::Settled(settled) => {
                let Some(name) = shadow_name(&settled.shadow) else { return };
                // the settlement IS the seat letting go: it no longer holds the name, whichever arrives first,
                // this or the seated update that drops it (Codex on #6240)
                Arc::make_mut(&mut self.held).remove(&name);
                // eligible for a sweep from now on, whatever comes of a release below: a release that fails
                // (a transient error) leaves it for the next sweep to retry, never forgotten (Codex on #6240)
                Arc::make_mut(&mut self.settled).insert(name.clone());
                if settled.learned == 0 && !settled.published {
                    let dir = self.train_dir.clone();
                    let held = Arc::clone(&self.held);
                    ctx.need(Urgency::Want);
                    let removing = name.clone();
                    ctx.off(move || release(&name, &dir, &held), move |released| match released {
                        Released::Removed(freed) => Wake::Done { what: "settled", freed, removed: vec![removing] },
                        // kept or failed: its name stays settled, for a later sweep to retry
                        Released::Kept(_) | Released::Failed(_) => Wake::Done { what: "settled-kept", freed: 0, removed: Vec::new() },
                    });
                }
            }
            Wake::Disk(volumes) => {
                if self.sweeping {
                    return;
                }
                let Some(drive) = drive_holding(&volumes.0, &self.train_dir) else { return };
                let used = drive.total_bytes.saturating_sub(drive.available_bytes);
                if drive.total_bytes == 0 || (used as f64 / drive.total_bytes as f64) < DiskPressureLevel::HIGH_AT {
                    return;
                }
                let want = used.saturating_sub((drive.total_bytes as f64 * DiskPressureLevel::WARNING_AT) as u64);
                let dir = self.train_dir.clone();
                let held = Arc::clone(&self.held);
                let settled = Arc::clone(&self.settled);
                let booted = self.booted;
                self.sweeping = true;
                ctx.need(Urgency::Want);
                ctx.off(move || sweep(&dir, &held, &Sweepable { booted, settled: &settled }, want), |swept| Wake::Done { what: "swept", freed: swept.freed, removed: swept.removed });
            }
            Wake::Done { what, freed, removed } => {
                if what == "swept" {
                    self.sweeping = false;
                }
                // a removed shadow is gone: its name leaves the settled set, which so holds only what may
                // still be on disk (Codex on #6240: it grew with every seat ever settled)
                if !removed.is_empty() {
                    let settled = Arc::make_mut(&mut self.settled);
                    for name in &removed {
                        settled.remove(name);
                    }
                }
                continuum_base::probe!(class = "training.shadows.cleaned", what, freed,
                    "the engine-train cleaner released shadows nothing holds");
                ctx.need(Urgency::Content);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn shadow_file(seat: u8) -> String {
        format!("{}-{}-{:032x}.gguf", "a".repeat(32), "b".repeat(64), seat)
    }

    // what this catches (the IntelMac 2026-11-23: 11,130 leftover shadows, 46 GB): a cleaner that
    // removes what must survive (a held shadow, a shadow a generation came from, a staged gene) or
    // keeps what nothing holds.
    #[test]
    fn only_shadows_nothing_holds_are_released() {
        let dir = tempfile::tempdir().expect("test: dir");
        let train = dir.path().join("engine-train");
        std::fs::create_dir_all(&train).expect("test: train dir");
        let write = |name: &str| {
            std::fs::write(train.join(name), b"shadow").expect("test: file");
            OsString::from(name)
        };
        let free = write(&shadow_file(1));
        let held_shadow = write(&shadow_file(2));
        let published = write(&shadow_file(3));
        std::fs::create_dir_all(train.join(format!("{}.checkpoint", shadow_file(3)))).expect("test: generation");
        let gene = write("gene-0123456789abcdef.gguf");
        let held: BTreeSet<OsString> = [held_shadow.clone()].into();

        assert!(matches!(release(&free, &train, &held), Released::Removed(6)));
        assert!(matches!(release(&held_shadow, &train, &held), Released::Kept(_)) && train.join(&held_shadow).exists(), "a live seat's shadow survives");
        assert!(matches!(release(&published, &train, &held), Released::Kept(_)) && train.join(&published).exists(), "a shadow a generation came from survives");
        assert!(matches!(release(&gene, &train, &held), Released::Kept(_)) && train.join(&gene).exists(), "a staged gene is never a shadow");
    }

    // what this catches (Fable on #6240): a sweep that unlinks a shadow a live seat trains on because
    // the seat started during the sweep, or between the engine writing its shadow and the trainer
    // saying it is held; and a held shadow missed because the engine spelled the train dir
    // differently than the cleaner.
    #[test]
    fn a_sweep_takes_only_shadows_it_has_evidence_are_dead() {
        let dir = tempfile::tempdir().expect("test: dir");
        let train = dir.path().join("engine-train");
        std::fs::create_dir_all(&train).expect("test: train dir");
        let write = |seat: u8| {
            let name = shadow_file(seat);
            std::fs::write(train.join(&name), b"shadow").expect("test: file");
            OsString::from(name)
        };
        let before_boot = write(1);
        let booted = std::fs::metadata(train.join(&before_boot)).and_then(|m| m.modified()).expect("test: written time");
        let booted = booted + std::time::Duration::from_secs(1);
        // written "after boot": its time is set past the boot instant
        let unsettled = write(2);
        let settled_name = write(3);
        let later = booted + std::time::Duration::from_secs(1);
        for name in [&unsettled, &settled_name] {
            std::fs::File::options().write(true).open(train.join(name)).and_then(|f| f.set_modified(later)).expect("test: set time");
        }
        let settled: BTreeSet<OsString> = [settled_name.clone()].into();
        // the engine reported the held shadow through a differently spelled path; the cleaner keys by name
        let reported = dir.path().join("engine-train/../engine-train").join(&before_boot);
        let held: BTreeSet<OsString> = [shadow_name(&reported).expect("test: a name")].into();

        let swept = sweep(&train, &held, &Sweepable { booted, settled: &settled }, u64::MAX);
        assert_eq!(swept.freed, 6, "only the settled shadow written since boot goes");
        assert_eq!(swept.removed, vec![settled_name.clone()], "and it names what it removed, so the owner prunes it (Codex on #6240)");
        assert!(!train.join(&settled_name).exists());
        assert!(train.join(&unsettled).exists(), "a shadow written since boot that never settled may be a live seat's");
        assert!(train.join(&before_boot).exists(), "a held shadow survives whatever path spelled it");
    }

    // what this catches (Codex on #6240): a settlement that arrives before the seated update that
    // drops its shadow, so the cleaner still reads it held, keeps it, and never takes it; the
    // cleaner must release a settled shadow whichever of the two arrives first.
    #[tokio::test]
    async fn a_settled_shadow_goes_whichever_of_settlement_and_seated_arrives_first() {
        use continuum_sage::component::{presence_key, Bus, Owner, Telemetry, TELEMETRY};
        use continuum_sage::dataflow::{OwnerId, Provenance, RunId, StageId, StageIncarnation};

        let dir = tempfile::tempdir().expect("test: dir");
        let train = dir.path().join("engine-train");
        std::fs::create_dir_all(&train).expect("test: train dir");
        let bus = Arc::new(Bus::new());
        let mut owner = Owner::with_bus(Arc::clone(&bus), OwnerId(uuid::Uuid::new_v4()), 0);
        let _cleaner = owner.start(Cleaner::of(train.clone(), std::time::SystemTime::now()));
        let mut seated = bus.publisher::<Seated>(SEATED, Key::ONE).expect("test: the seated topic");
        let mut settled = bus.emitter::<Settled>(SETTLED).expect("test: the settled topic");
        let mut said = 0u64;
        let mut by = || {
            said += 1;
            Provenance { by: RunId { incarnation: StageIncarnation { stage: StageId::named("trainer-fixture"), owner: OwnerId(uuid::Uuid::nil()), epoch: 0 }, seq: said }, from: Vec::new() }
        };
        let telemetry = bus.reader::<Telemetry>(TELEMETRY, presence_key(Cleaner::NAME));
        let handled = |at_least: u64| {
            let mut telemetry = telemetry.clone();
            async move {
                while telemetry.latest().is_none_or(|t| t.value.handled < at_least) {
                    telemetry.changed().await.expect("test: the bus outlives the cleaner");
                }
            }
        };
        let write = |seat: u8| {
            let path = train.join(shadow_file(seat));
            std::fs::write(&path, b"shadow").expect("test: file");
            path
        };

        // settlement first: the cleaner still reads the shadow held when it is told the seat let go
        let first = write(1);
        seated.publish(Seated([first.clone()].into()), by());
        handled(1).await;
        settled.emit(Settled { shadow: first.clone(), learned: 0, published: false });
        handled(3).await; // the settlement and the release it handed off
        assert!(!first.exists(), "released though the seated update had not yet dropped it");
        seated.publish(Seated(BTreeSet::new()), by());
        handled(4).await;

        // the seated update first, then the settlement
        let second = write(2);
        seated.publish(Seated([second.clone()].into()), by());
        handled(5).await;
        seated.publish(Seated(BTreeSet::new()), by());
        handled(6).await;
        settled.emit(Settled { shadow: second.clone(), learned: 0, published: false });
        handled(8).await;
        assert!(!second.exists(), "released after the seated update dropped it too");

        // what this catches too (Codex on #6240): a settlement whose release fails (a transient error),
        // its name then forgotten, so nothing ever retries it. Here the release fails on a read-only
        // dir; once writable again, the next sweep under pressure takes it.
        #[cfg(unix)]
        {
            use continuum_kernel::system_resources::drives::{DriveInfo, DriveKind, DriveRole};
            use std::os::unix::fs::PermissionsExt;
            let third = write(3);
            std::fs::set_permissions(&train, std::fs::Permissions::from_mode(0o555)).expect("test: read-only");
            settled.emit(Settled { shadow: third.clone(), learned: 0, published: false });
            handled(10).await; // the settlement, and its release that failed
            std::fs::set_permissions(&train, std::fs::Permissions::from_mode(0o755)).expect("test: writable");
            assert!(third.exists(), "the failed release kept it");
            let mut disk = bus.publisher::<Volumes>(DISK, Key::ONE).expect("test: the disk topic");
            let full = DriveInfo { mount: dir.path().to_path_buf(), total_bytes: 100, available_bytes: 1, role: DriveRole::System, kind: DriveKind::Flash };
            disk.publish(Volumes(vec![full]), by());
            handled(12).await; // the frame, and the sweep it handed off
            assert!(!third.exists(), "the next sweep retried the settled shadow and took it");
        }
        drop(owner);
    }

    #[test]
    fn a_shadow_name_is_exact() {
        assert!(is_shadow(&shadow_file(1)));
        assert!(!is_shadow("gene-0123456789abcdef.gguf"));
        assert!(!is_shadow(&format!("{}.checkpoint", shadow_file(1))));
        assert!(!is_shadow(&shadow_file(1).replace(".gguf", ".bin")));
    }
}
