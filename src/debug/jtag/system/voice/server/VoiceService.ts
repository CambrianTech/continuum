/**
 * Voice Service
 *
 * High-level API for TTS/STT used by PersonaUser and other AI agents.
 * Handles adapter selection, fallback, and audio format conversion.
 */

import { Commands } from '../../core/shared/Commands';
import { Events } from '../../core/shared/Events';
import type { VoiceConfig, TTSAdapter } from '../shared/VoiceConfig';
import { DEFAULT_VOICE_CONFIG, TTS_ADAPTERS } from '../shared/VoiceConfig';
import type { VoiceSynthesizeParams, VoiceSynthesizeResult } from '../../../commands/voice/synthesize/shared/VoiceSynthesizeTypes';
import { AUDIO_SAMPLE_RATE } from '../../../shared/AudioConstants';

import { VoiceSynthesize } from '../../../commands/voice/synthesize/shared/VoiceSynthesizeTypes';
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
   */
  private async synthesizeWithAdapter(
    text: string,
    adapter: TTSAdapter,
    voice: string,
    speed: number
  ): Promise<SynthesizeSpeechResult> {
    const timeout = this.config.maxSynthesisTimeMs;

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error(`TTS synthesis timeout (${timeout}ms)`));
      }, timeout);

      // Call voice/synthesize command
      VoiceSynthesize.execute({
        text,
        adapter,
        voice,
        speed,
        sampleRate: AUDIO_SAMPLE_RATE,
      }).then((result) => {
        const handle = result.handle;

        // Subscribe to audio event
        const unsubAudio = Events.subscribe(`voice:audio:${handle}`, (event: any) => {
          try {
            // Decode base64 to buffer
            const audioBuffer = Buffer.from(event.audio, 'base64');

            // Convert to i16 array (WebSocket format)
            const audioSamples = new Int16Array(audioBuffer.length / 2);
            for (let i = 0; i < audioSamples.length; i++) {
              audioSamples[i] = audioBuffer.readInt16LE(i * 2);
            }

            clearTimeout(timer);
            unsubAudio();

            resolve({
              audioSamples,
              sampleRate: event.sampleRate || 16000,
              durationMs: event.duration * 1000,
              adapter: event.adapter,
            });
          } catch (err) {
            clearTimeout(timer);
            unsubAudio();
            reject(err);
          }
        });

        // Subscribe to error event
        Events.subscribe(`voice:error:${handle}`, (event: any) => {
          clearTimeout(timer);
          unsubAudio();
          reject(new Error(event.error));
        });
      }).catch((err) => {
        clearTimeout(timer);
        reject(err);
      });
    });
  }

  /**
   * Transcribe audio to text (future - not implemented yet)
   */
  async transcribeAudio(audioSamples: Int16Array, sampleRate: number): Promise<string> {
    // TODO: Implement STT via voice/transcribe command
    throw new Error('Not implemented yet');
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
