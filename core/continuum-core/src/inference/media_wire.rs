//! Native media framing on the existing AIRC inference stream. No new bus or queue.
use crate::ai::stream_sinks::{MediaChunk, MAX_GENERATION_CHUNK_BYTES};
use airc_core::{Body, Headers};

pub(crate) const KIND: &str = "inference.media.v1";
pub(crate) const ACCEPT_PARAM: &str = "mediaStreamVersion";
const MIME: &str = "continuum.media.mime";
const SEQUENCE: &str = "continuum.media.sequence";
const TIME: &str = "continuum.media.presentation-us";

pub(crate) fn encode(media: &MediaChunk) -> Result<(Headers, Body), String> {
    validate(&media.mime_type, media.data.len())?;
    Ok((Headers::from([
        (MIME.into(), media.mime_type.clone()),
        (SEQUENCE.into(), media.sequence.to_string()),
        (TIME.into(), media.presentation_time_us.to_string()),
    ]), Body::Binary(media.data.to_vec())))
}

pub(crate) fn decode(headers: &Headers, body: Option<&Body>) -> Result<MediaChunk, String> {
    let Some(Body::Binary(bytes)) = body else {
        return Err("Native media stream requires a binary body".into());
    };
    let mime = headers.get(MIME).ok_or("Native media stream has no MIME type")?;
    validate(mime, bytes.len())?;
    let number = |key: &str| -> Result<u64, String> {
        headers.get(key).and_then(|value| value.parse().ok())
            .ok_or_else(|| format!("Native media stream has invalid {key}"))
    };
    Ok(MediaChunk {
        sequence: number(SEQUENCE)?,
        presentation_time_us: number(TIME)?,
        mime_type: mime.clone(),
        data: bytes.clone().into(),
    })
}

fn validate(mime: &str, bytes: usize) -> Result<(), String> {
    if bytes == 0 || bytes > MAX_GENERATION_CHUNK_BYTES {
        return Err("Native media body outside shared stream byte budget".into());
    }
    if mime.len() > 256 || mime.chars().any(char::is_control)
        || !(mime.starts_with("audio/") || mime.starts_with("image/") || mime.starts_with("video/"))
        || mime.split('/').nth(1).is_none_or(str::is_empty)
    {
        return Err("Invalid native media MIME type".into());
    }
    Ok(())
}
