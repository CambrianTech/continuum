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
mod batch;
pub use batch::{genes_key, same_genes};
mod graphs;
mod learn;
pub use kv_home::KvHome;
use graphs::{KeptGraphs, Resident};
use kv_home::KvMemory;
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
    /// She is one of the minds the tier serves for as long as her blocks live (unified_tier/graphs.rs).
    _resident: Resident,
}

/// Why a stop could not hand a mind's blocks over (UNIFIED-PAGING slice 2). The manifest's last
/// generation stands; the stop reports her as not preserved.
#[derive(Debug, thiserror::Error)]
pub enum HandOverRefusal {
    #[error("the fork refused to write her record: {0}")]
    Record(#[from] PersonaRefusal),
    #[error("her files were not made durable: {0}")]
    Io(#[from] std::io::Error),
}

impl Blocks {
    /// Hand her blocks to the next start (UNIFIED-PAGING slice 2, custody by rename). Her record goes to
    /// `record`, stamped with the stop's `generation` and made durable. Then, where her storage is her
    /// mapped file: the fork's view of it released, every dirty page written and the file synced, the
    /// mapping dropped and the file renamed to `kv`. Returns whether `kv` now holds her rows (otherwise her
    /// record does). Her unit is parked, never bound, when a caller holds her blocks; this consumes them,
    /// so nothing views her file after.
    pub fn hand_over(self, record: &std::path::Path, kv: &std::path::Path, generation: u64) -> Result<bool, HandOverRefusal> {
        self.native.write_file(record, generation)?;
        continuum_host::durable::fence_path(record)?;
        let Blocks { native, mut memory, .. } = self;
        drop(native); // the fork's tensors over her mapping go before the mapping does
        Ok(memory.hand_over(kv)?)
    }

    /// Hand her checkpoint over beside her blocks, before [`Self::hand_over`] consumes them: written to
    /// `path` for the stop's `generation` only while it is still valid against these blocks, and made
    /// durable, so a manifest names it only once it is whole.
    pub fn hand_over_checkpoint(&self, checkpoint: &PersonaCheckpoint, path: &std::path::Path, generation: u64) -> Result<(), HandOverRefusal> {
        checkpoint.write_file(&self.native, path, generation)?;
        continuum_host::durable::fence_path(path)?;
        Ok(())
    }

    /// Her checkpoint back into `into` from what the stop of `generation` handed over, once these blocks
    /// were restored from her record of it ([`UnifiedTier::restore`]): bound to her restored cells, so it
    /// rewinds as one taken before the stop.
    pub fn restore_checkpoint(&mut self, into: &mut PersonaCheckpoint, path: &std::path::Path, generation: u64) -> Result<(), PersonaRefusal> {
        into.read_file(&mut self.native, path, generation)
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
    /// Who occupies each execution stream, by her whole id: a stream holds one generation of her. One stream per
    /// mind the context can bind at once (its sequences: the fork's move needs a stream per bound mind).
    streams: Vec<Option<MindId>>,
    last_rebind: Option<Rebind>,
    /// The last submitted decode's landed flag, until the device is known to have finished it.
    in_flight: Option<Rc<Cell<bool>>>,
    /// The gene set on the context now (`None` before any step).
    applied_genes: Option<Genes>,
    /// Where each mind's KV storage lives; `Native` from the first mind on whose device cannot view
    /// host memory.
    home: KvHome,
    /// The one decode batch, sized to the engine's batch at construction and cleared each step.
    batch: Batch,
    /// A decode graph kept for each mind it serves, and whom its stream serves now (fork #170).
    graphs: KeptGraphs,
}

impl<'c, 'm> UnifiedTier<'c, 'm> {
    pub fn new(ctx: &'c mut Context<'m>, home: KvHome) -> Self {
        let batch = Batch::allocated(i32::try_from(ctx.n_batch()).unwrap_or(i32::MAX), 1); // unwrap_or: a batch past i32 is the most llama can address
        // Her lessons replay the routing she served (IN-PROCESS-LEARNING.md), so the tier records it from
        // her first step. A context that cannot record has its lessons refused by the engine, by name.
        match ctx.record_routing() {
            Ok(capture) => continuum_base::probe!(class = "persona.routing.capture", capture = ?capture, "the tier records the routing its minds serve"),
            Err(refused) => continuum_base::probe!(class = "persona.routing.capture_refused", refused = %refused,
                "the tier cannot record its minds' routing; their lessons on this base are refused until it can"),
        }
        let streams = vec![None; ctx.n_seq_max() as usize]; // u32 to usize: lossless on every target this runs on
        Self { ctx, streams, last_rebind: None, in_flight: None, applied_genes: None, home, batch, graphs: KeptGraphs::new() }
    }

    /// What the context holds now, per allocator (UNIFIED-PAGING.md, slice 3b).
    pub fn held(&self, into: &mut llama::Held) -> Result<(), llama::Unaccounted> {
        self.ctx.held(into)
    }

    /// Her KV memory: a block over her own file, attached so the stream views it, where this node's
    /// device can view host memory. Where it cannot (a discrete GPU), none of her own: bound, her rows are
    /// the stream's own memory, and they move out and back by copy, so the context's one backing is the
    /// only device memory any mind uses (UNIFIED-PAGING.md, "A mind's memory").
    fn kv_memory(&mut self, mind: MindId, native: &mut PersonaBlocks) -> Result<KvMemory, TierRefusal> {
        let KvHome::Mapped { instance } = &mut self.home else { return Ok(KvMemory::Native) };
        let dir = instance.dir().to_path_buf();
        let bytes = PersonaBlocks::storage_bytes(self.ctx);
        if bytes == 0 {
            continuum_base::probe!(class = "persona.kv_home.native", "this context is not the Core's shape: minds' KV moves by row copy");
            self.home = KvHome::Native;
            return Ok(KvMemory::Native);
        }
        if !PersonaBlocks::storage_viewable(self.ctx) {
            return Ok(KvMemory::Native);
        }
        let path = dir.join(live_name(mind));
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

    /// Her blocks back from what a stop handed over (UNIFIED-PAGING slice 2), bound when she is next
    /// scheduled. With her KV file (`kv`): mapped where the stop left it, attached as her storage, and her
    /// record read over it, so the fork checks owner, epoch, generation, layout and the storage's bytes
    /// before anything moves. Only then is the file renamed into this core's directory: a refused
    /// restore leaves it where the manifest names it. Without one, her record carries her rows (a device
    /// without attachable storage, or a format-1 record migrated once) and they are copied in.
    pub fn restore(&mut self, mind: MindId, record: &std::path::Path, kv: Option<&std::path::Path>, generation: u64) -> Result<Blocks, TierRefusal> {
        let Some(kv) = kv else {
            let mut blocks = self.blocks_for(mind)?;
            blocks.native.read_file(record, generation).map_err(|e| native(mind, "the fork refused her record", e))?;
            blocks.saved = Saved::Restored;
            return Ok(blocks);
        };
        let KvHome::Mapped { instance, .. } = &self.home else { return Err(refused(mind, "her KV file needs a mapped home on this node")) };
        let live = live_file(instance, mind);
        let bytes = PersonaBlocks::storage_bytes(self.ctx);
        if bytes == 0 || !PersonaBlocks::storage_viewable(self.ctx) {
            return Err(refused(mind, "this node's device cannot view her KV file"));
        }
        // her mapping first, so on any refusal below the fork's blocks over it drop before it does
        let (mut memory, at, len) = KvMemory::handed(mind, kv.to_path_buf(), bytes).map_err(|_| refused(mind, "her KV file is absent or not the length her storage maps"))?;
        let mut native = PersonaBlocks::new(self.ctx, owner_of(mind)).ok_or_else(|| refused(mind, "this context's memory has no persona move"))?;
        // SAFETY: `memory` owns the mapping and is held in her `Blocks` after `native`, so it outlives the fork
        // blocks that view it; nothing but the fork writes it.
        unsafe { native.attach_storage(self.ctx, at, len) }.map_err(|refusal| self::native(mind, "the fork refused her KV file as her storage", refusal))?;
        native.read_file(record, generation).map_err(|e| self::native(mind, "the fork refused her record", e))?;
        memory.adopt(&live).map_err(|_| refused(mind, "her KV file could not be moved into this core's directory"))?;
        Ok(Blocks { native, saved: Saved::Restored, memory, _resident: self.graphs.resident() })
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
        self.graphs.bind(self.ctx, stream.seq(), owner_of(mind).persona);
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
        Ok(Blocks { native, saved: Saved::Never, memory, _resident: self.graphs.resident() })
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
    pub tokens: Tokens,
    pub from: Pos,
    pub logits_at_end: bool,
    /// The gene set she decodes under. The tier applies it only when it differs from the set
    /// already on the context, and only for her: no step is decoded under another mind's genes.
    pub genes: Genes,
}

/// The tokens of a step, named, never copied: the one token she just sampled, or a span of her
/// prompt, which is shared once at admission (a chunk is a range of it, never a `to_vec`; a decoded
/// token was a fresh `vec!` every step).
#[derive(Clone)]
pub enum Tokens {
    One(i32),
    Span { of: Arc<[i32]>, from: usize, to: usize },
}

impl Tokens {
    /// A span over its own copy of `tokens`: for a caller holding no shared prompt (an example, a
    /// replay); the serving path names spans of her shared prompt instead.
    pub fn copied(tokens: &[i32]) -> Self {
        Self::Span { of: Arc::from(tokens), from: 0, to: tokens.len() }
    }

    pub fn as_slice(&self) -> &[i32] {
        match self {
            Self::One(token) => std::slice::from_ref(token),
            Self::Span { of, from, to } => of.get(*from..*to).unwrap_or(&[]), // unwrap_or: a span past her prompt names nothing, refused below as an empty step
        }
    }
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

    /// Her step is a batch of one (unified_tier/batch.rs): one decode path for one mind or several.
    fn submit(&mut self, mind: MindId, step: Step) -> Result<Decoded, TierRefusal> {
        self.decode_steps(&[(mind, &step)])
    }

    fn submit_batch(&mut self, steps: &[(MindId, &Step)]) -> Result<Decoded, TierRefusal> {
        self.decode_steps(steps)
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

    fn clear(&mut self, mind: MindId) -> Result<(), TierRefusal> {
        let stream = self.stream_of(mind).ok_or_else(|| refused(mind, "not bound"))?;
        self.settle()?;
        if !self.ctx.memory_seq_truncate(stream.seq(), Pos::ZERO) || self.ctx.memory_seq_pos_max(stream.seq()).is_some() {
            return Err(refused(mind, "the engine could not empty her sequence"));
        }
        Ok(())
    }
}

const TIER: &str = "unified";

/// Her KV file's name in a core's own directory: one per generation of her, so an earlier epoch never maps it.
fn live_name(mind: MindId) -> String {
    format!("{}.{}.kv", mind.persona.0, mind.epoch.0)
}

/// Her KV file in `instance`'s directory.
fn live_file(instance: &continuum_blocks::Instance, mind: MindId) -> std::path::PathBuf {
    instance.dir().join(live_name(mind))
}

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
