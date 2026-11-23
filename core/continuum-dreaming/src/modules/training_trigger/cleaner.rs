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

/// Remove `shadow` when it lies directly in `train_dir`, is a shadow, is not held, and no generation
/// was published from it.
pub fn release(shadow: &Path, train_dir: &Path, held: &BTreeSet<PathBuf>) -> Released {
    if shadow.parent() != Some(train_dir) {
        return Released::Kept("not in this node's train dir");
    }
    if !shadow.file_name().and_then(|n| n.to_str()).is_some_and(is_shadow) {
        return Released::Kept("not a seat's shadow");
    }
    if held.contains(shadow) {
        return Released::Kept("a live seat holds it");
    }
    let mut generation = shadow.as_os_str().to_owned();
    generation.push(".checkpoint");
    if Path::new(&generation).exists() {
        return Released::Kept("a generation was published from it");
    }
    let bytes = match std::fs::metadata(shadow) {
        Ok(meta) => meta.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Released::Removed(0),
        Err(error) => return Released::Failed(error),
    };
    match std::fs::remove_file(shadow) {
        Ok(()) => Released::Removed(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Released::Removed(0),
        Err(error) => Released::Failed(error),
    }
}

/// Every shadow in `train_dir` no seat holds, oldest first, released until `want` bytes are freed.
/// Returns the bytes freed.
pub fn sweep(train_dir: &Path, held: &BTreeSet<PathBuf>, want: u64) -> u64 {
    let Ok(entries) = std::fs::read_dir(train_dir) else { return 0 };
    let mut shadows: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(is_shadow))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    shadows.sort();
    let mut freed = 0u64;
    for (_, shadow) in shadows {
        if freed >= want {
            break;
        }
        if let Released::Removed(bytes) = release(&shadow, train_dir, held) {
            freed = freed.saturating_add(bytes);
        }
    }
    freed
}

/// What wakes the cleaner.
pub enum Wake {
    Settled(Arc<Settled>),
    Seated(Arc<Seated>),
    Disk(Arc<Volumes>),
    /// File work it handed off, finished.
    Done { what: &'static str, freed: u64 },
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
    held: Arc<BTreeSet<PathBuf>>,
    /// A sweep is running off the worker; disk frames meanwhile wait for the next.
    sweeping: bool,
}

impl Cleaner {
    pub fn of(train_dir: PathBuf) -> Self {
        Self { train_dir, held: Arc::default(), sweeping: false }
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
            Wake::Seated(seated) => self.held = Arc::new(seated.0.clone()),
            Wake::Settled(settled) => {
                if settled.learned == 0 && !settled.published {
                    let (shadow, dir, held) = (settled.shadow.clone(), self.train_dir.clone(), Arc::clone(&self.held));
                    ctx.need(Urgency::Want);
                    ctx.off(move || release(&shadow, &dir, &held), |released| match released {
                        Released::Removed(freed) => Wake::Done { what: "settled", freed },
                        Released::Kept(_) | Released::Failed(_) => Wake::Done { what: "settled-kept", freed: 0 },
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
                let (dir, held) = (self.train_dir.clone(), Arc::clone(&self.held));
                self.sweeping = true;
                ctx.need(Urgency::Want);
                ctx.off(move || sweep(&dir, &held, want), |freed| Wake::Done { what: "swept", freed });
            }
            Wake::Done { what, freed } => {
                if what == "swept" {
                    self.sweeping = false;
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

    fn shadow_name(seat: u8) -> String {
        format!("{}-{}-{:032x}.gguf", "a".repeat(32), "b".repeat(64), seat)
    }

    // what this catches (the IntelMac 2026-11-23: 11,130 leftover shadows, 46 GB): a cleaner that
    // removes what must survive (a held shadow, a shadow a generation came from, a staged gene, a
    // checkpoint dir, a file outside the train dir) or that keeps what nothing holds.
    #[test]
    fn only_shadows_nothing_holds_are_released() {
        let dir = tempfile::tempdir().expect("test: dir");
        let train = dir.path().join("engine-train");
        std::fs::create_dir_all(&train).expect("test: train dir");
        let write = |name: &str| {
            let path = train.join(name);
            std::fs::write(&path, b"shadow").expect("test: file");
            path
        };
        let free = write(&shadow_name(1));
        let held_shadow = write(&shadow_name(2));
        let published = write(&shadow_name(3));
        std::fs::create_dir_all(train.join(format!("{}.checkpoint", shadow_name(3)))).expect("test: generation");
        let gene = write("gene-0123456789abcdef.gguf");
        let held: BTreeSet<PathBuf> = [held_shadow.clone()].into();

        assert!(matches!(release(&free, &train, &held), Released::Removed(6)));
        assert!(matches!(release(&held_shadow, &train, &held), Released::Kept(_)) && held_shadow.exists(), "a live seat's shadow survives");
        assert!(matches!(release(&published, &train, &held), Released::Kept(_)) && published.exists(), "a shadow a generation came from survives");
        assert!(matches!(release(&gene, &train, &held), Released::Kept(_)) && gene.exists(), "a staged gene is never a shadow");
        let outside = dir.path().join(shadow_name(4));
        std::fs::write(&outside, b"shadow").expect("test: outside");
        assert!(matches!(release(&outside, &train, &held), Released::Kept(_)) && outside.exists(), "outside the train dir is never touched");

        let leftover = write(&shadow_name(5));
        assert_eq!(sweep(&train, &held, u64::MAX), 6, "a sweep frees the one leftover nothing holds");
        assert!(!leftover.exists());
        assert!(held_shadow.exists() && published.exists() && gene.exists(), "and nothing that must survive");
        assert!(train.join(format!("{}.checkpoint", shadow_name(3))).exists(), "the generation stays");
    }

    #[test]
    fn a_shadow_name_is_exact() {
        assert!(is_shadow(&shadow_name(1)));
        assert!(!is_shadow("gene-0123456789abcdef.gguf"));
        assert!(!is_shadow(&format!("{}.checkpoint", shadow_name(1))));
        assert!(!is_shadow(&shadow_name(1).replace(".gguf", ".bin")));
    }
}
