//! Disk relief: a SAGE component that gives a volume back its room when the volume is under pressure.
//!
//! The disk monitor publishes every poll's drives as one frame on the node's bus (`resources/disk`).
//! This component watches those frames. When a volume reaches High it asks the pools living on THAT
//! volume, the most to give first, for the bytes that bring it back below Warning, and publishes what
//! each gave (`resources/disk_relief`).
//!
//! It replaces asking pools by their own budgets (the broker's `relieve`): on the M5 (2026-11-22) the
//! volume sat at 92% while every pool read itself as under budget (the KV page store: own pressure 0.0,
//! volume 0.92), so nothing was asked, every five seconds, and 42 GB of finished worktrees and build
//! output stayed. A pool joins by being registered with the broker with a `root()`; the broker leaves
//! such pools to this component.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use continuum_sage::component::{node, Component, Ctx, Key, Owner, Urgency, Wiring};
use continuum_sage::dataflow::{OwnerId, Provenance, Publisher, RunId, Snapshot, StageId, StageIncarnation, TopicName};
use continuum_sage::Qos;

use super::disk_pressure::DiskPressureLevel;
use super::drives::{drive_holding, DriveInfo};
use crate::paging::pool::ResourcePool;

/// Every drive, as the disk monitor measured it on one poll.
pub const DISK: TopicName = TopicName("resources/disk");
/// What one relief pass asked of each pool on a pressured volume, and what each gave.
pub const RELIEF: TopicName = TopicName("resources/disk_relief");

/// One poll's drives.
#[derive(Debug, Clone)]
pub struct Volumes(pub Vec<DriveInfo>);

/// One pressured volume's relief.
#[derive(Debug, Clone, PartialEq)]
pub struct Relief {
    pub mount: PathBuf,
    pub level: DiskPressureLevel,
    /// The bytes that bring the volume back below Warning.
    pub want: u64,
    /// Each pool asked, in order, and what it freed.
    pub asked: Vec<(String, u64)>,
    pub freed: u64,
}

static POOLS: parking_lot::RwLock<Vec<Arc<dyn ResourcePool>>> = parking_lot::RwLock::new(Vec::new());

/// A pool whose bytes live on a disk joins relief (a second registration under its name replaces it).
pub fn register(pool: Arc<dyn ResourcePool>) {
    let mut pools = POOLS.write();
    pools.retain(|p| p.tier_name() != pool.tier_name());
    pools.push(pool);
}

/// The relief each pressured volume in `drives` needs, asking `pools` (a whole model's worth of file
/// work: call it off the async runtime).
pub fn relieve(drives: &[DriveInfo], pools: &[Arc<dyn ResourcePool>]) -> Vec<Relief> {
    let mut reliefs = Vec::new();
    for drive in drives {
        if drive.total_bytes == 0 {
            continue;
        }
        let used = drive.total_bytes.saturating_sub(drive.available_bytes);
        let level = DiskPressureLevel::from_pressure(used as f64 / drive.total_bytes as f64);
        if !matches!(level, DiskPressureLevel::High | DiskPressureLevel::Critical) {
            continue;
        }
        let target = (drive.total_bytes as f64 * DiskPressureLevel::WARNING_AT) as u64;
        let want = used.saturating_sub(target);
        let mut here: Vec<&Arc<dyn ResourcePool>> = pools
            .iter()
            .filter(|pool| {
                pool.root()
                    .is_some_and(|root| drive_holding(drives, &root).is_some_and(|holder| holder.mount == drive.mount))
            })
            .collect();
        // The most to give first: the fewest pools disturbed for the room.
        here.sort_by_key(|pool| std::cmp::Reverse(pool.usage_bytes()));
        let mut asked = Vec::new();
        let mut freed = 0u64;
        for pool in here {
            if freed >= want {
                break;
            }
            let gave = pool.evict_at_least(want - freed);
            asked.push((pool.tier_name().to_owned(), gave));
            freed = freed.saturating_add(gave);
        }
        reliefs.push(Relief { mount: drive.mount.clone(), level, want, asked, freed });
    }
    reliefs
}

/// The disk frame, as relief is handed it.
pub struct Frame(Arc<Volumes>);

impl From<Snapshot<Volumes>> for Frame {
    fn from(snapshot: Snapshot<Volumes>) -> Self {
        Frame(snapshot.value)
    }
}

struct DiskRelief {
    said: Publisher<Vec<Relief>>,
}

impl Component for DiskRelief {
    type Input = Frame;
    const NAME: &'static str = "disk-relief";
    const QOS: Qos = Qos::Utility;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<Volumes>(DISK, Key::ONE);
    }

    async fn handle(&mut self, Frame(volumes): Frame, ctx: &mut Ctx<Self>) {
        let drives = volumes.0.clone();
        let pools: Vec<Arc<dyn ResourcePool>> = POOLS.read().clone();
        let needs = drives.iter().any(|d| {
            let used = d.total_bytes.saturating_sub(d.available_bytes) as f64;
            d.total_bytes > 0 && used / d.total_bytes as f64 >= DiskPressureLevel::HIGH_AT
        });
        if !needs {
            ctx.need(Urgency::Content);
            return;
        }
        ctx.need(Urgency::Urgent);
        let reliefs = match tokio::task::spawn_blocking(move || relieve(&drives, &pools)).await {
            Ok(reliefs) => reliefs,
            Err(join) => {
                continuum_base::probe!(class = "disk.relief.failed", why = %join, "the relief pass did not finish");
                Vec::new()
            }
        };
        for relief in &reliefs {
            continuum_base::probe!(
                class = "disk.relief",
                mount = %relief.mount.display(),
                level = ?relief.level,
                want = relief.want,
                freed = relief.freed,
                asked = ?relief.asked,
                "a pressured volume asked the pools on it for room"
            );
        }
        ctx.publish(&mut self.said, reliefs);
        ctx.need(Urgency::Content);
    }
}

/// Start disk relief as one of the node's parts, at boot: it runs until the node's owner drops
/// (SAGE.md: a component's lifetime is owned by its node, never started by its first caller).
pub fn start_on(node: &mut Owner) {
    match node.bus().publisher::<Vec<Relief>>(RELIEF, Key::ONE) {
        Ok(said) => {
            node.start(DiskRelief { said });
        }
        Err(taken) => continuum_base::probe!(class = "disk.relief.not_started", topic = taken.0 .0, "disk relief's topic is claimed elsewhere; it does not run"),
    }
}

/// Where the disk monitor's polls are said: a publisher, not a task, claimed on first use.
static FRAMES: OnceLock<Option<parking_lot::Mutex<Publisher<Volumes>>>> = OnceLock::new();

/// The disk monitor's poll, said on the node's bus for relief to watch. Never waits.
pub fn frame(drives: Vec<DriveInfo>) {
    let Some(frames) = FRAMES.get_or_init(|| node().publisher::<Volumes>(DISK, Key::ONE).ok().map(parking_lot::Mutex::new)) else {
        return;
    };
    let by = Provenance {
        by: RunId { incarnation: StageIncarnation { stage: StageId::named("disk-monitor"), owner: OwnerId(uuid::Uuid::nil()), epoch: 0 }, seq: 0 },
        from: Vec::new(),
    };
    frames.lock().publish(Volumes(drives), by);
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::paging::pool::ResourcePoolEntry;
    use crate::system_resources::drives::{DriveKind, DriveRole};

    struct Held {
        name: &'static str,
        root: PathBuf,
        bytes: AtomicU64,
        asked: AtomicU64,
    }

    impl ResourcePool for Held {
        fn tier_name(&self) -> &str {
            self.name
        }
        // A generous budget, ten times what it holds: by its own pressure this pool never needs
        // relief (the M5's shape); only the volume's pressure asks it.
        fn capacity_bytes(&self) -> u64 {
            self.bytes.load(Ordering::Relaxed).saturating_mul(10)
        }
        fn usage_bytes(&self) -> u64 {
            self.bytes.load(Ordering::Relaxed)
        }
        fn evict_at_least(&self, want: u64) -> u64 {
            self.asked.store(want, Ordering::Relaxed);
            let gave = want.min(self.bytes.load(Ordering::Relaxed));
            self.bytes.fetch_sub(gave, Ordering::Relaxed);
            gave
        }
        fn snapshot(&self) -> Vec<ResourcePoolEntry> {
            Vec::new()
        }
        fn root(&self) -> Option<PathBuf> {
            Some(self.root.clone())
        }
    }

    fn held(name: &'static str, root: &str, bytes: u64) -> Arc<Held> {
        Arc::new(Held { name, root: root.into(), bytes: AtomicU64::new(bytes), asked: AtomicU64::new(0) })
    }

    // what this catches: the M5 at 92% with every pool under its own budget and nothing asked
    // (2026-11-22). A volume at High asks the pools ON IT, the most to give first, for exactly the
    // bytes that bring it below Warning; a pool on another volume is not asked; a volume below High
    // asks nothing.
    #[test]
    fn a_volume_at_high_asks_the_pools_on_it_for_the_room_back_below_warning() {
        let drive = |mount: &str, total: u64, available: u64| DriveInfo { mount: mount.into(), total_bytes: total, available_bytes: available, role: DriveRole::System, kind: DriveKind::Flash };
        let drives = vec![drive("/", 1_000, 80), drive("/Volumes/cold", 1_000, 500)];
        let worktrees = held("worktrees", "/home/.continuum/worktrees", 150);
        let builds = held("build-output", "/home/.continuum/cache/cargo-target", 50);
        let archive = held("archive", "/Volumes/cold/models", 400);
        let pools: Vec<Arc<dyn ResourcePool>> = vec![builds.clone(), worktrees.clone(), archive.clone()];

        let reliefs = relieve(&drives, &pools);
        assert_eq!(reliefs.len(), 1, "only the pressured volume");
        let relief = &reliefs[0];
        assert_eq!((relief.level, relief.want), (DiskPressureLevel::High, 120), "920 used, 800 is Warning: 120 back");
        assert_eq!(relief.asked, vec![("worktrees".to_owned(), 120)], "the pool with the most gives first, and is enough");
        assert_eq!(builds.asked.load(Ordering::Relaxed), 0, "no pool is disturbed past the room needed");
        assert_eq!(archive.asked.load(Ordering::Relaxed), 0, "a pool on another volume is never asked");

        let short = held("worktrees", "/home/.continuum/worktrees", 100);
        let pools: Vec<Arc<dyn ResourcePool>> = vec![short.clone(), builds.clone()];
        let relief = &relieve(&drives, &pools)[0];
        assert_eq!(relief.asked, vec![("worktrees".to_owned(), 100), ("build-output".to_owned(), 20)], "the next pool gives the rest");
        assert_eq!(relief.freed, 120);

        assert!(relieve(&[drive("/", 1_000, 300)], &pools).is_empty(), "70% used asks nothing");
    }
}
