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
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { generateUUID } from '@system/core/types/CrossPlatformUUID';
import { Events } from '@system/core/shared/Events';

// Valid TTS adapters (must match streaming-core TTS registry)
const VALID_ADAPTERS = ['piper', 'kokoro', 'silence'];

export class VoiceSynthesizeServerCommand extends CommandBase<VoiceSynthesizeParams, VoiceSynthesizeResult> {
  private voiceClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/synthesize', context, subpath, commander);
    this.voiceClient = new RustCoreIPCClient('/tmp/continuum-core.sock');
    this.voiceClient.connect().catch(err => {
      console.error('Failed to connect to continuum-core:', err);
    });
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
    const adapter = params.adapter || 'piper';
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

    try {
      // Call Rust TTS via IPC (continuum-core)
      const response = await this.voiceClient.voiceSynthesize(
        params.text,
        params.voice || 'af', // Default to female American English
        adapter
      );

      const audioBase64 = response.audio.toString('base64');
      const durationSec = response.durationMs / 1000;

      console.log(`🔊 Synthesized ${response.audio.length} bytes (${durationSec.toFixed(2)}s)`);
      console.log(`🔊 Emitting voice:audio:${handle} (${audioBase64.length} chars base64)`);

      // Emit real synthesized audio
      await Events.emit(`voice:audio:${handle}`, {
        handle,
        audio: audioBase64,
        sampleRate: response.sampleRate,
        duration: durationSec,
        adapter: response.adapter,
        final: true
      });

      console.log(`🔊 Emitting voice:done:${handle}`);
      await Events.emit(`voice:done:${handle}`, {
        handle,
        duration: durationSec,
        adapter: response.adapter
      });

      console.log(`🔊 synthesizeAndEmit complete for handle ${handle}`);
    } catch (err) {
      console.error(`🔊 TTS synthesis failed:`, err);
      throw err;
    }
  }
}
