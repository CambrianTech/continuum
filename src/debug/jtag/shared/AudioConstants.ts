/**
 * Audio Constants - SINGLE SOURCE OF TRUTH
 *
 * AUTO-GENERATED from shared/audio-constants.json
 * DO NOT EDIT MANUALLY - run: npx tsx generator/generate-audio-constants.ts
 *
 * All audio-related constants MUST be imported from here.
 * DO NOT hardcode sample rates, buffer sizes, etc. anywhere else.
 */

/**
 * Standard sample rate for all audio in the system.
 * - CallServer (Rust) uses this
 * - TTS adapters resample to this
 * - STT expects this
 * - Browser AudioContext uses this
 */
export const AUDIO_SAMPLE_RATE = 16000;

/**
 * Frame size in samples (512 samples = 32ms at 16kHz)
 * Must be power of 2 for Web Audio API compatibility
 */
export const AUDIO_FRAME_SIZE = 512;

/**
 * Frame duration in milliseconds
 * Derived from AUDIO_FRAME_SIZE / AUDIO_SAMPLE_RATE * 1000
 */
export const AUDIO_FRAME_DURATION_MS = 32;

/**
 * Playback buffer duration in seconds
 * Larger = more latency but handles jitter better
 */
export const AUDIO_PLAYBACK_BUFFER_SECONDS = 2;

/**
 * Audio broadcast channel capacity (number of frames)
 * At 32ms per frame, 2000 frames = ~64 seconds of buffer
 */
export const AUDIO_CHANNEL_CAPACITY = 2000;

/**
 * Bytes per sample (16-bit PCM = 2 bytes)
 */
export const BYTES_PER_SAMPLE = 2;

/**
 * WebSocket call server port
 */
export const CALL_SERVER_PORT = 50053;

/**
 * Call server URL
 */
export const CALL_SERVER_URL = `ws://127.0.0.1:${CALL_SERVER_PORT}`;
