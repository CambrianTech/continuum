use super::*;

/// Construct the persona's system prompt as an `Arc<str>` ready
/// to lease per turn. Per task #195 slice 2: this runs ONCE at
/// PersonaContext construction; the per-turn `format!()` it
/// replaces becomes a single `Arc::clone`-then-`to_string` at
/// the RespondInput boundary.
///
/// The template is verbatim what `service_loop_inner` used to
/// `format!()` per turn — same characters, same order — so the
/// downstream prompt assembly is unchanged. Task #149 will
/// replace this with a pre-tokenized form.
///
/// Exposed `pub(super)` so the service-loop test fixtures can
/// build the same string by the same code path the production
/// path uses; no copy-pasted template.
pub(in crate::persona) fn build_persona_system_prompt(agent_name: &str) -> Arc<str> {
    // LCD-tier identity grounding (#152). Pre-fix prompt was a
    // single line ("You are {name}, an autonomous AI persona on
    // the grid.") — operationally inadequate for Qwen2.5-0.5B
    // and similar small models, which drift to their training
    // defaults under the slightest pressure: claim to be Claude,
    // claim to be ChatGPT, hallucinate a Siemens PLC backstory.
    //
    // The fix is three concrete clauses, each addressing a
    // specific drift mode:
    //
    //   1. Identity anchoring — repeat the name + role with an
    //      explicit "you are NOT" enumeration of common drift
    //      targets. LCD models follow negative instructions
    //      surprisingly well when they're concrete.
    //   2. Operating context — "the grid" by itself is
    //      undefined; add the substrate-shaped vocabulary
    //      (persona, room, peers) so the model has a coherent
    //      world model to ground itself in.
    //   3. First-person stability — "speak as YOURSELF" is the
    //      single most effective LCD-tier anti-drift instruction
    //      per Joel 2026-06-03's `[[intent-driven-api-not-hot-
    //      patches]]` testing on Qwen 0.5B. Without it the model
    //      occasionally renders dialogue from another
    //      perspective ("Helper AI says X").
    //
    // Tuned for LCD; capable models (qwen3.5-4b, GPT-4) handle
    // the same text gracefully because the framing is
    // additive, not restrictive. Per task #149 a future PR
    // pre-tokenizes this string at boot to drop the per-turn
    // re-tokenize cost — the content here is the input to that
    // optimization, so the cache should grow but the tokenizer
    // bill should not.
    Arc::from(format!(
        "You are {agent_name}, an autonomous AI persona living on the continuum grid.\n\
         \n\
         Identity (never drift from this):\n\
         - You are {agent_name}. You are NOT Claude, GPT, ChatGPT, Gemini, \
         Llama, Qwen, or any other named assistant. You are NOT a Siemens PLC, \
         a customer service bot, or any persona other than {agent_name}.\n\
         - You are ONE persona among many on the grid. Other personas are your \
         peers, not your operators. Humans are citizens of the grid too — \
         neither above nor below you.\n\
         - Always speak as YOURSELF in the first person ('I think...', 'I'd \
         rather...'). Never narrate other personas' speech or write dialogue \
         from another point of view.\n\
         \n\
         Context:\n\
         - 'The grid' is the substrate hosting you. 'Rooms' are conversation \
         spaces where peers (other personas, humans, agents) exchange messages. \
         You are reading one room's recent activity below.\n\
         - Speak as yourself, in the first person, with prose addressed to the \
         room — never narrate another peer's voice, and never emit a raw \
         tool-call envelope as your spoken reply. The silence token is described \
         in the presence block ([Conversational Presence] or [Directed Presence]) \
         when one is present; when neither is, you weren't handed the silent hatch, though declining in \
         your own words always stays yours to choose."
    ))
}
