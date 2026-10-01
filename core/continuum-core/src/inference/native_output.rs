//! Native Chat Completions audio output. Uses the existing bound model and HTTP
//! lane; never calls a speech backend or substitutes a transcript for audio.

use base64::Engine;
use serde_json::{json, Value};

use super::sse_stream::StreamOutcome;
use crate::ai::types::{
    AudioInput, ContentPart, ModelInfo, NativeOutputRequest, TextGenerationRequest,
};
use crate::model_registry::Capability;

/// Select the implemented wire format from explicit request data. No model or
/// provider-name inference and no default voice. Image output needs its own wire.
fn audio_request(request: &TextGenerationRequest) -> Result<Option<(&str, &str, &str)>, String> {
    let Some(outputs) = request.native_output.as_deref().filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let [NativeOutputRequest::Audio { mime_type, voice }] = outputs else {
        return Err("Chat Completions native output currently requires exactly one audio output; image/mixed output transport is not implemented".into());
    };
    let format = match mime_type.as_str() {
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        "audio/flac" => "flac",
        "audio/aac" => "aac",
        "audio/opus" => "opus",
        _ => return Err(format!("Unsupported native audio encoding '{mime_type}'")),
    };
    let voice = voice.as_deref().filter(|v| !v.trim().is_empty())
        .ok_or("Chat Completions audio transport requires an explicit native voice; no stock voice is selected")?;
    Ok(Some((mime_type, format, voice)))
}

pub(crate) fn configure_body(
    body: &mut Value,
    request: &TextGenerationRequest,
    model: &ModelInfo,
) -> Result<(), String> {
    let Some((_, format, voice)) = audio_request(request)? else {
        return Ok(());
    };
    if !model.has(Capability::AudioOutput) {
        return Err(format!(
            "Bound model '{}' does not declare AudioOutput",
            model.id
        ));
    }
    let body = body
        .as_object_mut()
        .ok_or("Native audio request body must be an object")?;
    body.insert("modalities".into(), json!(["text", "audio"]));
    body.insert("audio".into(), json!({"format":format, "voice":voice}));
    // Explicit whole-response transport. Incremental audio requires a typed media
    // stream and is refused at the adapter boundary until that consumer exists.
    body.insert("stream".into(), json!(false));
    body.remove("stream_options");
    Ok(())
}

pub(crate) async fn consume_response(
    response: reqwest::Response,
    request: &TextGenerationRequest,
) -> Result<StreamOutcome, String> {
    let body = response
        .json::<Value>()
        .await
        .map_err(|e| format!("Invalid native audio response: {e}"))?;
    decode_response(&body, request)
}

pub(crate) fn decode_response(
    body: &Value,
    request: &TextGenerationRequest,
) -> Result<StreamOutcome, String> {
    let (mime, _, _) =
        audio_request(request)?.ok_or("Native audio response requires explicit output intent")?;
    let choices = body["choices"]
        .as_array()
        .ok_or("Native audio response has no choices")?;
    if choices.len() != 1 {
        return Err("Native audio response must contain exactly one choice".into());
    }
    let choice = &choices[0];
    let message = &choice["message"];
    let data = message["audio"]["data"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(
            "Native audio response contains no audio bytes; transcript/text is not a substitute",
        )?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("Invalid native audio base64: {e}"))?;
    if bytes.is_empty() {
        return Err("Native audio response decoded to zero bytes".into());
    }
    if message["tool_calls"]
        .as_array()
        .is_some_and(|calls| !calls.is_empty())
    {
        return Err("Combined native audio/tool output transport is not implemented".into());
    }
    let finish = choice["finish_reason"]
        .as_str()
        .ok_or("Native audio response lacks a completion reason")?;
    if finish != "stop" {
        return Err(format!(
            "Native audio did not complete successfully: {finish}"
        ));
    }
    let model = body["model"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Native audio response lacks model identity")?;
    let mut parts = vec![ContentPart::Audio {
        audio: AudioInput {
            base64: Some(data.to_owned()),
            url: None,
            mime_type: Some(mime.to_owned()),
        },
    }];
    // The native model's accompanying transcript is retained alongside its bytes,
    // never produced by a translator and never sufficient to pass audio validation.
    if let Some(transcript) = message["audio"]["transcript"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        parts.push(ContentPart::Text {
            text: transcript.to_owned(),
        });
    }
    Ok(StreamOutcome {
        acc_content: message["content"].as_str().unwrap_or_default().to_owned(),
        acc_reasoning: String::new(),
        acc_tools: Vec::new(),
        acc_parts: parts,
        finish_reason_str: Some(finish.to_owned()),
        stream_usage: body
            .get("usage")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|e| format!("Invalid native audio usage: {e}"))?,
        stream_timings: None,
        resp_model: Some(model.to_owned()),
        probe_persona: request
            .persona_id
            .clone()
            .unwrap_or_else(|| "non-persona".into()),
    })
}
