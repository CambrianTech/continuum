use candle_core::quantized::gguf_file::Content;

/// Input encoders explicitly represented by a projector artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectorEncoders {
    pub has_vision: bool,
    pub has_audio: bool,
}

pub fn projector_encoders(content: &Content) -> Result<ProjectorEncoders, String> {
    let flag = |key: &str| -> Result<bool, String> {
        match content.metadata.get(key) {
            None => Ok(false), // The projector schema omits flags for absent encoders.
            Some(value) => value
                .to_bool()
                .map_err(|error| format!("projector flag {key}: {error}")),
        }
    };
    Ok(ProjectorEncoders {
        has_vision: flag("clip.has_vision_encoder")?,
        has_audio: flag("clip.has_audio_encoder")?,
    })
}

