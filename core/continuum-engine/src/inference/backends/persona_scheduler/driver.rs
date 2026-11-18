//! Th            engine.forget(gone).map_err(|e| e.to_string())?;
 driver thread: serves turns one after another on one context, through the persona engine.

use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use continuum_persona_engine::engine::PersonaEngine;
use continuum_persona_engine::persona::{Epoch, PersonaId};
use continuum_persona_engine::pipeline::Identified;
use continuum_persona_engine::turn::{plan, History, Prefill};
use continuum_persona_llama::{Genes, Step, UnifiedTier, STREAMS};
use llama::{ContextParams, LoraAdapter, Model};
use uuid::Uuid;

use super::{sampling, GenerationRequest, SchedulerConfig, TokenEvent};
use crate::inference::footprint_registry::{self, FootprintKey, ResourceType};
use crate::inference::kv_quant::Residency;
use crate::runtime;

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

type Engine<'c, 'm> = PersonaEngine<Mind, UnifiedTier<'c, 'm>>;

/// Run until every handle is dropped.
pub(super) fn run(model: &Model, config: &SchedulerConfig, requests: &Receiver<GenerationRequest>) {
    let log = runtime::logger("persona-scheduler");
    let streams = u32::try_from(STREAMS).unwrap_or(u32::MAX); // unwrap_or: STREAMS is 2
    let mut ctx = match model.new_context(ContextParams {
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
    }) {
        Ok(ctx) => ctx,
        Err(e) => {
            log.error(&format!("persona scheduler context refused: {e}"));
            for req in requests {
                let _ = req.response_tx.send(TokenEvent::Error(format!("persona scheduler has no context: {e}")));
            }
            return;
        }
    };
    log.info(&format!("persona scheduler ready (n_ctx={}, streams={STREAMS}, one mind decodes at a time)", config.n_ctx));
    let mut engine: Engine<'_, '_> = PersonaEngine::new(UnifiedTier::new(&mut ctx));
    let mut parked: HashMap<PersonaId, Mind> = HashMap::new();
    for req in requests {
        let respond = req.response_tx.clone();
        let event = serve(model, config, &mut engine, &mut parked, req).unwrap_or_else(|e| {
            log.warn(&format!("turn refused: {e}"));
            TokenEvent::Error(e)
        });
        let _ = respond.send(event);
    }
}

/// One turn: switch to her, prefill what her blocks do not hold, decode until she stops.
fn serve(
    model: &Model,
    config: &SchedulerConfig,
    engine: &mut Engine<'_, '_>,
    parked: &mut HashMap<PersonaId, Mind>,
    req: GenerationRequest,
) -> Result<TokenEvent, String> {
    let started = Instant::now();
    let one_off = req.persona_id.is_none();
    let id = PersonaId(req.persona_id.unwrap_or_else(Uuid::new_v4));
    // special=true so chat-template markers tokenize as the model's own special tokens
    let prompt = model.tokenize(&req.prompt, true, true)?;
    let basis = Basis { genes: signature(&req.active_loras) };
    let genes: Genes = req.active_loras.iter().map(|(_, adapter, scale)| (Arc::clone(adapter), *scale)).collect::<Vec<_>>().into();

    switch_to(engine, parked, id, one_off)?;
    let from = {
        let her = engine.active_mut().ok_or("no active mind after the switch")?;
        let from = match plan(History { basis: &her.basis, tokens: &her.held }, History { basis: &basis, tokens: &prompt }) {
            Prefill::Append { from } => from,
            Prefill::Restart => 0,
        };
        her.held.truncate(from);
        her.basis = basis;
        from
    };

    let mut sampler = sampling::sampler(model, &req.sampling);
    let mut pos = i32::try_from(from).map_err(|_| "her history is longer than a context position")?;
    let batch = usize::try_from(config.n_batch.max(1)).unwrap_or(1); // unwrap_or: n_batch fits usize
    let mut next = None;
    for chunk in prompt[from..].chunks(batch) {
        engine
            .submit(Step { tokens: chunk.to_vec(), from: pos, logits_at_end: true, genes: Arc::clone(&genes) })
            .map_err(|e| e.to_string())?;
        pos += i32::try_from(chunk.len()).map_err(|_| "a chunk longer than a batch")?;
        let last = i32::try_from(chunk.len() - 1).map_err(|_| "a chunk longer than a batch")?;
        next = Some(engine.tier().sample(&mut sampler, last));
    }
    record(engine, |her| her.held.extend_from_slice(&prompt[from..]))?;
    report_footprint(engine, id, one_off);

    let mut token = next.ok_or("nothing was decoded for her turn")?;
    let mut generated = 0usize;
    let mut output = String::new();
    loop {
        if model.is_eog_token(token) {
            break;
        }
        let piece = model.token_to_piece(token);
        output.push_str(&piece);
        let _ = req.response_tx.send(TokenEvent::Token(piece));
        generated += 1;
        if generated >= req.max_tokens || req.stop_sequences.iter().any(|stop| output.ends_with(stop)) {
            break;
        }
        engine.submit(Step { tokens: vec![token], from: pos, logits_at_end: true, genes: Arc::clone(&genes) }).map_err(|e| e.to_string())?;
        pos += 1;
        record(engine, |her| her.held.push(token))?;
        token = engine.tier().sample(&mut sampler, 0);
    }
    report_footprint(engine, id, one_off);
    Ok(TokenEvent::Done { tokens_generated: generated, elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX) }) // unwrap_or: a turn past u64 ms saturates
}

/// Make `id` the active mind: her parked state comes back (or a new mind starts), the previous mind
/// moves out to the RAM tier and is parked, or released if she was a one-off.
fn switch_to(engine: &mut Engine<'_, '_>, parked: &mut HashMap<PersonaId, Mind>, id: PersonaId, one_off: bool) -> Result<(), String> {
    if engine.active() == Some(id) {
        return Ok(());
    }
    let mind = parked.remove(&id).unwrap_or(Mind { id, epoch: Epoch(1), held: Vec::new(), basis: Basis::default(), one_off });
    engine.stage(mind).map_err(|refused| {
        let reason = refused.reason.to_string();
        parked.insert(refused.returned.id, refused.returned);
        reason
    })?;
    engine.settle().map_err(|e| e.to_string())?;
    engine.handoff().map_err(|e| e.to_string())?;
    if engine.retiring().is_some() {
        let previous = engine.release_retired().map_err(|e| format!("{e:?}"))?;
        if previous.one_off {
            let gone = previous.id;
            engine.forget(gone).map_err(|e| e.to_string())?;
            footprint_registry::global().remove(&FootprintKey::for_persona(gone.0, ResourceType::KvCache, Residency::Active), 0);
        } else {
            parked.insert(previous.id, previous);
        }
    }
    Ok(())
}

/// Update the active mind's own record (what her blocks now hold).
fn record(engine: &mut Engine<'_, '_>, change: impl FnOnce(&mut Mind)) -> Result<(), String> {
    change(engine.active_mut().ok_or("no active mind to record")?);
    Ok(())
}

/// Attribute her stream's bytes to her in the footprint registry (one-offs are not attributed).
fn report_footprint(engine: &Engine<'_, '_>, id: PersonaId, one_off: bool) {
    if one_off {
        return;
    }
    if let Some(bytes) = engine.tier().state_bytes_of(id).filter(|bytes| *bytes > 0) {
        footprint_registry::global().report_authoritative(FootprintKey::for_persona(id.0, ResourceType::KvCache, Residency::Active), bytes);
    }
}

/// A gene set's identity: sorted `id:scale`, comma-joined; empty is the base model.
fn signature(genes: &[(String, Arc<LoraAdapter>, f32)]) -> String {
    let mut parts: Vec<String> = genes.iter().map(|(id, _, scale)| format!("{id}:{scale}")).collect();
    parts.sort_unstable();
    parts.join(",")
}
