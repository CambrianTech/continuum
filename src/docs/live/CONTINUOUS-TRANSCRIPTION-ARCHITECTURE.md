# Continuous Transcription Architecture (Option C)

## Vision

End-to-end low-latency, highly accurate voice transcription with continuous streaming output. Inspired by NVIDIA PersonaPlex full-duplex architecture, adapted for our STT→LLM→TTS pipeline.

## Current Architecture (Problematic)

```
User speaks → Buffer audio → Detect silence → Transcribe entire utterance → Emit result
                            ↑
                        Problem: Waits for silence, causes skipping
```

**Issues:**
- Waits for silence before transcribing (704ms delay after each utterance)
- User sees NO transcription until they stop talking
- Natural pauses trigger premature "speech ended"
- Flaky behavior during conversational speech

## Target Architecture (Continuous)

```
User speaks → Buffer audio → Transcribe every 1-2s → Emit partial results → Continue
                            ↑                         ↑
                    No waiting for silence    Real-time feedback
```

**Benefits:**
- Words appear as user speaks (like Google Docs voice typing)
- No waiting for silence
- Natural pauses don't trigger "speech ended"
- Smooth, responsive UX

---

## Technical Design

### 1. Streaming Transcription Flow

```rust
// Continuous processing loop
while audio_available {
    // Accumulate 1-2 seconds of audio
    if buffer.duration >= TRANSCRIPTION_INTERVAL {
        // Transcribe immediately (don't wait for silence)
        let partial_text = transcribe_chunk(buffer);

        // Emit partial result
        emit_partial_transcription(partial_text);

        // Keep buffer tail for context overlap
        buffer.slide_window();
    }
}

// Only declare "speech ended" after extended silence (5+ seconds)
if silence_duration > FINAL_SILENCE_THRESHOLD {
    emit_final_transcription();
}
```

### 2. Constants (Research-Based)

```rust
/// Transcription interval - transcribe every N samples
/// 1.5s = 24000 samples at 16kHz (balanced latency vs accuracy)
const TRANSCRIPTION_INTERVAL_SAMPLES: usize = 24000;

/// Context overlap - keep this much audio from previous chunk
/// 0.5s = 8000 samples (prevents word boundary issues)
const CONTEXT_OVERLAP_SAMPLES: usize = 8000;

/// Final silence threshold - only declare "ended" after this much silence
/// 5s = 80000 samples (long enough to not trigger during thinking pauses)
const FINAL_SILENCE_THRESHOLD_SAMPLES: usize = 80000;

/// Frame size (can experiment: 32ms, 80ms, or 100ms)
/// 80ms = 1280 samples at 16kHz (PersonaPlex-inspired)
const FRAME_SIZE_SAMPLES: usize = 1280;
```

### 3. Partial Transcription Events

```rust
pub enum TranscriptionEvent {
    /// Partial transcription during ongoing speech
    Partial {
        text: String,
        confidence: f32,
        timestamp: u64,
        is_final: false,
    },

    /// Final transcription after extended silence
    Final {
        text: String,
        confidence: f32,
        timestamp: u64,
        is_final: true,
    },
}
```

### 4. Sliding Window Buffer

```rust
pub struct SlidingAudioBuffer {
    /// Ring buffer for audio samples
    buffer: Vec<f32>,

    /// Write position in ring buffer
    write_pos: usize,

    /// Total samples written (for timestamp tracking)
    total_samples: usize,

    /// Last transcribed position
    last_transcribed_pos: usize,
}

impl SlidingAudioBuffer {
    /// Add new audio samples
    pub fn push(&mut self, samples: &[f32]);

    /// Extract next chunk for transcription (with context overlap)
    pub fn extract_chunk(&mut self) -> Vec<f32>;

    /// Check if ready for transcription
    pub fn ready_for_transcription(&self) -> bool {
        (self.total_samples - self.last_transcribed_pos) >= TRANSCRIPTION_INTERVAL_SAMPLES
    }
}
```

---

## TDD Implementation Plan

### Phase 1: Core Buffer Logic (TDD)

**Tests FIRST:**
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_sliding_buffer_push() {
        // Test: buffer accumulates samples correctly
    }

    #[test]
    fn test_sliding_buffer_extract_with_overlap() {
        // Test: extract_chunk includes context overlap
    }

    #[test]
    fn test_ready_for_transcription_timing() {
        // Test: fires every 1.5s (24000 samples)
    }

    #[test]
    fn test_buffer_wrap_around() {
        // Test: ring buffer wraps correctly at boundary
    }
}
```

**Then implement**: `SlidingAudioBuffer`

### Phase 2: Continuous Transcription Logic (TDD)

**Tests FIRST:**
```rust
#[test]
fn test_transcribe_every_interval() {
    // Test: transcription triggers every 1.5s, not on silence
}

#[test]
fn test_partial_transcription_emission() {
    // Test: partial results emitted during ongoing speech
}

#[test]
fn test_final_transcription_after_long_silence() {
    // Test: final result only after 5s silence
}

#[test]
fn test_no_premature_speech_ended() {
    // Test: natural pauses don't trigger "speech ended"
}
```

**Then implement**: `ContinuousTranscriptionStream`

### Phase 3: Adapter Integration (TDD)

**Tests FIRST:**
```rust
#[test]
fn test_whisper_adapter_continuous_mode() {
    // Test: Whisper adapter handles partial chunks
}

#[test]
fn test_context_overlap_improves_accuracy() {
    // Test: overlapping context reduces word boundary errors
}

#[test]
fn test_multiple_partial_results_concatenation() {
    // Test: partial results combine into coherent transcript
}
```

**Then implement**: Adapter changes for continuous mode

### Phase 4: Integration Testing

**Tests:**
```rust
#[test]
fn test_end_to_end_continuous_transcription() {
    // Test: Real audio file → continuous partial results → final result
}

#[test]
fn test_latency_under_2_seconds() {
    // Test: First partial result appears within 2s of speech start
}

#[test]
fn test_accuracy_vs_batch_mode() {
    // Test: Continuous mode accuracy ≥ batch mode accuracy
}
```

---

## Adapter Pattern Integration

### New Trait: `ContinuousSTT`

```rust
#[async_trait]
pub trait ContinuousSTT: SpeechToText {
    /// Transcribe a chunk with context from previous chunk
    async fn transcribe_continuous(
        &self,
        audio: &[f32],
        context: Option<&str>, // Previous partial result for context
    ) -> Result<PartialTranscription, STTError>;

    /// Is this adapter capable of continuous mode?
    fn supports_continuous(&self) -> bool {
        false // Default: adapters opt-in
    }
}

pub struct PartialTranscription {
    pub text: String,
    pub confidence: f32,
    pub is_final: bool,
}
```

### Adapter Implementations

```rust
// Whisper supports continuous mode
impl ContinuousSTT for WhisperSTT {
    fn supports_continuous(&self) -> bool {
        true
    }

    async fn transcribe_continuous(
        &self,
        audio: &[f32],
        context: Option<&str>,
    ) -> Result<PartialTranscription, STTError> {
        // Whisper transcription with context awareness
    }
}

// Stub adapter for testing
impl ContinuousSTT for StubSTT {
    fn supports_continuous(&self) -> bool {
        true
    }

    async fn transcribe_continuous(&self, audio: &[f32], _context: Option<&str>)
        -> Result<PartialTranscription, STTError>
    {
        Ok(PartialTranscription {
            text: format!("STUB: {} samples", audio.len()),
            confidence: 1.0,
            is_final: false,
        })
    }
}
```

---

## Performance Targets

| Metric | Target | Rationale |
|--------|--------|-----------|
| First partial result | <2s after speech starts | User feedback loop |
| Transcription interval | 1-2s | Balance latency vs accuracy |
| Context overlap | 0.5s | Prevent word boundary errors |
| Final silence threshold | 5s | Don't trigger on thinking pauses |
| Accuracy vs batch mode | ≥95% | Continuous shouldn't sacrifice quality |
| CPU overhead | <20% increase | Sustainable for real-time |

---

## Rollout Plan

### Stage 1: Feature Flag (Week 1)
```rust
// Config: ~/.continuum/config.env
ENABLE_CONTINUOUS_TRANSCRIPTION=false  // Default: off

// Runtime toggle
./jtag voice/stt/mode --continuous  // Enable continuous mode
./jtag voice/stt/mode --batch       // Revert to batch mode
```

### Stage 2: A/B Testing (Week 2)
- 50% users get continuous mode
- Measure: accuracy, latency, user satisfaction
- Collect: transcription logs, timing data

### Stage 3: Default On (Week 3)
- If A/B test shows improvement, make continuous mode default
- Keep batch mode as fallback for compatibility

---

## PersonaPlex Learnings Applied

### What We Learned
1. **80ms frames** (vs our 32ms) - less frequent processing, smoother decisions
2. **No silence detection** - continuous processing eliminates "waiting for silence" issues
3. **Full-duplex model** - single end-to-end model (audio in → audio out)
4. **24kHz sample rate** - higher fidelity for better model performance

### What We'll Adopt
1. ✅ **Continuous processing** - transcribe every 1-2s, not on silence
2. ✅ **Larger frames** - experiment with 80ms (1280 samples at 16kHz)
3. ✅ **Partial results** - emit transcriptions as speech happens
4. ❌ **Full-duplex end-to-end model** - Future work (requires different model architecture)

### What We'll Defer
- **24kHz sample rate** - Whisper trained on 16kHz, would need resampling
- **Full-duplex listen+speak** - Requires end-to-end transformer (not STT→LLM→TTS)
- **Neural codec** - Moshi's Mimi codec, we use raw PCM/Opus

---

## Migration Path (Backwards Compatibility)

```rust
pub enum TranscriptionMode {
    /// Batch mode - wait for silence, transcribe entire utterance
    Batch,

    /// Continuous mode - transcribe every 1-2s, emit partials
    Continuous,
}

// Mixer selects mode based on adapter capability and config
let mode = if adapter.supports_continuous() && config.enable_continuous {
    TranscriptionMode::Continuous
} else {
    TranscriptionMode::Batch
};
```

---

## Success Metrics

### Objective Measurements
- [ ] First partial result latency: <2s
- [ ] Transcription accuracy: ≥95% (vs ground truth)
- [ ] Word skip rate: <5% (vs batch mode)
- [ ] CPU overhead: <20% increase

### Subjective Measurements
- [ ] User reports: "transcription feels responsive"
- [ ] User reports: "doesn't skip words anymore"
- [ ] User reports: "can see words as I speak"

---

## Timeline

| Week | Focus | Deliverables |
|------|-------|--------------|
| 1 | TDD Core Buffer | SlidingAudioBuffer + tests |
| 2 | TDD Continuous Logic | ContinuousTranscriptionStream + tests |
| 3 | Adapter Integration | ContinuousSTT trait + Whisper impl |
| 4 | Integration Testing | End-to-end tests + performance validation |
| 5 | Feature Flag Rollout | Config, runtime toggle, docs |
| 6 | A/B Testing | Metrics collection, user feedback |
| 7 | Production Default | Make continuous mode default |

---

## Next PR Checklist

- [ ] Create new branch: `feature/continuous-transcription`
- [ ] Implement `SlidingAudioBuffer` with TDD
- [ ] Implement `ContinuousTranscriptionStream` with TDD
- [ ] Add `ContinuousSTT` trait
- [ ] Update Whisper adapter for continuous mode
- [ ] Add feature flag: `ENABLE_CONTINUOUS_TRANSCRIPTION`
- [ ] Write integration tests
- [ ] Document performance benchmarks
- [ ] Update CLAUDE.md with continuous transcription patterns
- [ ] Create PR with detailed performance data

---

**Philosophy**: Build the right architecture with TDD, ship incrementally with feature flags, validate with data before making default.
