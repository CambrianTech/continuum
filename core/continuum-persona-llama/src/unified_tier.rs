//! [`UnifiedTier`]: the persona engine's residency tier on one libllama context, as on the M5's unified
//! memory. Two execution streams: the active mind decodes in one while the next mind binds into the
//! other; every other mind's blocks stay in host memory exactly as she left them. A move out is the
//! fork's native save (her stream empties); a move in is its native bind (her blocks take an empty
//! stream, answering to the sequence she is bound as). Both are synchronous copies, so the fence a
//! move returns has already landed.

use std::collections::HashMap;

use continuum_persona_engine::persona::{Epoch, PersonaId};
use continuum_persona_engine::residency::{Direction, Fence, FenceTag, Tier, TierRefusal};
use llama::{Context, PersonaBlocks, PersonaOwner};

/// One of the context's two execution streams, by the sequence id the context knows it as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stream(i32);

impl Stream {
    /// The sequence id to decode her tokens as.
    pub fn seq(self) -> i32 {
        self.0
    }
}

/// The two streams: one decodes, the other takes the next mind's copy-in.
const STREAMS: [Stream; 2] = [Stream(0), Stream(1)];

/// A move this tier performed: the fork's copies are synchronous, so it has landed when it exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landed(FenceTag);

impl Fence for Landed {
    fn tag(&self) -> FenceTag {
        self.0
    }

    fn complete(&self) -> bool {
        true
    }
}

/// One mind this tier holds: her blocks (in RAM, or bound), the epoch they belong to, and the stream
/// she is bound to while she is in the execution stream.
struct Mind {
    blocks: PersonaBlocks,
    epoch: Epoch,
    bound: Option<Stream>,
}

/// The persona engine's tier over one context's native persona blocks.
pub struct UnifiedTier<'c, 'm> {
    ctx: &'c mut Context<'m>,
    minds: HashMap<PersonaId, Mind>,
    /// Each persona's owner stamp for the fork, issued once per persona by this tier: exact within the
    /// process the RAM tier lives in (a persona is a 128-bit id; the fork stamps 64 bits).
    handles: HashMap<PersonaId, u64>,
}

impl<'c, 'm> UnifiedTier<'c, 'm> {
    pub fn new(ctx: &'c mut Context<'m>) -> Self {
        Self { ctx, minds: HashMap::new(), handles: HashMap::new() }
    }

    /// The stream `persona` is bound to, if she is in the execution stream.
    pub fn stream_of(&self, persona: PersonaId) -> Option<Stream> {
        self.minds.get(&persona).and_then(|mind| mind.bound)
    }

    /// The context, for the active mind's decode.
    pub fn context(&mut self) -> &mut Context<'m> {
        self.ctx
    }

    /// How many minds this tier holds (bound or in RAM).
    pub fn held(&self) -> usize {
        self.minds.len()
    }

    fn free_stream(&self) -> Option<Stream> {
        STREAMS.into_iter().find(|stream| !self.minds.values().any(|mind| mind.bound == Some(*stream)))
    }
}

impl Tier for UnifiedTier<'_, '_> {
    type Fence = Landed;

    fn name(&self) -> &'static str {
        TIER
    }

    fn begin(&mut self, what: FenceTag) -> Result<Landed, TierRefusal> {
        let persona = what.persona;
        match what.direction {
            Direction::In => {
                let stream = self.free_stream().ok_or_else(|| refused(persona, "both streams are bound"))?;
                let Self { ctx, minds, handles } = self;
                let mind = mind_at(ctx, minds, handles, persona, what.epoch)?;
                if mind.bound.is_some() {
                    return Err(refused(persona, "already bound"));
                }
                // a mind never saved has nothing to bind: she starts on an empty stream
                if mind.blocks.owner().is_some() {
                    mind.blocks.bind(ctx, stream.seq()).map_err(|_| refused(persona, "the fork refused her bind"))?;
                }
                mind.bound = Some(stream);
            }
            Direction::Out => {
                let Self { ctx, minds, .. } = self;
                let mind = minds.get_mut(&persona).ok_or_else(|| refused(persona, "not held here"))?;
                if mind.epoch != what.epoch {
                    return Err(refused(persona, "another epoch of her is bound"));
                }
                let stream = mind.bound.ok_or_else(|| refused(persona, "not bound"))?;
                mind.blocks.save(ctx, stream.seq()).map_err(|_| refused(persona, "the fork refused her save"))?;
                mind.bound = None;
            }
        }
        Ok(Landed(what))
    }
}

const TIER: &str = "unified";

fn refused(persona: PersonaId, reason: &'static str) -> TierRefusal {
    TierRefusal { tier: TIER, persona, reason }
}

/// Her mind at `epoch`: made with her owner on first sight; a new epoch starts her blocks over (refused while
/// the old ones are bound).
fn mind_at<'a>(
    ctx: &mut Context<'_>,
    minds: &'a mut HashMap<PersonaId, Mind>,
    handles: &mut HashMap<PersonaId, u64>,
    persona: PersonaId,
    epoch: Epoch,
) -> Result<&'a mut Mind, TierRefusal> {
    if let Some(held) = minds.get(&persona) {
        if held.epoch != epoch {
            if held.bound.is_some() {
                return Err(refused(persona, "a new epoch while her old blocks are bound"));
            }
            minds.remove(&persona);
        }
    }
    let next = handles.len() as u64 + 1;
    let handle = *handles.entry(persona).or_insert(next);
    match minds.entry(persona) {
        std::collections::hash_map::Entry::Occupied(held) => Ok(held.into_mut()),
        std::collections::hash_map::Entry::Vacant(slot) => {
            let owner = PersonaOwner { persona: handle, epoch: epoch.0 };
            let blocks = PersonaBlocks::new(ctx, owner).ok_or_else(|| refused(persona, "this context's memory has no persona move"))?;
            Ok(slot.insert(Mind { blocks, epoch, bound: None }))
        }
    }
}
