//! Request-to-backend payload mapping: sampling config + base64/data-URL media decode.

use super::*;

pub(super) fn sampling_config_from_request(request: &TextGenerationRequest) -> SamplingConfig {
    let mut sampling = SamplingConfig::chat();
    if let Some(t) = request.temperature {
        sampling.temperature = t as f64;
    }
    if let Some(k) = request.top_k {
        sampling.top_k = k as usize;
    }
    if let Some(p) = request.top_p {
        sampling.top_p = p as f64;
    }
    if let Some(rp) = request.repeat_penalty {
        sampling.repeat_penalty = rp;
    }
    if matches!(request.response_format, Some(ResponseFormat::JsonObject)) {
        sampling.grammar = Some(JSON_GRAMMAR.to_string());
    }
    sampling
}

/// Decode an `ImageInput` to raw bytes the multimodal projector can
/// consume. Prefers `base64` (already in-process); URL fetching is
/// deliberately not supported here — that's a sensory-bridge upstream
/// concern (the bridge fetches once + caches; doing it again at adapter
/// time would silently re-fetch on every request). If the bridge handed
/// us a URL-only image, that's a configuration bug worth surfacing.
pub(super) fn decode_image_bytes(image: &crate::ai::types::ImageInput) -> Result<Vec<u8>, String> {
    decode_data_url_or_base64(image.base64.as_deref(), image.url.as_deref(), "ImageInput")
}

/// Audio analogue of `decode_image_bytes`. Same base64-or-data-URL
/// shape (sensory-bridge upstream encodes captured PCM/WAV/MP3/FLAC
/// to base64 before passing through the persona pipeline), same
/// no-URL-fetching policy.
pub(super) fn decode_audio_bytes(audio: &crate::ai::types::AudioInput) -> Result<Vec<u8>, String> {
    decode_data_url_or_base64(audio.base64.as_deref(), audio.url.as_deref(), "AudioInput")
}

/// Common base64 / data-URL decode for the modality-typed wrappers.
/// Splits on the first comma to tolerate `data:image/jpeg;base64,...`
/// or `data:audio/wav;base64,...` prefixes the caller may have included
/// upstream. Errors point at the modality so the diagnosis is specific.
pub(super) fn decode_data_url_or_base64(
    b64: Option<&str>,
    url: Option<&str>,
    modality_label: &str,
) -> Result<Vec<u8>, String> {
    use base64::{engine::general_purpose, Engine};
    if let Some(b64) = b64 {
        let payload = b64.split_once(',').map(|(_, rest)| rest).unwrap_or(b64);
        general_purpose::STANDARD
            .decode(payload.as_bytes())
            .map_err(|e| format!("{modality_label}.base64 not valid base64: {e}"))
    } else if url.is_some() {
        Err(format!(
            "llamacpp_adapter received an URL-only {modality_label}; the sensory \
             bridge should resolve URLs to base64 before reaching the local \
             adapter (avoids per-request refetches and lets the adapter run \
             without network access)"
        ))
    } else {
        Err(format!(
            "{modality_label} has neither base64 nor url — nothing to decode"
        ))
    }
}
