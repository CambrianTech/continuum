#![cfg(test)]

use super::*;
use crate::inference::lane_send::PRE_STREAM_HEADER_TIMEOUT_SECS;
use crate::inference::request_body::close_trailing_assistant;
use crate::inference::serving_guard::{
    settled_on_another_model, snapshot_guarantees, unguaranteed_model_refusal,
};
use crate::inference::sse_stream::{OpenAIStreamChunk, STREAM_IDLE_TIMEOUT_SECS};

use crate::ai::types::{ChatMessage, ImageInput, MessageContent};

use super::embedding::{build_embedding_body, parse_embedding_response, parse_embedding_usage};

mod a_ready_burst_reaches_the_consumer_one_chunk_per_poll;
mod a_stale_lease_posts_nothing;
mod lane_transport;
mod lora_page_in;
mod only_a_finish_frame_is_a_terminal_receipt;
mod outer_lane_custody;
mod prefill_progress_is_liveness;
mod request_shaping;
mod tail_tools_reach_the_verified_lane_only;
mod unguaranteed_model_refusal_says_which_situation;

/// Minimal adapter for pure payload-assembly tests — no network, no registry.
fn test_adapter() -> OpenAICompatibleAdapter {
    OpenAICompatibleAdapter::new(OpenAICompatibleConfig {
        provider_id: "test-gateway".into(),
        name: "Test Gateway".into(),
        base_url: "http://127.0.0.1:0".into(),
        api_key_env: None,
        default_model: "test-model".into(),
        capabilities: std::collections::BTreeSet::new(),
        models: Vec::new(),
        model_prefixes: Vec::new(),
        requires_auth: false,
        tool_protocol: crate::model_registry::ToolProtocol::NativeFunctionCalling,
        single_resident_model: false,
        dynamic_model_catalog: false,
        llamacpp_sampling_extensions: false,
        tail_tools: false,
    })
}
