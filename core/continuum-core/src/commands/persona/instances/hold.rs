//! `persona/instances/hold` — the reconciler-respecting quiesce
//! ([[despawn-is-not-quiesce-a-reconciler-resurrects-citizens-continuously]]).
//!
//! Despawn records "not now"; the hosting reconciler (rightly) re-adopts
//! on-disk citizens on every serving edge, so a despawn alone dissolves in
//! minutes. A HOLD records "not until I say": a persisted, expiring
//! allow-list the reconciler consults before adopting
//! (`persona::roster_hold`, checked in `spawn_all`). Setting a hold does NOT
//! despawn anyone — despawn the extras once after setting it; the reconciler
//! then stops undoing you. An explicit `persona/spawn` stays sovereign over
//! the hold (a human's direct command beats standing intent).
//!
//! ## Gating
//!
//! `Privileged` — it gates which citizens the substrate will host.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::CommandError;

/// Set, clear, or read the standing roster hold.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RosterHoldParams.ts"
)]
pub struct RosterHoldParams {
    /// Agent names allowed to be hosted while the hold stands (e.g. ["Atlas"]).
    /// Omit together with `clear` to just READ the current hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub only: Option<Vec<String>>,
    /// How long the hold stands, in minutes (clamped to 1..=1440). Required
    /// when `only` is given — an unbounded hold is a mystery outage waiting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub minutes: Option<u64>,
    /// Why — carried into every skip probe so a held-down fleet explains itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// Remove any standing hold.
    #[serde(default)]
    pub clear: bool,
}

/// The hold's state after this call.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RosterHoldReport.ts"
)]
pub struct RosterHoldReport {
    /// Whether a hold now stands.
    pub active: bool,
    /// Allowed agent names while it stands (empty when inactive).
    pub only: Vec<String>,
    /// Unix ms when the hold lapses (0 when inactive).
    #[ts(type = "number")]
    pub until_ms: u64,
    /// The recorded reason (empty when inactive).
    pub reason: String,
    /// Human-readable summary of what this call did.
    pub detail: String,
}

crate::action_command! {
    /// Reconciler-respecting quiesce: a persisted, EXPIRING allow-list of
    /// citizens the hosting reconciler may adopt. Set it before a measurement
    /// window (then despawn the extras once — they stay down); it survives
    /// reboots and self-expires. `--clear` lifts it; calling with no params
    /// reads the current state. Explicit persona/spawn is not gated.
    pub struct PersonaRosterHold;
    name: "persona/instances/hold",
    access: Privileged,
    params: RosterHoldParams,
    output: RosterHoldReport,
    run(_this, _ctx, p) => {
        if p.clear {
            let existed = crate::persona::roster_hold::clear();
            return Ok(RosterHoldReport {
                active: false,
                only: vec![],
                until_ms: 0,
                reason: String::new(),
                detail: if existed {
                    "hold cleared — the reconciler hosts the full roster again".into()
                } else {
                    "no hold was standing".into()
                },
            });
        }
        if let Some(only) = p.only.clone() {
            let minutes = p.minutes.ok_or_else(|| {
                CommandError::Invalid(
                    "setting a hold requires `minutes` (1..=1440) — an unbounded hold \
                     is a standing outage, not a measurement window"
                        .to_string(),
                )
            })?;
            let hold = crate::persona::roster_hold::set(
                only,
                minutes,
                p.reason.clone().unwrap_or_else(|| "operator hold".to_string()), // reason is prose for probes; a labeled default is honest, nothing budgets on it
            )
            .map_err(CommandError::Invalid)?;
            return Ok(RosterHoldReport {
                active: true,
                detail: format!(
                    "hold set — the reconciler hosts ONLY {:?} until the hold lapses. \
                     Despawn the extras once; they will stay down.",
                    hold.only
                ),
                only: hold.only,
                until_ms: hold.until_ms,
                reason: hold.reason,
            });
        }
        // Read.
        match crate::persona::roster_hold::active() {
            Some(h) => Ok(RosterHoldReport {
                active: true,
                detail: format!("hold standing: only {:?} ({})", h.only, h.reason),
                only: h.only,
                until_ms: h.until_ms,
                reason: h.reason,
            }),
            None => Ok(RosterHoldReport {
                active: false,
                only: vec![],
                until_ms: 0,
                reason: String::new(),
                detail: "no hold standing — the reconciler hosts the full roster".into(),
            }),
        }
    }
}
