//! Where a node's loaded genes live: on the device, or paged out to host RAM (the LOADED tier of
//! GENE-REUSE-FORK-MINT §3d; the mechanism is fork CambrianTech/llama.cpp#72). One owner per node:
//! one adapter serves every mind on the engine, so residency is the node's, never a mind's.
//!
//! - **No budget of its own.** Genes compete with minds' KV for one device, so the owner is a
//!   [`DeviceTenant`] of the lanes' pool (`inference-lanes`): its device bytes count there, and
//!   the PressureBroker's `evict_at_least` reaches it first (a gene pages out in milliseconds, a
//!   lane's eviction costs a mind her KV). It pages OUT only when asked to relieve that budget.
//! - **Temperature: LRU-2.** A gene's heat is the time of its SECOND most recent use by any mind
//!   (then its last use): a gene several minds or several turns run outranks one a single turn
//!   touched, recency decides among equals, and there is no decay constant to tune. A gene in any
//!   mind's current phenotype is pinned: it is never paged out while she runs it.
//! - **Prefetch in the quiet.** After each turn ([`GeneResidency::record_turn`]) a reconcile runs
//!   off the request path and pages back in every pinned gene that is in host RAM, so the next
//!   request does not pay the engine's on-demand page-in (a counted miss,
//!   `lora_page_in_on_demand_total`).
//! - **Each mind's phenotype lives here.** It is what pins her genes, and the composer's
//!   hysteresis and held scales read it back ([`GeneResidency::previous`]): one copy.
//!
//! Probes: `genome.page` (gene, direction, bytes, ms) and `genome.page.failed`.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use uuid::Uuid;

use crate::ai::types::ActiveAdapterRequest;
use crate::genome::composer::Phenotype;
use crate::inference::coordinator_pool::DeviceTenant;
use crate::inference::llama_server::{gene_seats, page_gene, GeneSeat};
use crate::paging::pool::ResourcePoolEntry;

/// When a gene ran (ms since the epoch): its last and second-last use, and how often.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Heat {
    pub last_ms: u64,
    pub prior_ms: u64,
    pub uses: u64,
}

impl Heat {
    fn touch(&mut self, now_ms: u64) {
        if now_ms != self.last_ms {
            self.prior_ms = self.last_ms;
            self.last_ms = now_ms;
        }
        self.uses += 1;
    }

    /// The LRU-2 order key: hotter is greater.
    fn key(&self) -> (u64, u64) {
        (self.prior_ms, self.last_ms)
    }
}

/// One page the owner decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageMove {
    pub id: i64,
    pub path: String,
    pub to_device: bool,
}

/// PURE: the prefetch: every pinned gene that is in host RAM goes back to the device.
pub fn prefetch(seats: &[GeneSeat], pinned: &BTreeSet<String>) -> Vec<PageMove> {
    seats
        .iter()
        .filter(|s| !s.on_device && pinned.contains(&s.path))
        .map(|s| PageMove { id: s.id, path: s.path.clone(), to_device: true })
        .collect()
}

/// PURE: the victims for relieving `want` bytes: unpinned genes on the device with known bytes,
/// coldest first (LRU-2), until `want` is covered. Returns them and the bytes they free.
pub fn victims(
    seats: &[GeneSeat],
    pinned: &BTreeSet<String>,
    heat: &HashMap<String, Heat>,
    want: u64,
) -> (Vec<PageMove>, u64) {
    let mut cold: Vec<(&GeneSeat, u64)> = seats
        .iter()
        .filter(|s| s.on_device && !pinned.contains(&s.path))
        .filter_map(|s| s.n_bytes.map(|b| (s, b)))
        .collect();
    cold.sort_by_key(|(s, _)| (heat.get(&s.path).copied().unwrap_or_default().key(), s.path.clone())); // unwrap_or_default: a gene no turn has run since this process started is the coldest there is
    let mut freed = 0u64;
    let mut moves = Vec::new();
    for (seat, bytes) in cold {
        if freed >= want {
            break;
        }
        freed += bytes;
        moves.push(PageMove { id: seat.id, path: seat.path.clone(), to_device: false });
    }
    (moves, freed)
}

#[derive(Default)]
struct State {
    /// Each mind's current phenotype: pins its genes, and is the composer's "previous".
    phenotypes: HashMap<Uuid, Vec<ActiveAdapterRequest>>,
    heat: HashMap<String, Heat>,
    /// The engine's catalog as last read.
    seats: Vec<GeneSeat>,
}

impl State {
    fn pinned(&self) -> BTreeSet<String> {
        self.phenotypes.values().flatten().filter(|g| g.scale != 0.0).map(|g| g.path.clone()).collect()
    }
}

/// The node's gene residency owner. See the module docs.
#[derive(Default)]
pub struct GeneResidency {
    state: Mutex<State>,
    reconciling: AtomicBool,
    again: AtomicBool,
}

impl GeneResidency {
    pub fn global() -> &'static Arc<GeneResidency> {
        static ONE: std::sync::OnceLock<Arc<GeneResidency>> = std::sync::OnceLock::new();
        ONE.get_or_init(|| Arc::new(GeneResidency::default()))
    }

    /// What `persona` ran last turn (empty when she has run nothing this process).
    pub fn previous(&self, persona: Uuid) -> Vec<ActiveAdapterRequest> {
        self.state.lock().phenotypes.get(&persona).cloned().unwrap_or_default() // unwrap_or_default: a mind with no turn this process ran no genes
    }

    /// `persona` runs `phenotype` from now: it pins her genes and heats each one.
    pub fn record_turn(&self, persona: Uuid, phenotype: &Phenotype, now_ms: u64) {
        let mut state = self.state.lock();
        for gene in &phenotype.genes {
            state.heat.entry(gene.path.clone()).or_default().touch(now_ms);
        }
        if phenotype.genes.is_empty() {
            state.phenotypes.remove(&persona);
        } else {
            state.phenotypes.insert(persona, phenotype.genes.clone());
        }
    }

    /// Bring the engine's catalog in and page back in what her minds run, off the request path.
    /// Coalesced: a reconcile asked for while one runs runs once more after it.
    pub fn schedule_reconcile(self: &Arc<Self>) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return; // no runtime (a sync test): nothing to page, and the engine pages in on demand
        };
        if self.reconciling.swap(true, Ordering::AcqRel) {
            self.again.store(true, Ordering::Release);
            return;
        }
        let me = Arc::clone(self);
        runtime.spawn(async move {
            loop {
                me.reconcile().await;
                if !me.again.swap(false, Ordering::AcqRel) {
                    break;
                }
            }
            me.reconciling.store(false, Ordering::Release);
        });
    }

    async fn reconcile(&self) {
        let seats = match gene_seats().await {
            Ok(seats) => seats,
            Err(error) => {
                continuum_base::probe!(
                    class = "genome.page.failed",
                    error = %error,
                    "the engine's gene catalog did not read: residency unchanged"
                );
                return;
            }
        };
        let moves = {
            let mut state = self.state.lock();
            state.seats = seats;
            prefetch(&state.seats, &state.pinned())
        };
        self.apply(moves).await;
    }

    async fn apply(&self, moves: Vec<PageMove>) {
        for m in moves {
            match page_gene(m.id, m.to_device).await {
                Ok(done) => {
                    continuum_base::probe!(
                        class = "genome.page",
                        gene = %m.path,
                        to = if done.on_device { "device" } else { "host" },
                        bytes = done.n_moved,
                        ms = done.t_ms,
                        "a gene moved between the device and host RAM"
                    );
                    let mut state = self.state.lock();
                    if let Some(seat) = state.seats.iter_mut().find(|s| s.id == m.id) {
                        seat.on_device = done.on_device;
                        seat.n_bytes = Some(done.n_bytes);
                    }
                }
                Err(error) => continuum_base::probe!(
                    class = "genome.page.failed",
                    gene = %m.path,
                    to = if m.to_device { "device" } else { "host" },
                    error = %error,
                    "a gene page did not happen: it stays where it was"
                ),
            }
        }
    }
}

impl DeviceTenant for GeneResidency {
    fn device_bytes(&self) -> u64 {
        self.state.lock().seats.iter().filter(|s| s.on_device).filter_map(|s| s.n_bytes).sum()
    }

    /// Decides the coldest unpinned genes now and pages them out off the broker's thread; the
    /// bytes returned are what those pages free (a refused one is probed and its gene stays).
    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return 0; // no runtime to page on: this tenant frees nothing, and the broker moves on to the lanes
        };
        let (moves, freed) = {
            let mut state = self.state.lock();
            let pinned = state.pinned();
            let (moves, freed) = victims(&state.seats, &pinned, &state.heat, want_bytes);
            for m in &moves {
                if let Some(seat) = state.seats.iter_mut().find(|s| s.id == m.id) {
                    seat.on_device = false; // committed: no second eviction counts it again
                }
            }
            (moves, freed)
        };
        if !moves.is_empty() {
            // the tenant registered with the pool IS the global owner (register_with_broker)
            let me = Arc::clone(GeneResidency::global());
            runtime.spawn(async move { me.apply(moves).await });
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        let state = self.state.lock();
        let pinned = state.pinned();
        state
            .seats
            .iter()
            .filter(|s| s.on_device)
            .map(|s| {
                let heat = state.heat.get(&s.path).copied().unwrap_or_default(); // unwrap_or_default: no turn this process ran it
                ResourcePoolEntry {
                    key: format!("gene:{}", s.path),
                    size_bytes: s.n_bytes.unwrap_or(0), // unwrap_or: an engine without byte reporting; the entry still names the gene
                    pinned_count: u32::from(pinned.contains(&s.path)),
                    loaded_at: 0, // the engine does not report when it loaded a gene; 0 reads as unknown on the dashboard
                    last_access_at: heat.last_ms,
                    access_count: heat.uses,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
