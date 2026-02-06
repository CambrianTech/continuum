/**
 * Voice Service
 *
 * High-level API for TTS/STT used by PersonaUser and other AI agents.
 * Handles adapter selection, fallback, and audio format conversion.
 */

import { Events } from '../../core/shared/Events';
import type { VoiceConfig, TTSAdapter } from '../shared/VoiceConfig';
import { DEFAULT_VOICE_CONFIG } from '../shared/VoiceConfig';
import { AUDIO_SAMPLE_RATE } from '../../../shared/AudioConstants';
import { VoiceSynthesize } from '../../../commands/voice/synthesize/shared/VoiceSynthesizeTypes';
import { VoiceTranscribe } from '../../../commands/voice/transcribe/shared/VoiceTranscribeTypes';
export interface SynthesizeSpeechRequest {
  text: string;
  userId?: string;         // For per-user preferences
  adapter?: TTSAdapter;    // Override default
  voice?: string;
  speed?: number;
}

export interface SynthesizeSpeechResult {
  audioSamples: Int16Array;  // Ready for WebSocket
  sampleRate: number;
  durationMs: number;
  adapter: string;
}

/**
 * Voice Service
 *
 * Usage:
 *   const voice = new VoiceService();
 *   const result = await voice.synthesizeSpeech({ text: "Hello" });
 *   // result.audioSamples is i16 array ready for WebSocket
 */
export class VoiceService {
  private config: VoiceConfig;

  constructor(config: VoiceConfig = DEFAULT_VOICE_CONFIG) {
    this.config = config;
  }

  /**
   * Synthesize speech from text
   *
   * Returns i16 audio samples ready for WebSocket transmission.
   * Automatically handles:
   * - Adapter selection (default or override)
   * - Base64 decoding
   * - Format conversion to i16
   *
   * NO FALLBACKS - fails immediately if adapter doesn't work
   */
  async synthesizeSpeech(request: SynthesizeSpeechRequest): Promise<SynthesizeSpeechResult> {
    const adapter = request.adapter || this.config.tts.adapter;
    const adapterConfig = this.config.tts.adapters[adapter as keyof typeof this.config.tts.adapters];

    const voice = request.voice || (adapterConfig as any)?.voice || 'default';
    const speed = request.speed || (adapterConfig as any)?.speed || 1.0;

    // NO FALLBACKS - fail immediately if this doesn't work
    return await this.synthesizeWithAdapter(request.text, adapter, voice, speed);
  }

  /**
   * Synthesize with specific adapter
   *
   * Two-phase timeout strategy:
   * Phase 1: Command must return a handle (should be instant, ~15s safety)
   * Phase 2: Once handle received, Piper IS generating — no timeout.
   *          Only a catastrophic safety net (5 min) for total process death.
   *
   * We KNOW synthesis is active once the handle returns. Don't kill
   * expensive GPU/CPU work just because it takes longer than expected.
   */
  private async synthesizeWithAdapter(
    text: string,
    adapter: TTSAdapter,
    voice: string,
    speed: number
  ): Promise<SynthesizeSpeechResult> {
    const COMMAND_TIMEOUT_MS = 15_000;          // Handle must arrive fast
    const SAFETY_TIMEOUT_MS = 5 * 60 * 1_000;  // 5 min — catastrophic only

    return new Promise((resolve, reject) => {
      let safetyTimer: ReturnType<typeof setTimeout> | null = null;
      let settled = false;

      const settle = () => { settled = true; };

      // Phase 1: Command execution timeout
      const commandTimer = setTimeout(() => {
        if (settled) return;
        settle();
        reject(new Error(`TTS command timeout — failed to start synthesis within ${COMMAND_TIMEOUT_MS}ms`));
      }, COMMAND_TIMEOUT_MS);

      // Call voice/synthesize command
      VoiceSynthesize.execute({
        text,
        adapter,
        voice,
        speed,
        sampleRate: AUDIO_SAMPLE_RATE,
      }).then((result) => {
        // Phase 1 complete — handle received, synthesis is actively running
        clearTimeout(commandTimer);
        if (settled) return;

        const handle = result.handle;
        console.log(`🔊 VoiceService: Synthesis started (handle=${handle.slice(0, 8)}), adapter is generating...`);

        // Phase 2: Safety-only timeout — adapter is actively working
        safetyTimer = setTimeout(() => {
          if (settled) return;
          settle();
          unsubAudio();
          unsubError();
          reject(new Error(`TTS synthesis unresponsive — no audio or error after ${SAFETY_TIMEOUT_MS / 1000}s (catastrophic failure)`));
        }, SAFETY_TIMEOUT_MS);

        // Subscribe to audio event
        const unsubAudio = Events.subscribe(`voice:audio:${handle}`, (event: any) => {
          if (settled) return;
          try {
            // Decode base64 to buffer
            const audioBuffer = Buffer.from(event.audio, 'base64');

            // Convert to i16 array (WebSocket format)
            const audioSamples = new Int16Array(audioBuffer.length / 2);
            for (let i = 0; i < audioSamples.length; i++) {
              audioSamples[i] = audioBuffer.readInt16LE(i * 2);
            }

            settle();
            if (safetyTimer) clearTimeout(safetyTimer);
            unsubAudio();
            unsubError();

            resolve({
              audioSamples,
              sampleRate: event.sampleRate || 16000,
              durationMs: event.duration * 1000,
              adapter: event.adapter,
            });
          } catch (err) {
            settle();
            if (safetyTimer) clearTimeout(safetyTimer);
            unsubAudio();
            unsubError();
            reject(err);
          }
        });

        // Subscribe to error event
        const unsubError = Events.subscribe(`voice:error:${handle}`, (event: any) => {
          if (settled) return;
          settle();
          if (safetyTimer) clearTimeout(safetyTimer);
          unsubAudio();
          unsubError();
          reject(new Error(event.error));
        });
      }).catch((err) => {
        clearTimeout(commandTimer);
        if (settled) return;
        settle();
        reject(err);
      });
    });
  }

  /**
   * Transcribe audio to text via voice/transcribe command (Rust Whisper STT)
   */
  async transcribeAudio(audioSamples: Int16Array, sampleRate: number): Promise<string> {
    // Convert Int16Array to base64 for the command
    const buffer = Buffer.from(audioSamples.buffer, audioSamples.byteOffset, audioSamples.byteLength);
    const audio = buffer.toString('base64');

    const result = await VoiceTranscribe.execute({
      audio,
      format: 'pcm16',
      language: 'auto',
    });

    if (!result.success) {
      throw new Error(result.error?.message ?? 'Transcription failed');
    }

    return result.text;
  }
}

/**
 * Singleton instance for convenience
 */
let _voiceService: VoiceService | null = null;

export function getVoiceService(): VoiceService {
  if (!_voiceService) {
    _voiceService = new VoiceService();
  }
  return _voiceService;
}
