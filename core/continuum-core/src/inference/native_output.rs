//! Native media wire normalization. Lifecycle and delivery belong to the shared
//! inference stream; this module never collects a whole audio response.
use base64::Engine;
use serde_json::{json, Value};
use crate::ai::types::{ModelInfo, NativeOutputRequest, TextGenerationRequest};
use crate::ai::adapter::GenerationChunk;
use crate::ai::stream_sinks::{GenerationSink, MediaChunk, MAX_GENERATION_CHUNK_BYTES};
use crate::model_registry::Capability;

/// This wire's streaming PCM contract, not a model-name capability guess.
pub(crate) const PCM_MIME: &str = "audio/pcm;rate=24000;channels=1;format=s16le";

pub(crate) fn requested(request: &TextGenerationRequest) -> bool {
    request.native_output.as_ref().is_some_and(|v| !v.is_empty())
}

pub(crate) fn configure_body(body: &mut Value, request: &TextGenerationRequest, model: &ModelInfo) -> Result<(), String> {
    if !requested(request) { return Ok(()); }
    let [NativeOutputRequest::Audio { mime_type, voice }] = request.native_output.as_deref().unwrap() else {
        return Err("Native image/mixed output streaming transport is not implemented".into());
    };
    if !model.has(Capability::AudioOutput) { return Err(format!("Bound model '{}' does not declare AudioOutput", model.id)); }
    if mime_type != PCM_MIME { return Err(format!("Native audio streaming requires explicit '{PCM_MIME}'; no batch encoding or transcoding")); }
    let voice = voice.as_deref().filter(|s| !s.trim().is_empty()).ok_or("Native audio wire requires an explicit bound voice selector")?;
    body["modalities"] = json!(["text", "audio"]);
    body["audio"] = json!({"format":"pcm16", "voice":voice});
    body["stream"] = json!(true);
    Ok(())
}

/// Per-stream codec cursor only. No task, timer, channel or full-output buffer.
#[derive(Default)]
pub(crate) struct AudioCursor { sequence: u64, samples: u64 }
impl AudioCursor {
    pub fn push(&mut self, audio: &Value, sink: &GenerationSink) -> Result<bool, String> {
        let Some(data) = audio.get("data") else { return Ok(false); };
        let data = data.as_str().ok_or("Native audio delta data is not base64 text")?;
        if data.is_empty() { return Ok(false); }
        // Bound allocation before decoding, at the shared stream's frame budget.
        if data.len() > MAX_GENERATION_CHUNK_BYTES.div_ceil(3) * 4 { return Err("Native audio delta exceeds stream frame budget".into()); }
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| format!("Invalid native audio delta: {e}"))?;
        if bytes.len() % 2 != 0 { return Err("Native PCM delta contains an incomplete s16le sample".into()); }
        if bytes.is_empty() { return Ok(false); }
        let sample_count = bytes.len() as u64 / 2;
        sink.send(GenerationChunk::Media(std::sync::Arc::new(MediaChunk {
            sequence: self.sequence,
            presentation_time_us: self.samples * 1_000_000 / 24_000,
            mime_type: PCM_MIME.into(), data: bytes.into(),
        })))?;
        self.sequence += 1;
        self.samples += sample_count;
        Ok(true)
    }
    pub fn finish(&self, reason: Option<&str>) -> Result<(), String> {
        if reason != Some("stop") { return Err("Native audio stream lacks successful terminal completion".into()); }
        if self.samples == 0 { return Err("Native audio stream produced no media; transcript is not a substitute".into()); }
        Ok(())
    }
}
