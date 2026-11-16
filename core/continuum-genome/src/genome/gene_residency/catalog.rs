//! The node's one owner of which genes the serving engine holds (CONTINUUM-ENGINE 4.10 P4a), the tier
//! above [`super::GeneResidency`]'s paging: a gene joins through the seat path every learner uses
//! (measured, leased into custody, registered as a residency, loaded inside it) and leaves bound to
//! the engine identity the change was read under. The engine decides what changes and asks here
//! through [`GenomeCatalog`]; nothing else mutates its catalog.

use std::collections::HashMap;
use std::sync::Arc;

use crate::genome::learner::seat::{seat_gene, Authority, GeneLoad, Seat, SeatRefusal};
use crate::inference::llama_server::genome_catalog::{
    CatalogChange, CatalogEnd, CatalogOutcome, CatalogRefusal, CatalogStep, GenomeCatalog,
};
use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::llama_server::{AdapterChangeRefusal, LlamaServerError};

/// One gene on one engine: the same gene loaded on two engines is two seats, each paying for its
/// own copy, so retiring it from one never touches the other (Codex on #6053).
type SeatKey = (EngineIncarnation, String);

/// Every gene this core loaded into a serving engine, by engine and gene path, with the seat that
/// pays for it.
pub struct GeneCatalog {
    authority: Authority,
    seats: tokio::sync::Mutex<HashMap<SeatKey, Seat>>,
}

impl GeneCatalog {
    pub fn new(authority: Authority) -> Arc<Self> {
        Arc::new(Self { authority, seats: tokio::sync::Mutex::new(HashMap::new()) })
    }

    /// Whether this core holds a seat for `gene` on `engine`.
    pub async fn holds(&self, engine: EngineIncarnation, gene: &str) -> bool {
        self.seats.lock().await.contains_key(&(engine, gene.to_owned()))
    }
}

fn ended(applied: Vec<CatalogStep>, end: CatalogEnd) -> CatalogOutcome {
    CatalogOutcome { applied, end }
}

#[async_trait::async_trait]
impl GenomeCatalog for GeneCatalog {
    async fn converge(&self, change: CatalogChange<'_>) -> CatalogOutcome {
        let CatalogChange { operation, engine, ledger, retire, add } = change;
        let incarnation = engine.process();
        let mut seats = self.seats.lock().await;
        let mut applied = Vec::new();
        // Retire first, so a busy retire (a turn holds the gene) stops the change before any gene joins.
        for gone in retire {
            let key = (incarnation, gone.gene.clone());
            let unloaded = match seats.remove(&key) {
                Some(seat) if seat.ledger == ledger => match seat.unload().await {
                    Ok(()) => {
                        // an unconfirmed release leaves the residency's leases in custody, orphaned, for its owner
                        let _released = seat.release().await;
                        Ok(())
                    }
                    Err(error) => {
                        seats.insert(key, seat);
                        Err(error)
                    }
                },
                // This incarnation answers a different ledger: the seat's residency is not this
                // engine's to settle here; it stays held, and the change refuses.
                Some(seat) => {
                    seats.insert(key, seat);
                    return ended(applied, CatalogEnd::Refused(CatalogRefusal::EngineReplaced));
                }
                // Loaded at launch, with no seat of ours: unload the id this engine lists, bound to it.
                None => engine.unseat(&ledger, gone.id).await.map(|_| ()),
            };
            match unloaded {
                Ok(()) => applied.push(CatalogStep::Retired(gone.gene)),
                Err(LlamaServerError::AdapterChange(AdapterChangeRefusal::Busy)) => return ended(applied, CatalogEnd::Busy),
                Err(error) => return ended(applied, CatalogEnd::Refused(CatalogRefusal::Refused(error.to_string()))),
            }
        }
        for gene in add {
            let consumer = format!("gene:{}", gene.gene);
            let load = GeneLoad { operation, engine: engine.clone(), bound: &ledger, consumer: &consumer, name: &gene.name };
            match seat_gene(&self.authority, load).await {
                Ok(seat) => {
                    let key = (incarnation, gene.gene);
                    applied.push(CatalogStep::Added(key.1.to_owned()));
                    seats.insert(key, seat);
                }
                Err(SeatRefusal::EngineReplaced) => return ended(applied, CatalogEnd::Refused(CatalogRefusal::EngineReplaced)),
                Err(SeatRefusal::AwaitingMemory(wait)) => return ended(applied, CatalogEnd::Refused(CatalogRefusal::AwaitingMemory(wait.to_string()))),
                Err(other) => return ended(applied, CatalogEnd::Refused(CatalogRefusal::Refused(other.to_string()))),
            }
        }
        ended(applied, CatalogEnd::Done)
    }
}

#[cfg(test)]
mod tests;
