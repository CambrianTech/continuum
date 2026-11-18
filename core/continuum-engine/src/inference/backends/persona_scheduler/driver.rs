//! The driver thread: serves turns one after another on one context, through the persona engine.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use continuum_persona_engine::engine::{DecodeError, HandoffError, PersonaEngine, StageError};
use continuum_persona_engine::persona::{Epoch, PersonaId};
use continuum_persona_engine::pipeline::{Identified, RetireRefusal};
use continuum_persona_engine::residency::TierRefusal;
use continuum_persona_engine::turn::{plan, History, Prefill};
use continuum_persona_llama::{Genes, Step, UnifiedTier, STREAMS};
use llama::{ContextParams, LoraAdapter, Model};
use uuid::Uuid;

use super::{sampling, GenerationRequest, SchedulerConfig, TokenEvent};
use crate::inference::footprint_registry::{self, FootprintKey, ResourceType};
use crate::inference::kv_quant::Residency;
use crate::runtime;

/// Why a turn did not complete. The mind stays in the engine's custody either way.
#[derive(Debug, thiserror::Error)]
pub(super) enum TurnError {
    #[error("her prompt did not tokenize: {0}")]
    Tokenize(String),
    #[error(transparent)]
    Stage(#[from] StageError),
    #[error(transparent)]
    Handoff(#[from] HandoffError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Tier(#[from] TierRefusal),
    #[error(transparent)]
    Retire(#[from] RetireRefusal),
    #[error("no mind is active after the switch")]
    NoneActive,
    #[error("her history is longer than a context position holds")]
    TooLong,
    #[error("nothing was decoded for her turn")]
    NothingDecoded,
}

/// The context could not be made, so no turn can run.
#[derive(Debug, thiserror::Error)]
enum StartError {
    #[error("the persona scheduler's context was refused: {0}")]
    Context(String),
    #[error("the persona scheduler's batch size is zero")]
    ZeroBatch,
}

/// What her blocks were computed under beside the token ids. The model is this context's own, and no
/// native media reaches this path (the prompt is text), so her gene set (ids and scales) is the rest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Basis {
    genes: String,
}

/// One mind this scheduler serves: her id and epoch, the tokens her blocks hold, and their basis.
struct Mind {
    id: PersonaId,
    epoch: Epoch,
    held: Vec<i32>,
    basis: Basis,
    /// A one-off (no persona): her blocks are released when her turn ends.
    one_off: bool,
}

impl Identified for Mind {
    fn persona(&self) -> PersonaId {
        self.id
    }
    fn epoch(&self) -> Epoch {
        self.epoch
    }
}

/// The driver's state: the model, the one engine over the one context, and every mind not active.
struct Driver<'c, 'm> {
    model: &'m Model,
    batch: NonZeroUsize,
    engine: PersonaEngine<Mind, UnifiedTier<'c, 'm>>,
    parked: HashMap<PersonaId, Mind>,
}

/// Run until every handle is dropped.
pub(super) fn run(model: &Model, config: &SchedulerConfig, requests: &Receiver<GenerationRequest>) {
    let log = runtime::logger("persona-scheduler");
    let mut ctx = match context(model, config) {
        Ok(ctx) => ctx,
        Err(refused) => {
            log.error(&refused.to_string());
            for req in requests {
                let _ = req.response_tx.send(TokenEvent::Error(refused.to_string()));
            }
            return;
        }
    };
    let Some(batch) = usize::try_from(config.n_batch).ok().and_then(NonZeroUsize::new) else {
        log.error(&StartError::ZeroBatch.to_string());
        for req in requests {
            let _ = req.response_tx.send(TokenEvent::Error(StartError::ZeroBatch.to_string()));
        }
        return;
    };
    log.info(&format!("persona scheduler ready (n_ctx={}, streams={STREAMS}, one mind decodes at a time)", config.n_ctx));
    let mut driver = Driver { model, batch, engine: PersonaEngine::new(UnifiedTier::new(&mut ctx)), parked: HashMap::new() };
    for req in requests {
        let respond = req.response_tx.clone();
        let event = match driver.serve(req) {
            Ok(done) => done,
            Err(refused) => {
                log.warn(&format!("turn refused: {refused}"));
                TokenEvent::Error(refused.to_string())
            }
        };
        let _ = respond.send(event);
    }
}

/// The one context: `STREAMS` sequences (the active mind's and the next one's).
fn context<'m>(model: &'m Model, config: &SchedulerConfig) -> Result<llama::Context<'m>, StartError> {
    let streams = u32::try_from(STREAMS).map_err(|_| StartError::Context("stream count exceeds a sequence count".into()))?;
    model
        .new_context(ContextParams {
            n_ctx: config.n_ctx,
            n_batch: config.n_batch,
            n_ubatch: config.n_ubatch,
            n_seq_max: streams,
            flash_attn: config.flash_attn,
            fused_gdn_ar: config.fused_gdn_ar,
            fused_gdn_ch: config.fused_gdn_ch,
            type_k: config.type_k,
            type_v: config.type_v,
            embeddings: false,
            pooling_type: llama::PoolingType::None,
            expert_observer: None,
        })
        .map_err(StartError::Context)
}

impl Driver<'_, '_> {
    /// One turn: switch to her, prefill what her blocks do not hold, decode until she stops.
    fn serve(&mut self, req: GenerationRequest) -> Result<TokenEvent, TurnError> {
        let started = Instant::now();
        let (id, one_off) = match req.persona_id {
            Some(persona) => (PersonaId(persona), false),
            None => (PersonaId(Uuid::new_v4()), true),
        };
        // special=true so chat-template markers tokenize as the model's own special tokens
        let prompt = self.model.tokenize(&req.prompt, true, true).map_err(TurnError::Tokenize)?;
        let basis = Basis { genes: signature(&req.active_loras) };
        let genes: Genes = req.active_loras.iter().map(|(_, adapter, scale)| (Arc::clone(adapter), *scale)).collect::<Vec<_>>().into();

        self.switch_to(id, one_off)?;
        let from = {
            let her = self.engine.active_mut().ok_or(TurnError::NoneActive)?;
            let from = match plan(History { basis: &her.basis, tokens: &her.held }, History { basis: &basis, tokens: &prompt }) {
                Prefill::Append { from } => from,
                Prefill::Restart => 0,
            };
            her.held.truncate(from);
            her.basis = basis;
            from
        };

        let mut sampler = sampling::sampler(self.model, &req.sampling);
        let mut pos = i32::try_from(from).map_err(|_| TurnError::TooLong)?;
        let mut next = None;
        for chunk in prompt[from..].chunks(self.batch.get()) {
            self.engine.submit(Step { tokens: chunk.to_vec(), from: pos, logits_at_end: true, genes: Arc::clone(&genes) })?;
            pos += i32::try_from(chunk.len()).map_err(|_| TurnError::TooLong)?;
            let last = i32::try_from(chunk.len() - 1).map_err(|_| TurnError::TooLong)?;
            next = Some(self.engine.tier().sample(&mut sampler, last));
        }
        self.record(|her| her.held.extend_from_slice(&prompt[from..]))?;
        self.report_footprint(id, one_off);

        let mut token = next.ok_or(TurnError::NothingDecoded)?;
        let mut generated = 0usize;
        let mut output = String::new();
        loop {
            if self.model.is_eog_token(token) {
                break;
            }
            let piece = self.model.token_to_piece(token);
            output.push_str(&piece);
            let _ = req.response_tx.send(TokenEvent::Token(piece));
            generated += 1;
            if generated >= req.max_tokens || req.stop_sequences.iter().any(|stop| output.ends_with(stop)) {
                break;
            }
            self.engine.submit(Step { tokens: vec![token], from: pos, logits_at_end: true, genes: Arc::clone(&genes) })?;
            pos += 1;
            self.record(|her| her.held.push(token))?;
            token = self.engine.tier().sample(&mut sampler, 0);
        }
        self.report_footprint(id, one_off);
        let took = started.elapsed();
        let elapsed_ms = took.as_secs().saturating_mul(1000).saturating_add(u64::from(took.subsec_millis()));
        Ok(TokenEvent::Done { tokens_generated: generated, elapsed_ms })
    }

    /// Make `id` the active mind: her parked state comes back (or a new mind starts), the previous
    /// mind moves out to the RAM tier and is parked, or forgotten if she was a one-off.
    fn switch_to(&mut self, id: PersonaId, one_off: bool) -> Result<(), TurnError> {
        if self.engine.active() == Some(id) {
            return Ok(());
        }
        let mind = match self.parked.remove(&id) {
            Some(parked) => parked,
            None => Mind { id, epoch: Epoch(1), held: Vec::new(), basis: Basis::default(), one_off },
        };
        if let Err(refused) = self.engine.stage(mind) {
            self.parked.insert(refused.returned.id, refused.returned);
            return Err(refused.reason.into());
        }
        self.engine.settle()?;
        self.engine.handoff()?;
        if self.engine.retiring().is_some() {
            let previous = self.engine.release_retired()?;
            if previous.one_off {
                self.engine.forget(previous.id)?;
            } else {
                self.parked.insert(previous.id, previous);
            }
        }
        Ok(())
    }

    /// Update the active mind's own record (what her blocks now hold).
    fn record(&mut self, change: impl FnOnce(&mut Mind)) -> Result<(), TurnError> {
        change(self.engine.active_mut().ok_or(TurnError::NoneActive)?);
        Ok(())
    }

    /// Attribute her stream's bytes to her in the footprint registry (one-offs are not attributed).
    fn report_footprint(&self, id: PersonaId, one_off: bool) {
        if one_off {
            return;
        }
        if let Some(bytes) = self.engine.tier().state_bytes_of(id).filter(|bytes| *bytes > 0) {
            footprint_registry::global().report_authoritative(FootprintKey::for_persona(id.0, ResourceType::KvCache, Residency::Active), bytes);
        }
    }
}

/// A gene set's identity: sorted `id:scale`, comma-joined; empty is the base model.
fn signature(genes: &[(String, Arc<LoraAdapter>, f32)]) -> String {
    let mut parts: Vec<String> = genes.iter().map(|(id, _, scale)| format!("{id}:{scale}")).collect();
    parts.sort_unstable();
    parts.join(",")
}
