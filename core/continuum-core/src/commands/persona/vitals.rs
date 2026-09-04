//! `persona/vitals` — a citizen's latest vitals map, from the ONE emitter that
//! also drives the roster tiles and the brain HUD (`persona:vitals`). A page
//! opened cold (deep link to a citizen who is not in the viewer's focused
//! room) has no roster slot to read from; this verb answers the same map.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaVitalsParams.ts")]
pub struct PersonaVitalsParams {
    /// The persona's peer id.
    #[serde(rename = "personaId")]
    #[ts(type = "string")]
    pub persona_id: Uuid,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaVitalsResult.ts")]
pub struct PersonaVitalsResult {
    #[serde(rename = "personaId")]
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Normalized 0..=100 readouts keyed by axis (`reason`, `recall`, `act`,
    /// `speed`, `focus`, `activity`, `queue`, …). Empty = nothing sampled yet.
    #[ts(type = "Record<string, number>")]
    pub vitals: BTreeMap<String, u8>,
    /// Age of the sample in ms; absent when nothing was sampled.
    #[serde(rename = "sampledAgoMs")]
    #[ts(optional, type = "number")]
    pub sampled_ago_ms: Option<u64>,
}

crate::action_command! {
    /// The latest vitals map for one citizen — the same pulse the roster and
    /// the brain HUD draw, readable by id from any page.
    pub struct PersonaVitals;
    name: "persona/vitals",
    access: AiSafe,
    params: PersonaVitalsParams,
    output: PersonaVitalsResult,
    run(_this, _ctx, p) => {
        let id = p.persona_id;
        let (vitals, age) = crate::ipc::vitals_emitter::last_vitals(id)
            .map(|(v, a)| (v, Some(a.as_millis() as u64)))
            .unwrap_or_default(); // unwrap_or: nothing sampled yet = an honest empty map, never a fabricated pulse
        Ok(PersonaVitalsResult { persona_id: id, vitals, sampled_ago_ms: age })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: an unknown citizen answers an honest empty map, never a
    // fabricated pulse (a malformed id is refused at deserialization — the id is
    // typed, never text).
    #[tokio::test]
    async fn answers_empty_for_an_unsampled_citizen() {
        let cmd = PersonaVitals;
        let out = cmd
            .run(&Ctx::default(), PersonaVitalsParams { persona_id: Uuid::from_u128(0x5151) })
            .await
            .unwrap();
        assert!(out.vitals.is_empty() && out.sampled_ago_ms.is_none());
        assert_eq!(PersonaVitals::NAME, "persona/vitals");
    }
}
