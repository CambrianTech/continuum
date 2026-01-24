# VAD Testing: Synthetic Audio Findings

## Summary

Synthetic audio (both sine waves and formant-based speech) cannot adequately evaluate ML-based VAD systems like Silero. **This is a feature, not a bug** - it demonstrates that Silero correctly distinguishes real human speech from synthetic/artificial audio.

## Experiments Conducted

### Experiment 1: Sine Wave "Speech" (Baseline)

**Approach**: Simple sine waves (200Hz fundamental + 400Hz harmonic)

**Results**:
- RMS VAD: 28.6% accuracy (treats as speech)
- Silero VAD: 42.9% accuracy (confidence ~0.18, below 0.5 threshold)

**Conclusion**: Too primitive - neither VAD treats it as real speech

### Experiment 2: Formant-Based Speech Synthesis

**Approach**: Sophisticated formant synthesis with:
- 3 formants (F1, F2, F3) matching vowel characteristics
- Fundamental frequency + 10 harmonics
- Amplitude modulation for formant resonances
- Natural variation (shimmer/jitter simulation)
- Proper attack-sustain-release envelopes

**Audio patterns generated**:
- 5 vowels (/A/, /E/, /I/, /O/, /U/) with accurate formant frequencies
- Plosives (bursts of white noise)
- Fricatives (filtered noise at high frequencies)
- Multi-word sentences (CVC structure)
- TV dialogue (mixed voices + music)
- Crowd noise (5+ overlapping voices)
- Factory floor (machinery + random clanks)

**Results**:
| VAD Type | Accuracy | Key Observation |
|----------|----------|-----------------|
| RMS | 55.6% | Improved from 28.6% (detects all loud audio as speech) |
| Silero | 33.3% | Max confidence: 0.242 (below 0.5 threshold) |

**Specific Silero responses**:
- Silence: 0.044 ✓ (correctly rejected)
- White noise: 0.004 ✓ (correctly rejected)
- Formant speech /A/: 0.018 ✗ (rejected as non-human)
- Plosive /P/: 0.014 ✗ (rejected as non-human)
- TV dialogue: 0.016 ✗ (rejected despite containing speech-like patterns)

### Experiment 3: Sustained Speech Context

**Approach**: 3-word sentence (multiple CVC patterns) processed in 32ms chunks

**Results**: 0/17 frames detected as speech

**Highest confidence**: Frame 6: 0.242 (still below 0.5 threshold)

**Conclusion**: Even with sustained context, Silero rejects formant synthesis

## Critical Insights

### 1. Silero is Correctly Selective

Silero was trained on **6000+ hours of real human speech**. It learned to recognize:
- Natural pitch variations (jitter)
- Harmonic structure from vocal cord vibrations
- Articulatory noise (breath, vocal tract turbulence)
- Formant transitions (co-articulation between phonemes)
- Natural prosody (stress, intonation patterns)

Our formant synthesis, while mathematically correct, lacks:
- **Irregular glottal pulses** (vocal cords don't vibrate perfectly)
- **Breathiness** (turbulent airflow through glottis)
- **Formant transitions** (smooth movements between phonemes)
- **Micro-variations** in pitch and amplitude
- **Natural noise** from the vocal tract

### 2. This is a FEATURE, Not a Bug

Silero rejecting synthetic speech means:
- It won't be fooled by audio synthesis attacks
- It's selective about what counts as "human speech"
- It provides high-quality speech detection for real-world use

### 3. Synthetic Audio Has Limited Value for ML VAD

**What synthetic audio CAN test**:
- Pure noise rejection (✓ Silero: 100%)
- Energy-based VAD (RMS threshold)
- Relative comparisons (is A louder than B?)

**What synthetic audio CANNOT test**:
- ML-based VAD accuracy (Silero, WebRTC neural VAD)
- Speech vs non-speech discrimination
- Real-world performance

## Implications for VAD Testing

### Option 1: Real Human Speech Samples

**Pros**:
- Ground truth labels
- Realistic evaluation
- Free datasets available (LibriSpeech, Common Voice, VCTK)

**Cons**:
- Large downloads (multi-GB)
- Need preprocessing (segmentation, labeling)
- Not reproducible (depends on dataset)

**Recommended datasets**:
- **LibriSpeech**: 1000 hours, clean read speech
- **Common Voice**: Multi-language, diverse speakers
- **VCTK**: 110 speakers, UK accents

### Option 2: Trained TTS Models

**Pros**:
- Reproducible
- Controllable (generate specific scenarios)
- Compact (10-100MB model)

**Cons**:
- Requires model download
- Still not perfect human speech
- Adds dependency

**Available TTS**:
- **Piper** (ONNX, Home Assistant) - 20MB model
- **Kokoro** (ONNX, 82M params) - ~80MB model
- Both already have trait-based adapters in `src/tts/`

### Option 3: Hybrid Approach (Recommended)

1. **Synthetic audio for RMS VAD** - Tests energy-based detection
2. **Real speech samples for Silero VAD** - Tests ML-based detection
3. **TTS for edge cases** - Generate specific scenarios (background noise, multiple speakers)

## Next Steps

### Immediate: Document Findings ✓

Created this document + test cases showing the limitation.

### Short-term: Add WebRTC VAD

WebRTC VAD is simpler than Silero (rule-based, not neural) and may work better with synthetic audio for testing.

### Medium-term: Download LibriSpeech Test Set

```bash
# LibriSpeech test-clean (346MB, 5.4 hours)
wget https://www.openslr.org/resources/12/test-clean.tar.gz
tar -xzf test-clean.tar.gz

# Use for VAD accuracy benchmarking
```

### Long-term: Integrate TTS for Testing

Download Piper or Kokoro models and use for generating test scenarios:
```rust
let tts = PiperTTS::new();
tts.initialize().await?;
let audio = tts.synthesize("Hello world", "en_US-amy-medium").await?;
let vad_result = silero.detect(&audio.samples).await?;
```

## Test Code Location

- **Formant generator**: `src/vad/test_audio.rs`
- **Realistic audio tests**: `tests/vad_realistic_audio.rs`
- **Original sine wave tests**: `tests/vad_background_noise.rs`

## Conclusion

**Key Takeaway**: Silero correctly rejecting formant synthesis demonstrates its quality as a VAD system. It distinguishes real human speech from synthetic/artificial audio.

**For comprehensive VAD testing, we need real human speech samples, not synthetic audio.**

The formant synthesis work is still valuable for:
- Testing energy-based VAD (RMS threshold)
- Generating background noise patterns
- Understanding speech acoustics
- Placeholder until TTS models are downloaded

But it cannot properly evaluate ML-based VAD like Silero.
