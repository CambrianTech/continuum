//! The render receipt: what one submitted prompt showed her, bound to whose mind it was and
//! which request carried it (HER-TOOLS, "The render receipt"). The view takes its half when the
//! prompt is sent (`View::sent`); the deliberation faculty binds it and hands it to the one sink
//! where the request was served, so a prompt the engine never accepted is never receipted.
//! The task ledger (#5585) is the sink: it acknowledges a task's end from the first receipt that
//! shows that task's item, and writes nothing else. The memory incarnation joins the binding
//! with that ledger's `WorkingMemory::incarnation`.

use uuid::Uuid;

/// One submitted, served prompt: whose, which request, and what it showed her in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderReceipt {
    pub owner: Uuid,
    /// The memory that supplied the prompt, never the registry's current instance
    /// looked up after generation. A replaced resident cannot consume this receipt.
    pub incarnation: Uuid,
    pub request: String,
    pub view: continuum_context::view::Receipt,
}

/// Where render receipts go. One per mind, installed at assembly ([`install`]); none installed,
/// a receipt is dropped (nothing consumes it yet).
pub trait RenderReceiptSink: Send + Sync {
    fn record(&self, receipt: RenderReceipt);
}

static SINKS: parking_lot::RwLock<Option<std::collections::HashMap<Uuid, std::sync::Arc<dyn RenderReceiptSink>>>> =
    parking_lot::RwLock::new(None);

/// Install the one sink `owner`'s receipts go to (the task ledger, #5585). A second install for
/// the same mind replaces the first.
pub fn install(owner: Uuid, sink: std::sync::Arc<dyn RenderReceiptSink>) {
    SINKS.write().get_or_insert_with(Default::default).insert(owner, sink);
}

/// Hand a served prompt's receipt to its owner's sink, if one is installed.
pub(crate) fn deliver(receipt: RenderReceipt) {
    let sink = SINKS.read().as_ref().and_then(|sinks| sinks.get(&receipt.owner).cloned());
    if let Some(sink) = sink {
        sink.record(receipt);
    }
}
