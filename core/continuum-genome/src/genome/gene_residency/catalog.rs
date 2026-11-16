//! The node's one owner of which genes the serving engine holds (CONTINUUM-ENGINE 4.10 P4a), the tier
//! above [`super::GeneResidency`]'s paging: a gene joins through the seat path every learner uses
//! (measured, leased into custody, registered as a residency, loaded inside it) and leaves bound to
//! the engine identity the change was read under. The engine decides what changes and asks here
//! through [`GenomeCatalog`]; nothing else mutates its catalog.

use std::collections::HashMap;
use std::sync::Arc;

use crate::genome::learner::seat::{seat_gene, Authority, Seat, SeatRefusal};
use crate::inference::llama_server::genome_catalog::{
    CatalogChange, CatalogEnd, CatalogOutcome, CatalogRefusal, CatalogStep, GenomeCatalog,
};
use crate::inference::llama_server::{AdapterChangeRefusal, LlamaServerError};

/// Every gene this core loaded into a serving engine, by gene path, with the seat that pays for it.
pub struct GeneCatalog {
    authority: Authority,
    seats: tokio::sync::Mutex<HashMap<String, Seat>>,
}

impl GeneCatalog {
    pub fn new(authority: Authority) -> Arc<Self> {
        Arc::new(Self { authority, seats: tokio::sync::Mutex::new(HashMap::new()) })
    }
}

fn ended(applied: Vec<CatalogStep>, end: CatalogEnd) -> CatalogOutcome {
    CatalogOutcome { applied, end }
}

#[async_trait::async_trait]
impl GenomeCatalog for GeneCatalog {
    async fn converge(&self, change: CatalogChange) -> CatalogOutcome {
        let CatalogChange { engine, ledger, retire, add } = change;
        let mut seats = self.seats.lock().await;
        let mut applied = Vec::new();
        // Retire first, so a busy retire (a turn holds the gene) stops the change before any gene joins.
        for gone in retire {
            let unloaded = match seats.remove(&gone.gene) {
                Some(seat) if seat.ledger == ledger => match seat.unload().await {
                    Ok(()) => {
                        // an unconfirmed release leaves the residency's leases in custody, orphaned, for its owner
                        let _released = seat.release().await;
                        Ok(())
                    }
                    Err(error) => {
                        seats.insert(gone.gene.clone(), seat);
                        Err(error)
                    }
                },
                // Loaded at launch, or seated on an engine since replaced (its custody settles that
                // residency): unload the id this engine lists, bound to this engine.
                _ => engine.unseat(&ledger, gone.id).await.map(|_| ()),
            };
            match unloaded {
                Ok(()) => applied.push(CatalogStep::Retired(gone.gene)),
                Err(LlamaServerError::AdapterChange(AdapterChangeRefusal::Busy)) => return ended(applied, CatalogEnd::Busy),
                Err(error) => return ended(applied, CatalogEnd::Refused(CatalogRefusal::Refused(error.to_string()))),
            }
        }
        for gene in add {
            let consumer = format!("gene:{}", gene.gene);
            match seat_gene(&self.authority, engine.clone(), &ledger, &consumer, &gene.name).await {
                Ok(seat) => {
                    seats.insert(gene.gene.clone(), seat);
                    applied.push(CatalogStep::Added(gene.gene));
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
