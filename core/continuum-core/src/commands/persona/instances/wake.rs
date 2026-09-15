//! `persona/instances/wake` — the operator's word to a RESTING seat.
//!
//! The substrate pages a mindless seat out (card aed15611: her hour was recitals and
//! gate-passes; `citizen_health` flushed her checkpoint and recorded the rest in
//! `persona::resting_seat`). She returns on a CHANGE — a deploy, a trained gene —
//! or on this word: the record is dropped and the hosting reconciler re-draws her
//! at its next pass, resuming her from her checkpoint. With no name, reads who rests.
//!
//! ## Gating
//!
//! `Privileged` — it changes which citizens the substrate hosts.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaWakeParams.ts"
)]
pub struct PersonaWakeParams {
    /// The resting citizen's agent name (case-insensitive). Omit to READ who rests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}

/// One resting seat, as the operator reads it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RestingSeatView.ts"
)]
pub struct RestingSeatView {
    pub agent_name: String,
    pub reason: String,
    #[ts(type = "number")]
    pub since_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaWakeReport.ts"
)]
pub struct PersonaWakeReport {
    /// Who was woken by this call (empty on a read, or when nobody by that name rested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub woke: Option<String>,
    /// Who still rests after this call.
    pub resting: Vec<RestingSeatView>,
    pub detail: String,
}

crate::action_command! {
    /// Wake a seat the substrate paged out for being mindless (the hourly health
    /// tick's page-out): drops her resting record so the reconciler re-draws her at
    /// its next pass, resumed from her checkpoint. No name = read who rests.
    pub struct PersonaWake;
    name: "persona/instances/wake",
    access: Privileged,
    params: PersonaWakeParams,
    output: PersonaWakeReport,
    run(_this, _ctx, p) => {
        let view = |seats: Vec<crate::persona::resting_seat::RestingSeat>| -> Vec<RestingSeatView> {
            seats
                .into_iter()
                .map(|s| RestingSeatView { agent_name: s.agent_name, reason: s.reason, since_ms: s.since_ms })
                .collect()
        };
        let Some(name) = p.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) else {
            let resting = view(crate::persona::resting_seat::resting());
            return Ok(PersonaWakeReport {
                detail: if resting.is_empty() {
                    "nobody is resting — every seat the substrate can host is hostable".into()
                } else {
                    format!("{} seat(s) resting until a deploy, a trained gene, or this verb with --name", resting.len())
                },
                woke: None,
                resting,
            });
        };
        if !crate::persona::resting_seat::wake(name) {
            let resting = view(crate::persona::resting_seat::resting());
            return Err(CommandError::NotFound(format!(
                "no resting seat named '{name}' — resting: {:?}",
                resting.iter().map(|s| s.agent_name.as_str()).collect::<Vec<_>>()
            )));
        }
        crate::probe!(
            class = "persona.mindless.woken",
            persona = name,
            "the operator's word: a resting seat returns at the next reconcile"
        );
        let resting = view(crate::persona::resting_seat::resting());
        Ok(PersonaWakeReport {
            woke: Some(name.to_string()),
            detail: format!("{name} wakes — the reconciler re-draws her at its next pass, resumed from her checkpoint"),
            resting,
        })
    }
}
