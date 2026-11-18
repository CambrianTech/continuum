//! [`UnifiedTier`]: the persona engine's residency tier on one libllama context, as on the M5's unified
//! memory. Two execution streams: the active mind decodes in one while the next mind binds into the
//! other; every other mind's blocks stay in host memory exactly as she left them. A move out is the
//! fork's native save (her stream empties); a move in is its native bind (her blocks take an empty
//! stream, answering to the sequence she is bound as). Both are synchronous copies, so the fence a
//! move returns has already landed.

use std::collections::HashMap;

use continuum_persona_engine::persona::{Epoch, PersonaId};
use continuum_persona_engine::residency::{Direction, Fence, FenceTag, Native, Tier, TierRefusal};
use llama::{Context, PersonaBlocks, PersonaOwner, PersonaRefusal};

/// One of the context's two execution streams, by the sequence id the context knows it as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stream(i32);

impl Stream {
    /// The sequence id to decode her tokens as.
    pub fn seq(self) -> i32 {
        self.0
    }
}

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

/// How a mind's last bind met her blocks: on the sequence she was saved from (her cells keep their
/// label), or on the other one (the fork relabels her cells, a walk over her span; Codex on #6134).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rebind {
    SameSequence,
    OtherSequence,
}

/// One mind this tier holds: her blocks (in RAM, or bound), the epoch they belong to, and the stream
/// she was last saved from.
struct Mind {
    blocks: PersonaBlocks,
    epoch: Epoch,
    saved_from: Option<Stream>,
}

/// The persona engine's tier over one context's native persona blocks.
pub struct UnifiedTier<'c, 'm> {
    ctx: &'c mut Context<'m>,
    /// Who occupies each execution stream: occupancy is the streams', never a scan of every mind.
    streams: [Option<PersonaId>; 2],
    minds: HashMap<PersonaId, Mind>,
    /// Each persona's owner stamp for the fork, issued once per persona by this tier: exact within the
    /// process the RAM tier lives in (a persona is a 128-bit id; the fork stamps 64 bits).
    handles: HashMap<PersonaId, u64>,
    last_rebind: Option<Rebind>,
}

impl<'c, 'm> UnifiedTier<'c, 'm> {
    pub fn new(ctx: &'c mut Context<'m>) -> Self {
        Self { ctx, streams: [None, None], minds: HashMap::new(), handles: HashMap::new(), last_rebind: None }
    }

    /// The stream `persona` is bound to, if she is in the execution stream.
    pub fn stream_of(&self, persona: PersonaId) -> Option<Stream> {
        self.streams.iter().position(|held| *held == Some(persona)).map(stream_at)
    }

    /// The context, for the active mind's decode.
    pub fn context(&mut self) -> &mut Context<'m> {
        self.ctx
    }

    /// How many minds this tier holds (bound or in RAM).
    pub fn held(&self) -> usize {
        self.minds.len()
    }

    /// How the last move in met her blocks; `None` when she had never been saved.
    pub fn last_rebind(&self) -> Option<Rebind> {
        self.last_rebind
    }

    fn move_in(&mut self, persona: PersonaId, epoch: Epoch) -> Result<(), TierRefusal> {
        if self.stream_of(persona).is_some() {
            return Err(refused(persona, "already bound"));
        }
        let slot = self.streams.iter().position(Option::is_none).ok_or_else(|| refused(persona, "both streams are bound"))?;
        let stream = stream_at(slot);
        self.hold(persona, epoch)?;
        let mind = self.minds.get_mut(&persona).ok_or_else(|| refused(persona, "not held here"))?;
        self.last_rebind = None;
        match mind.saved_from {
            // A mind never saved has nothing to bind: she starts on this stream as it is, so it must
            // hold nothing (the context may have been in use before this tier wrapped it; Codex on
            // #6134). The engine's own memory is the evidence, never this tier's bookkeeping.
            None if self.ctx.memory_seq_pos_max(stream.seq()) >= 0 => {
                return Err(refused(persona, "her stream already holds another sequence's state"));
            }
            None => {}
            Some(from) => {
                mind.blocks.bind(self.ctx, stream.seq()).map_err(|e| native(persona, "the fork refused her bind", e))?;
                self.last_rebind = Some(if from == stream { Rebind::SameSequence } else { Rebind::OtherSequence });
            }
        }
        self.streams[slot] = Some(persona);
        Ok(())
    }

    fn move_out(&mut self, persona: PersonaId, epoch: Epoch) -> Result<(), TierRefusal> {
        let slot = self.streams.iter().position(|held| *held == Some(persona)).ok_or_else(|| refused(persona, "not bound"))?;
        let stream = stream_at(slot);
        let mind = self.minds.get_mut(&persona).ok_or_else(|| refused(persona, "not held here"))?;
        if mind.epoch != epoch {
            return Err(refused(persona, "another epoch of her is bound"));
        }
        mind.blocks.save(self.ctx, stream.seq()).map_err(|e| native(persona, "the fork refused her save", e))?;
        mind.saved_from = Some(stream);
        self.streams[slot] = None;
        Ok(())
    }

    /// Her blocks at `epoch`, made with her owner on first sight. An older epoch than the one held is
    /// stale and refused, leaving her blocks as they are. A newer epoch prepares its blocks first and
    /// replaces the old ones only once they exist, so a refused allocation keeps her custody (Codex on
    /// #6134).
    fn hold(&mut self, persona: PersonaId, epoch: Epoch) -> Result<(), TierRefusal> {
        if let Some(held) = self.minds.get(&persona) {
            if epoch == held.epoch {
                return Ok(());
            }
            if epoch < held.epoch {
                return Err(refused(persona, "a stale epoch: her newer blocks are kept"));
            }
        }
        let next = self.handles.len() as u64 + 1;
        let handle = *self.handles.entry(persona).or_insert(next);
        let owner = PersonaOwner { persona: handle, epoch: epoch.0 };
        let blocks = PersonaBlocks::new(self.ctx, owner).ok_or_else(|| refused(persona, "this context's memory has no persona move"))?;
        self.minds.insert(persona, Mind { blocks, epoch, saved_from: None });
        Ok(())
    }
}

impl Tier for UnifiedTier<'_, '_> {
    type Fence = Landed;

    fn name(&self) -> &'static str {
        TIER
    }

    fn begin(&mut self, what: FenceTag) -> Result<Landed, TierRefusal> {
        match what.direction {
            Direction::In => self.move_in(what.persona, what.epoch)?,
            Direction::Out => self.move_out(what.persona, what.epoch)?,
        }
        Ok(Landed(what))
    }
}

const TIER: &str = "unified";

fn stream_at(slot: usize) -> Stream {
    Stream(slot as i32)
}

fn refused(persona: PersonaId, reason: &'static str) -> TierRefusal {
    TierRefusal { tier: TIER, persona, reason, native: Native::NotAsked }
}

/// The fork refused: keep its status and name in the refusal.
fn native(persona: PersonaId, reason: &'static str, refusal: PersonaRefusal) -> TierRefusal {
    TierRefusal { tier: TIER, persona, reason, native: Native::Refused { status: refusal.status, name: refusal.name } }
}
