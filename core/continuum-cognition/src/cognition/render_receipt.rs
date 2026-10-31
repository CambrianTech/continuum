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
    pub request: String,
    pub view: continuum_context::view::Receipt,
}

/// Where render receipts go. One per mind, installed at assembly; none installed, a receipt is
/// dropped (nothing consumes it yet).
pub trait RenderReceiptSink: Send + Sync {
    fn record(&self, receipt: RenderReceipt);
}
