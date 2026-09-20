//! `persona/placement/reserve` — a spiller asks THIS seat for a slot before it moves a
//! mind here (card c84d885a, S1b; [`crate::persona::placement_reservation`]).
//!
//! The responder knows its queue; the requester does not. The seat answers from its own
//! live numbers — served lanes, everything in flight, grants already outstanding, and its
//! measured leased-in wait — and holds the slot for one placement cooldown. Called over
//! airc by another node's placement tick, the same envelope and reply path as a leased
//! `ai/generate`; callable locally to read what the seat would say.
//!
//! ## Gating
//!
//! `Privileged`, like `ai/generate` — it commits this seat's capacity to another node.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::persona::placement_reservation::PlacementReservationReport;

/// Who asks, for whom.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PlacementReserveParams.ts"
)]
pub struct PlacementReserveParams {
    /// The mind the asking node wants to run on this seat.
    #[ts(type = "string")]
    pub mind: Uuid,
    /// The asking node's peer id — recorded as the grant's holder.
    #[ts(type = "string")]
    pub requester: Uuid,
}

crate::action_command! {
    /// Ask this seat to hold one serving slot for a mind another node is about to move
    /// here. Answers from the seat's OWN live truth: lanes minus everything in flight
    /// minus slots already granted, plus its measured leased-in wait with the sample
    /// count. A grant lasts one placement cooldown; a re-ask for the same mind renews it.
    pub struct PersonaPlacementReserve;
    name: "persona/placement/reserve",
    access: Privileged,
    params: PlacementReserveParams,
    output: PlacementReservationReport,
    run(_this, _ctx, p) => {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0); // JUSTIFIED unwrap_or: a clock before the epoch grants nothing that can lapse — the ledger expires by the same clock
        Ok(crate::persona::placement_reservation::grant(p.mind, p.requester, now_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the wire name a spiller's tick asks by (`RESERVE_PATH`) and the
    // verb's registered name are ONE string — renaming either strands every spill on a
    // "unknown command" refusal; and it stays Privileged like the generate it guards.
    #[test]
    fn the_reserve_verb_answers_under_the_name_the_spiller_asks_by() {
        assert_eq!(PersonaPlacementReserve::NAME, crate::persona::placement_reservation::RESERVE_PATH);
        assert_eq!(PersonaPlacementReserve::ACCESS, AccessLevel::Privileged);
    }
}
