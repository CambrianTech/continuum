/**
 * Voice Synthesize Command - Server Implementation
 *
 * Synthesize text to speech using Kokoro (primary, local ONNX) or Edge-TTS (cloud).
 *
 * Architecture:
 * - Kokoro/Piper/Silence: TypeScript -> IPC -> Rust continuum-core worker
 * - Edge-TTS: TypeScript -> Microsoft Edge TTS API (no Rust, no IPC)
 *
 * Kokoro is the default — 82M params, ~97ms TTFB, natural voices.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { VoiceSynthesizeParams, VoiceSynthesizeResult } from '../shared/VoiceSynthesizeTypes';
import { AUDIO_SAMPLE_RATE } from '../../../../shared/AudioConstants';
import { createVoiceSynthesizeResultFromParams } from '../shared/VoiceSynthesizeTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { generateUUID } from '@system/core/types/CrossPlatformUUID';
import { Events } from '@system/core/shared/Events';

// Valid TTS adapters: Rust-side (IPC) and TypeScript-side (direct)
const RUST_ADAPTERS = ['kokoro', 'piper', 'silence'];
const TS_ADAPTERS = ['edge-tts'];
const VALID_ADAPTERS = [...RUST_ADAPTERS, ...TS_ADAPTERS];

// Max queued Piper requests — Piper blocks the Rust event loop (~42s/request).
// Kokoro is fast enough (~97ms) that queuing is unnecessary.
const MAX_PIPER_QUEUE = 2;

interface QueuedTtsRequest {
  resolve: (value: any) => void;
  reject: (reason: any) => void;
  fn: () => Promise<any>;
  handle: string;
}

export class VoiceSynthesizeServerCommand extends CommandBase<VoiceSynthesizeParams, VoiceSynthesizeResult> {
  private voiceClient: RustCoreIPCClient;

  // Piper-only sequential queue — Piper blocks the Rust event loop (~42s),
  // so concurrent IPC requests get dropped. Kokoro is fast (~97ms) and
  // uses spawn_blocking, so it doesn't need queuing.
  private static _piperQueue: QueuedTtsRequest[] = [];
  private static _piperProcessing = false;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/synthesize', context, subpath, commander);
    this.voiceClient = new RustCoreIPCClient('/tmp/continuum-core.sock');
    this.voiceClient.connect().catch(err => {
      console.error('Failed to connect to continuum-core:', err);
    });
  }

  /**
   * Enqueue a Piper-only TTS request for sequential processing.
   * Other adapters (Kokoro, Edge-TTS) bypass this queue entirely.
   */
  private enqueuePiper<T>(handle: string, fn: () => Promise<T>): Promise<T> {
    if (VoiceSynthesizeServerCommand._piperQueue.length >= MAX_PIPER_QUEUE) {
      const queueSize = VoiceSynthesizeServerCommand._piperQueue.length;
      return Promise.reject(new Error(
        `Piper queue full (${queueSize}/${MAX_PIPER_QUEUE}) — Piper is single-threaded, dropping request`
      ));
    }

    return new Promise<T>((resolve, reject) => {
      VoiceSynthesizeServerCommand._piperQueue.push({ resolve, reject, fn, handle });
      console.log(`🔊 Piper queue: added ${handle.slice(0, 8)} (queue=${VoiceSynthesizeServerCommand._piperQueue.length}/${MAX_PIPER_QUEUE})`);
      VoiceSynthesizeServerCommand.processPiperQueue();
    });
  }

  private static async processPiperQueue(): Promise<void> {
    if (VoiceSynthesizeServerCommand._piperProcessing) return;
    VoiceSynthesizeServerCommand._piperProcessing = true;

    while (VoiceSynthesizeServerCommand._piperQueue.length > 0) {
      const item = VoiceSynthesizeServerCommand._piperQueue.shift()!;
      console.log(`🔊 Piper queue: processing ${item.handle.slice(0, 8)} (remaining=${VoiceSynthesizeServerCommand._piperQueue.length})`);
      try {
        const result = await item.fn();
        item.resolve(result);
      } catch (err) {
        item.reject(err);
      }
    }

    VoiceSynthesizeServerCommand._piperProcessing = false;
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

    // Validate adapter if provided (default: kokoro)
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
      sampleRate: params.sampleRate || AUDIO_SAMPLE_RATE,
      duration: 0, // Unknown until complete
      adapter,
    });
  }

  /**
   * Async synthesis — routes to the correct adapter:
   * - Piper: queued (blocks Rust event loop, ~42s)
   * - Kokoro: direct IPC (fast, spawn_blocking, ~97ms)
   * - Edge-TTS: direct TypeScript (no IPC, Microsoft cloud)
   * - Silence: direct IPC (instant)
   */
  private async synthesizeAndEmit(
    handle: string,
    params: VoiceSynthesizeParams,
    adapter: string,
    speed: number
  ): Promise<void> {
    console.log(`🔊 synthesizeAndEmit [${adapter}] started for handle ${handle.slice(0, 8)}`);

    try {
      let response: { audio: Buffer; sampleRate: number; durationMs: number; adapter: string };

      if (adapter === 'piper') {
        // Piper: sequential queue (blocks Rust event loop)
        response = await this.enqueuePiper(handle, () =>
          this.voiceClient.voiceSynthesize(params.text, params.voice || 'af', adapter)
        );
      } else if (RUST_ADAPTERS.includes(adapter)) {
        // Kokoro, Silence: direct IPC (no queue needed)
        response = await this.voiceClient.voiceSynthesize(
          params.text, params.voice || 'af', adapter
        );
      } else if (adapter === 'edge-tts') {
        // Edge-TTS: TypeScript-only, no Rust IPC
        response = await this.synthesizeWithEdgeTTS(params.text, params.voice);
      } else {
        throw new Error(`Unknown adapter: ${adapter}`);
      }

      const audioBase64 = response.audio.toString('base64');
      const durationSec = response.durationMs / 1000;

      console.log(`🔊 [${adapter}] Synthesized ${response.audio.length} bytes (${durationSec.toFixed(2)}s) handle=${handle.slice(0, 8)}`);

      // Emit audio event
      await Events.emit(`voice:audio:${handle}`, {
        handle,
        audio: audioBase64,
        sampleRate: response.sampleRate,
        duration: durationSec,
        adapter,
        final: true
      });

      await Events.emit(`voice:done:${handle}`, {
        handle,
        duration: durationSec,
        adapter
      });
    } catch (err) {
      console.error(`🔊 [${adapter}] TTS synthesis failed:`, err);
      throw err;
    }
  }

  /**
   * Edge-TTS synthesis — runs entirely in TypeScript via Microsoft's free TTS API.
   * No Rust IPC needed. Returns MP3 decoded to PCM i16 at 16kHz.
   */
  private async synthesizeWithEdgeTTS(
    text: string,
    voice?: string
  ): Promise<{ audio: Buffer; sampleRate: number; durationMs: number; adapter: string }> {
    const { MsEdgeTTS, OUTPUT_FORMAT } = await import('edge-tts-node');

    const edgeTTS = new MsEdgeTTS({ enableLogger: false });
    const edgeVoice = voice || 'en-US-AriaNeural';
    await edgeTTS.setMetadata(edgeVoice, OUTPUT_FORMAT.AUDIO_24KHZ_48KBITRATE_MONO_MP3);

    // Collect MP3 stream into buffer
    const stream = edgeTTS.toStream(text);
    const chunks: Buffer[] = [];
    for await (const chunk of stream) {
      chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
    }
    const mp3Buffer = Buffer.concat(chunks);
    edgeTTS.close();

    // Decode MP3 to PCM i16 at 16kHz using ffmpeg
    const { execSync } = await import('child_process');
    const tmpInput = `/tmp/edge-tts-${Date.now()}.mp3`;
    const tmpOutput = `/tmp/edge-tts-${Date.now()}.raw`;

    const fs = await import('fs');
    fs.writeFileSync(tmpInput, mp3Buffer);

    try {
      execSync(
        `ffmpeg -y -i "${tmpInput}" -f s16le -acodec pcm_s16le -ar ${AUDIO_SAMPLE_RATE} -ac 1 "${tmpOutput}"`,
        { stdio: 'pipe', timeout: 30000 }
      );

      const pcmBuffer = fs.readFileSync(tmpOutput);
      const durationMs = (pcmBuffer.length / 2 / AUDIO_SAMPLE_RATE) * 1000;

      return {
        audio: pcmBuffer,
        sampleRate: AUDIO_SAMPLE_RATE,
        durationMs,
        adapter: 'edge-tts'
      };
    } finally {
      // Clean up temp files
      try { fs.unlinkSync(tmpInput); } catch {}
      try { fs.unlinkSync(tmpOutput); } catch {}
    }
  }
}
