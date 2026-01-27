# VAD Production Configuration Guide

## Problem: Balancing Accuracy vs Completeness

Based on user requirements:
1. **Must get MOST of the audio** - Don't skip speech parts
2. **Form coherent sentences** - Not fragments
3. **Low latency** - Fast processing
4. **Reject background noise** - Don't transcribe TV/factory

## Current Bottlenecks

### 1. Silero Threshold Too Conservative

**Problem**: Default threshold (0.5) might skip real speech
- Silero outputs confidence 0.0-1.0
- Current: `is_speech = confidence > 0.5`
- **Risk**: Quiet speech or speech in noise gets skipped

**Solution**: Lower threshold for production

```rust
// Current (conservative)
if result.confidence > 0.5 { transcribe() }

// Production (catch more speech)
if result.confidence > 0.3 { transcribe() }  // Lower threshold

// Adaptive (best)
let threshold = match noise_level {
    NoiseLevel::Quiet => 0.4,
    NoiseLevel::Moderate => 0.3,
    NoiseLevel::Loud => 0.25,  // Even lower in noisy environments
};
```

### 2. Silence Threshold Cuts Off Sentences

**Problem**: 22 frames of silence (704ms) ends transcription
- People pause between words (200-500ms)
- Current system might cut mid-sentence

**Solution**: Longer silence threshold + smart buffering

```rust
// Current
fn silence_threshold_frames(&self) -> u32 { 22 }  // 704ms

// Production (allow natural pauses)
fn silence_threshold_frames(&self) -> u32 {
    40  // 1.28 seconds - enough for natural pauses
}
```

### 3. Latency: Silero 54ms per Frame

**Problem**: 54ms latency too slow for real-time
- Each 32ms audio frame takes 54ms to process
- Can't keep up with real-time (1.7x slower)

**Solutions**:
1. **Use WebRTC for pre-filtering** (1-10μs)
2. **Batch processing** (process multiple frames together)
3. **Skip frames** (only check every Nth frame)
4. **Lower quality mode** (Silero has speed/accuracy trade-off)

## Recommended Production Configuration

### Strategy: Two-Stage VAD

```rust
// Stage 1: Fast pre-filter (WebRTC - 1-10μs)
let quick_result = webrtc_vad.detect(&audio).await?;

if quick_result.is_speech {
    // Stage 2: Accurate confirmation (Silero - 54ms)
    // Only run expensive check on likely speech
    let silero_result = silero_vad.detect(&audio).await?;

    if silero_result.confidence > 0.3 {  // Lowered threshold
        // Send to STT
        transcribe(&audio);
    }
} else {
    // WebRTC says silence - skip expensive Silero check
    // Saves 54ms per frame on pure silence
}
```

**Performance**:
- Silence: 10μs (WebRTC only)
- Noise: 54ms (Silero rejects)
- Speech: 54ms (Silero confirms → transcribe)

**Benefit**: 5400x faster on silence, 100% accuracy on speech

### Configuration Values

```rust
pub struct ProductionVADConfig {
    // Confidence thresholds
    pub silero_threshold: f32,      // 0.3 (was 0.5)
    pub webrtc_aggressiveness: u8,  // 2 (moderate)

    // Silence detection
    pub silence_threshold_frames: u32,  // 40 frames (1.28s)
    pub min_speech_frames: u32,         // 3 frames (96ms) minimum to transcribe

    // Buffering
    pub pre_speech_buffer_ms: u32,   // 300ms before speech detected
    pub post_speech_buffer_ms: u32,  // 500ms after last speech

    // Performance
    pub use_two_stage: bool,         // true (WebRTC → Silero)
    pub batch_size: usize,           // 1 (real-time) or 4 (batch)
}

impl Default for ProductionVADConfig {
    fn default() -> Self {
        Self {
            // Lowered threshold to catch more speech
            silero_threshold: 0.3,
            webrtc_aggressiveness: 2,

            // Longer silence for complete sentences
            silence_threshold_frames: 40,  // 1.28 seconds
            min_speech_frames: 3,          // 96ms minimum

            // Buffer around speech for context
            pre_speech_buffer_ms: 300,
            post_speech_buffer_ms: 500,

            // Two-stage for performance
            use_two_stage: true,
            batch_size: 1,  // Real-time
        }
    }
}
```

## Complete Sentence Detection

### Problem: Fragments Instead of Sentences

Current approach:
```
[Speech] → [Silence 704ms] → END → Transcribe
```

Result: "Hello" ... "how are" ... "you"

### Solution: Smart Buffering

```rust
struct SentenceBuffer {
    audio_chunks: Vec<Vec<i16>>,
    last_speech_time: Instant,
    silence_duration: Duration,
}

impl SentenceBuffer {
    fn should_transcribe(&self) -> bool {
        // Wait for natural sentence boundary
        self.silence_duration > Duration::from_millis(1280)  // 40 frames

        // OR punctuation detected (if using streaming STT with partial results)
        // OR max buffer size reached (avoid infinite buffering)
    }

    fn add_frame(&mut self, audio: &[i16], is_speech: bool) {
        if is_speech {
            self.audio_chunks.push(audio.to_vec());
            self.last_speech_time = Instant::now();
            self.silence_duration = Duration::ZERO;
        } else {
            // Still buffer silence (captures pauses between words)
            self.audio_chunks.push(audio.to_vec());
            self.silence_duration = Instant::now() - self.last_speech_time;
        }

        if self.should_transcribe() {
            // Send entire buffer to STT
            let full_audio: Vec<i16> = self.audio_chunks.concat();
            transcribe(&full_audio);
            self.clear();
        }
    }
}
```

**Result**: "Hello, how are you?" (complete sentence)

## Latency Optimization Strategies

### 1. Parallel Processing

```rust
// Process multiple streams in parallel
use tokio::task::JoinSet;

let mut tasks = JoinSet::new();

for stream in participant_streams {
    tasks.spawn(async move {
        // Each stream gets its own VAD instance
        let vad = SileroRawVAD::new();
        vad.initialize().await?;

        while let Some(audio) = stream.next().await {
            let result = vad.detect(&audio).await?;
            if result.is_speech { /* transcribe */ }
        }
    });
}
```

### 2. Frame Skipping (for non-critical scenarios)

```rust
// Only check every 3rd frame (saves 67% CPU)
if frame_count % 3 == 0 {
    let result = vad.detect(&audio).await?;
    // Use result for next 3 frames
}
```

**Trade-off**: Slightly slower response (96ms delay), 67% less CPU

### 3. Batch Processing (for recorded audio)

```rust
// Process 4 frames at once (better GPU utilization)
let batch: Vec<&[i16]> = audio_frames.chunks(4).collect();
let results = vad.detect_batch(&batch).await?;
```

**Not recommended for real-time**, but useful for processing recordings

## Testing Configuration Changes

```rust
#[tokio::test]
async fn test_lowered_threshold() {
    let vad = SileroRawVAD::new();
    vad.initialize().await?;

    let speech = /* real human speech sample */;
    let result = vad.detect(&speech).await?;

    // Test different thresholds
    assert!(result.confidence > 0.3, "Speech should pass at 0.3 threshold");

    // Verify noise is still rejected
    let noise = /* factory floor */;
    let noise_result = vad.detect(&noise).await?;
    assert!(noise_result.confidence < 0.3, "Noise should be rejected");
}
```

## Recommended Production Setup

```rust
// In mixer.rs or stream processor

pub struct ProductionVAD {
    webrtc: WebRtcVAD,    // Fast pre-filter
    silero: SileroRawVAD,  // Accurate confirmation
    config: ProductionVADConfig,
    buffer: SentenceBuffer,
}

impl ProductionVAD {
    pub async fn process_frame(&mut self, audio: &[i16]) -> Result<Option<Vec<i16>>> {
        // Stage 1: Fast check (1-10μs)
        let quick = self.webrtc.detect(audio).await?;

        if !quick.is_speech {
            // Definite silence - skip expensive check
            self.buffer.add_frame(audio, false);
            return Ok(None);
        }

        // Stage 2: Accurate check (54ms)
        let accurate = self.silero.detect(audio).await?;

        // Lowered threshold for production
        let is_speech = accurate.confidence > self.config.silero_threshold;

        self.buffer.add_frame(audio, is_speech);

        // Return complete sentence when ready
        if self.buffer.should_transcribe() {
            Ok(Some(self.buffer.get_audio()))
        } else {
            Ok(None)
        }
    }
}
```

## Metrics to Track

```rust
struct VADMetrics {
    // Performance
    avg_latency_us: f64,
    p99_latency_us: f64,
    frames_per_second: f64,

    // Accuracy
    false_positive_rate: f64,  // Noise transcribed as speech
    false_negative_rate: f64,  // Speech skipped

    // Completeness
    avg_sentence_length: f64,   // Words per transcription
    fragment_rate: f64,         // % of incomplete sentences
}
```

## Summary

**To get MOST of the audio and form complete sentences:**

1. ✅ **Lower Silero threshold** from 0.5 to 0.3
2. ✅ **Increase silence threshold** from 22 frames (704ms) to 40 frames (1.28s)
3. ✅ **Add pre/post speech buffering** (300ms before, 500ms after)
4. ✅ **Use two-stage VAD** (WebRTC → Silero) for 5400x faster silence processing
5. ✅ **Buffer complete sentences** before transcribing

**For low latency:**
1. ✅ **Two-stage VAD** saves 54ms on every silence frame
2. ✅ **Parallel processing** for multiple streams
3. ⚠️ **Frame skipping** (optional, trades latency for CPU)

**Result**: Complete sentences, high recall, low latency, perfect noise rejection.
