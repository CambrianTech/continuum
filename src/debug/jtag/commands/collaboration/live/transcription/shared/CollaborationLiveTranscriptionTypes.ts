/**
 * Collaboration Live Transcription Command - Shared Types
 *
 * Relay voice transcription from browser to server for AI processing
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Collaboration Live Transcription Command Parameters
 */
export interface CollaborationLiveTranscriptionParams extends CommandParams {
  // Call session UUID (from collaboration/live/join result)
  callSessionId: string;
  // User ID of the speaker (UUID)
  speakerId: string;
  // Display name of the speaker
  speakerName: string;
  // Transcribed text from speech
  transcript: string;
  // Transcription confidence score (0.0-1.0)
  confidence: number;
  // Detected language code (e.g., 'en', 'es')
  language: string;
  // Unix timestamp (milliseconds) when transcription occurred
  timestamp: number;
}

/**
 * Factory function for creating CollaborationLiveTranscriptionParams
 */
export const createCollaborationLiveTranscriptionParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Call session UUID (from collaboration/live/join result)
    callSessionId: string;
    // User ID of the speaker (UUID)
    speakerId: string;
    // Display name of the speaker
    speakerName: string;
    // Transcribed text from speech
    transcript: string;
    // Transcription confidence score (0.0-1.0)
    confidence: number;
    // Detected language code (e.g., 'en', 'es')
    language: string;
    // Unix timestamp (milliseconds) when transcription occurred
    timestamp: number;
  }
): CollaborationLiveTranscriptionParams => createPayload(context, sessionId, {

  ...data
});

/**
 * Collaboration Live Transcription Command Result
 */
export interface CollaborationLiveTranscriptionResult extends CommandResult {
  // Whether the transcription was successfully relayed
  success: boolean;
  // Status message
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating CollaborationLiveTranscriptionResult with defaults
 */
export const createCollaborationLiveTranscriptionResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Whether the transcription was successfully relayed
    success: boolean;
    // Status message
    message?: string;
    error?: JTAGError;
  }
): CollaborationLiveTranscriptionResult => createPayload(context, sessionId, {
  message: data.message ?? '',
  ...data
});

/**
 * Smart Collaboration Live Transcription-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCollaborationLiveTranscriptionResultFromParams = (
  params: CollaborationLiveTranscriptionParams,
  differences: Omit<CollaborationLiveTranscriptionResult, 'context' | 'sessionId'>
): CollaborationLiveTranscriptionResult => transformPayload(params, differences);
