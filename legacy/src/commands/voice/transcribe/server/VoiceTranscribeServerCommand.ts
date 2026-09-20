/**
 * Voice Transcribe Command - Server Implementation
 *
 * Transcribe audio to text using Rust Whisper (STT).
 * Wraps the streaming-core Whisper adapter for speech-to-text conversion.
 *
 * Architecture: TypeScript command -> gRPC -> Rust streaming-core worker
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { VoiceTranscribeParams, VoiceTranscribeResult, TranscriptSegment } from '../shared/VoiceTranscribeTypes';
import { createVoiceTranscribeResultFromParams } from '../shared/VoiceTranscribeTypes';
import { VoiceGrpcClient } from '@system/core/services/VoiceGrpcClient';

export class VoiceTranscribeServerCommand extends CommandBase<VoiceTranscribeParams, VoiceTranscribeResult> {
  private voiceClient: VoiceGrpcClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/transcribe', context, subpath, commander);
    this.voiceClient = VoiceGrpcClient.sharedInstance();
  }

  async execute(params: VoiceTranscribeParams): Promise<VoiceTranscribeResult> {
    console.log('🎤 SERVER: Executing Voice Transcribe');

    // Validate required parameter
    if (!params.audio || params.audio.trim() === '') {
      throw new ValidationError(
        'audio',
        `Missing required parameter 'audio'. ` +
        `Provide base64-encoded PCM audio data. See voice/transcribe README for details.`
      );
    }

    // Decode base64 audio
    let audioBuffer: Buffer;
    try {
      audioBuffer = Buffer.from(params.audio, 'base64');
    } catch {
      throw new ValidationError(
        'audio',
        `Invalid base64 encoding for 'audio' parameter.`
      );
    }

    // Call Rust worker via gRPC
    try {
      const response = await this.voiceClient.transcribe({
        audio: audioBuffer,
        sampleRate: params.format === 'wav' ? undefined : 16000, // wav has header, pcm16 is 16kHz
        language: params.language || 'auto',
        model: params.model || 'base',
      });

      // Map segments to our format
      const segments: TranscriptSegment[] = response.segments.map(s => ({
        word: s.word,
        start: s.start,
        end: s.end,
      }));

      return createVoiceTranscribeResultFromParams(params, {
        success: true,
        text: response.text,
        language: response.language,
        confidence: response.confidence,
        segments,
      });
    } catch (err) {
      // Check if voice worker is not running
      const errorMessage = err instanceof Error ? err.message : String(err);
      if (errorMessage.includes('not found') || errorMessage.includes('UNAVAILABLE')) {
        throw new Error(
          `Voice worker not available. Ensure streaming-core worker is running on port 50052. ` +
          `Error: ${errorMessage}`
        );
      }
      throw err;
    }
  }
}
