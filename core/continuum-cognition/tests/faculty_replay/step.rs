//! One deliberation faculty per resident, stepped act by act through her real prompt path
//! (`prompt_view_within`), so her view epoch and her tool head carry from act to act as they do
//! live. After an act the engine served, her tail joins her view as sent (`serve_view`), the one
//! step of the request path that changes what her next prompt starts from.

use std::sync::Arc;

use continuum_cognition::ai::adapter::AIProviderAdapter;
use continuum_cognition::ai::types::NativeToolSpec;
use continuum_cognition::cognition::llm_deliberation_faculty::test_support::ScriptedAdapter;
use continuum_cognition::cognition::llm_deliberation_faculty::{DeliberationPromptView, LlmDeliberationFaculty};
use continuum_context::allocation::id as claim;

use super::decode::Resident;
use super::rebuild::{self, Room};
use super::wire;

/// What the act's one allocation granted, in its tokens.
#[derive(Clone, Copy, Debug, Default)]
pub struct Grants {
    pub view: u64,
    pub history: u64,
    pub tools: u64,
    pub framing: u64,
    pub reply: u64,
}

/// One replayed act beside the act as captured.
pub struct ActSample {
    pub rendered_chars: usize,
    /// What it shares from its start with the replay's previous act.
    pub shared_chars: usize,
    pub captured_chars: usize,
    /// What the captured request shares from its start with the captured request before it.
    pub captured_shared_chars: usize,
    pub turned_over: bool,
    /// Why the faculty would not have sent it (`None`: sent).
    pub refused: Option<&'static str>,
    pub grants: Grants,
}

/// Her name and identity as her captured system prompt carries them: the identity is the system
/// message up to the faculty's `[Taking your turn]` block, which names her (`You are <name>.`).
fn identity(system: &str) -> (String, String) {
    const TURN: &str = "\n\n[Taking your turn]\n";
    let (identity, turn) = system.split_once(TURN).unwrap_or((system, "")); // unwrap_or: a system message without the block is identity alone
    let name = turn
        .split_once("You are ")
        .and_then(|(_, rest)| rest.split_once(". "))
        .map_or("resident", |(name, _)| name); // map_or: a capture that does not name her is replayed as "resident"
    (identity.to_owned(), name.to_owned())
}

/// Every verb her captured requests offered her (head and tail), once each, in first-offered order.
fn tools(resident: &Resident) -> Vec<NativeToolSpec> {
    let mut seen = std::collections::HashSet::new();
    resident
        .acts
        .iter()
        .flat_map(|act| act.request.tools.iter().flatten().chain(act.request.tail_tools.iter().flatten()))
        .filter(|tool| seen.insert(tool.name.clone()))
        .cloned()
        .collect()
}

fn refusal(view: &DeliberationPromptView) -> Option<&'static str> {
    if view.protocol_error.is_some() {
        Some("protocol")
    } else if view.budget_fault.is_some() {
        Some("budget_fault")
    } else if view.output_shortfall.is_some() {
        Some("output_shortfall")
    } else if view.capacity_error.is_some() {
        Some("capacity")
    } else {
        None
    }
}

fn grants(view: &DeliberationPromptView) -> Grants {
    let mut grants = Grants::default();
    for grant in view.allocation.iter().flat_map(|receipt| receipt.claims.iter()) {
        let tokens = u64::from(grant.grant);
        match grant.source.as_str() {
            source if source.starts_with(claim::VIEW) => grants.view += tokens,
            claim::HISTORY | claim::LAST_MOVE => grants.history += tokens,
            claim::TOOLS => grants.tools += tokens,
            claim::FRAMING => grants.framing += tokens,
            claim::REPLY => grants.reply += tokens,
            _ => {}
        }
    }
    grants
}

/// Her name, and every act replayed in order.
pub fn replay(resident: &Resident) -> Result<(String, Vec<ActSample>), String> {
    let first = resident.acts.first().ok_or_else(|| format!("{}: no act in the capture since the given time", resident.persona))?;
    let window = resident
        .acts
        .iter()
        .find_map(|act| act.window)
        .ok_or_else(|| format!("{}: no captured act carries its context window", resident.persona))?;
    let (identity, name) = identity(first.request.system_prompt.as_deref().unwrap_or("")); // unwrap_or: a request with no system message has no identity
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(ScriptedAdapter::new(Vec::new()));
    let memory = rebuild::memory();
    let faculty = LlmDeliberationFaculty::new(resident.persona, name.clone(), identity, adapter)
        .with_tools(tools(resident))
        .with_working_memory(Arc::clone(&memory))
        .with_context_window(window);

    let mut room = Room::default();
    let mut samples = Vec::with_capacity(resident.acts.len());
    let mut previous: Option<String> = None;
    let mut previous_captured: Option<String> = None;
    for act in &resident.acts {
        let ws = room.act(act, &memory)?;
        let epoch = faculty.view_epoch();
        let view = faculty.prompt_view_within(&ws, act.window.unwrap_or(window)); // unwrap_or: an act without its window receipt was fitted to her binding's
        let turned_over = epoch.is_some() && faculty.view_epoch() != epoch;
        let refused = refusal(&view);
        if refused.is_none() && act.served {
            faculty.serve_view(&view);
        }
        let rendered = wire::render(&view.system, view.tools.specs.as_deref().unwrap_or(&[]), &view.messages, &view.tools.tail); // unwrap_or: no surface sends no verbs
        let request = &act.request;
        let captured = wire::render(
            request.system_prompt.as_deref().unwrap_or(""), // unwrap_or: no system message renders empty
            request.tools.as_deref().unwrap_or(&[]),        // unwrap_or: no head sends no verbs
            &request.messages,
            request.tail_tools.as_deref().unwrap_or(&[]), // unwrap_or: no tail sends no verbs
        );
        samples.push(ActSample {
            rendered_chars: rendered.chars().count(),
            shared_chars: previous.as_deref().map_or(0, |p| wire::shared_prefix_chars(p, &rendered)), // map_or: her first act shares nothing
            captured_chars: captured.chars().count(),
            captured_shared_chars: previous_captured.as_deref().map_or(0, |p| wire::shared_prefix_chars(p, &captured)), // map_or: as above
            turned_over,
            refused,
            grants: grants(&view),
        });
        previous = Some(rendered);
        previous_captured = Some(captured);
    }
    Ok((name, samples))
}
