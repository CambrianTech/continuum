#![cfg(test)]

use super::*;

fn make_response(
    finish: FinishReason,
    text: &str,
    tool_calls: Option<Vec<ToolCall>>,
) -> TextGenerationResponse {
    TextGenerationResponse {
        text: text.to_string(),
        finish_reason: finish,
        model: "scripted".to_string(),
        provider: "scripted".to_string(),
        usage: UsageMetrics::default(),
        response_time_ms: 0,
        request_id: "scripted".to_string(),
        content: None,
        tool_calls,
        reasoning: None,
        routing: None,
        error: None,
        timing: None,
    }
}

fn read_tool() -> NativeToolSpec {
    NativeToolSpec {
        name: "code/read".to_string(),
        description: "Read a file from the workspace".to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: json!({ "path": { "type": "string" } }),
            required: Some(vec!["path".to_string()]),
            definitions: None,
        },
    }
}

/// Adapter that replays a canned response sequence and records every
/// request it received — so a test can assert the agent loop both
/// re-generated and threaded the tool results back into the re-prompt.
/// Lean (4 methods) because the trait now defaults the long tail.
struct ScriptedAdapter {
    responses: Mutex<VecDeque<TextGenerationResponse>>,
    seen: Mutex<Vec<TextGenerationRequest>>,
    rejections: Mutex<VecDeque<crate::ai::inference_error::InferenceError>>,
    live_window: Option<u32>,
    on_window_read: Mutex<Option<WindowReadHook>>,
    on_request: Mutex<Option<RequestHook>>,
}

type WindowReadHook = Box<dyn FnOnce() + Send>;

type RequestHook = Box<dyn FnMut(&TextGenerationRequest) -> Option<crate::ai::inference_error::InferenceError> + Send>;

impl ScriptedAdapter {
    fn new(responses: Vec<TextGenerationResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            seen: Mutex::new(Vec::new()),
            rejections: Mutex::new(VecDeque::new()),
            live_window: None,
            on_window_read: Mutex::new(None),
            on_request: Mutex::new(None),
        }
    }
    fn call_count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

#[async_trait]
impl AIProviderAdapter for ScriptedAdapter {
    fn provider_id(&self) -> &str {
        "scripted"
    }
    fn name(&self) -> &str {
        "scripted"
    }
    fn default_model(&self) -> &str {
        "scripted"
    }
    fn live_served_window(&self) -> Option<u32> {
        // Inject the exact load → observation → publication interleaving
        // without scheduler timing or a second adapter implementation.
        let hook = self
            .on_window_read
            .lock()
            .expect("fixture hook lock")
            .take();
        if let Some(hook) = hook {
            hook();
        }
        self.live_window
    }
    async fn generate_stream_checked(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>,
    ) -> Result<TextGenerationResponse, crate::ai::inference_error::InferenceError>
    {
        let injected = self.on_request.lock().expect("fixture request hook")
            .as_mut().and_then(|hook| hook(&request));
        if let Some(error) = injected {
            self.seen.lock().expect("fixture request log").push(request);
            return Err(error);
        }
        let rejection = self
            .rejections
            .lock()
            .expect("fixture rejection queue")
            .pop_front();
        if let Some(error) = rejection {
            self.seen.lock().expect("fixture request log").push(request);
            return Err(error);
        }
        self.generate_stream(request, sink)
            .await
            .map_err(Into::into)
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        self.seen.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| "scripted adapter exhausted".to_string())
    }
}

mod window_feedback;

mod capacity_and_wire;

/// A writing activity: history worth trading, then the ask. Shared by the flex
/// reply tests below so each compares one workspace shape.
fn writing_workspace(room: crate::identity::ActivityRoom, history: usize, deliverable: bool) -> Workspace {
    use crate::cognition::workspace::Burst;
    let mut turns: Vec<_> = (0..history)
        .map(|i| {
            BurstTurn::attributed(
                i % 2 == 1,
                if i % 2 == 1 { "Benchy" } else { "Peer" },
                format!("distinct_history_{i} ").repeat(60),
                Some(i as u64),
            )
        })
        .collect();
    turns.push(BurstTurn::attributed(
        false,
        "Peer",
        "Write the migration file now: create the accounts table with its indexes.",
        Some(history as u64 + 1),
    ));
    let mut ws = Workspace::new(Burst::from_turns(room, turns));
    ws.workspace_deliverable = deliverable;
    ws
}

mod act_verdicts;
