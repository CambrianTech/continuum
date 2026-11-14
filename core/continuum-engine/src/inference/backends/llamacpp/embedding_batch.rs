//! One embedding request, tokenized and planned before anything is claimed: its sequences, how each
//! is embedded (whole, or in windows when longer than the lane's window), and the context that
//! embeds the widest piece. Its memory is known before the context exists, so the lease asks for exactly
//! what this request allocates, never a bound for the longest text, and never more than the window's.
use llama::Model;

/// Where a backend's layers are, as CONFIGURED (its offload setting and whether this build of
/// llama offloads at all), never measured: llama reports no context's per-buffer placement
/// before it exists (card 9bddda93). What is not known stays explicit as `Split`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    /// Every layer on the GPU: KV and compute in device memory, the CPU's share and the output
    /// buffer in host memory.
    Device,
    /// No layer on a GPU: everything in host memory.
    Host,
    /// Some layers on the GPU: the split is not measured, so each pool is charged its upper bound.
    Split,
}

/// What a context allocates in each pool, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextBytes {
    pub device: u64,
    pub host: u64,
}

/// The widest piece the embed lane embeds in one context, in tokens: the model's trained context
/// or the range its context cost was measured over (`MEASURED_UP_TO`), whichever is smaller. A
/// longer text is embedded in windows of it (`Plan::Windows`), every token kept.
///
/// The model's trained context alone was the window before: Qwen3-Embedding-0.6B trains at 32,768
/// tokens, so one 19,456-token text sized a 13,547 MiB context (the M5, canary 958ea3056,
/// 2026-11-14 03:35Z: `embed.vram.refused` requested 14,205,059,072 B beside 13,915,160,576 B,
/// `embedding.neural.failed`), and recall lost that item. A longer text now stands as the
/// token-weighted mean of its windows (the combiner card 57ef2a06 already runs past 32,768 tokens);
/// its effect on recall ranking for texts between 2,048 and 32,768 tokens is not measured yet. Every
/// byte past the measured range was an extrapolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EmbedWindow(u32);

impl EmbedWindow {
    /// The window for a model trained at `n_ctx_train` tokens; none for a model that reports no
    /// trained context, where nothing it embeds can be planned.
    pub(crate) fn for_model(n_ctx_train: u32) -> Option<Self> {
        (n_ctx_train > 0).then_some(Self(n_ctx_train.min(MEASURED_UP_TO)))
    }

    pub(crate) fn tokens(self) -> u32 {
        self.0
    }

    /// What the widest context this window allows allocates in each pool: the lane's context need.
    pub(crate) fn context_bytes(self, placement: Placement) -> ContextBytes {
        context_bytes(self.0, placement)
    }
}

/// Why a model with no trained context embeds nothing.
pub(crate) const NO_TRAINED_CONTEXT: &str = "embedding model reports no trained context: no window can be planned";

/// The largest context the rates below were measured at (four sizes, 256 to 2,048 tokens).
const MEASURED_UP_TO: u32 = 2_048;

/// A measured rate: `mib` MiB llama reported for a context of `tokens` tokens (the largest
/// per-token reading of four sizes, 256 to 2,048, each linear within 3%). Qwen3-Embedding-0.6B
/// Q8_0, engine-c, flash attention on, measured on the M5 on 2026-11-10; logs in
/// `~/.continuum/evidence/replicate-2026-11-10/` (`metal-embed-ctx-*`, `cpu-embed-ctx-*`).
#[derive(Debug, Clone, Copy)]
struct Measured {
    mib: f64,
    tokens: u32,
}

impl Measured {
    fn over(self, n_ctx: u32) -> u64 {
        (self.mib * 1024.0 * 1024.0 / f64::from(self.tokens) * f64::from(n_ctx)).ceil() as u64
    }
}

/// KV, wherever it resides (MTL0 and CPU report the same).
const KV: Measured = Measured { mib: 28.0, tokens: 256 };
/// The device's compute buffer with every layer offloaded (MTL0).
const DEVICE_COMPUTE: Measured = Measured { mib: 150.25, tokens: 256 };
/// The CPU's compute share beside a device context.
const DEVICE_CPU_COMPUTE: Measured = Measured { mib: 24.05, tokens: 2_048 };
/// The CPU's compute buffer with no layer offloaded.
const HOST_COMPUTE: Measured = Measured { mib: 154.25, tokens: 256 };
/// The output buffer, the same at every size (0.58 MiB), rounded up.
const OUTPUT: u64 = 1024 * 1024;

/// What a context of `n_ctx` tokens allocates in each pool, at `placement`. The rates are this
/// model's on Metal and on the CPU; another backend's are measured there (REPLICATE.md).
pub(crate) fn context_bytes(n_ctx: u32, placement: Placement) -> ContextBytes {
    let n = llama::ContextParams::allocated_n_ctx(n_ctx);
    let on_device = || KV.over(n) + DEVICE_COMPUTE.over(n);
    let on_host = || KV.over(n) + HOST_COMPUTE.over(n) + OUTPUT;
    match placement {
        Placement::Device => ContextBytes { device: on_device(), host: DEVICE_CPU_COMPUTE.over(n) + OUTPUT },
        Placement::Host => ContextBytes { device: 0, host: on_host() },
        Placement::Split => ContextBytes { device: on_device(), host: on_host() },
    }
}

/// Where a model file's `bytes` of weights reside at `placement`: on the device with every layer
/// offloaded, on the host with none, and charged in full to both when the split is not measured
/// (as a context's bytes are). The file's length is the weights' size within its metadata, a few
/// MiB of the ~0.6 GiB Qwen3-Embedding-0.6B Q8_0.
pub(crate) fn resident_weights(bytes: u64, placement: Placement) -> ContextBytes {
    match placement {
        Placement::Device => ContextBytes { device: bytes, host: 0 },
        Placement::Host => ContextBytes { device: 0, host: bytes },
        Placement::Split => ContextBytes { device: bytes, host: bytes },
    }
}

/// One request's sequences, each one's plan, and the window that embeds the widest piece.
pub(crate) struct EmbeddingBatch {
    sequences: Vec<Vec<i32>>,
    plans: Vec<Plan>,
    widest: u32,
}

impl EmbeddingBatch {
    /// Tokenize `texts` and plan each within the model's [`EmbedWindow`]. Non-causal embedding
    /// needs each piece in one batch, so the context is the widest piece: a text that fits is one
    /// piece, a longer one its windows; never a private token ceiling or head clip.
    pub(crate) fn tokenize(model: &Model, texts: &[String]) -> Result<Self, String> {
        let window = EmbedWindow::for_model(model.n_ctx_train()).ok_or(NO_TRAINED_CONTEXT)?;
        let sequences = texts
            .iter()
            .map(|text| model.tokenize(text, true, false))
            .collect::<Result<Vec<_>, _>>()?;
        if sequences.iter().any(Vec::is_empty) {
            return Err("embedding tokenization produced an empty sequence".into());
        }
        let limit = usize::try_from(window.tokens()).map_err(|_| "embedding window exceeds native representation")?;
        // The special tokens are read only when a text needs windows: a batch of texts that fit
        // never depends on how a tokenizer treats an empty string (Codex on #5901).
        let specials = if sequences.iter().any(|tokens| tokens.len() > limit) {
            model.tokenize("", true, false)?
        } else {
            Vec::new()
        };
        let plans = plan(&sequences, &specials, limit)?;
        let widest = widest(&sequences, &plans).ok_or("empty embedding request")?;
        let widest = u32::try_from(widest).map_err(|_| "embedding sequence length exceeds native representation")?;
        Ok(Self { sequences, plans, widest })
    }

    /// What this batch's context allocates in each pool at `placement`: what the embed lane leases
    /// before it creates the context.
    pub(crate) fn context_bytes(&self, placement: Placement) -> ContextBytes {
        context_bytes(self.widest, placement)
    }

    /// Embed every piece in one context sized to the widest; KV is cleared between pieces.
    pub(crate) fn embed(self, model: &Model) -> Result<Vec<Vec<f32>>, String> {
        let mut ctx = model.new_context(llama::ContextParams {
            embeddings: true,
            embeddings_only: true,
            pooling_type: llama::PoolingType::Last,
            n_ctx: self.widest,
            n_batch: self.widest,
            n_ubatch: self.widest,
            ..Default::default()
        })?;
        embed_plans(&self.sequences, &self.plans, |tokens| {
            ctx.memory_clear(true);
            ctx.embed(tokens)
        })
    }
}


/// How one tokenized text is embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Plan {
    /// It fits the context: its own tokens, embedded exactly as they are.
    Direct,
    /// It does not (card 57ef2a06: BigMama's 38,603-token example at a 32,768 context errored the
    /// whole batch): every window of its text, each framed by the model's special tokens as the
    /// whole text is, never its head alone.
    Windows { frame: Frame, windows: Vec<std::ops::Range<usize>> },
}

/// The special tokens a tokenizer adds around every text (what it returns for ""), split into the
/// part before the text and the part after. Which side is the model's: a BOS model's are a head,
/// Qwen3-Embedding's are a tail (`add_bos_token` false, `add_eos_token` true in its GGUF, and its
/// last-token pooling reads that EOS), so a window framed only by a head would refuse every Qwen3
/// text the moment one needed windows.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    head: Vec<i32>,
    tail: Vec<i32>,
}

impl Frame {
    /// The split of `specials` that `tokens` begins and ends with; none when it carries no such
    /// framing (its windows would be built from the wrong tokens).
    fn around(specials: &[i32], tokens: &[i32]) -> Option<Self> {
        (0..=specials.len())
            .rev()
            .find(|&k| tokens.len() >= specials.len() && tokens.starts_with(&specials[..k]) && tokens.ends_with(&specials[k..]))
            .map(|k| Self { head: specials[..k].to_vec(), tail: specials[k..].to_vec() })
    }

    fn len(&self) -> usize {
        self.head.len() + self.tail.len()
    }
}

/// The widest piece the plans embed, in tokens: what the context is sized to; none for no texts.
fn widest(sequences: &[Vec<i32>], plans: &[Plan]) -> Option<usize> {
    plans
        .iter()
        .zip(sequences)
        .map(|(plan, tokens)| match plan {
            Plan::Direct => tokens.len(),
            Plan::Windows { frame, windows } => windows.iter().map(|w| frame.len() + w.len()).max().unwrap_or(0), // unwrap_or: plan() never returns an empty window list
        })
        .max()
}

/// Each text's plan. Only a text over `limit` is windowed, and it must carry `specials` as its frame.
fn plan(sequences: &[Vec<i32>], specials: &[i32], limit: usize) -> Result<Vec<Plan>, String> {
    sequences
        .iter()
        .map(|tokens| {
            if tokens.len() <= limit {
                return Ok(Plan::Direct);
            }
            let frame = Frame::around(specials, tokens).ok_or_else(|| {
                format!(
                    "a {}-token embedding sequence is not framed by the model's {} special tokens: its windows would be built from the wrong tokens",
                    tokens.len(),
                    specials.len()
                )
            })?;
            let windows = windows(tokens.len(), frame.head.len(), frame.tail.len(), limit)?;
            Ok(Plan::Windows { frame, windows })
        })
        .collect()
}

/// One vector per text, in order: a direct text is `embed` of its own tokens, untouched; a windowed
/// text is its windows combined by `weighted_mean`.
fn embed_plans(
    sequences: &[Vec<i32>],
    plans: &[Plan],
    mut embed: impl FnMut(&[i32]) -> Result<Vec<f32>, String>,
) -> Result<Vec<Vec<f32>>, String> {
    let mut out = Vec::with_capacity(sequences.len());
    for (tokens, plan) in sequences.iter().zip(plans) {
        out.push(match plan {
            Plan::Direct => embed(tokens)?,
            Plan::Windows { frame, windows } => {
                let mut pieces = Vec::with_capacity(windows.len());
                for window in windows {
                    let mut piece = Vec::with_capacity(frame.len() + window.len());
                    piece.extend_from_slice(&frame.head);
                    piece.extend_from_slice(&tokens[window.clone()]);
                    piece.extend_from_slice(&frame.tail);
                    pieces.push((embed(&piece)?, window.len()));
                }
                weighted_mean(&pieces)?
            }
        });
    }
    Ok(out)
}

/// The text-token windows of a `len`-token sequence framed by `head` special tokens before its
/// text and `tail` after, each fitting `limit` with the frame carried: one window when it fits,
/// else contiguous windows covering every text token once.
fn windows(len: usize, head: usize, tail: usize, limit: usize) -> Result<Vec<std::ops::Range<usize>>, String> {
    let frame = head + tail;
    let span = limit.checked_sub(frame).filter(|span| *span > 0).ok_or_else(|| {
        format!("embedding model context {limit} cannot hold its own {frame} special tokens and any text")
    })?;
    let text = head..len.saturating_sub(tail);
    if text.is_empty() {
        return Err("embedding sequence holds no text within its special tokens".into());
    }
    let end = text.end;
    Ok(text.step_by(span).map(|start| start..(start + span).min(end)).collect())
}

/// The vector a text's windows stand for together: their mean weighted by the text tokens each
/// holds, L2-normalized like every window (`ctx.embed`), so a one-window text is its window.
fn weighted_mean(pieces: &[(Vec<f32>, usize)]) -> Result<Vec<f32>, String> {
    let (first, _) = pieces.first().ok_or("a text with no embedded window")?;
    let mut sum = vec![0.0f32; first.len()];
    for (vector, weight) in pieces {
        if vector.len() != sum.len() {
            return Err(format!("embedding windows disagree on dimension: {} vs {}", vector.len(), sum.len()));
        }
        for (s, x) in sum.iter_mut().zip(vector) {
            *s += x * *weight as f32;
        }
    }
    let norm = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !norm.is_finite() || norm == 0.0 {
        return Err("embedding windows cancelled to a degenerate vector".into());
    }
    Ok(sum.into_iter().map(|x| x / norm).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    /// The batch `tokenize` builds from these tokens at `limit`, without a model.
    fn planned(sequences: Vec<Vec<i32>>, specials: &[i32], limit: usize) -> EmbeddingBatch {
        let plans = plan(&sequences, specials, limit).expect("test: plans");
        let widest = u32::try_from(widest(&sequences, &plans).expect("test: texts")).expect("test: fits u32");
        EmbeddingBatch { sequences, plans, widest }
    }

    // what this catches (the M5 on 26960c09d, 758 failed embeddings in 3 h, 9% success; Codex on
    // #5910: one context's bytes are in two pools): a lease sized for the longest text, or charged
    // all to one pool. A short text leases its own small context; each pool covers what llama
    // reported for it at 2,048 tokens; an unmeasured split is charged in full to both.
    #[test]
    fn a_context_leases_what_it_allocates_in_each_pool() {
        let short = context_bytes(40, Placement::Device);
        assert!(short.device < 200 * MIB && short.host < 10 * MIB, "a short text: {short:?}");
        assert_eq!(short, context_bytes(256, Placement::Device), "llama pads to 256 tokens");
        let device = context_bytes(2_048, Placement::Device);
        assert!(device.device >= (224 + 1_200) * MIB, "MTL0 KV 224 + compute 1,200.96 MiB: {device:?}");
        assert!(device.host >= 24 * MIB, "CPU compute 24.05 + output 0.58 MiB: {device:?}");
        let host = context_bytes(2_048, Placement::Host);
        assert_eq!(host.device, 0);
        assert!(host.host >= (224 + 1_200) * MIB, "CPU KV 224 + compute 1,200.97 MiB: {host:?}");
        let split = context_bytes(2_048, Placement::Split);
        assert_eq!((split.device, split.host), (device.device, host.host), "an unmeasured split: the upper bound in each pool");
    }

    // what this catches (dyno, the M5 on canary 958ea3056, 2026-11-14 03:35Z): an embed context
    // sized to the model's trained 32,768 tokens. Before: one 19,456-token text planned whole at that
    // limit leased 14,205,059,072 B, the receipt's exact request, refused beside 13,915,160,576 B.
    // After: the window derives from the measured range, the same text is windows of it, and the
    // context is what a 2,048-token window allocates, every text token still embedded once.
    #[test]
    fn an_embed_context_derives_from_the_window_never_the_trained_context() {
        let text: Vec<i32> = std::iter::once(1).chain(2..19_457).collect(); // 19,456 tokens, prefix [1]
        let before = planned(vec![text.clone()], &[1], 32_768);
        assert_eq!(before.context_bytes(Placement::Device).device, 14_205_059_072, "the receipt's request");
        let window = EmbedWindow::for_model(32_768).expect("test: a trained context");
        assert_eq!(window.tokens(), 2_048);
        let after = planned(vec![text], &[1], 2_048);
        assert!(after.widest <= window.tokens(), "no piece past the window");
        assert_eq!(after.context_bytes(Placement::Device), window.context_bytes(Placement::Device));
        assert!(after.context_bytes(Placement::Device).device < 1_500 * MIB, "{:?}", after.context_bytes(Placement::Device));
        let pieces = match &after.plans[0] {
            Plan::Windows { windows, .. } => windows.clone(),
            Plan::Direct => Vec::new(), // fails the sum below: a 19,456-token text must be windowed
        };
        assert_eq!(pieces.iter().map(|w| w.len()).sum::<usize>(), 19_456 - 1, "every text token once");
        assert_eq!(EmbedWindow::for_model(512).expect("test: small model").tokens(), 512, "a smaller trained context is the window");
        assert_eq!(EmbedWindow::for_model(0), None, "no trained context: no window");
    }

    // what this catches (card 57ef2a06): a text longer than the model's context clipped to its
    // head, or rejected so the whole batch embeds nothing. Every text token lands in exactly one
    // window, each window fits the context with the frame carried, and a text that fits is one
    // window.
    #[test]
    fn windows_cover_every_text_token_once_within_the_context() {
        assert_eq!(windows(100, 1, 0, 32_768).expect("test: fits"), vec![1..100]);
        let long = windows(38_603, 1, 0, 32_768).expect("test: plans");
        assert_eq!(long, vec![1..32_768, 32_768..38_603], "the BigMama example: two windows, nothing dropped");
        assert!(long.iter().all(|w| 1 + w.len() <= 32_768), "each window fits with its prefix");
        assert_eq!(long.iter().map(|w| w.len()).sum::<usize>(), 38_603 - 1, "every text token once");
        assert_eq!(windows(11, 0, 1, 4).expect("test: a tail frame"), vec![0..3, 3..6, 6..9, 9..10], "the tail token is never a text token");
        assert!(windows(10, 4, 0, 4).is_err(), "a context that cannot hold its frame and text refuses");
        assert!(windows(10, 2, 2, 4).is_err(), "head and tail both count against the context");
        assert!(windows(1, 1, 0, 8).is_err(), "a sequence of only its frame holds no text");
    }

    // what this catches (Qwen3-Embedding-0.6B-Q8_0.gguf on the M5: add_bos_token false,
    // add_eos_token true): windows framed only by a head. The tokenizer's "" is [EOS], which no Qwen3
    // text begins with, so every text past the window refused; each window must end with the EOS
    // its last-token pooling reads, as the whole text does, and hold every text token once.
    #[test]
    fn a_tail_framed_text_is_windowed_with_its_tail() {
        const EOS: i32 = 151_643;
        let long: Vec<i32> = (2..12).chain(std::iter::once(EOS)).collect(); // 10 text tokens, then EOS
        let plans = plan(&[long.clone()], &[EOS], 4).expect("test: a tail frame plans");
        let frame = Frame { head: Vec::new(), tail: vec![EOS] };
        assert_eq!(plans[0], Plan::Windows { frame, windows: vec![0..3, 3..6, 6..9, 9..10] });
        let mut pieces = Vec::new();
        embed_plans(&[long], &plans, |piece| {
            pieces.push(piece.to_vec());
            Ok(vec![1.0, piece.len() as f32])
        })
        .expect("test: embeds");
        assert!(pieces.iter().all(|p| p.len() <= 4 && p.last() == Some(&EOS)), "each window fits and ends with EOS: {pieces:?}");
        let text: Vec<i32> = pieces.iter().flat_map(|p| p[..p.len() - 1].to_vec()).collect();
        assert_eq!(text, (2..12).collect::<Vec<i32>>(), "every text token once, in order");
        assert_eq!(Frame::around(&[1, EOS], &[1, 5, EOS]), Some(Frame { head: vec![1], tail: vec![EOS] }), "a head and a tail");
    }

    // what this catches: windows combined unweighted (a short tail outweighs most of the text) or
    // left unnormalized (cosine stops being a dot product); one window must be itself.
    #[test]
    fn windows_combine_by_their_token_share_into_a_unit_vector() {
        let one = weighted_mean(&[(vec![0.6, 0.8], 7)]).expect("test: one window");
        assert!((one[0] - 0.6).abs() < 1e-6 && (one[1] - 0.8).abs() < 1e-6, "{one:?}");
        let two = weighted_mean(&[(vec![1.0, 0.0], 3), (vec![0.0, 1.0], 1)]).expect("test: two windows");
        assert!(two[0] > two[1] * 2.9 && two[0] < two[1] * 3.1, "3:1 by tokens: {two:?}");
        assert!(((two[0] * two[0] + two[1] * two[1]).sqrt() - 1.0).abs() < 1e-6, "unit length");
        assert!(weighted_mean(&[(vec![1.0], 1), (vec![1.0, 0.0], 1)]).is_err(), "dimensions must agree");
        assert!(weighted_mean(&[(vec![1.0, 0.0], 2), (vec![-1.0, 0.0], 2)]).is_err(), "windows that cancel are refused, never normalized to NaN");
        assert!(weighted_mean(&[]).is_err(), "a text with no window has no vector");
    }

    // what this catches (Codex on #5901): a text that fits re-encoded through the window combiner
    // (multiplied by its token count and renormalized, so its f32 vector drifts from the model's),
    // a prefix-only sequence refused, or outputs out of order in a mixed batch. A text that fits is
    // EXACTLY embed(its tokens); a long one is planned into windows; order is kept.
    #[test]
    fn a_text_that_fits_is_embedded_exactly_and_a_mixed_batch_keeps_its_order() {
        // A deterministic stand-in for the model: a non-normalized vector from the tokens, so any
        // re-normalization of a direct text would show as a difference.
        let fake = |tokens: &[i32]| -> Result<Vec<f32>, String> {
            Ok(vec![tokens.len() as f32, tokens.iter().map(|t| *t as f32).sum::<f32>() + 0.1])
        };
        let short = vec![1, 5, 9];
        let prefix_only = vec![1];
        let long: Vec<i32> = std::iter::once(1).chain(2..12).collect(); // 11 tokens, prefix [1]
        let sequences = vec![short.clone(), long.clone(), prefix_only.clone()];
        let plans = plan(&sequences, &[1], 8).expect("test: plans");
        assert_eq!(plans[0], Plan::Direct);
        assert_eq!(plans[1], Plan::Windows { frame: Frame { head: vec![1], tail: Vec::new() }, windows: vec![1..8, 8..11] }, "only the over-limit text is windowed");
        assert_eq!(plans[2], Plan::Direct, "a prefix-only sequence that fits is embedded as before, never refused");
        let out = embed_plans(&sequences, &plans, fake).expect("test: embeds");
        assert_eq!(out.len(), 3, "one output per input");
        assert_eq!(out[0], fake(&short).expect("test: fake"), "a fitting text is exactly embed(its tokens)");
        assert_eq!(out[2], fake(&prefix_only).expect("test: fake"));
        assert!(((out[1][0] * out[1][0] + out[1][1] * out[1][1]).sqrt() - 1.0).abs() < 1e-6, "the windowed text is a unit vector");
        assert!(plan(&[vec![7; 20]], &[1], 8).is_err(), "a long text not framed by the special tokens refuses");
    }

    // what this catches (Codex on 57ef2a06): the last, short window of a long text weighted like a
    // full one, so a few hundred tail tokens pull the vector as hard as 32k head tokens.
    #[test]
    fn the_short_final_window_weighs_only_its_own_tokens() {
        let plan = windows(38_603, 1, 0, 32_768).expect("test: plans");
        let weights: Vec<usize> = plan.iter().map(|w| w.len()).collect();
        assert_eq!(weights, vec![32_767, 5_835], "each window weighs the text tokens it holds");
        let mean = weighted_mean(&[(vec![1.0, 0.0], weights[0]), (vec![0.0, 1.0], weights[1])]).expect("test: combine");
        assert!(mean[0] / mean[1] > 5.6 && mean[0] / mean[1] < 5.62, "the head dominates by its token share: {mean:?}");
    }
}
