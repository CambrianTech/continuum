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

export interface ModelInfo {
  modelId: string;
  loaded: boolean;
  memoryBytes: number;
  dtype: string;
}

export interface AdapterInfo {
  adapterId: string;
  path: string;
  scale: number;
  active: boolean;
}

export interface ServerStatus {
  healthy: boolean;
  currentModel: string;
  memoryUsedBytes: number;
  memoryTotalBytes: number;
  requestsPending: number;
  requestsCompleted: number;
  activeAdapters: string[];
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

  // ========================================================================
  // Model Management
  // ========================================================================

  /**
   * Load a model by HuggingFace ID
   */
  async loadModel(modelId: string, dtype?: string): Promise<{ success: boolean; error?: string; loadTimeMs: number }> {
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + 300000); // 5 minutes for model loading
      this.client.loadModel({ model_id: modelId, dtype: dtype || '' }, { deadline }, (err: Error | null, response: any) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            success: response.success,
            error: response.error || undefined,
            loadTimeMs: Number(response.load_time_ms),
          });
        }
      });
    });
  }

  /**
   * Unload the current model
   */
  async unloadModel(): Promise<{ success: boolean; error?: string }> {
    return new Promise((resolve, reject) => {
      this.client.unloadModel({}, (err: Error | null, response: any) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            success: response.success,
            error: response.error || undefined,
          });
        }
      });
    });
  }

  /**
   * List loaded models
   */
  async listModels(): Promise<ModelInfo[]> {
    return new Promise((resolve, reject) => {
      this.client.listModels({}, (err: Error | null, response: any) => {
        if (err) {
          reject(err);
        } else {
          resolve((response.models || []).map((m: any) => ({
            modelId: m.model_id,
            loaded: m.loaded,
            memoryBytes: Number(m.memory_bytes),
            dtype: m.dtype,
          })));
        }
      });
    });
  }

  // ========================================================================
  // LoRA Adapter Management
  // ========================================================================

  /**
   * Load a LoRA adapter
   *
   * @param adapterId - Unique identifier for this adapter
   * @param adapterPath - Path to the adapter safetensors file
   * @param options - Loading options
   * @param options.scale - LoRA scale factor (default: 1.0)
   * @param options.merge - If true, merge weights into model immediately (slower but permanent)
   */
  async loadAdapter(
    adapterId: string,
    adapterPath: string,
    options?: { scale?: number; merge?: boolean }
  ): Promise<{ success: boolean; error?: string; loadTimeMs: number }> {
    return new Promise((resolve, reject) => {
      // Longer timeout when merging weights (rebuilds model)
      const timeoutMs = options?.merge ? 300000 : 60000;
      const deadline = new Date(Date.now() + timeoutMs);
      this.client.loadAdapter(
        {
          adapter_id: adapterId,
          adapter_path: adapterPath,
          scale: options?.scale ?? 1.0,
          merge: options?.merge ?? false,
        },
        { deadline },
        (err: Error | null, response: any) => {
          if (err) {
            reject(err);
          } else {
            resolve({
              success: response.success,
              error: response.error || undefined,
              loadTimeMs: Number(response.load_time_ms),
            });
          }
        }
      );
    });
  }

  /**
   * Unload a LoRA adapter
   */
  async unloadAdapter(adapterId: string): Promise<{ success: boolean; error?: string }> {
    return new Promise((resolve, reject) => {
      this.client.unloadAdapter({ adapter_id: adapterId }, (err: Error | null, response: any) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            success: response.success,
            error: response.error || undefined,
          });
        }
      });
    });
  }

  /**
   * List loaded adapters
   */
  async listAdapters(): Promise<AdapterInfo[]> {
    return new Promise((resolve, reject) => {
      this.client.listAdapters({}, (err: Error | null, response: any) => {
        if (err) {
          reject(err);
        } else {
          resolve((response.adapters || []).map((a: any) => ({
            adapterId: a.adapter_id,
            path: a.path,
            scale: a.scale,
            active: a.active,
          })));
        }
      });
    });
  }

  // ========================================================================
  // Genome (Multi-Adapter Stacking)
  // ========================================================================

  /**
   * Apply a genome (stack multiple adapters with scales)
   *
   * Formula: W' = W + Σ(scale_i × B_i @ A_i)
   *
   * @param adapters - Array of {adapterId, scale} pairs
   */
  async applyGenome(adapters: Array<{ adapterId: string; scale: number }>): Promise<{
    success: boolean;
    error?: string;
    applyTimeMs: number;
    adaptersApplied: number;
    layersMerged: number;
  }> {
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + 300000); // 5 minutes for genome rebuild
      const entries = adapters.map(a => ({
        adapter_id: a.adapterId,
        scale: a.scale,
      }));

      this.client.applyGenome({ adapters: entries }, { deadline }, (err: Error | null, response: any) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            success: response.success,
            error: response.error || undefined,
            applyTimeMs: Number(response.apply_time_ms),
            adaptersApplied: response.adapters_applied,
            layersMerged: response.layers_merged,
          });
        }
      });
    });
  }

  // ========================================================================
  // Server Status
  // ========================================================================

  /**
   * Get server status
   */
  async status(): Promise<ServerStatus> {
    return new Promise((resolve, reject) => {
      this.client.status({}, (err: Error | null, response: any) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            healthy: response.healthy,
            currentModel: response.current_model,
            memoryUsedBytes: Number(response.memory_used_bytes),
            memoryTotalBytes: Number(response.memory_total_bytes),
            requestsPending: response.requests_pending,
            requestsCompleted: response.requests_completed,
            activeAdapters: response.active_adapters || [],
          });
        }
      });
    });
  }
}
