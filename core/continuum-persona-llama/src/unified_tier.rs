//! [`UnifiedTier`]: the persona engine's residency tier on one libllama context, as on the M5's unified
//! memory. Two execution streams: the active mind decodes in one while the next mind binds into the
//! other; every other mind's blocks stay in host memory exactly as she left them. A move out is the
//! fork's native save (her stream empties); a move in is its native bind (her blocks take an empty
//! stream, answering to the sequence she is bound as). Both are synchronous copies, so the fence a
//! move returns has already landed.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use continuum_persona_engine::persona::{Epoch, PersonaId};
use continuum_persona_engine::residency::{Completion, Direction, Execute, Fence, FenceTag, Native, Tier, TierRefusal};
use llama::{Batch, BindRefusal, Context, LoraAdapter, PersonaBlocks, PersonaCheckpoint, PersonaOwner, PersonaRefusal, Sampler};

/// Execution streams one context holds: the active mind's and the next mind's. The context is
/// created with this many sequences; no more are ever decoded at once (one decodes, one binds).
pub const STREAMS: usize = 2;

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
    /// Her blocks came back from her file after a restart (CONTINUUM-ENGINE 4.16).
    FromFile,
}

/// What her blocks hold beyond their preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Saved {
    /// Nothing: she starts on a stream as it is.
    Never,
    /// Saved out of this stream.
    From(Stream),
    /// Read back from her file after a restart.
    Restored,
}

/// One mind this tier holds: her blocks (in RAM, or bound), the epoch they belong to, and what they
/// hold.
struct Mind {
    blocks: PersonaBlocks,
    epoch: Epoch,
    saved: Saved,
}

/// Her owner stamp for the fork: a fold of her 128-bit id, the same in every process, so the blocks
/// she wrote before a restart are hers after it (a per-process counter would refuse her own file as
/// another owner's). The tier keys minds by the full id; the stamp is the fork's check that blocks
/// are bound only under the owner they were saved for.
fn owner_stamp(persona: PersonaId) -> u64 {
    let (high, low) = persona.0.as_u64_pair();
    high ^ low
}

/// The owner her blocks at `epoch` must carry to bind.
fn owner_of(persona: PersonaId, epoch: Epoch) -> PersonaOwner {
    PersonaOwner { persona: owner_stamp(persona), epoch: epoch.0 }
}

/// The persona engine's tier over one context's native persona blocks.
pub struct UnifiedTier<'c, 'm> {
    ctx: &'c mut Context<'m>,
    /// Who occupies each execution stream: occupancy is the streams', never a scan of every mind.
    streams: [Option<PersonaId>; STREAMS],
    minds: HashMap<PersonaId, Mind>,
    last_rebind: Option<Rebind>,
    /// The last submitted decode's landed flag, until the device is known to have finished it.
    in_flight: Option<Rc<Cell<bool>>>,
    /// The gene set on the context now (`None` before any step).
    applied_genes: Option<Genes>,
}

impl<'c, 'm> UnifiedTier<'c, 'm> {
    pub fn new(ctx: &'c mut Context<'m>) -> Self {
        Self { ctx, streams: [None; STREAMS], minds: HashMap::new(), last_rebind: None, in_flight: None, applied_genes: None }
    }

    /// The stream `persona` is bound to, if she is in the execution stream.
    pub fn stream_of(&self, persona: PersonaId) -> Option<Stream> {
        self.streams.iter().position(|held| *held == Some(persona)).map(stream_at)
    }

    /// The logits of output `i` of the last decode (the batch index whose logits were asked for).
    /// libllama synchronizes on result retrieval, so reading them also attests the last decode landed.
    pub fn logits_ith(&self, i: i32) -> &[f32] {
        let logits = self.ctx.logits_ith(i);
        if let Some(landed) = &self.in_flight {
            landed.set(true);
        }
        logits
    }

    /// The last position `persona`'s stream holds, while she is bound.
    pub fn position_of(&self, persona: PersonaId) -> Option<i32> {
        self.stream_of(persona).map(|stream| self.ctx.memory_seq_pos_max(stream.seq()))
    }

    /// How many decodes on this context reused a built graph (libllama's counter).
    pub fn graphs_reused(&self) -> u64 {
        self.ctx.graphs_reused()
    }

    /// Sample her next token from output `i` of the last decode. Like [`Self::logits_ith`], reading the
    /// logits synchronizes, so it also attests the last decode landed.
    pub fn sample(&self, sampler: &mut Sampler, i: i32) -> i32 {
        let token = sampler.sample(self.ctx, i);
        if let Some(landed) = &self.in_flight {
            landed.set(true);
        }
        token
    }

    /// The bytes `persona`'s stream holds, while she is bound (her footprint).
    pub fn state_bytes_of(&self, persona: PersonaId) -> Option<u64> {
        self.stream_of(persona).map(|stream| self.ctx.seq_state_bytes(stream.seq()))
    }

    /// The raw context, past every binding: only for custody replays that must put foreign state on a
    /// stream (an adopted context). Never on the production path.
    #[cfg(feature = "test-fixtures")]
    pub fn raw_context(&mut self) -> &mut Context<'m> {
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
        // Every check runs before anything changes (Codex on #6134): a newer epoch replaces her older
        // blocks only once the move is known to succeed, so a refusal keeps the blocks she had.
        let rebind_from = match self.minds.get(&persona) {
            Some(held) if epoch < held.epoch => return Err(refused(persona, "a stale epoch: her newer blocks are kept")),
            Some(held) if epoch == held.epoch => held.saved,
            // a mind never seen, or a newer epoch of her, starts over on this stream
            _ => Saved::Never,
        };
        // A mind with nothing to bind starts on this stream as it is, so it must hold nothing (the
        // context may have been in use before this tier wrapped it). The engine's own memory is the
        // evidence, never this tier's bookkeeping.
        if rebind_from == Saved::Never && self.ctx.memory_seq_pos_max(stream.seq()) >= 0 {
            return Err(refused(persona, "her stream already holds another sequence's state"));
        }
        self.hold(persona, epoch)?;
        let owner = owner_of(persona, epoch);
        let mind = self.minds.get_mut(&persona).ok_or_else(|| refused(persona, "not held here"))?;
        self.last_rebind = None;
        if rebind_from != Saved::Never {
            mind.blocks.bind(self.ctx, stream.seq(), owner).map_err(|e| bind_refused(persona, e))?;
            self.last_rebind = Some(match rebind_from {
                Saved::From(from) if from == stream => Rebind::SameSequence,
                Saved::Restored => Rebind::FromFile,
                _ => Rebind::OtherSequence,
            });
        }
        self.streams[slot] = Some(persona);
        Ok(())
    }


    fn move_out(&mut self, persona: PersonaId, epoch: Epoch) -> Result<(), TierRefusal> {
        // her save reads her cells: whatever was submitted for her must have landed first
        self.settle()?;
        let slot = self.streams.iter().position(|held| *held == Some(persona)).ok_or_else(|| refused(persona, "not bound"))?;
        let stream = stream_at(slot);
        let mind = self.minds.get_mut(&persona).ok_or_else(|| refused(persona, "not held here"))?;
        if mind.epoch != epoch {
            return Err(refused(persona, "another epoch of her is bound"));
        }
        mind.blocks.save(self.ctx, stream.seq()).map_err(|e| native(persona, "the fork refused her save", e))?;
        mind.saved = Saved::From(stream);
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
        let blocks = PersonaBlocks::new(self.ctx, owner_of(persona, epoch)).ok_or_else(|| refused(persona, "this context's memory has no persona move"))?;
        self.minds.insert(persona, Mind { blocks, epoch, saved: Saved::Never });
        Ok(())
    }

    /// Demote `persona` to `path` (CONTINUUM-ENGINE 4.16): the blocks her last copy-out saved, written
    /// as raw rows. Refused while she is bound: she leaves the stream through the pipeline
    /// (`PersonaEngine::retire_active` or a handoff), never behind its back. She stays held in RAM as
    /// well; the file is what a restart reads her back from.
    pub fn write_file(&self, persona: PersonaId, epoch: Epoch, path: &std::path::Path) -> Result<(), TierRefusal> {
        if self.stream_of(persona).is_some() {
            return Err(refused(persona, "bound: she retires through the pipeline before her file is written"));
        }
        let mind = self.minds.get(&persona).ok_or_else(|| refused(persona, "not held here"))?;
        if mind.epoch != epoch {
            return Err(refused(persona, "another epoch of her is held"));
        }
        mind.blocks.write_file(path).map_err(|e| native(persona, "the fork refused to write her blocks", e))
    }

    /// Hold `persona` at `epoch` from her file: her blocks back in RAM, bound when she is next
    /// scheduled. A file of another owner, epoch or memory layout is refused and nothing is held.
    pub fn read_file(&mut self, persona: PersonaId, epoch: Epoch, path: &std::path::Path) -> Result<(), TierRefusal> {
        if self.minds.contains_key(&persona) {
            return Err(refused(persona, "already held: a restore never replaces blocks she holds"));
        }
        let mut blocks = PersonaBlocks::new(self.ctx, owner_of(persona, epoch)).ok_or_else(|| refused(persona, "this context's memory has no persona move"))?;
        blocks.read_file(path).map_err(|e| native(persona, "the fork refused her file", e))?;
        self.minds.insert(persona, Mind { blocks, epoch, saved: Saved::Restored });
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

    fn release(&mut self, persona: PersonaId) -> Result<(), TierRefusal> {
        if self.stream_of(persona).is_some() {
            return Err(refused(persona, "bound: move her out before releasing her"));
        }
        self.minds.remove(&persona);
        Ok(())
    }
}

/// One step of the bound mind's decode: her tokens from position `from`, with the logits of the last
/// one if `logits_at_end`. It names no sequence: the tier decodes on the stream she is bound to.
#[derive(Clone)]
pub struct Step {
    pub tokens: Vec<i32>,
    pub from: i32,
    pub logits_at_end: bool,
    /// The gene set she decodes under. The tier applies it only when it differs from the set
    /// already on the context, and only for her: no step is decoded under another mind's genes.
    pub genes: Genes,
}

/// A gene set: adapter handles and their scales, shared so steps name it without copying. Empty is
/// the base model.
pub type Genes = Arc<[(Arc<LoraAdapter>, f32)]>;

/// The base model's gene set (no adapters).
pub fn base_genes() -> Genes {
    Arc::from(Vec::new())
}

/// A decode this tier submitted. `llama_decode` may return while the device still runs it (libllama
/// synchronizes on result retrieval, Codex on #6139), so it reports landed only once the tier saw a
/// result read or settled.
#[derive(Debug, Clone)]
pub struct Decoded {
    landed: Rc<Cell<bool>>,
}

impl Completion for Decoded {
    fn complete(&self) -> bool {
        self.landed.get()
    }
}

impl Execute for UnifiedTier<'_, '_> {
    type Work = Step;
    type Done = Decoded;

    fn submit(&mut self, persona: PersonaId, epoch: Epoch, step: Step) -> Result<Decoded, TierRefusal> {
        let stream = self.stream_of(persona).ok_or_else(|| refused(persona, "not bound"))?;
        let held = self.minds.get(&persona).map(|mind| mind.epoch);
        if held != Some(epoch) {
            return Err(refused(persona, "another epoch of her is bound"));
        }
        // A step at `from` replaces whatever she held from there (a re-decoded last token, a restart
        // at 0, a failed turn's partial tail), so her stream never holds two tokens at one position.
        if self.ctx.memory_seq_pos_max(stream.seq()) >= step.from {
            self.settle()?;
            if !self.ctx.memory_seq_rm(stream.seq(), step.from, -1) {
                return Err(refused(persona, "the engine could not drop her positions"));
            }
        }
        if !self.applied_genes.as_ref().is_some_and(|applied| Arc::ptr_eq(applied, &step.genes)) {
            let genes: Vec<(&LoraAdapter, f32)> = step.genes.iter().map(|(adapter, scale)| (adapter.as_ref(), *scale)).collect();
            let applied = if genes.is_empty() { self.ctx.clear_loras() } else { self.ctx.set_loras(&genes) };
            applied.map_err(|message| TierRefusal { tier: TIER, persona, reason: "the engine refused her genes", native: Native::Reported(message) })?;
            self.applied_genes = Some(Arc::clone(&step.genes));
        }
        let last = step.tokens.len().checked_sub(1).ok_or_else(|| refused(persona, "an empty step"))?;
        let count = i32::try_from(step.tokens.len()).map_err(|_| refused(persona, "a step longer than a batch can hold"))?;
        let mut batch = Batch::allocated(count, 1);
        for (offset, (i, &token)) in (0..count).zip(step.tokens.iter().enumerate()) {
            batch.push(token, step.from + offset, &[stream.seq()], step.logits_at_end && i == last);
        }
        self.ctx.decode(&batch).map_err(|message| TierRefusal {
            tier: TIER,
            persona,
            reason: "the engine refused her decode",
            native: Native::Reported(message),
        })?;
        let landed = Rc::new(Cell::new(false));
        self.in_flight = Some(Rc::clone(&landed));
        Ok(Decoded { landed })
    }

    fn settle(&mut self) -> Result<(), TierRefusal> {
        self.ctx.synchronize();
        if let Some(landed) = self.in_flight.take() {
            landed.set(true);
        }
        Ok(())
    }

    /// Her recurrent rows copied out where she stands; her attention KV stays in place (the fork's
    /// `llama_persona_checkpoint_*`). Its buffers are reused take after take.
    type Checkpoint = PersonaCheckpoint;

    fn checkpoint(&mut self, persona: PersonaId, epoch: Epoch, into: &mut PersonaCheckpoint) -> Result<u32, TierRefusal> {
        let stream = self.bound_at(persona, epoch)?;
        self.settle()?;
        let owner = owner_of(persona, epoch);
        into.take(&mut *self.ctx, stream.seq(), owner).map_err(|refusal| native(persona, "the fork refused her checkpoint", refusal))?;
        into.pos().ok_or_else(|| refused(persona, "her checkpoint holds no position"))
    }

    fn rewind(&mut self, persona: PersonaId, epoch: Epoch, from: &PersonaCheckpoint) -> Result<(), TierRefusal> {
        let stream = self.bound_at(persona, epoch)?;
        self.settle()?;
        let owner = owner_of(persona, epoch);
        from.rewind(&mut *self.ctx, stream.seq(), owner).map_err(|refusal| native(persona, "the fork refused her rewind", refusal))
    }
}

impl UnifiedTier<'_, '_> {
    /// Her stream, when she is bound here at `epoch`.
    fn bound_at(&self, persona: PersonaId, epoch: Epoch) -> Result<Stream, TierRefusal> {
        let stream = self.stream_of(persona).ok_or_else(|| refused(persona, "not bound"))?;
        match self.minds.get(&persona).map(|mind| mind.epoch) {
            Some(held) if held == epoch => Ok(stream),
            _ => Err(refused(persona, "another epoch of her is bound")),
        }
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

/// A refused bind, by who refused: her blocks' stamp (another owner or a stale epoch) or the fork.
fn bind_refused(persona: PersonaId, refusal: BindRefusal) -> TierRefusal {
    match refusal {
        BindRefusal::WrongOwner { .. } => refused(persona, "her blocks are stamped for another owner or epoch"),
        BindRefusal::Native(native_refusal) => native(persona, "the fork refused her bind", native_refusal),
    }
}
