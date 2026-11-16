//! Prefill during the tool (card 377d4afa; CONTINUUM-ENGINE.md 2.4: the act is a dependency graph,
//! independent passes overlap).
//!
//! When her act dispatches a tool, the prompt she just sent is already known, and her view only
//! appends within an epoch, so it is the head of her next prompt (replay of the M5 captures,
//! 2026-11-13: every tool continuation's prompt began with the whole previous one). The
//! [`WarmPrefix`] pass prefills it into her page's slot WHILE the tool runs, through the same
//! adapter path and admission as any of her turns: a one-token generation with the prompt cache on,
//! holding no serving lane (it is adapter-owned work) and waiting for a decode permit like any
//! request. Where her slot kept her KV, the engine finds it whole and the pass costs one token;
//! where another mind took the slot, the pass pages or prefills it back before her result lands.
//!
//! Her next generation subscribes to the pass's artifact, never a timer: settled ready, it starts
//! on a warm prefix and the probe credits the prefill that ran inside the tool; failed, abandoned
//! or superseded, she prefills as before and the probe says why. Her next generation asks only once
//! her tool's result is in her view, so a pass already settled then was warm before the result.

use continuum_kernel::runtime::stage::{
    spawn_when_ready, Arrival, DependentStage, Inputs, Settled, Subscribes, TopicName,
};

use super::*;
use crate::ai::adapter::PromptCache;
use crate::ai::inference_error::InferenceError;
use crate::ai::types::{DispatchTerms, Think};
use crate::inference::slots::PageKey;

/// The artifact the warm pass produces and her next generation subscribes to.
const PREFIX_WARM: TopicName = TopicName("cognition/prefix.warm");

/// Her previous prompt, prefilled into her slot while her tool ran: the engine's own receipt.
#[derive(Debug)]
pub(super) struct PrefixWarm {
    /// The prompt tokens the slot now holds for her (reused plus prefilled by the pass).
    prefix_tokens: u32,
    /// What the pass prefilled: zero when her slot had kept her KV.
    prefill: std::time::Duration,
}

/// Why the pass left nothing to credit: her next generation prefills as it always did.
#[derive(Debug)]
pub(super) enum PrefixMiss {
    /// The engine refused or failed the pass.
    Refused(InferenceError),
    /// The engine answered without its timing receipt, so no warmth can be claimed.
    Unmeasured,
}

/// The warm pass. It depends on nothing but the dispatch that spawns it.
struct WarmPrefix {
    adapter: Arc<dyn AIProviderAdapter>,
    request: TextGenerationRequest,
}

#[async_trait]
impl DependentStage for WarmPrefix {
    type Inputs = ();
    type Produces = PrefixWarm;
    type Fails = PrefixMiss;

    fn produces(&self) -> TopicName {
        PREFIX_WARM
    }

    async fn run(self, _inputs: <() as Inputs>::Settled) -> Result<PrefixWarm, PrefixMiss> {
        let response = self
            .adapter
            .generate_under(self.request, DispatchTerms::thinking(Think::ModelDefault))
            .await
            .map_err(PrefixMiss::Refused)?;
        let timing = response.timing.ok_or(PrefixMiss::Unmeasured)?;
        let prefill = std::time::Duration::try_from_secs_f64(timing.prefill_ms / 1000.0)
            .map_err(|_| PrefixMiss::Unmeasured)?;
        Ok(PrefixWarm { prefix_tokens: timing.cached_tokens.saturating_add(timing.prefill_tokens), prefill })
    }
}

/// The pass in flight for her page, until her next generation takes it.
struct Warming {
    page: PageKey,
    warm: Subscribes<PrefixWarm, PrefixMiss>,
}

/// The faculty's one pending warm pass.
#[derive(Default)]
pub(super) struct PendingPrefix(parking_lot::Mutex<Option<Warming>>);

/// Her next generation found no warmth to credit, and why.
fn missed(reason: &'static str, persona: &str) {
    continuum_base::probe!(
        class = "delib.prefill.during_tool.missed",
        persona = %persona,
        reason = reason,
        "her prefix was not warmed during the tool: her next generation prefills it as before"
    );
}

impl LlmDeliberationFaculty {
    /// Her act dispatched its tool: warm the prompt she just sent (`template` with `messages`) into
    /// her page's slot while the tool runs. Never blocks the act; a newer pass supersedes an older
    /// one nobody took.
    pub(super) fn prefill_during_tool(
        &self,
        adapter: &Arc<dyn AIProviderAdapter>,
        room: Uuid,
        template: Option<&TextGenerationRequest>,
        messages: &[ChatMessage],
    ) {
        let (Some(page), Some(template), PromptCache::PinnedSlot) = (self.page(room), template, adapter.prompt_cache()) else {
            return;
        };
        let mut request = TextGenerationRequest::clone(template);
        request.messages = messages.to_vec();
        request.max_tokens = Some(1);
        request.request_id = Some(Uuid::new_v4().to_string());
        let pass = WarmPrefix { adapter: Arc::clone(adapter), request };
        let warm = spawn_when_ready(pass, ());
        if self.prefix_warm.0.lock().replace(Warming { page, warm }).is_some() {
            missed("superseded", &self.persona_name);
        }
    }

    /// Her next generation's subscription to the warm pass for `room`'s page: it starts when the
    /// pass has settled, whatever it settled with.
    pub(super) async fn prefix_warm_settled(&self, room: Uuid) {
        let page = self.page(room);
        let taken = {
            let mut pending = self.prefix_warm.0.lock();
            let ours = pending.as_ref().is_some_and(|warming| Some(warming.page) == page);
            if ours { pending.take() } else { None }
        };
        let Some(mut warming) = taken else {
            return;
        };
        let met = warming.warm.settled().await;
        match met.settled {
            Settled::Ready(warm) => {
                let (warmed_before_result, saved) = match met.arrival {
                    Arrival::AlreadySettled => (true, warm.prefill),
                    Arrival::WaitedFor(waited) => (false, warm.prefill.saturating_sub(waited)),
                };
                continuum_base::probe!(
                    class = "delib.prefill.during_tool",
                    persona = %self.persona_name,
                    prefix_tokens = warm.prefix_tokens as u64,
                    warmed_before_result = warmed_before_result,
                    saved_ms = saved.as_millis() as u64,
                    "her prefix was prefilled while her tool ran"
                );
            }
            Settled::Failed(miss) => match &*miss {
                PrefixMiss::Refused(error) => {
                    continuum_base::probe!(
                        class = "delib.prefill.during_tool.missed",
                        persona = %self.persona_name,
                        reason = "refused",
                        error = ?error,
                        "the engine refused her warm pass: her next generation prefills as before"
                    );
                }
                PrefixMiss::Unmeasured => missed("unmeasured", &self.persona_name),
            },
            Settled::Abandoned => missed("abandoned", &self.persona_name),
        }
    }
}
