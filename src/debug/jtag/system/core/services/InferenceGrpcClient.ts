/**
 * InferenceGrpcClient - gRPC client for Rust inference worker
 *
 * Replaces the broken Unix socket InferenceWorkerClient with proper gRPC:
 * - Built-in cancellation
 * - Proper timeouts
 * - Streaming responses
 * - No stuck mutexes
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import * as path from 'path';

// Load proto file
const PROTO_PATH = path.join(__dirname, '../../../workers/inference-grpc/proto/inference.proto');

const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
});

const inferenceProto = grpc.loadPackageDefinition(packageDefinition) as any;

export interface GenerateResult {
  text: string;
  tokens: number;
  durationMs: number;
}

export interface GenerateProgress {
  tokensGenerated: number;
  tokensTotal: number;
}

export class InferenceGrpcClient {
  private client: any;
  private static instance: InferenceGrpcClient | null = null;

  constructor(address: string = '127.0.0.1:50051') {
    this.client = new inferenceProto.inference.Inference(
      address,
      grpc.credentials.createInsecure()
    );
    console.log(`[InferenceGrpcClient] Connected to ${address}`);
  }

  static sharedInstance(): InferenceGrpcClient {
    if (!InferenceGrpcClient.instance) {
      InferenceGrpcClient.instance = new InferenceGrpcClient();
    }
    return InferenceGrpcClient.instance;
  }

  /**
   * Ping the server
   */
  async ping(): Promise<{ message: string; timestamp: number }> {
    return new Promise((resolve, reject) => {
      this.client.ping({}, (err: Error | null, response: any) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            message: response.message,
            timestamp: Number(response.timestamp),
          });
        }
      });
    });
  }

  /**
   * Generate text with streaming progress
   *
   * @param modelId - Model to use (e.g., "Qwen/Qwen2-1.5B-Instruct")
   * @param prompt - The prompt to generate from
   * @param options - Generation options
   */
  async generate(
    modelId: string,
    prompt: string,
    options?: {
      maxTokens?: number;
      temperature?: number;
      timeoutMs?: number;
      onProgress?: (progress: GenerateProgress) => void;
      signal?: AbortSignal;
    }
  ): Promise<GenerateResult> {
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + (options?.timeoutMs ?? 120000));
      const maxTokens = options?.maxTokens ?? 100;
      const temperature = options?.temperature ?? 0.7;

      const call = this.client.generate(
        { model_id: modelId, prompt, max_tokens: maxTokens, temperature },
        { deadline }
      );

      // Handle abort signal
      if (options?.signal) {
        options.signal.addEventListener('abort', () => {
          call.cancel();
          reject(new Error('Generation cancelled'));
        });
      }

      call.on('data', (response: any) => {
        if (response.progress) {
          options?.onProgress?.({
            tokensGenerated: response.progress.tokens_generated,
            tokensTotal: response.progress.tokens_total,
          });
        } else if (response.complete) {
          resolve({
            text: response.complete.text,
            tokens: response.complete.tokens,
            durationMs: response.complete.duration_ms,
          });
        }
      });

      call.on('error', (err: Error) => {
        reject(err);
      });

      call.on('end', () => {
        // If we get here without resolve, something went wrong
      });
    });
  }

  /**
   * Close the client connection
   */
  close(): void {
    this.client.close();
  }
}
