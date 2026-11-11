//! The Orpheus audio protocol: framing ids and audio-code geometry, resolved
//! from the loaded model's own vocabulary.

use super::TTSError;

// ─── The Orpheus audio protocol (canopylabs), resolved from the model itself ──
// The first cut of this adapter invented `<|text_start|>`/`<|audio_end|>` tokens
// that DO NOT EXIST in the model; it could never have produced speech. Every
// framing id is now looked up by its token name in the GGUF's own vocabulary at
// load, so a different fine-tune fails there with a message, never with garbage
// audio. The protocol, id-level:
//   prompt  = [human start] ++ encode("{voice}: {text}") ++ [text end, human end, third end, audio start]
//   output  = … <audio tokens>, audio end
// Each audio token encodes (frame position, code): id − base − position·band.

/// Which SNAC codebook layer each of a frame's audio tokens feeds: the canonical
/// canopylabs interleave (their decoder, verbatim). Layer 1 takes positions 1
/// AND 4; layer 2 takes 2, 3, 5 and 6. The first cut had [0,1,1,2,2,2,2], which
/// scrambles the mid and fine codebooks into noise. A frame is this many tokens.
pub(super) const FRAME_LAYOUT: [usize; 7] = [0, 1, 2, 2, 1, 2, 2];
pub(super) const NUM_CODEBOOKS: usize = 3;

/// The neural codec an Orpheus fine-tune speaks, as the protocol declares it
/// (the vocabulary names tokens, not codebook widths).
pub(super) struct Codec {
    /// Entries in each codebook: one audio-id band per frame position.
    pub(super) codes: u32,
}

/// SNAC 24 kHz (hubertsiuzdak/snac_24khz), the codec canopylabs Orpheus is
/// trained on: three codebooks of 4,096 entries, coarse to fine at 1, 2 and 4
/// codes per frame.
pub(super) const SNAC_24KHZ: Codec = Codec { codes: 4096 };

/// The framing and audio ids of a loaded Orpheus vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Protocol {
    /// `<custom_token_3>`: start of the human turn.
    pub(super) human_start: u32,
    /// Llama-3 `<|eot_id|>`: closes the text.
    pub(super) text_end: u32,
    /// `<custom_token_4>`: end of the human turn.
    pub(super) human_end: u32,
    /// `<custom_token_5>`: the reference's third end token.
    pub(super) third_end: u32,
    /// `<custom_token_1>`: start of audio, which tells the model to speak THIS
    /// text. Missing it (measured 2026-09-02 against the reference
    /// engine_class.py) made the model free-generate filler ("and again").
    pub(super) audio_start: u32,
    /// `<custom_token_2>`: end of audio, the generation stop.
    pub(super) audio_end: u32,
    /// `<custom_token_10>`: the first audio code.
    pub(super) audio_base: u32,
    /// Codes per frame position: the codec's codebook width, its whole span
    /// verified present and contiguous in the vocabulary from the base.
    pub(super) band: u32,
}

impl Protocol {
    /// Resolve every id by its token name. `lookup` is the vocabulary; `codec`
    /// is the codec its audio ids address.
    pub(super) fn resolve(
        lookup: impl Fn(&str) -> Option<u32>,
        codec: &Codec,
    ) -> Result<Self, TTSError> {
        let id = |name: &str| {
            lookup(name).ok_or_else(|| {
                TTSError::ModelNotLoaded(format!(
                    "GGUF vocabulary has no {name}: not a canopylabs Orpheus layout"
                ))
            })
        };
        let audio_base = id("<custom_token_10>")?;
        // Every id the codec can address must be the next custom token, so a
        // hole or a renumbering refuses at load instead of remapping every
        // later code. Tokens past the span (canopylabs has one) are not audio.
        let span = codec.codes * FRAME_LAYOUT.len() as u32;
        if let Some(k) = (0..span).find(|k| {
            lookup(&format!("<custom_token_{}>", 10 + k)) != audio_base.checked_add(*k)
        }) {
            return Err(TTSError::ModelNotLoaded(format!(
                "GGUF audio span breaks at <custom_token_{}>: the codec addresses {span} contiguous ids from <custom_token_10>",
                10 + k
            )));
        }
        let band = codec.codes;
        Ok(Self {
            human_start: id("<custom_token_3>")?,
            text_end: id("<|eot_id|>")?,
            human_end: id("<custom_token_4>")?,
            third_end: id("<custom_token_5>")?,
            audio_start: id("<custom_token_1>")?,
            audio_end: id("<custom_token_2>")?,
            audio_base,
            band,
        })
    }

    /// Is this id an audio code? The FULL span, one band per frame position
    /// (the 3-band first cut rejected every token above band 2, discarding most
    /// of the audio stream).
    pub(super) fn is_audio(&self, token_id: u32) -> bool {
        token_id >= self.audio_base
            && token_id - self.audio_base < self.band * FRAME_LAYOUT.len() as u32
    }

    /// The codebook entry an audio id carries.
    pub(super) fn code(&self, token_id: u32) -> i64 {
        ((token_id - self.audio_base) % self.band) as i64
    }
}

/// Frames per decode call, from the decoder's three code inputs (coarse to
/// fine). Each is `[batch, codes]` with batch 1 or dynamic. All dynamic decodes
/// the whole utterance in one call (`None`). All fixed must hold 1, 2 and 4
/// codes per frame for the same frame count. Anything else cannot decode the
/// windows `snac_decode` builds, so it refuses at load.
pub(super) fn snac_window(shapes: &[Vec<i64>]) -> Result<Option<usize>, String> {
    let malformed = || format!("SNAC decoder inputs {shapes:?} are not [batch, codes] at 1:2:4 per frame");
    if shapes.len() != NUM_CODEBOOKS
        || shapes.iter().any(|shape| shape.len() != 2 || !matches!(shape[0], 1 | -1))
    {
        return Err(malformed());
    }
    let lengths: Vec<i64> = shapes.iter().map(|shape| shape[1]).collect();
    if lengths.iter().all(|length| *length == -1) {
        return Ok(None);
    }
    let frames = usize::try_from(lengths[0]).map_err(|_| malformed())?;
    let scaled = lengths
        .iter()
        .enumerate()
        .all(|(layer, length)| usize::try_from(*length).ok() == frames.checked_shl(layer as u32));
    if frames == 0 || !scaled {
        return Err(malformed());
    }
    Ok(Some(frames))
}

/// The canopylabs vocabulary's framing, as verified against the GGUF's own
/// token table on 2026-09-02: 128,256 Llama ids, then `<custom_token_0..28682>`.
#[cfg(test)]
pub(super) fn canopylabs() -> Result<Protocol, TTSError> {
    Protocol::resolve(vocabulary(28682, None), &SNAC_24KHZ)
}

/// A vocabulary of the canopylabs shape whose custom tokens run to `last`,
/// optionally missing one.
#[cfg(test)]
fn vocabulary(last: u32, hole: Option<u32>) -> impl Fn(&str) -> Option<u32> {
    move |name| match name {
        "<|eot_id|>" => Some(128009),
        _ => name
            .strip_prefix("<custom_token_")
            .and_then(|rest| rest.strip_suffix('>'))
            .and_then(|k| k.parse::<u32>().ok())
            .filter(|k| *k <= last && Some(*k) != hole)
            .map(|k| 128256 + k),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_resolves_the_canopylabs_ids_by_name() -> Result<(), TTSError> {
        // what this catches: drift in the id-level framing. Resolving by token
        // name must land on the verified canopylabs ids, and the audio band on
        // the SNAC codebook (4096 codes per frame position, a 28,672-id span).
        let p = canopylabs()?;
        assert_eq!(
            (p.human_start, p.text_end, p.human_end, p.third_end),
            (128259, 128009, 128260, 128261)
        );
        assert_eq!((p.audio_start, p.audio_end, p.audio_base), (128257, 128258, 128266));
        assert_eq!(p.band, 4096);
        Ok(())
    }

    #[test]
    fn a_vocabulary_without_the_framing_refuses_at_load() {
        // what this catches: a different fine-tune loading, then speaking
        // garbage; the first cut framed with tokens the model never had.
        assert!(Protocol::resolve(|name| (name == "<custom_token_10>").then_some(7), &SNAC_24KHZ).is_err());
        assert!(Protocol::resolve(|_| None, &SNAC_24KHZ).is_err());
    }

    #[test]
    fn an_audio_id_is_anywhere_in_the_full_seven_band_span() -> Result<(), TTSError> {
        // what this catches: a 3-band bound rejected most of the real audio stream.
        let p = canopylabs()?;
        let span = p.band * FRAME_LAYOUT.len() as u32;
        assert!(!p.is_audio(p.audio_base - 1));
        assert!(p.is_audio(p.audio_base));
        assert!(p.is_audio(p.audio_base + 6000));
        assert!(p.is_audio(p.audio_base + span - 1));
        assert!(!p.is_audio(p.audio_base + span));
        Ok(())
    }

    #[test]
    fn the_band_is_the_codec_width_whatever_tokens_follow_the_span() -> Result<(), TTSError> {
        // what this catches (Codex on #5919): a band inferred from how many custom
        // tokens follow the base. Seven more names made it 4097, and a valid
        // audio_base + 4096 decoded to 4096, which no 4,096-entry codebook holds.
        let p = Protocol::resolve(vocabulary(28682 + 7, None), &SNAC_24KHZ)?;
        assert_eq!(p.band, 4096);
        assert_eq!(p.code(p.audio_base + 4096), 0);
        Ok(())
    }

    #[test]
    fn a_hole_in_the_audio_span_refuses_at_load() {
        // what this catches: a missing middle token silently shrinking the band
        // and remapping every later code.
        assert!(Protocol::resolve(vocabulary(28682, Some(5000)), &SNAC_24KHZ).is_err());
        assert!(Protocol::resolve(vocabulary(10 + 7 * 4096 - 2, None), &SNAC_24KHZ).is_err());
    }

    #[test]
    fn the_decoder_window_is_read_only_from_compatible_shapes() {
        // what this catches (Codex on #5919): zero read as a dynamic axis, and a
        // fixed 12/24/47 export passing load, then failing every synthesis.
        let shapes = |lengths: [i64; 3]| lengths.iter().map(|n| vec![1, *n]).collect::<Vec<_>>();
        assert_eq!(snac_window(&shapes([12, 24, 48])), Ok(Some(12)));
        assert_eq!(snac_window(&shapes([-1, -1, -1])), Ok(None));
        assert!(snac_window(&shapes([12, 24, 47])).is_err());
        assert!(snac_window(&shapes([0, 0, 0])).is_err());
        assert!(snac_window(&shapes([12, -1, 48])).is_err());
        assert!(snac_window(&[vec![1, 12], vec![1, 24]]).is_err());
        assert!(snac_window(&[vec![1, 1, 12], vec![1, 24], vec![1, 48]]).is_err());
        assert!(snac_window(&[vec![2, 12], vec![2, 24], vec![2, 48]]).is_err());
    }
}
