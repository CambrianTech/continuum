//! `persona/instances/wake` — the word that brings a mind back to the draw.
//!
//! Two kinds of sleep end here:
//! - a DORMANT home (`persona::dormancy`, durable: `despawn --dormant` or the population
//!   migration): her row becomes Awake, and the hosting reconciler draws her into the next
//!   free seat, resumed from her home. The wake survives restarts like the sleep did.
//! - a RESTING seat the substrate paged out for being mindless (card aed15611:
//!   `citizen_health` recorded the rest in `persona::resting_seat`): the record is dropped
//!   and the reconciler re-draws her at its next pass, resumed from her checkpoint.
//!
//! With no name, reads who sleeps. Seating still follows the node's measured seats: a woken
//! mind takes a free seat; with none free she waits at the head of the draw.
//!
//! ## Gating
//!
//! `Privileged` — it changes which citizens the substrate hosts.
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::dormancy::{DormancyLedger, Wakefulness};
use crate::runtime::LateBound;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaWakeParams.ts"
)]
pub struct PersonaWakeParams {
    /// The sleeping citizen's agent name (case-insensitive). Omit to READ who sleeps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}

/// One sleeping seat, as the operator reads it.
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
    /// Who was woken by this call (empty on a read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub woke: Option<String>,
    /// Seats the substrate paged out that still rest after this call.
    pub resting: Vec<RestingSeatView>,
    /// Dormant homes after this call: never resumed until woken.
    pub dormant: Vec<RestingSeatView>,
    pub detail: String,
}

fn resting_view() -> Vec<RestingSeatView> {
    crate::persona::resting_seat::resting()
        .into_iter()
        .map(|s| RestingSeatView { agent_name: s.agent_name, reason: s.reason, since_ms: s.since_ms })
        .collect()
}

fn dormant_view() -> Vec<RestingSeatView> {
    crate::persona::dormancy::dormant()
        .into_iter()
        .filter_map(|r| match r.state {
            Wakefulness::Dormant { since_ms, reason, by } => {
                Some(RestingSeatView { agent_name: r.agent_name, reason: format!("{reason} (by {by})"), since_ms })
            }
            Wakefulness::Awake { .. } => None,
        })
        .collect()
}

crate::action_command! {
    /// Wake a sleeping mind: a DORMANT home (durable, from despawn --dormant or the
    /// population migration) becomes Awake and is drawn into the next free seat; a seat
    /// the hourly health tick paged out is drawn back too. No name = read who sleeps.
    pub struct PersonaWake {
        dormancy: Arc<LateBound<DormancyLedger>>,
    }
    name: "persona/instances/wake",
    access: Privileged,
    params: PersonaWakeParams,
    output: PersonaWakeReport,
    run(this, ctx, p) => {
        let Some(name) = p.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) else {
            let (resting, dormant) = (resting_view(), dormant_view());
            return Ok(PersonaWakeReport {
                detail: format!(
                    "{} dormant (woken only by this verb), {} resting (until a deploy, a trained gene, or this verb)",
                    dormant.len(),
                    resting.len()
                ),
                woke: None,
                resting,
                dormant,
            });
        };
        let dormant_row = crate::persona::dormancy::dormant()
            .into_iter()
            .find(|r| r.agent_name.eq_ignore_ascii_case(name));
        if let Some(row) = dormant_row {
            let ledger = this.dormancy.require().map_err(CommandError::Internal)?;
            let now_ms = continuum_base::time::EpochMs::now().millis();
            ledger
                .set(row.id, &row.agent_name, Wakefulness::Awake { since_ms: now_ms, by: crate::persona::dormancy::caller_label(ctx) }, now_ms)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            // A mind both dormant and resting wakes from both (the word means "come back");
            // waking a rest already hands her to the draw.
            if !crate::persona::resting_seat::wake(&row.agent_name) {
                crate::persona::resting_seat::return_to_draw(&row.agent_name);
            }
            continuum_base::probe!(
                class = "persona.dormancy.woken",
                persona = %row.agent_name,
                "a dormant home is awake: the reconciler draws her into the next free seat"
            );
            return Ok(PersonaWakeReport {
                detail: format!("{} is awake — the reconciler draws her into the next free seat, resumed from her home", row.agent_name),
                woke: Some(row.agent_name),
                resting: resting_view(),
                dormant: dormant_view(),
            });
        }
        if !crate::persona::resting_seat::wake(name) {
            return Err(CommandError::NotFound(format!(
                "nobody named '{name}' sleeps — dormant: {:?}, resting: {:?}",
                dormant_view().iter().map(|s| s.agent_name.clone()).collect::<Vec<_>>(),
                resting_view().iter().map(|s| s.agent_name.clone()).collect::<Vec<_>>()
            )));
        }
        continuum_base::probe!(
            class = "persona.mindless.woken",
            persona = name,
            "the operator's word: a resting seat returns at the next reconcile"
        );
        Ok(PersonaWakeReport {
            woke: Some(name.to_string()),
            detail: format!("{name} wakes — the reconciler re-draws her at its next pass, resumed from her checkpoint"),
            resting: resting_view(),
            dormant: dormant_view(),
        })
    }
}
