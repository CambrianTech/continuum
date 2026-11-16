#![cfg(test)]

//! Prefill during the tool (`prefill_during_tool.rs`): her act warms the prompt she just sent
//! into her slot while the tool runs, and her next generation starts only once that warm pass
//! has settled. Event order only, never elapsed time.

use super::*;

fn act() -> TextGenerationResponse {
    let call = ToolCall { id: "t1".to_string(), name: "code/read".to_string(), input: json!({ "path": "deploy.md" }) };
    make_response(FinishReason::ToolUse, "", Some(vec![call]))
}

fn warmed() -> TextGenerationResponse {
    let mut warm = make_response(FinishReason::Length, "", None);
    warm.timing = Some(crate::ai::types::GenerationTiming { cached_tokens: 900, ..Default::default() });
    warm
}

fn ivar(adapter: Arc<ScriptedAdapter>) -> LlmDeliberationFaculty {
    LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
        .with_tools(vec![read_tool()])
        .with_context_window(32_768)
}

fn caching(responses: Vec<TextGenerationResponse>) -> Arc<ScriptedAdapter> {
    Arc::new(ScriptedAdapter { prompt_cache: crate::ai::adapter::PromptCache::PinnedSlot, ..ScriptedAdapter::new(responses) })
}

// what this catches: the warm pass sending anything but the prompt she just sent (then the
// engine warms a prefix her next prompt does not begin with), and her next generation reaching
// the engine before the pass settled (then the two race for her slot and the pass buys nothing).
#[tokio::test]
async fn her_next_generation_starts_after_the_pass_warmed_the_prompt_she_sent() {
    let adapter = caching(vec![act(), warmed(), make_response(FinishReason::Stop, "deploy.md says it landed", None)]);
    let faculty = ivar(Arc::clone(&adapter));
    let ws = Workspace::new("teammate asks: did the deploy fix land?");
    let first = faculty.contribute(&ws).await.expect("verdict");
    assert!(matches!(first.decision, Some(Decision::Act { .. })), "her act dispatches the tool");
    faculty.contribute(&ws).await.expect("verdict");

    let seen = adapter.seen.lock().expect("fixture request log");
    assert_eq!(seen.len(), 3, "her act, the warm pass, then her next generation");
    assert_eq!(seen[1].max_tokens, Some(1), "the pass only prefills");
    let sent = |at: usize| serde_json::to_value(&seen[at].messages).ok();
    assert_eq!(sent(1), sent(0), "the pass warms exactly the prompt she sent");
    assert_ne!(seen[2].max_tokens, Some(1), "her next generation came after the pass settled");
}

// what this catches: a failed pass that blocks or drops her next generation: she must prefill
// normally, and an adapter that keeps no prompt must never be sent a pass at all.
#[tokio::test]
async fn a_refused_pass_degrades_to_a_normal_prefill_and_no_cache_means_no_pass() {
    let adapter = caching(vec![act(), make_response(FinishReason::Stop, "done", None)]);
    *adapter.on_request.lock().expect("fixture hook") = Some(Box::new(|request: &TextGenerationRequest| {
        (request.max_tokens == Some(1)).then(|| crate::ai::inference_error::InferenceError::Unavailable("engine refused the pass".into()))
    }));
    let faculty = ivar(Arc::clone(&adapter));
    let ws = Workspace::new("teammate asks: did the deploy fix land?");
    faculty.contribute(&ws).await.expect("verdict");
    let next = faculty.contribute(&ws).await.expect("her next generation still runs");
    assert!(next.decision.is_some(), "the refused pass cost her nothing but the pass");
    assert_eq!(adapter.call_count(), 3, "her act, the refused pass, her next generation");

    let plain = Arc::new(ScriptedAdapter::new(vec![act(), make_response(FinishReason::Stop, "done", None)]));
    let faculty = ivar(Arc::clone(&plain));
    faculty.contribute(&ws).await.expect("verdict");
    faculty.contribute(&ws).await.expect("verdict");
    assert_eq!(plain.call_count(), 2, "no prompt cache, no pass");
}
