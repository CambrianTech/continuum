/**
 * VoiceGrpcClient - gRPC client for Rust streaming-core TTS/STT
 *
 * Mirrors InferenceGrpcClient pattern:
 * - Singleton instance
 * - Proper timeouts
 * - Clean interface for voice operations
 *
 * Connects to the streaming-core Rust worker which implements:
 * - Kokoro TTS (primary, #1 on TTS Arena)
 * - Fish Speech, F5-TTS, StyleTTS2, XTTSv2 (additional adapters)
 * - Whisper STT
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import * as path from 'path';

// Lazy-loaded proto to avoid module-level side effects
let _voiceProto: any = null;

function getVoiceProto(): any {
  if (!_voiceProto) {
    const PROTO_PATH = path.join(__dirname, '../../../workers/streaming-core/proto/voice.proto');
    const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
    });
    _voiceProto = grpc.loadPackageDefinition(packageDefinition);
  }
  return _voiceProto;
}

// ----- TTS Types -----

export interface TTSSynthesizeRequest {
  text: string;
  voice?: string;
  adapter?: string;
  speed?: number;
  sampleRate?: number;
}

export interface TTSSynthesizeResponse {
  audio: Buffer; // PCM 16-bit audio
  sampleRate: number;
  durationMs: number;
  adapter: string;
}

export interface TTSStreamChunk {
  audio: Buffer;
  isLast: boolean;
  chunkIndex: number;
}

// ----- STT Types -----

export interface STTTranscribeRequest {
  audio: Buffer; // PCM 16-bit audio
  sampleRate?: number;
  language?: string;
  model?: string;
}

export interface STTTranscribeResponse {
  text: string;
  language: string;
  confidence: number;
  segments: STTSegment[];
}

export interface STTSegment {
  word: string;
  start: number;
  end: number;
  confidence: number;
}

// ----- Client -----

export class VoiceGrpcClient {
  private client: any;
  private static instance: VoiceGrpcClient | null = null;
  private connected: boolean = false;

  constructor(private address: string = '127.0.0.1:50052') {
    // Port 50052 for voice (50051 is inference)
  }

  static sharedInstance(): VoiceGrpcClient {
    if (!VoiceGrpcClient.instance) {
      VoiceGrpcClient.instance = new VoiceGrpcClient();
    }
    return VoiceGrpcClient.instance;
  }

  private ensureClient(): void {
    if (!this.client) {
      try {
        const proto = getVoiceProto();
        const VoiceService = (proto as any).voice?.VoiceService;
        if (!VoiceService) {
          throw new Error('VoiceService not found in proto. Worker may not be running.');
        }
        this.client = new VoiceService(
          this.address,
          grpc.credentials.createInsecure()
        );
        this.connected = true;
      } catch (err) {
        this.connected = false;
        throw new Error(`Failed to connect to voice worker: ${err instanceof Error ? err.message : String(err)}`);
      }
    }
  }

  /**
   * Check if voice worker is available
   */
  async ping(): Promise<{ message: string; adapterCount: number }> {
    this.ensureClient();
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + 5000);
      this.client.Ping({}, { deadline }, (err: any, response: any) => {
        if (err) {
          this.connected = false;
          reject(new Error(`Voice worker ping failed: ${err.message}`));
        } else {
          resolve({
            message: response.message || 'pong',
            adapterCount: response.adapter_count || 0,
          });
        }
      });
    });
  }

  /**
   * Synthesize text to speech (batch mode)
   */
  async synthesize(request: TTSSynthesizeRequest): Promise<TTSSynthesizeResponse> {
    this.ensureClient();
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + 60000); // 60s timeout for TTS
      this.client.Synthesize(
        {
          text: request.text,
          voice: request.voice || '',
          adapter: request.adapter || 'kokoro',
          speed: request.speed || 1.0,
          sample_rate: request.sampleRate || 24000,
        },
        { deadline },
        (err: any, response: any) => {
          if (err) {
            reject(new Error(`TTS synthesis failed: ${err.message}`));
          } else {
            resolve({
              audio: Buffer.from(response.audio, 'base64'),
              sampleRate: response.sample_rate || 24000,
              durationMs: response.duration_ms || 0,
              adapter: response.adapter || 'kokoro',
            });
          }
        }
      );
    });
  }

  /**
   * Synthesize text to speech (streaming mode)
   * Returns async iterator of audio chunks
   */
  async *synthesizeStream(request: TTSSynthesizeRequest): AsyncGenerator<TTSStreamChunk> {
    this.ensureClient();
    const stream = this.client.SynthesizeStream({
      text: request.text,
      voice: request.voice || '',
      adapter: request.adapter || 'kokoro',
      speed: request.speed || 1.0,
      sample_rate: request.sampleRate || 24000,
    });

    let chunkIndex = 0;
    for await (const chunk of stream) {
      yield {
        audio: Buffer.from(chunk.audio, 'base64'),
        isLast: chunk.is_last || false,
        chunkIndex: chunkIndex++,
      };
    }
  }

  /**
   * Transcribe audio to text
   */
  async transcribe(request: STTTranscribeRequest): Promise<STTTranscribeResponse> {
    this.ensureClient();
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + 30000); // 30s timeout for STT
      this.client.Transcribe(
        {
          audio: request.audio.toString('base64'),
          sample_rate: request.sampleRate || 16000,
          language: request.language || 'auto',
          model: request.model || 'base',
        },
        { deadline },
        (err: any, response: any) => {
          if (err) {
            reject(new Error(`STT transcription failed: ${err.message}`));
          } else {
            resolve({
              text: response.text || '',
              language: response.language || 'en',
              confidence: response.confidence || 0,
              segments: (response.segments || []).map((s: any) => ({
                word: s.word || '',
                start: s.start || 0,
                end: s.end || 0,
                confidence: s.confidence || 0,
              })),
            });
          }
        }
      );
    });
  }

  /**
   * List available TTS adapters
   */
  async listAdapters(): Promise<Array<{ name: string; loaded: boolean; voiceCount: number }>> {
    this.ensureClient();
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + 5000);
      this.client.ListAdapters({}, { deadline }, (err: any, response: any) => {
        if (err) {
          reject(new Error(`Failed to list adapters: ${err.message}`));
        } else {
          resolve(
            (response.adapters || []).map((a: any) => ({
              name: a.name || '',
              loaded: a.loaded || false,
              voiceCount: a.voice_count || 0,
            }))
          );
        }
      });
    });
  }

  /**
   * Check connection status
   */
  isConnected(): boolean {
    return this.connected;
  }

  /**
   * Close the client connection
   */
  close(): void {
    if (this.client) {
      grpc.closeClient(this.client);
      this.client = null;
      this.connected = false;
    }
  }
}
