//! [`UnifiedTier`]: the persona engine's residency tier on one libllama context, the Core's (object
//! model 4.11). One execution stream, the occupant's, holding the whole context: the mind swapped in
//! has all of it. Every other mind's blocks stay in host memory exactly as she left them. A swap moves
//! the occupant out first (the fork's native save: her rows copied to her blocks, her stream emptied)
//! and then moves the next mind in (its native bind into the emptied stream). Both are synchronous
//! copies, so the fence a move returns has already landed.
//!
//! One stream, because the fork moves a mind by copying her own stream's rows (it requires per-stream
//! KV), and llama.cpp sizes each stream at the context divided by the stream count: a second stream to
//! bind the next mind into would halve the occupant's context, and buys nothing while a swap is a
//! synchronous copy (2026-11-21: a 24,320-token half held seven dying turns while the other half held
//! nobody decoding). The next mind's prefetch lives in host memory, where her blocks already are.

use std::cell::Cell;

use std::rc::Rc;
use std::sync::Arc;

use continuum_persona_engine::persona::MindId;
use continuum_persona_engine::residency::{Completion, Direction, Execute, Fence, FenceTag, Native, Tier, TierRefusal};
use llama::{Batch, BindRefusal, ChatRefusal, Context, LoraAdapter, PersonaBlocks, PersonaCheckpoint, PersonaOwner, PersonaRefusal, Pos, TurnSampler};

mod kv_home;
mod learn;
pub use kv_home::KvHome;
use kv_home::KvMemory;
pub use learn::LearnReply;

/// Execution streams the Core's context holds: the occupant's, which is the whole context. Not a
/// tuning number: the fork's move needs a stream per bound mind, and the Core binds only its occupant.
pub const STREAMS: usize = 1;

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

/// Her blocks: the unit's blocks part (CONTINUUM-ENGINE 4.17). The native blocks her moves copy,
/// stamped with her owner at creation, and what they hold. The unit carries them; the tier moves
/// them when handed them and keeps no copy of who she is.
pub struct Blocks {
    native: PersonaBlocks,
    saved: Saved,
    /// Her KV storage's memory (her mapped file, or none: the fork's own). Declared after `native`, so it
    /// drops after the fork blocks that view it.
    memory: KvMemory,
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
    streams: [Option<MindId>; STREAMS],
    last_rebind: Option<Rebind>,
    /// The last submitted decode's landed flag, until the device is known to have finished it.
    in_flight: Option<Rc<Cell<bool>>>,
    /// The gene set on the context now (`None` before any step).
    applied_genes: Option<Genes>,
    /// Where each mind's KV storage lives; `Native` from the first mind on whose device cannot view
    /// host memory.
    home: KvHome,
}

impl<'c, 'm> UnifiedTier<'c, 'm> {
    pub fn new(ctx: &'c mut Context<'m>, home: KvHome) -> Self {
        Self { ctx, streams: [None; STREAMS], last_rebind: None, in_flight: None, applied_genes: None, home }
    }

    /// Her KV memory: a block over her own file, attached so the stream views it, where this node's
    /// device can view host memory; the fork's own otherwise (the fork's `storage_bytes` is 0 then, and
    /// the tier keeps that answer for every later mind).
    fn kv_memory(&mut self, mind: MindId, native: &mut PersonaBlocks) -> Result<KvMemory, TierRefusal> {
        let KvHome::Mapped(dir) = &self.home else { return Ok(KvMemory::Native) };
        let bytes = PersonaBlocks::storage_bytes(self.ctx);
        if bytes == 0 {
            continuum_base::probe!(class = "persona.kv_home.native", "this context is not the Core's shape: minds' KV moves by row copy");
            self.home = KvHome::Native;
            return Ok(KvMemory::Native);
        }
        if !PersonaBlocks::storage_viewable(self.ctx) {
            // a device that cannot wrap host memory (CUDA): her storage in the fork's own device memory, viewed the
            // same way; where it does not fit she moves by row copy until room is made (the Placer's decision)
            return Ok(match native.reserve_storage(self.ctx) {
                Ok(()) => KvMemory::Device,
                Err(refusal) => {
                    continuum_base::probe!(class = "persona.kv_home.no_device_room", persona = %mind.persona.0, why = %refusal, bytes = bytes as u64, "her KV storage did not fit in device memory: she moves by row copy");
                    KvMemory::Native
                }
            });
        }
        let path = dir.join(format!("{}.{}.kv", mind.persona.0, mind.epoch.0));
        let (memory, at, len) = KvMemory::mapped(mind, path, bytes).map_err(|_| refused(mind, "her KV file could not be mapped"))?;
        // SAFETY: `memory` owns the mapping and is held in her `Blocks` after `native`, so it outlives the
        // fork blocks that view it; nothing but the fork writes it.
        unsafe { native.attach_storage(self.ctx, at, len) }.map_err(|refusal| self::native(mind, "the fork refused her KV storage", refusal))?;
        Ok(memory)
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

    /// How the last move in met her blocks; `None` when she had never been saved.
    pub fn last_rebind(&self) -> Option<Rebind> {
        self.last_rebind
    }

    /// Her blocks read back from `path` (CONTINUUM-ENGINE 4.16), bound when she is next scheduled. A
    /// file of another owner, epoch or memory layout is refused and nothing is made.
    pub fn read_file(&mut self, mind: MindId, path: &std::path::Path) -> Result<Blocks, TierRefusal> {
        let mut blocks = self.blocks_for(mind)?;
        blocks.native.read_file(path).map_err(|e| native(mind, "the fork refused her file", e))?;
        blocks.saved = Saved::Restored;
        Ok(blocks)
    }

    fn move_in(&mut self, mind: MindId, blocks: &mut Blocks) -> Result<(), TierRefusal> {
        if self.streams.iter().flatten().any(|held| held.persona == mind.persona) {
            return Err(refused(mind, "already bound"));
        }
        let slot = self.streams.iter().position(Option::is_none).ok_or_else(|| refused(mind, "the occupant has not moved out"))?;
        let stream = stream_at(slot);
        // A mind with nothing to bind starts on this stream as it is, so it must hold nothing (the
        // context may have been in use before this tier wrapped it). The engine's own memory is the
        // evidence, never this tier's bookkeeping.
        if blocks.saved == Saved::Never && self.ctx.memory_seq_pos_max(stream.seq()).is_some() {
            return Err(refused(mind, "her stream already holds another sequence's state"));
        }
        self.last_rebind = None;
        // a mind with her own KV storage is bound even before her first save, so she decodes straight into
        // it from her first token (never into the stream's own memory, to be carried over at her first park)
        if blocks.saved == Saved::Never && blocks.memory.held() {
            blocks.native.bind_fresh(self.ctx, stream.seq()).map_err(|e| bind_refused(mind, e))?;
        }
        if blocks.saved != Saved::Never {
            blocks.native.bind(self.ctx, stream.seq(), owner_of(mind)).map_err(|e| bind_refused(mind, e))?;
            self.last_rebind = Some(match blocks.saved {
                Saved::From(from) if from == stream => Rebind::SameSequence,
                Saved::Restored => Rebind::FromFile,
                _ => Rebind::OtherSequence,
            });
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
        blocks.saved = Saved::From(stream);
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
        let mut native = PersonaBlocks::new(self.ctx, owner_of(mind)).ok_or_else(|| refused(mind, "this context's memory has no persona move"))?;
        let memory = self.kv_memory(mind, &mut native)?;
        Ok(Blocks { native, saved: Saved::Never, memory })
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

    fn submit(&mut self, mind: MindId, step: Step) -> Result<Decoded, TierRefusal> {
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
            applied.map_err(|message| TierRefusal { tier: TIER, mind, reason: "the engine refused her genes", native: Native::Reported(message.into()) })?;
            self.applied_genes = Some(Arc::clone(&step.genes));
        }
        let last = step.tokens.len().checked_sub(1).ok_or_else(|| refused(mind, "an empty step"))?;
        let count = i32::try_from(step.tokens.len()).map_err(|_| refused(mind, "a step longer than a batch can hold"))?;
        let mut batch = Batch::allocated(count, 1);
        for (i, &token) in step.tokens.iter().enumerate() {
            let pos = step.from.advanced(i).map_err(|_| refused(mind, "a step past what a sequence can address"))?;
            batch.push(token, pos, &[stream.seq()], step.logits_at_end && i == last);
        }
        self.ctx.decode(&batch).map_err(|message| {
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
                native = %message,
                "the engine refused a decode: what it held then"
            );
            TierRefusal { tier: TIER, mind, reason: "the engine refused her decode", native: Native::Reported(message.into()) }
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
        BindRefusal::NotFresh { .. } => refused(mind, "her blocks were saved before: a fresh bind is only for blocks never saved"),
        BindRefusal::Native(native_refusal) => native(mind, "the fork refused her bind", native_refusal),
    }
}
