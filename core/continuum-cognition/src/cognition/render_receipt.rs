//! The render receipt: what one submitted prompt showed her, bound to whose mind it was and
//! which request carried it (HER-TOOLS, "The render receipt"). The view takes its half when the
//! prompt is sent (`View::sent`); the deliberation faculty returns it with its contribution
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
