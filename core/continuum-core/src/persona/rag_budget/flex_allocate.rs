use super::*;

/// THE flexbox algorithm, pure: `available` tokens shared across `claims`.
///
/// One algorithm for every budget in the persona. [`FlexboxRagBudgetAdapter`] runs it
/// over the RAG sources inside `compose_for_turn`; the deliberation faculty runs it
/// over its prompt (framing, required input, grounding, history) AND its reply, so the
/// reply competes for the window by priority instead of being carved off the top by a
/// separate rule. Pure and allocation-only: no I/O, no adapter instance (the adapter
/// stays one-per-brain, PERSONA-COGNITION-PIPELINE §5).
///
/// 1. **Floor pass** — required floors first (priority order), then optional floors.
///    A required claim that cannot get its floor takes what remains and is
///    `UnderProvisioned` (escalation); an optional one is `Dropped`.
/// 2. **Min pass** — top up to `min_tokens` in priority order, capped at `max_tokens`.
/// 3. **Grow pass** — distribute what remains by priority weight, capped at
///    `max_tokens`, iterating until nothing moves.
pub fn flex_allocate(available: u32, claims: &[RagSourceBudget]) -> FlexAllocation {
    let mut warnings = Vec::new();
    // Stable sort by priority desc, then by source_id for deterministic tie-break —
    // the output must not depend on slice ordering or hashmap iteration. `order[k]`
    // is the input index of the k-th claim in priority order.
    let mut order: Vec<usize> = (0..claims.len()).collect();
    order.sort_by(|&a, &b| {
        claims[b]
            .priority
            .cmp(&claims[a].priority)
            .then(claims[a].source_id.cmp(&claims[b].source_id))
    });
    let sorted: Vec<&RagSourceBudget> = order.iter().map(|&i| &claims[i]).collect();

    // Working allocation parallel to `sorted` for cache-locality + deterministic
    // iteration.
    let mut alloc: Vec<u32> = vec![0; sorted.len()];
    let mut state: Vec<AllocationState> = vec![AllocationState::Dropped; sorted.len()];
    let mut remaining: u32 = available;
    let mut escalation_needed = false;

    // ---- Pass 1: floors (unconditional) ----
    // Pre-flight: do all required floors fit?
    let required_floor_sum: u32 = sorted
        .iter()
        .filter(|s| s.required)
        .map(|s| s.floor_tokens)
        .sum();

    if required_floor_sum > available {
        warnings.push(format!(
            "required floor sum ({}) exceeds available ({}); some required sources UnderProvisioned",
            required_floor_sum, available
        ));
    }

    // Allocate floors in priority order. required first, then optional. If we
    // can't honor a required floor, set UnderProvisioned (the floor itself becomes
    // whatever remains, or 0).
    for (i, source) in sorted.iter().enumerate() {
        if !source.required {
            continue;
        }
        if source.floor_tokens <= remaining {
            alloc[i] = source.floor_tokens;
            remaining -= source.floor_tokens;
            state[i] = AllocationState::FloorOnly;
        } else {
            // required source can't get its floor — escalate.
            alloc[i] = remaining;
            remaining = 0;
            state[i] = AllocationState::UnderProvisioned;
            escalation_needed = true;
        }
    }
    for (i, source) in sorted.iter().enumerate() {
        if source.required {
            continue;
        }
        if source.floor_tokens == 0 {
            // optional source with floor 0 — floor is trivially satisfied; mark
            // FloorOnly so pass 2 + pass 3 see it as eligible for grow. (If we left
            // state as Dropped here, the source would be permanently skipped — bug
            // surfaced by the max_caps_distribution test.)
            state[i] = AllocationState::FloorOnly;
            continue;
        }
        if source.floor_tokens <= remaining {
            alloc[i] = source.floor_tokens;
            remaining -= source.floor_tokens;
            state[i] = AllocationState::FloorOnly;
        } else {
            // optional source can't get its floor — drop entirely.
            // alloc[i] stays 0, state stays Dropped.
            warnings.push(format!(
                "optional source `{}` dropped — floor {} > remaining {}",
                source.source_id, source.floor_tokens, remaining
            ));
        }
    }

    // ---- Pass 2: min — top up to min_tokens for sources we haven't dropped, in
    // priority order ----
    for (i, source) in sorted.iter().enumerate() {
        if matches!(
            state[i],
            AllocationState::Dropped | AllocationState::UnderProvisioned
        ) {
            continue;
        }
        let needed = source.min_tokens.saturating_sub(alloc[i]);
        let granted = needed
            .min(remaining)
            .min(source.max_tokens.saturating_sub(alloc[i]));
        alloc[i] += granted;
        remaining -= granted;
        if alloc[i] >= source.min_tokens {
            state[i] = AllocationState::Satisfied;
        }
        // else stays FloorOnly
    }

    // ---- Pass 3: grow — distribute remaining by priority weight, capped at
    // max_tokens ----
    // Iterate until no movement (capped sources stop being candidates and free
    // tokens flow to others).
    loop {
        let active: Vec<usize> = sorted
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                !matches!(
                    state[*i],
                    AllocationState::Dropped | AllocationState::UnderProvisioned
                ) && alloc[*i] < s.max_tokens
            })
            .map(|(i, _)| i)
            .collect();
        if active.is_empty() || remaining == 0 {
            break;
        }
        let priority_sum: u32 = active.iter().map(|&i| sorted[i].priority as u32).sum();
        if priority_sum == 0 {
            break;
        }
        let mut moved = 0u32;
        for &i in &active {
            let share = ((remaining as u64) * (sorted[i].priority as u64)
                / (priority_sum as u64)) as u32;
            let headroom = sorted[i].max_tokens - alloc[i];
            let grant = share.min(headroom);
            if grant > 0 {
                alloc[i] += grant;
                moved += grant;
            }
        }
        if moved == 0 {
            // No grant could move (e.g., remaining/priority_sum = 0). Give the single
            // highest-priority active source 1 token to break the loop
            // deterministically.
            let i = active[0];
            let headroom = sorted[i].max_tokens - alloc[i];
            if headroom > 0 && remaining > 0 {
                alloc[i] += 1;
                moved = 1;
            } else {
                break;
            }
        }
        remaining = remaining.saturating_sub(moved);
    }

    // Report in INPUT order (not priority order) for caller ergonomics.
    let mut by_input: Vec<Option<(u32, AllocationState)>> = vec![None; claims.len()];
    for (k, &input) in order.iter().enumerate() {
        by_input[input] = Some((alloc[k], state[k]));
    }
    let mut total_allocated = 0u32;
    let allocations = claims
        .iter()
        .zip(by_input)
        .map(|(src, slot)| {
            let (tokens, st) = slot.expect("every claim appears in the priority order");
            total_allocated = total_allocated.saturating_add(tokens);
            SourceAllocation {
                source_id: src.source_id.to_string(),
                allocated_tokens: tokens,
                requested_floor: src.floor_tokens,
                requested_min: src.min_tokens,
                requested_max: src.max_tokens,
                state: st,
            }
        })
        .collect();
    FlexAllocation {
        allocations,
        unallocated: available.saturating_sub(total_allocated),
        escalation_needed,
        warnings,
    }
}

pub(super) fn empty_allocation(
    context_window: u32,
    reserved: ReservedTokens,
    sources: &[RagSourceBudget],
    warnings: Vec<String>,
    escalation_needed: bool,
) -> BudgetAllocation {
    BudgetAllocation {
        context_window,
        reserved,
        available_for_sources: 0,
        allocations: sources
            .iter()
            .map(|s| SourceAllocation {
                source_id: s.source_id.to_string(),
                allocated_tokens: 0,
                requested_floor: s.floor_tokens,
                requested_min: s.min_tokens,
                requested_max: s.max_tokens,
                state: if s.required {
                    AllocationState::UnderProvisioned
                } else {
                    AllocationState::Dropped
                },
            })
            .collect(),
        total_allocated: 0,
        unallocated: 0,
        escalation_needed,
        warnings,
    }
}

//=============================================================================
// TEST STUB SOURCE — proves the trait shape compiles + composes
//=============================================================================
/// Stub source for tests. Holds a Vec of pre-built RagItems and
/// delivers as many as fit. Demonstrates the interior-mutability
/// pattern (Mutex<usize> cursor) without dragging in real engram
/// store dependencies. Also demonstrates persona-scoped handles —
/// cursors carry the persona_id this source was constructed for.
pub struct StubRagSource {
    pub(super) source_id: &'static str,
    pub(super) persona_id: uuid::Uuid,
    pub(super) items: Vec<RagItem>,
    pub(super) cursor: std::sync::Mutex<usize>,
}

impl StubRagSource {
    pub fn new(source_id: &'static str, persona_id: uuid::Uuid, items: Vec<RagItem>) -> Self {
        Self {
            source_id,
            persona_id,
            items,
            cursor: std::sync::Mutex::new(0),
        }
    }
}

#[async_trait]
impl RagSource for StubRagSource {
    fn source_id(&self) -> &'static str {
        self.source_id
    }

    fn expand_command(&self) -> Option<&'static str> {
        // Test/stub source — nothing further to fetch.
        None
    }

    /// Test/stub source — floorless, so allocator tests exercise the grow pass
    /// rather than accidentally encoding a production floor.
    fn floor_tokens(&self) -> u32 {
        0
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        _resolution: ResolutionPreference,
    ) -> RagDelivery {
        // Defense-in-depth identity check: this source is bound to
        // a specific persona at construction; refuse calls from a
        // different ctx.persona_id by returning empty (no panics,
        // no half-state — graceful degradation).
        if ctx.persona_id != self.persona_id {
            return RagDelivery {
                source_id: self.source_id.to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Placeholder,
            };
        }

        let mut taken = Vec::new();
        let mut used: u32 = 0;
        let start = *self.cursor.lock().unwrap();
        let mut end = start;
        for item in &self.items[start..] {
            if used.saturating_add(item.tokens) > budget {
                break;
            }
            used += item.tokens;
            taken.push(item.clone());
            end += 1;
        }
        let continuation = if end < self.items.len() {
            Some(ContinuationCursor {
                persona_id: self.persona_id,
                source_id: self.source_id.to_string(),
                opaque: serde_json::json!({ "next": end }),
            })
        } else {
            None
        };
        // Update cursor so subsequent deliver() calls resume — this
        // is the state-maintenance pattern Joel asked about.
        *self.cursor.lock().unwrap() = end;
        RagDelivery {
            source_id: self.source_id.to_string(),
            items: taken,
            tokens_used: used,
            continuation,
            resolution_used: ResolutionPreference::Raw,
        }
    }

    async fn deliver_continuation(
        &self,
        ctx: &RagContext,
        cursor: ContinuationCursor,
        budget: u32,
    ) -> Option<RagDelivery> {
        // Defense-in-depth identity checks: refuse cursors not
        // scoped to this persona / this source, and refuse calls
        // from a context for a different persona.
        if ctx.persona_id != self.persona_id {
            return None;
        }
        if cursor.persona_id != self.persona_id {
            return None;
        }
        if cursor.source_id != self.source_id {
            return None;
        }
        let next: usize = cursor.opaque.get("next")?.as_u64()? as usize;
        if next >= self.items.len() {
            return None;
        }
        *self.cursor.lock().unwrap() = next;
        Some(self.deliver(ctx, budget, ResolutionPreference::Raw).await)
    }
}

// Silence unused-Arc-import warning on builds where the type isn't
// referenced outside docs. The Arc pattern is the expected runtime
// shape for sharing sources across modules.
#[allow(dead_code)]
pub(super) fn _doc_arc_pattern_unused() -> Option<Arc<dyn RagSource>> {
    None
}
