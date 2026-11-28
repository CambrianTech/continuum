//! Each mind's decode graph kept across a switch (the fork's llama-graph-slots.h). A scheduler holds the split and
//! allocation of one graph, so a context serving several minds rebuilt, re-split and re-allocated its graph at every
//! switch (0 of 74 steps reused in a live turn on the M5, 2026-11-28). The fork keeps one graph per mind, in her own
//! slot; its own reuse check decides whether her kept graph serves her step.

use super::Context;
use crate::sys;

impl Context<'_> {
    /// Keep a built graph for up to `minds` minds: the owner passes the minds it keeps resident. 0 keeps none.
    pub fn keep_graphs(&mut self, minds: usize) {
        // SAFETY: called between decodes, on the context's own thread; it only sizes the context's slot table.
        unsafe { sys::llama_set_graph_slots(self.ptr.as_ptr(), minds) }
    }

    /// Name whom sequence `seq` serves now, so her steps find her own kept graph.
    pub fn name_graph_owner(&mut self, seq: i32, owner: u64) {
        // SAFETY: as keep_graphs; it records one name in the context's slot table.
        unsafe { sys::llama_set_graph_slot_key(self.ptr.as_ptr(), seq, owner) }
    }

    /// The compute buffer bytes the kept graphs hold beside the context's own.
    pub fn kept_graph_bytes(&self) -> usize {
        // SAFETY: a read of the context's own slot table.
        unsafe { sys::llama_graph_slots_bytes(self.ptr.as_ptr()) }
    }
}
