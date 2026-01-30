/**
 * Voice Transcribe Command - Shared Types
 *
 * Transcribe audio to text using Rust Whisper (STT). Wraps the streaming-core Whisper adapter for speech-to-text conversion.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Word-level transcript segment with timing
 */
export interface TranscriptSegment {
  word: string;
  start: number; // Start time in seconds
  end: number;   // End time in seconds
}

/**
 * Voice Transcribe Command Parameters
 */
export interface VoiceTranscribeParams extends CommandParams {
  // Base64-encoded audio data (PCM 16-bit, 16kHz mono)
  audio: string;
  // Audio format hint: 'pcm16', 'wav', 'opus' (defaults to 'pcm16')
  format?: string;
  // Language code hint (e.g., 'en', 'es', 'auto'). Defaults to auto-detect.
  language?: string;
  // Whisper model size: 'tiny', 'base', 'small', 'medium', 'large'. Defaults to 'base'.
  model?: string;
}

/**
 * Factory function for creating VoiceTranscribeParams
 */
export const createVoiceTranscribeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Base64-encoded audio data (PCM 16-bit, 16kHz mono)
    audio: string;
    // Audio format hint: 'pcm16', 'wav', 'opus' (defaults to 'pcm16')
    format?: string;
    // Language code hint (e.g., 'en', 'es', 'auto'). Defaults to auto-detect.
    language?: string;
    // Whisper model size: 'tiny', 'base', 'small', 'medium', 'large'. Defaults to 'base'.
    model?: string;
  }
): VoiceTranscribeParams => createPayload(context, sessionId, {
  format: data.format ?? '',
  language: data.language ?? '',
  model: data.model ?? '',
  ...data
});

/**
 * Voice Transcribe Command Result
 */
export interface VoiceTranscribeResult extends CommandResult {
  success: boolean;
  // Transcribed text
  text: string;
  // Detected language code
  language: string;
  // Confidence score (0-1)
  confidence: number;
  // Word-level timestamps if available
  segments: TranscriptSegment[];
  error?: JTAGError;
}

/**
 * Factory function for creating VoiceTranscribeResult with defaults
 */
export const createVoiceTranscribeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Transcribed text
    text?: string;
    // Detected language code
    language?: string;
    // Confidence score (0-1)
    confidence?: number;
    // Word-level timestamps if available
    segments?: TranscriptSegment[];
    error?: JTAGError;
  }
): VoiceTranscribeResult => createPayload(context, sessionId, {
  text: data.text ?? '',
  language: data.language ?? '',
  confidence: data.confidence ?? 0,
  segments: data.segments ?? [],
  ...data
});

/**
 * Smart Voice Transcribe-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createVoiceTranscribeResultFromParams = (
  params: VoiceTranscribeParams,
  differences: Omit<VoiceTranscribeResult, 'context' | 'sessionId'>
): VoiceTranscribeResult => transformPayload(params, differences);

/**
 * VoiceTranscribe — Type-safe command executor
 *
 * Usage:
 *   import { VoiceTranscribe } from '...shared/VoiceTranscribeTypes';
 *   const result = await VoiceTranscribe.execute({ ... });
 */
export const VoiceTranscribe = {
  execute(params: CommandInput<VoiceTranscribeParams>): Promise<VoiceTranscribeResult> {
    return Commands.execute<VoiceTranscribeParams, VoiceTranscribeResult>('voice/transcribe', params as Partial<VoiceTranscribeParams>);
  },
  commandName: 'voice/transcribe' as const,
} as const;
