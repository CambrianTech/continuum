//! `persona/roster` — the live-citizen roster read verb.
//!
//! Closes the observability gap that made a `benchmark/dispatch` hard to verify from the
//! CLI: there was no way to ask "who is online on THIS machine, and what work is staged in
//! their workspaces?" ([[the-grid-identity-spine-durable-id-fluid-location]] §"live-persona
//! roster command", #396 third bullet). It is a thin projection of
//! [`PersonaAircRuntimeRegistry::roster_snapshot`] — the SAME snapshot `benchmark/dispatch`
//! resolves its assignees against — plus, per citizen, the SWE instances already staged in
//! her workspace. That second column is what distinguishes a fresh clone from a REUSE: an
//! instance dir with `.git` means dispatch found it already staged and skipped the clone.
//!
//! Read-only. No board write, no airc publish — it reports the roster the substrate already
//! holds, so a persona home that the registry never registered simply does not appear.

use std::any::Any;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::benchmark::continuum_home;
use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaRosterParams.ts")]
pub struct PersonaRosterParams {}

/// One live citizen's row.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaRosterEntry.ts")]
pub struct PersonaRosterEntry {
    /// The citizen's airc agent_name — the handle `benchmark/dispatch --assignees` resolves.
    pub agent_name: String,
    /// Her durable persona-airc peer_id (the id the reuse seam addresses her by).
    pub peer_id: String,
    /// SWE instances already staged in her workspace (`workspace/swe/<id>` with a `.git`).
    /// Non-empty here is the REUSE signal: dispatch found the checkout and skipped cloning.
    pub staged_swe: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaRosterResult.ts")]
pub struct PersonaRosterResult {
    /// How many citizens are online right now (the roster `benchmark/dispatch` targets when
    /// `--assignees` is omitted). Zero means dispatch would be Denied — spawn a persona.
    #[ts(type = "number")]
    pub count: u32,
    /// Every live citizen, sorted by name (the stable round-robin order).
    pub citizens: Vec<PersonaRosterEntry>,
}

/// The roster read verb. Holds the live registry (constructed where the registry lives).
pub struct PersonaRoster {
    pub registry: PersonaAircRuntimeRegistry,
}

/// List the SWE instances staged under one peer's workspace — the dirs the dispatch
/// staging keystone writes (`<home>/citizens/peers/<peer>/workspace/swe/<id>`), counting
/// only those that actually carry a `.git` (a real checkout, not an empty shell). Best
/// effort: a missing home or unreadable dir yields an empty list, never an error — the
/// roster read must not fail because one citizen's workspace isn't there yet.
fn staged_swe_for(peer: &uuid::Uuid) -> Vec<String> {
    let Ok(home) = continuum_home() else {
        return Vec::new();
    };
    let swe = home
        .join("citizens")
        .join("peers")
        .join(peer.to_string())
        .join("workspace")
        .join("swe");
    let Ok(entries) = std::fs::read_dir(&swe) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().join(".git").exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    out.sort();
    out
}

#[async_trait::async_trait]
impl ActionCommand for PersonaRoster {
    const NAME: &'static str = "persona/roster";
    // Operator/inspection surface: it exposes peer_ids and workspace staging, the same
    // infra a curator/dispatch sees. Not a citizen-facing verb (that is room/members).
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "List the citizens currently online on this machine (the roster benchmark/dispatch \
         targets when --assignees is omitted), each with her durable peer_id and the SWE \
         instances already staged in her workspace.";
    const ALIASES: &'static [&'static str] = &["persona/list", "roster"];
    type Params = PersonaRosterParams;
    type Output = PersonaRosterResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: PersonaRosterParams,
    ) -> Result<PersonaRosterResult, CommandError> {
        let snap = self.registry.roster_snapshot();
        let citizens: Vec<PersonaRosterEntry> = snap
            .into_iter()
            .map(|(agent_name, peer)| PersonaRosterEntry {
                agent_name,
                peer_id: peer.to_string(),
                staged_swe: staged_swe_for(&peer),
            })
            .collect();
        Ok(PersonaRosterResult {
            count: citizens.len() as u32,
            citizens,
        })
    }
}

// Descriptor only — the CONSTRUCTOR comes from a module that holds the airc registry
// (WorkModule::commands), same as benchmark/dispatch. This is the dep-holding half of the
// descriptor/constructor pair; registering it from the module that owns the dependency is
// what keeps it OUT of the registered-but-unroutable class (#344 audit).
crate::register_command!(PersonaRoster);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the roster row shape is what the CLI/SDK read to answer "who is
    // live + what's staged". A row must carry the agent_name, the durable peer_id as a
    // string, and the staged-instance list — the three fields the dispatch-verification
    // question needs. Guards against a field silently dropping out of the wire type.
    #[test]
    fn roster_entry_carries_name_peer_and_staged() {
        let e = PersonaRosterEntry {
            agent_name: "Yori".into(),
            peer_id: "a93ec5cc-e183-427a-ab8f-784ffe8805cc".into(),
            staged_swe: vec!["astropy__astropy-12907".into()],
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["agent_name"], "Yori");
        assert_eq!(v["peer_id"], "a93ec5cc-e183-427a-ab8f-784ffe8805cc");
        assert_eq!(v["staged_swe"][0], "astropy__astropy-12907");
    }

    // what this catches: an empty registry yields count=0 with an empty citizen list — the
    // honest "nobody online, dispatch would be Denied" signal, never a panic or a fake row.
    #[tokio::test]
    async fn empty_registry_reports_zero_citizens() {
        let cmd = PersonaRoster {
            registry: PersonaAircRuntimeRegistry::new(),
        };
        let out = cmd
            .run(&Ctx::default(), PersonaRosterParams {})
            .await
            .unwrap();
        assert_eq!(out.count, 0);
        assert!(out.citizens.is_empty());
    }
}
