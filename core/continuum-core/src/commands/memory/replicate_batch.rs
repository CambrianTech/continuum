//! `memory/replicate-batch` — persona-RAID slice 2's RECEIVING side.
//!
//! A peer's shipper posts a tail of its persona journal here (over the same
//! inbound command-RPC that answers any command); this node appends it to its
//! replica cold store (`~/.continuum/replicas/<persona>/journal.jsonl`) and
//! acks the high-water seq. Idempotent by (origin_node, seq) — the shipper
//! retries blindly on a lost ack. Design:
//! docs/architecture/PERSONA-RAID-WRITE-BEHIND.md.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::memory::replication::{replica_append_batch, JournalEntry};
use crate::sdk_codegen::CommandError;

/// Params for `memory/replicate-batch`. Wire keys are snake_case.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryReplicateBatchParams.ts"
)]
pub struct MemoryReplicateBatchParams {
    /// Whose memory this is (the persona being replicated).
    pub persona_id: String,
    /// A contiguous tail of the origin node's journal, ascending seq, ONE
    /// origin_node across the batch.
    pub entries: Vec<JournalEntry>,
}

/// Ack: the receiver's high-water for (persona, origin) after the append.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryReplicateBatchResult.ts"
)]
pub struct MemoryReplicateBatchResult {
    /// Highest seq durably held for this (persona, origin) — the shipper's
    /// next tail starts after this.
    pub acked_seq: u64,
}

crate::action_command! {
    /// Accept a replica batch of another node's persona journal into this
    /// node's cold store. Privileged: grid peers reach it through the
    /// command-RPC pump; it writes only under `~/.continuum/replicas/`.
    pub struct MemoryReplicateBatch;
    name: "memory/replicate-batch",
    access: Privileged,
    params: MemoryReplicateBatchParams,
    output: MemoryReplicateBatchResult,
    run(_this, _ctx, p) => {
        let acked = replica_append_batch(&p.persona_id, &p.entries)
            .map_err(|e| CommandError::Invalid(format!("memory/replicate-batch: {e}")))?;
        Ok(MemoryReplicateBatchResult { acked_seq: acked })
    }
}
