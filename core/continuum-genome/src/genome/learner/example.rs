//! One example as the engine learns it: the conversation she was served and the reply to learn
//! (card 4114a2c6). The one owner of this framing; the batch trainer reads it until it is deleted.

use crate::genome::fine_tuning::types::TrainingExample;
use crate::inference::request_body::{close_trailing_assistant, openai_tools, wire_messages, wire_tool_call};
use serde_json::{json, Value};

/// The messages and tools `/learn` receives for one example.
#[derive(Debug, Clone, PartialEq)]
pub struct LearnMessages {
    pub messages: Vec<Value>,
    pub tools: Option<Value>,
}

/// Frame `example` for the engine. A lived call goes as its served conversation: the system prompt
/// and every served message exactly as serving framed them (`wire_messages`: her earlier tool calls
/// kept, tool results as `role: tool`), then her reply with its reasoning and tool calls; earlier
/// assistant turns are context (`"train": false`), not this lesson, and the tools she was offered
/// ride along so the rendered prompt has the tool block she saw. Anything else is a user prompt and
/// the assistant completion, the framing the engine gives a prompt/completion pair.
pub fn learn_messages(example: &TrainingExample) -> LearnMessages {
    let Some(call) = example.lived.as_ref() else {
        return LearnMessages {
            messages: vec![json!({"role": "user", "content": example.prompt}), json!({"role": "assistant", "content": example.completion})],
            tools: None,
        };
    };
    // the history exactly as serving framed it; images drop (learning is text-only, and a text
    // model was served the description bridge anyway)
    let mut messages = wire_messages(&call.request.messages, call.request.system_prompt.as_deref(), false, false, "learner");
    // serving closes a history that ends in her own turn before she replies (a self-tick's
    // continuation); without it the learned render has two assistant turns in a row
    close_trailing_assistant(&mut messages);
    for message in messages.iter_mut().filter(|m| m["role"] == "assistant") {
        message["train"] = json!(false);
    }
    let reply = &call.response;
    let calls: Vec<Value> = reply.tool_calls.iter().flatten().map(|c| wire_tool_call(&c.id, &c.name, &c.input)).collect();
    let mut answer = json!({"role": "assistant", "content": reply.text});
    if let Some(reasoning) = reply.reasoning.as_deref().filter(|s| !s.is_empty()) {
        answer["reasoning_content"] = json!(reasoning);
    }
    if !calls.is_empty() {
        answer["tool_calls"] = json!(calls);
    }
    messages.push(answer);
    LearnMessages { messages, tools: call.request.tools.as_deref().filter(|t| !t.is_empty()).map(|tools| json!(openai_tools(tools))) }
}
