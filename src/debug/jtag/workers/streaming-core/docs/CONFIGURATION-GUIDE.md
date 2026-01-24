# Configuration Guide

Complete reference for all configuration options.

## ProductionVAD Configuration

### Default Configuration

```rust
use streaming_core::vad::{ProductionVAD, ProductionVADConfig};

// Use defaults (recommended for most use cases)
let vad = ProductionVAD::new();
vad.initialize().await?;
```

**Default settings**:
```rust
ProductionVADConfig {
    silero_threshold: 0.3,              // Balanced sensitivity
    webrtc_aggressiveness: 2,           // Moderate filtering
    silence_threshold_frames: 40,       // 1.28s pause
    min_speech_frames: 3,               // 96ms minimum
    pre_speech_buffer_ms: 300,          // 300ms before speech
    post_speech_buffer_ms: 500,         // 500ms after speech
    use_two_stage: true,                // Performance optimization
}
```

### Custom Configuration

```rust
let config = ProductionVADConfig {
    silero_threshold: 0.25,             // More sensitive
    silence_threshold_frames: 50,       // Longer pauses
    ..Default::default()                // Keep other defaults
};

let vad = ProductionVAD::with_config(config);
```

### Configuration Parameters

#### silero_threshold: f32

**Range**: 0.0 - 1.0
**Default**: 0.3
**Purpose**: Confidence threshold for speech detection

| Value | Sensitivity | Use Case | Trade-off |
|-------|-------------|----------|-----------|
| 0.5 | Conservative | Quiet environments | Misses quiet speech |
| 0.3 | **Balanced** | General use | **Recommended** ✅ |
| 0.25 | Sensitive | Noisy environments | Some false positives |
| 0.2 | Very sensitive | Very noisy (factory) | More false positives |

**Example**:
```rust
let config = ProductionVADConfig {
    silero_threshold: 0.25,  // Catch more speech in noisy environment
    ..Default::default()
};
```

#### webrtc_aggressiveness: u8

**Range**: 0 - 3
**Default**: 2
**Purpose**: WebRTC pre-filter aggressiveness

| Value | Behavior | Use Case |
|-------|----------|----------|
| 0 | Least aggressive | Very clean audio |
| 1 | Low | Clean audio |
| 2 | **Moderate** | **General use** ✅ |
| 3 | Most aggressive | Very noisy audio |

**Note**: Higher values = more false negatives but fewer false positives

#### silence_threshold_frames: u32

**Range**: 10 - 100 (typical)
**Default**: 40 (1.28 seconds)
**Purpose**: Frames of silence before ending speech

**Calculation**: `frames * 32ms = silence duration`

| Frames | Duration | Use Case |
|--------|----------|----------|
| 20 | 640ms | Fast response (may fragment) |
| 30 | 960ms | Quick pauses |
| 40 | **1.28s** | **Natural speech** ✅ |
| 50 | 1.6s | Long pauses (thoughtful speech) |
| 60 | 1.92s | Very long pauses |

**Example**:
```rust
let config = ProductionVADConfig {
    silence_threshold_frames: 50,  // 1.6s - good for thoughtful speech
    ..Default::default()
};
```

#### min_speech_frames: u32

**Range**: 1 - 10
**Default**: 3 (96ms)
**Purpose**: Minimum speech duration before transcribing

| Value | Duration | Purpose |
|-------|----------|---------|
| 1 | 32ms | Catch everything (noisy) |
| 2 | 64ms | Quick sounds |
| 3 | **96ms** | **Avoid spurious** ✅ |
| 5 | 160ms | Filter clicks/pops |
| 10 | 320ms | Only substantial speech |

#### pre_speech_buffer_ms: u32

**Range**: 0 - 1000
**Default**: 300ms
**Purpose**: Capture audio before speech detected

**Benefits**:
- Catches beginning of words
- Prevents cutting off "attack" sounds
- Improves transcription accuracy

**Example**:
```rust
let config = ProductionVADConfig {
    pre_speech_buffer_ms: 500,  // Capture more context
    ..Default::default()
};
```

#### post_speech_buffer_ms: u32

**Range**: 0 - 1000
**Default**: 500ms
**Purpose**: Continue capturing after last speech

**Benefits**:
- Catches trailing sounds
- Prevents cutting off "release" sounds
- Ensures complete words

#### use_two_stage: bool

**Default**: true
**Purpose**: Use WebRTC → Silero two-stage detection

**Performance**:
- `true`: 5400x faster on silence (recommended)
- `false`: Always use Silero (more accurate but slower)

**When to disable**:
- Never (always keep enabled for production)
- Maybe for testing/debugging only

## Environment-Specific Configurations

### Clean Environment (Home Office, Studio)

```rust
let config = ProductionVADConfig {
    silero_threshold: 0.4,              // Can be more selective
    webrtc_aggressiveness: 1,           // Low filtering
    silence_threshold_frames: 30,       // Quick responses
    ..Default::default()
};
```

### Moderate Environment (Office, Café)

```rust
// Use defaults - they're optimized for this
let vad = ProductionVAD::new();
```

### Noisy Environment (Factory, Street)

```rust
let config = ProductionVADConfig {
    silero_threshold: 0.25,             // More sensitive
    webrtc_aggressiveness: 3,           // Aggressive filtering
    silence_threshold_frames: 45,       // Longer pauses
    min_speech_frames: 5,               // Filter more noise
    ..Default::default()
};
```

### Very Noisy (Construction, Events)

```rust
let config = ProductionVADConfig {
    silero_threshold: 0.2,              // Very sensitive
    webrtc_aggressiveness: 3,           // Maximum filtering
    silence_threshold_frames: 50,       // Even longer pauses
    min_speech_frames: 5,               // Filter noise bursts
    pre_speech_buffer_ms: 400,          // More context
    post_speech_buffer_ms: 600,         // Catch everything
    ..Default::default()
};
```

## Mixer Configuration

### Basic Setup

```rust
use streaming_core::mixer::{AudioMixer, ParticipantStream};

// Create mixer with default settings
let mixer = AudioMixer::default_voice();  // 16kHz, 20ms frames

// Or custom settings
let mixer = AudioMixer::new(
    16000,  // sample_rate
    320,    // frame_size (20ms @ 16kHz)
);
```

### Adding Participants

```rust
// Human participant (with VAD)
let mut human = ParticipantStream::new(
    handle,
    "user-id".into(),
    "User Name".into(),
);
human.initialize_vad().await?;
mixer.add_participant(human);

// AI participant (no VAD)
let ai = ParticipantStream::new_ai(
    handle,
    "ai-id".into(),
    "AI Assistant".into(),
);
mixer.add_participant(ai);  // No VAD init needed
```

## TTS/STT Configuration

### TTS Registry

```rust
use streaming_core::tts;

// Initialize registry
tts::init_registry();

// Get adapter
let adapter = tts::get_registry()
    .read()
    .get_active()
    .unwrap();

// Configure adapter
adapter.set_param("speed", "1.2")?;        // 20% faster
adapter.set_param("pitch", "1.1")?;        // Slightly higher pitch

// Or switch adapter
tts::get_registry()
    .write()
    .set_active("kokoro")?;
```

### STT Registry

```rust
use streaming_core::stt;

// Initialize registry
stt::init_registry();

// Configure Whisper
let adapter = stt::get_registry()
    .read()
    .get("whisper")
    .unwrap();

adapter.set_param("language", "en")?;
adapter.set_param("beam_size", "5")?;      // Beam search width
```

## Runtime Configuration Changes

### Adjusting VAD During Runtime

```rust
// Create with initial config
let mut vad = ProductionVAD::new();
vad.initialize().await?;

// Cannot change config after initialization
// Instead, create new VAD with updated config
let new_config = ProductionVADConfig {
    silero_threshold: 0.25,  // Adjusted for noisier environment
    ..vad.config().clone()
};

let mut new_vad = ProductionVAD::with_config(new_config);
new_vad.initialize().await?;

// Replace in participant
participant.vad = Some(new_vad);
```

### Dynamic Adapter Switching

```rust
// Switch TTS adapter
tts::get_registry()
    .write()
    .set_active("piper")?;

// Initialize new adapter
tts::initialize().await?;

// Now all calls use Piper
let speech = tts::synthesize("Hello", "default").await?;
```

## Configuration Best Practices

### 1. Start with Defaults

```rust
// Don't over-optimize initially
let vad = ProductionVAD::new();  // Use defaults
```

### 2. Tune Based on Metrics

```rust
// Collect metrics first
let false_positive_rate = calculate_fpr();
let false_negative_rate = calculate_fnr();

// Then adjust
if false_positive_rate > 0.1 {
    // Raise threshold
    config.silero_threshold += 0.05;
}

if false_negative_rate > 0.1 {
    // Lower threshold
    config.silero_threshold -= 0.05;
}
```

### 3. Test Configuration Changes

```bash
# Always test after changing config
cargo test --test vad_production -- --ignored
cargo test --test mixer_production_vad_integration -- --ignored
```

### 4. Document Environment-Specific Configs

```rust
// Production deployment config
// Environment: Office with AC noise
// Tuned: 2024-01-24
let config = ProductionVADConfig {
    silero_threshold: 0.3,
    silence_threshold_frames: 40,
    // ... document why each setting
};
```

## Configuration File Example

```rust
// config/production.rs
use streaming_core::vad::ProductionVADConfig;

pub fn get_vad_config(environment: &str) -> ProductionVADConfig {
    match environment {
        "quiet" => ProductionVADConfig {
            silero_threshold: 0.4,
            silence_threshold_frames: 30,
            ..Default::default()
        },
        "moderate" => ProductionVADConfig::default(),
        "noisy" => ProductionVADConfig {
            silero_threshold: 0.25,
            webrtc_aggressiveness: 3,
            silence_threshold_frames: 45,
            ..Default::default()
        },
        _ => ProductionVADConfig::default(),
    }
}
```

**Usage**:
```rust
let config = get_vad_config("moderate");
let vad = ProductionVAD::with_config(config);
```
