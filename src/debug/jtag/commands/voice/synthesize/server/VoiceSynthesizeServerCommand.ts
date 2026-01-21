/**
 * Voice Synthesize Command - Server Implementation
 *
 * Synthesize text to speech using Rust TTS (Kokoro primary).
 * Wraps the streaming-core TTS adapters for text-to-speech conversion.
 *
 * Architecture: TypeScript command -> gRPC -> Rust streaming-core worker
 *
 * Supported adapters (in quality order per TTS Arena):
 * - kokoro: #1 rated (80.9% win rate) - default
 * - fish-speech: Natural conversational
 * - f5-tts: Zero-shot voice cloning
 * - styletts2: Style transfer
 * - xtts-v2: Multi-lingual
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { VoiceSynthesizeParams, VoiceSynthesizeResult } from '../shared/VoiceSynthesizeTypes';
import { createVoiceSynthesizeResultFromParams } from '../shared/VoiceSynthesizeTypes';
import { VoiceGrpcClient } from '@system/core/services/VoiceGrpcClient';
import { generateUUID } from '@system/core/types/CrossPlatformUUID';
import { Events } from '@system/core/shared/Events';

// Valid TTS adapters
const VALID_ADAPTERS = ['kokoro', 'fish-speech', 'f5-tts', 'styletts2', 'xtts-v2'];

export class VoiceSynthesizeServerCommand extends CommandBase<VoiceSynthesizeParams, VoiceSynthesizeResult> {
  private voiceClient: VoiceGrpcClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/synthesize', context, subpath, commander);
    this.voiceClient = VoiceGrpcClient.sharedInstance();
  }

  async execute(params: VoiceSynthesizeParams): Promise<VoiceSynthesizeResult> {
    console.log('🔊 SERVER: Executing Voice Synthesize');

    // Validate required parameter
    if (!params.text || params.text.trim() === '') {
      throw new ValidationError(
        'text',
        `Missing required parameter 'text'. ` +
        `Provide text to synthesize. See voice/synthesize README for details.`
      );
    }

    // Validate adapter if provided
    const adapter = params.adapter || 'kokoro';
    if (!VALID_ADAPTERS.includes(adapter)) {
      throw new ValidationError(
        'adapter',
        `Invalid adapter '${adapter}'. Valid adapters: ${VALID_ADAPTERS.join(', ')}`
      );
    }

    // Validate speed range
    const speed = params.speed ?? 1.0;
    if (speed < 0.5 || speed > 2.0) {
      throw new ValidationError(
        'speed',
        `Speed must be between 0.5 and 2.0, got ${speed}`
      );
    }

    // ALWAYS use handle-based streaming for voice
    // Voice synthesis is inherently async - return handle immediately, emit events
    const handle = generateUUID();

    // Start async synthesis - don't await!
    this.synthesizeAndEmit(handle, params, adapter, speed).catch(err => {
      console.error(`🔊 voice/synthesize: Async synthesis failed for handle ${handle}:`, err);
      // Emit error event
      Events.emit(`voice:error:${handle}`, {
        handle,
        error: err instanceof Error ? err.message : String(err)
      });
    });

    // Return handle immediately - audio will stream via events
    return createVoiceSynthesizeResultFromParams(params, {
      success: true,
      audio: '', // Audio comes via events, not response
      handle,
      sampleRate: params.sampleRate || 24000,
      duration: 0, // Unknown until complete
      adapter,
    });
  }

  /**
   * Async synthesis - emits audio chunks as events
   */
  private async synthesizeAndEmit(
    handle: string,
    params: VoiceSynthesizeParams,
    adapter: string,
    speed: number
  ): Promise<void> {
    console.log(`🔊 synthesizeAndEmit started for handle ${handle}`);

    // STUB: Generate silence until streaming-core is configured
    // 1 second of 16-bit PCM silence at 24kHz = 48000 bytes
    const sampleRate = params.sampleRate || 24000;
    const durationSec = 1.0;
    const numSamples = Math.floor(sampleRate * durationSec);
    const stubAudio = Buffer.alloc(numSamples * 2); // 16-bit = 2 bytes per sample

    // Generate a simple sine wave beep (440Hz) instead of silence so we know it works
    for (let i = 0; i < numSamples; i++) {
      const t = i / sampleRate;
      const sample = Math.sin(2 * Math.PI * 440 * t) * 0.3; // 440Hz at 30% volume
      const intSample = Math.floor(sample * 32767);
      stubAudio.writeInt16LE(intSample, i * 2);
    }

    const audioBase64 = stubAudio.toString('base64');
    console.log(`🔊 Emitting voice:audio:${handle} (${audioBase64.length} chars base64)`);

    // Emit stub audio immediately
    await Events.emit(`voice:audio:${handle}`, {
      handle,
      audio: audioBase64,
      sampleRate,
      duration: durationSec,
      adapter: 'stub',
      final: true
    });

    console.log(`🔊 Emitting voice:done:${handle}`);
    await Events.emit(`voice:done:${handle}`, {
      handle,
      duration: durationSec,
      adapter: 'stub'
    });

    console.log(`🔊 synthesizeAndEmit complete for handle ${handle}`);
  }
}
