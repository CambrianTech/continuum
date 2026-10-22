//! Where one request runs: the slot it pins and whether it may persist its prompt cache.

/// Where one request runs: the slot it pins (`id_slot`), and whether it may persist its
/// prompt cache there. THE one placement rule (card f3ba3b56, item 4): only a Turn that
/// names its activity owns a slot; every other request, a keyless Turn included, takes the
/// slot its admission borrowed or the scratch slot, and with no scratch slot it must not
/// persist a cache into a citizen's slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub slot: Option<u32>,
    pub cache_prompt: bool,
}

pub fn placement(keyed_turn: bool, admitted: Option<u32>, scratch: Option<u32>) -> Placement {
    if keyed_turn {
        Placement { slot: admitted, cache_prompt: true }
    } else {
        Placement { slot: admitted.or(scratch), cache_prompt: scratch.is_some() }
    }
}
