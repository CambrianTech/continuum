/**
 * Voice Synthesize Command - Shared Types
 *
 * Synthesize text to speech using Rust TTS (Kokoro primary). Wraps the streaming-core TTS adapters for text-to-speech conversion.
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Voice Synthesize Command Parameters
 */
export interface VoiceSynthesizeParams extends CommandParams {
  // Text to synthesize
  text: string;
  // Voice ID or name. Defaults to model's default voice.
  voice?: string;
  // TTS adapter: 'kokoro', 'fish-speech', 'f5-tts', 'styletts2', 'xtts-v2'. Defaults to 'kokoro'.
  adapter?: string;
  // Speech rate multiplier (0.5-2.0). Defaults to 1.0.
  speed?: number;
  // Output sample rate in Hz. Defaults to adapter's native rate (usually 24000).
  sampleRate?: number;
  // Output format: 'pcm16', 'wav', 'opus'. Defaults to 'pcm16'.
  format?: string;
  // Return streaming handle instead of complete audio. Defaults to false.
  stream?: boolean;
}

/**
 * Factory function for creating VoiceSynthesizeParams
 */
export const createVoiceSynthesizeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Text to synthesize
    text: string;
    // Voice ID or name. Defaults to model's default voice.
    voice?: string;
    // TTS adapter: 'kokoro', 'fish-speech', 'f5-tts', 'styletts2', 'xtts-v2'. Defaults to 'kokoro'.
    adapter?: string;
    // Speech rate multiplier (0.5-2.0). Defaults to 1.0.
    speed?: number;
    // Output sample rate in Hz. Defaults to adapter's native rate (usually 24000).
    sampleRate?: number;
    // Output format: 'pcm16', 'wav', 'opus'. Defaults to 'pcm16'.
    format?: string;
    // Return streaming handle instead of complete audio. Defaults to false.
    stream?: boolean;
  }
): VoiceSynthesizeParams => createPayload(context, sessionId, {
  voice: data.voice ?? '',
  adapter: data.adapter ?? '',
  speed: data.speed ?? 0,
  sampleRate: data.sampleRate ?? 0,
  format: data.format ?? '',
  stream: data.stream ?? false,
  ...data
});

/**
 * Voice Synthesize Command Result
 */
export interface VoiceSynthesizeResult extends CommandResult {
  success: boolean;
  // Base64-encoded audio data (if stream=false)
  audio: string;
  // Streaming handle for WebSocket subscription (if stream=true)
  handle: string;
  // Actual sample rate of returned audio
  sampleRate: number;
  // Audio duration in seconds
  duration: number;
  // TTS adapter that was used
  adapter: string;
  error?: JTAGError;
}

/**
 * Factory function for creating VoiceSynthesizeResult with defaults
 */
export const createVoiceSynthesizeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Base64-encoded audio data (if stream=false)
    audio?: string;
    // Streaming handle for WebSocket subscription (if stream=true)
    handle?: string;
    // Actual sample rate of returned audio
    sampleRate?: number;
    // Audio duration in seconds
    duration?: number;
    // TTS adapter that was used
    adapter?: string;
    error?: JTAGError;
  }
): VoiceSynthesizeResult => createPayload(context, sessionId, {
  audio: data.audio ?? '',
  handle: data.handle ?? '',
  sampleRate: data.sampleRate ?? 0,
  duration: data.duration ?? 0,
  adapter: data.adapter ?? '',
  ...data
});

/**
 * Smart Voice Synthesize-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createVoiceSynthesizeResultFromParams = (
  params: VoiceSynthesizeParams,
  differences: Omit<VoiceSynthesizeResult, 'context' | 'sessionId'>
): VoiceSynthesizeResult => transformPayload(params, differences);
