use super::*;

/// The persona's body for the act→observe motion: the HANDS that execute an
/// [`Decision::Act`], the HIPPOCAMPUS that remembers the result, and the
/// IDENTITY the action is performed as. Held on the [`WorkspaceCycle`] because
/// the cycle IS the persona's one mind — and per Joel, a persona (like a Claude
/// tab) is in MANY rooms at once, so the body is deliberately **room-agnostic**:
/// `room_id` flows per-act (the room *that* tick is about), never baked in here.
/// `None` on the cycle → a pure-cognition mind with no hands (harnesses, or any
/// persona whose spawn path built no executor): an `Act` verdict simply can't be
/// driven, and tools were never offered, so it never arises.
///
/// The act→observe driver ([`crate::cognition::act_observe`]) reads this; `run_in_room`
/// never touches it (that stays a pure single tick).
pub struct ActingBody {
    pub persona_id: Uuid,
    pub persona_name: String,
    /// Runs the tool calls an `Act` verdict carries (identity-bearing, so the
    /// ACL gates what the persona may actually do).
    pub executor: Arc<dyn crate::cognition::tool_executor::ToolExecutor>,
    /// The hippocampus the action's RESULT is admitted into as an Episodic
    /// engram — so the outcome becomes a thing the mind remembers and can be
    /// reminded of next tick, the same way it carries every other fact.
    pub admission: Arc<crate::cognition::admission_state::AdmissionState>,
    /// The volatile working-memory scratchpad the organism records each act into
    /// (proprioception). Distinct from `admission` (long-term, content-deduped): an
    /// act must be perceptible NEXT tick even when its result is a dedup no-op in
    /// long-term memory, so the mind sees its own hands and doesn't re-issue the
    /// identical act blind. Shared `Arc` with her view's sources (`context_streams`: one
    /// buffer, written here, read there).
    pub working_memory: Arc<crate::cognition::working_memory::WorkingMemory>,
}
