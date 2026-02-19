#!/usr/bin/env npx tsx
/**
 * Audio Constants Generator
 *
 * Generates TypeScript and Rust constant files from a single JSON source.
 * This ensures TS and Rust use EXACTLY the same values.
 *
 * Run with: npx tsx generator/generate-audio-constants.ts
 */

import * as fs from 'fs';
import * as path from 'path';

const SOURCE_FILE = path.join(__dirname, '../shared/audio-constants.json');
const TS_OUTPUT = path.join(__dirname, '../shared/AudioConstants.ts');
const RUST_OUTPUT = path.join(__dirname, '../workers/continuum-core/src/audio_constants.rs');

interface AudioConstants {
  AUDIO_SAMPLE_RATE: number;
  AUDIO_FRAME_SIZE: number;
  AUDIO_PLAYBACK_BUFFER_SECONDS: number;
  AUDIO_CHANNEL_CAPACITY: number;
  BYTES_PER_SAMPLE: number;
  CALL_SERVER_PORT: number;
}

function generateTypeScript(constants: AudioConstants): string {
  const frameDurationMs = (constants.AUDIO_FRAME_SIZE / constants.AUDIO_SAMPLE_RATE) * 1000;

  return `/**
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
export const AUDIO_SAMPLE_RATE = ${constants.AUDIO_SAMPLE_RATE};

/**
 * Frame size in samples (${constants.AUDIO_FRAME_SIZE} samples = ${frameDurationMs}ms at ${constants.AUDIO_SAMPLE_RATE / 1000}kHz)
 * Must be power of 2 for Web Audio API compatibility
 */
export const AUDIO_FRAME_SIZE = ${constants.AUDIO_FRAME_SIZE};

/**
 * Frame duration in milliseconds
 * Derived from AUDIO_FRAME_SIZE / AUDIO_SAMPLE_RATE * 1000
 */
export const AUDIO_FRAME_DURATION_MS = ${frameDurationMs};

/**
 * Playback buffer duration in seconds
 * Larger = more latency but handles jitter better
 */
export const AUDIO_PLAYBACK_BUFFER_SECONDS = ${constants.AUDIO_PLAYBACK_BUFFER_SECONDS};

/**
 * Audio broadcast channel capacity (number of frames)
 * At ${frameDurationMs}ms per frame, ${constants.AUDIO_CHANNEL_CAPACITY} frames = ~${Math.round(constants.AUDIO_CHANNEL_CAPACITY * frameDurationMs / 1000)} seconds of buffer
 */
export const AUDIO_CHANNEL_CAPACITY = ${constants.AUDIO_CHANNEL_CAPACITY};

/**
 * Bytes per sample (16-bit PCM = 2 bytes)
 */
export const BYTES_PER_SAMPLE = ${constants.BYTES_PER_SAMPLE};

/**
 * WebSocket call server port
 */
export const CALL_SERVER_PORT = ${constants.CALL_SERVER_PORT};

/**
 * Call server URL
 */
export const CALL_SERVER_URL = \`ws://127.0.0.1:\${CALL_SERVER_PORT}\`;
`;
}

function generateRust(constants: AudioConstants): string {
  const frameDurationMs = (constants.AUDIO_FRAME_SIZE / constants.AUDIO_SAMPLE_RATE) * 1000;

  return `//! Audio Constants - SINGLE SOURCE OF TRUTH
//!
//! AUTO-GENERATED from shared/audio-constants.json
//! DO NOT EDIT MANUALLY - run: npx tsx generator/generate-audio-constants.ts
//!
//! All audio-related constants MUST be imported from here.
//! DO NOT hardcode sample rates, buffer sizes, etc. anywhere else.

/// Standard sample rate for all audio in the system (Hz)
pub const AUDIO_SAMPLE_RATE: u32 = ${constants.AUDIO_SAMPLE_RATE};

/// Frame size in samples (${constants.AUDIO_FRAME_SIZE} samples = ${frameDurationMs}ms at ${constants.AUDIO_SAMPLE_RATE / 1000}kHz)
pub const AUDIO_FRAME_SIZE: usize = ${constants.AUDIO_FRAME_SIZE};

/// Frame duration in milliseconds
pub const AUDIO_FRAME_DURATION_MS: u64 = ${frameDurationMs};

/// Playback buffer duration in seconds
pub const AUDIO_PLAYBACK_BUFFER_SECONDS: u32 = ${constants.AUDIO_PLAYBACK_BUFFER_SECONDS};

/// Audio broadcast channel capacity (number of frames)
pub const AUDIO_CHANNEL_CAPACITY: usize = ${constants.AUDIO_CHANNEL_CAPACITY};

/// Bytes per sample (16-bit PCM = 2 bytes)
pub const BYTES_PER_SAMPLE: usize = ${constants.BYTES_PER_SAMPLE};

/// WebSocket call server port
pub const CALL_SERVER_PORT: u16 = ${constants.CALL_SERVER_PORT};
`;
}

async function main() {
  console.log('🎵 Generating audio constants from single source of truth...');

  // Read source JSON
  const jsonContent = fs.readFileSync(SOURCE_FILE, 'utf-8');
  const constants: AudioConstants & { _comment?: string } = JSON.parse(jsonContent);
  delete constants._comment;

  // Generate TypeScript
  const tsContent = generateTypeScript(constants as AudioConstants);
  fs.writeFileSync(TS_OUTPUT, tsContent);
  console.log(`✅ Generated TypeScript: ${TS_OUTPUT}`);

  // Generate Rust
  const rustContent = generateRust(constants as AudioConstants);
  fs.writeFileSync(RUST_OUTPUT, rustContent);
  console.log(`✅ Generated Rust: ${RUST_OUTPUT}`);

  console.log('🎵 Audio constants synchronized between TS and Rust');
}

main().catch(console.error);
