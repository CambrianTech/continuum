//! [`UnifiedTier`]: the persona engine's residency tier on one libllama context, the Core's (object
//! model 4.11, CONTINUUM-ENGINE 4.3). The context's KV is one pool that every seated mind grows into as
//! she needs; each seated mind holds one of its sequences, and her state stays in place while she is
//! seated, so a switch between seated minds moves nothing. A mind moves only when she is not seated
//! (her blocks paged back in, the fork's native bind scattering her rows into the pool's empty cells)
//! or when the pool needs her room (the fork's native save gathering exactly her cells out). Both are
//! synchronous copies, so the fence a move returns has already landed. No window is a share of the
//! context: every mind may grow to the whole pool, and when it runs out a seated mind is evicted, never
//! a window shrunk (2026-11-21: streams of half the context each held seven dying turns).

use std::cell::Cell;

use std::rc::Rc;
use std::sync::Arc;

use continuum_persona_engine::persona::MindId;
use continuum_persona_engine::residency::{Completion, Direction, Execute, Fence, FenceTag, Native, Tier, TierRefusal};
use llama::{Batch, BindRefusal, ChatRefusal, Context, DecodeRefusal, LoraAdapter, PersonaBlocks, PersonaCheckpoint, PersonaOwner, PersonaRefusal, Pos, TurnSampler};

mod learn;
pub use learn::LearnReply;

/// The context's execution stream, by the sequence id the context knows it as.
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

/// Her blocks: the unit's blocks part (CONTINUUM-ENGINE 4.17). The native blocks her moves copy,
/// stamped with her owner at creation, and what they hold. The unit carries them; the tier moves
/// them when handed them and keeps no copy of who she is.
pub struct Blocks {
    native: PersonaBlocks,
    /// Whether they hold her rows (a copy-out, or her file read back): bound, they are written in;
    /// otherwise she starts on an empty sequence.
    holds: bool,
}

impl Blocks {
    /// Write what her last copy-out saved to `path` as raw rows (CONTINUUM-ENGINE 4.16). Her unit is
    /// in the warm table, not in the stream, whenever a caller holds it to write: a bound mind's unit
    /// is the pipeline's.
    pub fn write_file(&self, path: &std::path::Path) -> Result<(), PersonaRefusal> {
        self.native.write_file(path)
    }
}

/// Her owner stamp for the fork: a fold of her 128-bit id, the same in every process, so the blocks
/// she wrote before a restart are hers after it. The fork's check that blocks bind only under the
/// owner and epoch they were made for.
fn owner_of(mind: MindId) -> PersonaOwner {
    let (high, low) = mind.persona.0.as_u64_pair();
    PersonaOwner { persona: high ^ low, epoch: mind.epoch.0 }
}

/// The persona engine's tier over one context's native persona blocks. Its own state is which stream
/// holds which mind; her blocks are her unit's.
pub struct UnifiedTier<'c, 'm> {
    ctx: &'c mut Context<'m>,
    /// Who occupies each execution stream, by her whole id: a stream holds one generation of her.
    streams: Vec<Option<MindId>>,
    /// The last submitted decode's landed flag, until the device is known to have finished it.
    in_flight: Option<Rc<Cell<bool>>>,
    /// The gene set on the context now (`None` before any step).
    applied_genes: Option<Genes>,
}

impl<'c, 'm> UnifiedTier<'c, 'm> {
    pub fn new(ctx: &'c mut Context<'m>) -> Self {
        let seats = ctx.n_seq_max().max(1) as usize; // u32 into usize: every target this builds for is 32 bits or wider
        Self { ctx, streams: vec![None; seats], in_flight: None, applied_genes: None }
    }

    /// The stream `mind` is bound to, if she is in the execution stream.
    pub fn stream_of(&self, mind: MindId) -> Option<Stream> {
        self.streams.iter().position(|held| *held == Some(mind)).map(stream_at)
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

    /// The last position `mind`'s stream holds, while she is bound.
    pub fn position_of(&self, mind: MindId) -> Option<Pos> {
        self.stream_of(mind).and_then(|stream| self.ctx.memory_seq_pos_max(stream.seq()))
    }

    /// How many decodes on this context reused a built graph (libllama's counter).
    pub fn graphs_reused(&self) -> u64 {
        self.ctx.graphs_reused()
    }

    /// Sample her next token from output `i` of the last decode with her turn's sampler. Like
    /// [`Self::logits_ith`], reading the logits synchronizes, so it also attests the last decode
    /// landed. A step her sampler refuses (a token its grammar cannot accept) is named, never a
    /// token.
    pub fn sample(&self, sampler: &mut TurnSampler, i: i32) -> Result<i32, ChatRefusal> {
        let token = sampler.sample(self.ctx, i)?;
        if let Some(landed) = &self.in_flight {
            landed.set(true);
        }
        Ok(token)
    }

    /// The bytes `mind`'s stream holds, while she is bound (her footprint in the context).
    pub fn state_bytes_of(&self, mind: MindId) -> Option<u64> {
        self.stream_of(mind).map(|stream| self.ctx.seq_state_bytes(stream.seq()))
    }

    /// The raw context, past every binding: only for custody replays that must put foreign state on a
    /// stream (an adopted context). Never on the production path.
    #[cfg(feature = "test-fixtures")]
    pub fn raw_context(&mut self) -> &mut Context<'m> {
        self.ctx
    }

    /// Her blocks read back from `path` (CONTINUUM-ENGINE 4.16), bound when she is next scheduled. A
    /// file of another owner, epoch or memory layout is refused and nothing is made.
    pub fn read_file(&mut self, mind: MindId, path: &std::path::Path) -> Result<Blocks, TierRefusal> {
        let mut blocks = self.blocks_for(mind)?;
        blocks.native.read_file(path).map_err(|e| native(mind, "the fork refused her file", e))?;
        blocks.holds = true;
        Ok(blocks)
    }

    fn move_in(&mut self, mind: MindId, blocks: &mut Blocks) -> Result<(), TierRefusal> {
        if self.streams.iter().flatten().any(|held| held.persona == mind.persona) {
            return Err(refused(mind, "already bound"));
        }
        let slot = self.streams.iter().position(Option::is_none).ok_or_else(|| no_room(mind, "every sequence is seated"))?;
        let stream = stream_at(slot);
        // A mind with nothing to bind starts on this stream as it is, so it must hold nothing (the
        // context may have been in use before this tier wrapped it). The engine's own memory is the
        // evidence, never this tier's bookkeeping.
        if !blocks.holds && self.ctx.memory_seq_pos_max(stream.seq()).is_some() {
            return Err(refused(mind, "her stream already holds another sequence's state"));
        }
        if blocks.holds {
            blocks.native.bind(self.ctx, stream.seq(), owner_of(mind)).map_err(|e| bind_refused(mind, e))?;
            blocks.holds = false; // her rows are in the pool now; her blocks are empty again
        }
        self.streams[slot] = Some(mind);
        Ok(())
    }

    fn move_out(&mut self, mind: MindId, blocks: &mut Blocks) -> Result<(), TierRefusal> {
        // her save reads her cells: whatever was submitted for her must have landed first
        self.settle()?;
        let slot = self.streams.iter().position(|held| *held == Some(mind)).ok_or_else(|| refused(mind, "not bound"))?;
        let stream = stream_at(slot);
        blocks.native.save(self.ctx, stream.seq()).map_err(|e| native(mind, "the fork refused her save", e))?;
        blocks.holds = true;
        self.streams[slot] = None;
        Ok(())
    }
}

impl Tier for UnifiedTier<'_, '_> {
    type Fence = Landed;
    type Blocks = Blocks;

    fn name(&self) -> &'static str {
        TIER
    }

    /// New blocks for `mind`, stamped with her owner and epoch.
    fn blocks_for(&mut self, mind: MindId) -> Result<Blocks, TierRefusal> {
        let native = PersonaBlocks::new(self.ctx, owner_of(mind)).ok_or_else(|| refused(mind, "this context's memory has no persona move"))?;
        Ok(Blocks { native, holds: false })
    }

    fn begin(&mut self, mind: MindId, blocks: &mut Blocks, direction: Direction) -> Result<Landed, TierRefusal> {
        match direction {
            Direction::In => self.move_in(mind, blocks)?,
            Direction::Out => self.move_out(mind, blocks)?,
        }
        Ok(Landed(FenceTag { mind, direction }))
    }
}

/// One step of the bound mind's decode: her tokens from position `from`, with the logits of the last
/// one if `logits_at_end`. It names no sequence: the tier decodes on the stream she is bound to.
#[derive(Clone)]
pub struct Step {
    pub tokens: Vec<i32>,
    pub from: Pos,
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

    fn submit(&mut self, mind: MindId, step: &Step) -> Result<Decoded, TierRefusal> {
        let stream = self.stream_of(mind).ok_or_else(|| refused(mind, "not bound"))?;
        // A step at `from` replaces whatever she held from there (a re-decoded last token, a restart
        // at 0, a failed turn's partial tail), so her stream never holds two tokens at one position.
        if self.ctx.memory_seq_pos_max(stream.seq()).is_some_and(|held| held >= step.from) {
            self.settle()?;
            if !self.ctx.memory_seq_truncate(stream.seq(), step.from) {
                return Err(refused(mind, "the engine could not drop her positions"));
            }
        }
        if !self.applied_genes.as_ref().is_some_and(|applied| Arc::ptr_eq(applied, &step.genes)) {
            let genes: Vec<(&LoraAdapter, f32)> = step.genes.iter().map(|(adapter, scale)| (adapter.as_ref(), *scale)).collect();
            let applied = if genes.is_empty() { self.ctx.clear_loras() } else { self.ctx.set_loras(&genes) };
            applied.map_err(|message| TierRefusal { tier: TIER, mind, reason: "the engine refused her genes", native: Native::Reported(message) })?;
            self.applied_genes = Some(Arc::clone(&step.genes));
        }
        let last = step.tokens.len().checked_sub(1).ok_or_else(|| refused(mind, "an empty step"))?;
        let count = i32::try_from(step.tokens.len()).map_err(|_| refused(mind, "a step longer than a batch can hold"))?;
        let mut batch = Batch::allocated(count, 1);
        for (i, &token) in step.tokens.iter().enumerate() {
            let pos = step.from.advanced(i).map_err(|_| refused(mind, "a step past what a sequence can address"))?;
            batch.push(token, pos, &[stream.seq()], step.logits_at_end && i == last);
        }
        self.ctx.decode(&batch).map_err(|refusal| {
            // The refusal says what the engine held when it refused (the M5, 2026-11-21: a cold
            // 26-36k replay prefill refused with no KV slot ~31 s in; the cause is not yet known).
            let occupied: Vec<(usize, Option<MindId>, Option<Pos>)> =
                self.streams.iter().enumerate().map(|(at, held)| (at, *held, self.ctx.memory_seq_pos_max(stream_at(at).seq()))).collect();
            continuum_base::probe!(
                class = "persona.engine.decode_refused",
                persona = ?mind,
                from = step.from.get(),
                tokens = count,
                n_ctx = self.ctx.n_ctx(),
                n_ctx_seq = self.ctx.n_ctx_seq(),
                n_seq_max = self.ctx.n_seq_max(),
                n_batch = self.ctx.n_batch(),
                n_ubatch = self.ctx.n_ubatch(),
                streams = ?occupied,
                native = %refusal,
                "the engine refused a decode: what it held then"
            );
            let said = if refusal == DecodeRefusal::NoRoom { Native::NoRoom } else { Native::Reported(refusal.to_string()) };
            TierRefusal { tier: TIER, mind, reason: "the engine refused her decode", native: said }
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

    fn checkpoint(&mut self, mind: MindId, into: &mut PersonaCheckpoint) -> Result<u32, TierRefusal> {
        let stream = self.stream_of(mind).ok_or_else(|| refused(mind, "not bound"))?;
        self.settle()?;
        into.take(&mut *self.ctx, stream.seq(), owner_of(mind)).map_err(|refusal| native(mind, "the fork refused her checkpoint", refusal))?;
        into.pos().ok_or_else(|| refused(mind, "her checkpoint holds no position"))
    }

    fn rewind(&mut self, mind: MindId, from: &PersonaCheckpoint) -> Result<(), TierRefusal> {
        let stream = self.stream_of(mind).ok_or_else(|| refused(mind, "not bound"))?;
        self.settle()?;
        from.rewind(&mut *self.ctx, stream.seq(), owner_of(mind)).map_err(|refusal| native(mind, "the fork refused her rewind", refusal))
    }
}

const TIER: &str = "unified";

fn stream_at(slot: usize) -> Stream {
    Stream(slot as i32)
}

fn refused(mind: MindId, reason: &'static str) -> TierRefusal {
    TierRefusal { tier: TIER, mind, reason, native: Native::NotAsked }
}

/// The fork refused: keep its status and name in the refusal.
fn native(mind: MindId, reason: &'static str, refusal: PersonaRefusal) -> TierRefusal {
    TierRefusal { tier: TIER, mind, reason, native: Native::Refused { status: refusal.status, name: refusal.name } }
}

/// A refused bind, by who refused: her blocks' stamp (another owner or a stale epoch) or the fork.
fn bind_refused(mind: MindId, refusal: BindRefusal) -> TierRefusal {
    match refusal {
        BindRefusal::WrongOwner { .. } => refused(mind, "her blocks are stamped for another owner or epoch"),
        // the pool has fewer empty cells than her rows: a seated mind evicted makes room
        BindRefusal::Native(full) if full.wants_room() => no_room(mind, "the pool has no room for her rows"),
        BindRefusal::Native(native_refusal) => native(mind, "the fork refused her bind", native_refusal),
    }
}

/// No room for her: a seated mind evicted makes it.
fn no_room(mind: MindId, reason: &'static str) -> TierRefusal {
    TierRefusal { tier: TIER, mind, reason, native: Native::NoRoom }
}
