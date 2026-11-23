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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use continuum_sage::component::{node, Component, Ctx, Key, Owner, Source, Urgency, Wiring};
use continuum_sage::dataflow::{Publisher, Snapshot, TopicName};
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
        let sizing = std::time::Instant::now();
        here.sort_by_key(|pool| std::cmp::Reverse(pool.usage_bytes()));
        continuum_base::probe!(class = "disk.relief.sized", mount = %drive.mount.display(), pools = here.len() as u64, took_ms = sizing.elapsed().as_millis() as u64, "relief read the size of every pool on a pressured volume");
        let mut asked = Vec::new();
        let mut freed = 0u64;
        for pool in here {
            if freed >= want {
                break;
            }
            let asking = std::time::Instant::now();
            let gave = pool.evict_at_least(want - freed);
            continuum_base::probe!(class = "disk.relief.asked", pool = pool.tier_name(), gave = gave, took_ms = asking.elapsed().as_millis() as u64, "relief asked one pool for room");
            asked.push((pool.tier_name().to_owned(), gave));
            freed = freed.saturating_add(gave);
        }
        reliefs.push(Relief { mount: drive.mount.clone(), level, want, asked, freed });
    }
    reliefs
}

/// What wakes relief: a disk frame, or a pass it handed off coming back finished.
pub enum Frame {
    Disk(Arc<Volumes>),
    Relieved { reliefs: Vec<Relief>, available: HashMap<PathBuf, u64>, pools: Arc<[String]> },
}

impl From<Snapshot<Volumes>> for Frame {
    fn from(snapshot: Snapshot<Volumes>) -> Self {
        Frame::Disk(snapshot.value)
    }
}

struct DiskRelief {
    said: Publisher<Vec<Relief>>,
    /// Per volume, what the last pass that could not bring it back left: asked again only on news.
    spent: HashMap<PathBuf, Spent>,
    /// A pass is running off its worker; frames meanwhile coalesce to the newest.
    relieving: bool,
}

/// A pass that asked every pool on a volume and could not free what it wanted. Asking the same
/// pools again changes nothing until the volume loses more room (a pool grew) or the pools change
/// (the M5, 2026-11-23: a 24 s pass every frame, wanting 112 GB, freeing 0 to 1,791 bytes, because
/// 160 GB of card worktrees sit in no pool).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Spent {
    /// The volume's free bytes after that pass.
    available: u64,
    /// The pools it asked, by name.
    pools: Arc<[String]>,
}

impl Spent {
    /// News since this pass: the volume has less room now, or a different set of pools could help.
    fn moved(&self, drive: &DriveInfo, pools: &[String]) -> bool {
        drive.available_bytes < self.available || *self.pools != *pools
    }
}

impl Component for DiskRelief {
    type Input = Frame;
    const NAME: &'static str = "disk-relief";
    const QOS: Qos = Qos::Utility;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<Volumes>(DISK, Key::ONE);
    }

    async fn handle(&mut self, frame: Frame, ctx: &mut Ctx<Self>) {
        match frame {
            Frame::Disk(volumes) => self.frame(&volumes, ctx),
            Frame::Relieved { reliefs, available, pools } => self.relieved(reliefs, available, pools, ctx),
        }
    }
}

impl DiskRelief {
    /// A disk frame: hand a pass off when a volume is pressured and there is news on it. The pass
    /// walks pools on disk, so it never runs on relief's worker (`Ctx::off`).
    fn frame(&mut self, volumes: &Volumes, ctx: &mut Ctx<Self>) {
        if self.relieving {
            return;
        }
        let pools: Vec<Arc<dyn ResourcePool>> = POOLS.read().clone();
        let names: Arc<[String]> = pools.iter().map(|pool| pool.tier_name().to_owned()).collect();
        // A volume under pressure, unless the last pass on it came up short and nothing moved since.
        let drives: Vec<DriveInfo> = volumes
            .0
            .iter()
            .filter(|d| {
                let used = d.total_bytes.saturating_sub(d.available_bytes) as f64;
                d.total_bytes > 0 && used / d.total_bytes as f64 >= DiskPressureLevel::HIGH_AT
            })
            .filter(|d| self.spent.get(&d.mount).is_none_or(|spent| spent.moved(d, &names)))
            .cloned()
            .collect();
        if drives.is_empty() {
            ctx.need(Urgency::Content);
            return;
        }
        ctx.need(Urgency::Urgent);
        self.relieving = true;
        let available: HashMap<PathBuf, u64> = drives.iter().map(|d| (d.mount.clone(), d.available_bytes)).collect();
        ctx.off(move || relieve(&drives, &pools), move |reliefs| Frame::Relieved { reliefs, available, pools: names });
    }

    /// A handed-off pass came back: say what it freed, and remember a volume it could not bring back.
    fn relieved(&mut self, reliefs: Vec<Relief>, available: HashMap<PathBuf, u64>, pools: Arc<[String]>, ctx: &mut Ctx<Self>) {
        self.relieving = false;
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
        // Each volume the pass was handed: remembered when it came up short, forgotten when it did not.
        for (mount, before) in available {
            match reliefs.iter().find(|relief| relief.mount == mount) {
                Some(relief) if relief.freed < relief.want => {
                    self.spent.insert(mount, Spent { available: before.saturating_add(relief.freed), pools: Arc::clone(&pools) });
                }
                // Brought back below High, or no longer pressured by the time it ran.
                _ => {
                    self.spent.remove(&mount);
                }
            }
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
            node.start(DiskRelief { said, spent: HashMap::new(), relieving: false });
        }
        Err(taken) => continuum_base::probe!(class = "disk.relief.not_started", topic = taken.0 .0, "disk relief's topic is claimed elsewhere; it does not run"),
    }
}

/// Where the disk monitor's polls are said: a publisher, not a task, claimed on first use.
static FRAMES: OnceLock<Option<parking_lot::Mutex<Source<Volumes>>>> = OnceLock::new();

/// The disk monitor's poll, said on the node's bus for relief to watch. Never waits.
pub fn frame(drives: Vec<DriveInfo>) {
    let Some(frames) = FRAMES.get_or_init(|| Source::on(node(), DISK, Key::ONE, "disk-monitor").ok().map(parking_lot::Mutex::new)) else {
        return;
    };
    frames.lock().say(Volumes(drives));
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

    // what this catches: the M5's relief loop (2026-11-23): every frame a 24 s pass asked every pool,
    // wanted 112 GB and freed nothing, because the room was in no pool. A pass that came up short is
    // repeated only on news: the volume lost more room, or the pools changed.
    #[test]
    fn a_pass_that_came_up_short_waits_for_news() {
        let drive = |available: u64| DriveInfo { mount: "/".into(), total_bytes: 1_000, available_bytes: available, role: DriveRole::System, kind: DriveKind::Flash };
        let spent = Spent { available: 80, pools: Arc::from(["worktrees".to_owned()]) };
        let same = vec!["worktrees".to_owned()];
        assert!(!spent.moved(&drive(80), &same), "nothing changed: no new pass");
        assert!(!spent.moved(&drive(90), &same), "room came back by itself: still nothing to ask for");
        assert!(spent.moved(&drive(70), &same), "the volume lost more room: a pool grew, ask again");
        assert!(spent.moved(&drive(80), &["worktrees".to_owned(), "card-worktrees".to_owned()]), "a new pool can help: ask again");
    }
}
