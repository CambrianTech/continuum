/**
 * CandleGrpcAdapter - Local inference via gRPC
 *
 * Smart adapter with LoRA compatibility detection:
 * - Reads adapter manifests for proper scale (α/r)
 * - Detects garbage output and auto-blocklists bad adapters
 * - Falls back to safe mode (base model) on adapter failure
 * - Tracks model corruption state and auto-reloads
 */

import { randomUUID } from 'crypto';
import { existsSync, readFileSync } from 'fs';
import { resolve, dirname } from 'path';
import { BaseAIProviderAdapter } from '../../../shared/BaseAIProviderAdapter';
import type {
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ModelCapability,
  ModelInfo,
  UsageMetrics,
  RoutingInfo,
} from '../../../shared/AIProviderTypesV2';
import { InferenceGrpcClient } from '../../../../../system/core/services/InferenceGrpcClient';

// Adapter manifest structure (from HuggingFace PEFT format)
interface AdapterManifest {
  alpha: number;
  base_model: string;
  peft_type: string;
  rank: number;
  repo_id: string;
  target_modules: string[];
}

// Compatibility check result
interface CompatibilityResult {
  compatible: boolean;
  scale: number;  // Proper scale = alpha / rank
  warnings: string[];
  errors: string[];
}

// Our base model identifier
const BASE_MODEL_ID = 'unsloth/Llama-3.2-3B-Instruct';

export class CandleGrpcAdapter extends BaseAIProviderAdapter {
  readonly providerId = 'candle';
  readonly providerName = 'Candle (gRPC)';
  readonly supportedCapabilities: ModelCapability[] = ['text-generation', 'chat'];

  private client: InferenceGrpcClient;

  // Serial execution: only one request at a time
  // The Rust gRPC server is single-threaded, so we must wait for each request to complete
  private inFlight: boolean = false;
  private queueDepth: number = 0;
  // Increased timeout: local inference takes ~15s per request
  // With queue depth of 3, worst case wait is ~60s (acceptable for local inference)
  private static readonly MAX_WAIT_TIME_MS = 90000; // 90 seconds max wait

  // Track currently applied adapters (for genome integration)
  private appliedAdapters: Set<string> = new Set();

  // Blocklist for adapters that produce garbage output
  // Persisted across requests until model is reloaded
  private blockedAdapters: Set<string> = new Set();

  // Track if model is corrupted (adapter produced garbage)
  private modelCorrupted: boolean = false;

  // Safe mode: skip all adapters, use base model only
  private safeMode: boolean = false;

  constructor() {
    super();
    this.client = InferenceGrpcClient.sharedInstance();
    this.baseTimeout = 120000; // 2 minutes
  }

  /**
   * Read adapter manifest and check compatibility with our base model
   */
  private checkAdapterCompatibility(adapterPath: string): CompatibilityResult {
    const adapterDir = dirname(adapterPath);
    const manifestPath = resolve(adapterDir, 'manifest.json');

    const result: CompatibilityResult = {
      compatible: true,
      scale: 1.0,
      warnings: [],
      errors: [],
    };

    // Try to read manifest
    if (!existsSync(manifestPath)) {
      result.warnings.push('No manifest.json - using default scale=1.0');
      return result;
    }

    try {
      const manifest: AdapterManifest = JSON.parse(readFileSync(manifestPath, 'utf-8'));

      // Calculate proper LoRA scale (alpha / rank)
      if (manifest.rank && manifest.alpha) {
        result.scale = manifest.alpha / manifest.rank;
        console.log(`[CandleGrpcAdapter] 🧬 Adapter scale: α=${manifest.alpha} / r=${manifest.rank} = ${result.scale.toFixed(2)}`);
      }

      // Check base model compatibility
      const baseModel = manifest.base_model?.toLowerCase() || '';

      // Check for quantized adapters (4bit, bnb) - likely incompatible with BF16 model
      if (baseModel.includes('4bit') || baseModel.includes('bnb')) {
        result.errors.push(`Adapter trained on quantized model (${manifest.base_model}) - incompatible with BF16`);
        result.compatible = false;
      }
      // Check for meta-llama vs unsloth mismatch
      else if (baseModel.includes('meta-llama') && !baseModel.includes('unsloth')) {
        // meta-llama and unsloth versions SHOULD be compatible (same architecture)
        // but may have different weight initializations
        result.warnings.push(`Adapter trained on meta-llama, we use unsloth (may work)`);
      }
      // Check for completely different model
      else if (!baseModel.includes('llama') && !baseModel.includes('3.2') && !baseModel.includes('3b')) {
        result.errors.push(`Adapter trained on different model: ${manifest.base_model}`);
        result.compatible = false;
      }

      // Warn about extreme scale values
      if (result.scale > 10) {
        result.warnings.push(`Very high scale (${result.scale.toFixed(1)}) - may amplify errors`);
      } else if (result.scale < 0.1) {
        result.warnings.push(`Very low scale (${result.scale.toFixed(2)}) - may have no effect`);
      }

    } catch (err) {
      result.warnings.push(`Could not parse manifest: ${err instanceof Error ? err.message : err}`);
    }

    return result;
  }

  /**
   * Detect garbage output patterns that indicate adapter corruption
   */
  private detectGarbageOutput(text: string): boolean {
    // Pattern 1: [/xxx] brackets pattern (seen with Legal adapter)
    if (/\[\/[^\]]*\]/.test(text)) return true;

    // Pattern 2: Excessive brackets
    const bracketCount = (text.match(/\[/g) || []).length;
    if (bracketCount > 5 && bracketCount > text.length / 20) return true;

    // Pattern 3: Random Unicode outside expected ranges (Arabic in English context)
    const unexpectedUnicode = text.match(/[\u0600-\u06FF\u0900-\u097F\u4E00-\u9FFF]/g);
    if (unexpectedUnicode && unexpectedUnicode.length > 3) return true;

    // Pattern 4: Repetitive nonsense
    if (/(.{2,10})\1{4,}/.test(text)) return true;

    // Pattern 5: Mostly special characters
    const specialChars = (text.match(/[^\w\s.,!?'"()-]/g) || []).length;
    if (text.length > 20 && specialChars / text.length > 0.3) return true;

    return false;
  }

  /**
   * Enter safe mode - skip adapters and reload base model
   */
  async enterSafeMode(reason: string): Promise<void> {
    console.warn(`[CandleGrpcAdapter] ⚠️  ENTERING SAFE MODE: ${reason}`);
    this.safeMode = true;
    this.modelCorrupted = false;

    // Try to reload base model
    try {
      console.log(`[CandleGrpcAdapter] 🔄 Reloading base model...`);
      await this.client.unloadModel();
      await this.client.loadModel(BASE_MODEL_ID);
      this.appliedAdapters.clear();
      console.log(`[CandleGrpcAdapter] ✅ Base model reloaded, safe mode active`);
    } catch (err) {
      console.error(`[CandleGrpcAdapter] ❌ Failed to reload model:`, err);
    }
  }

  /**
   * Exit safe mode and allow adapter loading again
   * Call this after fixing adapter issues
   */
  exitSafeMode(): void {
    console.log(`[CandleGrpcAdapter] ✅ Exiting safe mode, adapters enabled`);
    this.safeMode = false;
  }

  /**
   * Clear blocklist for specific adapter (e.g., after scale fix)
   */
  unblockAdapter(adapterName: string): void {
    if (this.blockedAdapters.has(adapterName)) {
      this.blockedAdapters.delete(adapterName);
      console.log(`[CandleGrpcAdapter] ✅ Unblocked adapter: ${adapterName}`);
    }
  }

  /**
   * Get current adapter status for debugging
   */
  getAdapterStatus(): {
    safeMode: boolean;
    modelCorrupted: boolean;
    appliedAdapters: string[];
    blockedAdapters: string[];
  } {
    return {
      safeMode: this.safeMode,
      modelCorrupted: this.modelCorrupted,
      appliedAdapters: [...this.appliedAdapters],
      blockedAdapters: [...this.blockedAdapters],
    };
  }

  async initialize(): Promise<void> {
    // Test connection
    try {
      const pong = await this.client.ping();
      console.log(`[CandleGrpcAdapter] Connected: ${pong.message}`);
    } catch (err) {
      console.error(`[CandleGrpcAdapter] Failed to connect:`, err);
      throw err;
    }
  }

  async shutdown(): Promise<void> {
    this.client.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const start = Date.now();
      await this.client.ping();
      return {
        status: 'healthy',
        apiAvailable: true,
        responseTime: Date.now() - start,
        errorRate: 0,
        lastChecked: Date.now(),
      };
    } catch (err) {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime: 0,
        errorRate: 1,
        lastChecked: Date.now(),
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    // Return fake models for now
    return [
      {
        id: 'llama3.2:3b',
        name: 'Llama 3.2 3B',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 8192,
        supportsStreaming: true,
        supportsFunctions: false,
      },
    ];
  }

  async restartProvider(): Promise<void> {
    // Close and reconnect
    this.client.close();
    this.client = InferenceGrpcClient.sharedInstance();
    await this.initialize();
  }

  protected async generateTextImpl(
    request: TextGenerationRequest
  ): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const requestId = request.requestId || randomUUID();

    // Track queue depth for logging
    this.queueDepth++;
    if (this.queueDepth > 1) {
      console.log(`[CandleGrpcAdapter] Queue depth: ${this.queueDepth} (waiting for in-flight request)`);
    }

    // Wait for current request to finish (simple serial lock)
    // Local inference takes ~15s per request, so we wait patiently
    while (this.inFlight) {
      await new Promise(resolve => setTimeout(resolve, 100));
      // Check if we've been waiting too long
      if (Date.now() - startTime > CandleGrpcAdapter.MAX_WAIT_TIME_MS) {
        this.queueDepth--;
        throw new Error(`CandleGrpcAdapter: Timeout waiting for slot (${CandleGrpcAdapter.MAX_WAIT_TIME_MS / 1000}s)`);
      }
    }

    this.inFlight = true;
    this.queueDepth--;

    // Track adapters applied for this request
    const adaptersAppliedThisRequest: string[] = [];

    try {
      // GENOME INTEGRATION: Load adapters from request if provided
      // This enables PersonaGenome to specify which LoRA skills should be active
      if (request.activeAdapters && request.activeAdapters.length > 0) {
        // Check for safe mode or corrupted model
        if (this.safeMode) {
          console.log(`[CandleGrpcAdapter] 🧬 SAFE MODE: Skipping ${request.activeAdapters.length} adapters`);
        } else if (this.modelCorrupted) {
          console.log(`[CandleGrpcAdapter] 🧬 MODEL CORRUPTED: Entering safe mode`);
          await this.enterSafeMode('Model corrupted by previous adapter');
        } else {
          console.log(`[CandleGrpcAdapter] 🧬 Loading ${request.activeAdapters.length} adapters from request`);

          for (const adapter of request.activeAdapters) {
            // Skip blocked adapters
            if (this.blockedAdapters.has(adapter.name)) {
              console.warn(`[CandleGrpcAdapter] 🧬 ⛔ Skipping blocked adapter: ${adapter.name}`);
              continue;
            }

            // Skip already applied adapters
            if (this.appliedAdapters.has(adapter.name)) {
              console.log(`[CandleGrpcAdapter] 🧬 Adapter ${adapter.name} already loaded`);
              adaptersAppliedThisRequest.push(adapter.name);
              continue;
            }

            // Resolve absolute path
            const absolutePath = resolve(adapter.path);
            if (!existsSync(absolutePath)) {
              console.warn(`[CandleGrpcAdapter] 🧬 Skipping adapter ${adapter.name}: file not found at ${absolutePath}`);
              continue;
            }

            // Check adapter compatibility and get proper scale
            const compatibility = this.checkAdapterCompatibility(absolutePath);

            if (!compatibility.compatible) {
              console.warn(`[CandleGrpcAdapter] 🧬 ⛔ Adapter ${adapter.name} INCOMPATIBLE:`);
              compatibility.errors.forEach(err => console.warn(`     ❌ ${err}`));
              this.blockedAdapters.add(adapter.name);
              continue;
            }

            // Log warnings but proceed
            compatibility.warnings.forEach(warn => console.warn(`[CandleGrpcAdapter] 🧬 ⚠️  ${adapter.name}: ${warn}`));

            try {
              // Load adapter with PROPER scale from manifest (α/r)
              console.log(`[CandleGrpcAdapter] 🧬 Loading adapter ${adapter.name} with scale=${compatibility.scale.toFixed(2)}...`);
              const loadResult = await this.client.loadAdapter(adapter.name, absolutePath, {
                scale: compatibility.scale,  // Use calculated scale, NOT hardcoded 1.0
                merge: true,  // Merge weights into model
              });

              if (loadResult.success) {
                this.appliedAdapters.add(adapter.name);
                adaptersAppliedThisRequest.push(adapter.name);
                console.log(`[CandleGrpcAdapter] 🧬 ✅ Adapter ${adapter.name} loaded in ${loadResult.loadTimeMs}ms`);
              } else {
                console.warn(`[CandleGrpcAdapter] 🧬 ❌ Failed to load adapter ${adapter.name}: ${loadResult.error}`);
              }
            } catch (loadErr) {
              console.warn(`[CandleGrpcAdapter] 🧬 ❌ Error loading adapter ${adapter.name}:`, loadErr);
            }
          }
        }
      }

      // Convert messages to prompt and truncate to prevent OOM
      // Llama 3.2 3B has 128K context but we limit to 8K chars (~2K tokens) for memory safety
      const MAX_PROMPT_CHARS = 8000;
      let prompt = this.formatMessagesAsLlama32(request);
      if (prompt.length > MAX_PROMPT_CHARS) {
        console.log(`[CandleGrpcAdapter] Truncating prompt from ${prompt.length} to ${MAX_PROMPT_CHARS} chars`);
        prompt = prompt.slice(-MAX_PROMPT_CHARS); // Keep the most recent context
      }

      // Cap maxTokens to prevent queue starvation
      // Local inference ~14 tok/sec, so 150 tokens = ~10 seconds
      const requestedTokens = request.maxTokens || 200;
      const maxTokens = Math.min(requestedTokens, 150);

      // SINGLE MODEL ROUTING: All requests go to the loaded model
      // The gRPC server loads Llama-3.2-3B-Instruct
      const modelId = 'Llama-3.2-3B-Instruct';

      console.log(`[CandleGrpcAdapter] Generate: model=${modelId}, prompt=${prompt.length} chars, maxTokens=${maxTokens}, queue=${this.queueDepth}, adapters=[${adaptersAppliedThisRequest.join(',')}]`);

      const result = await this.client.generate(modelId, prompt, {
        maxTokens,
        temperature: request.temperature,
        timeoutMs: this.baseTimeout,
      });

      const responseTime = Date.now() - startTime;

      // GARBAGE DETECTION: Check if adapter corrupted the output
      if (adaptersAppliedThisRequest.length > 0 && this.detectGarbageOutput(result.text)) {
        console.error(`[CandleGrpcAdapter] 🚨 GARBAGE OUTPUT DETECTED!`);
        console.error(`[CandleGrpcAdapter] 🚨 Output preview: "${result.text.slice(0, 100)}..."`);
        console.error(`[CandleGrpcAdapter] 🚨 Adapters applied: ${adaptersAppliedThisRequest.join(', ')}`);

        // Blocklist all adapters that were applied in this request
        for (const adapterName of adaptersAppliedThisRequest) {
          this.blockedAdapters.add(adapterName);
          console.error(`[CandleGrpcAdapter] 🚨 BLOCKLISTED adapter: ${adapterName}`);
        }

        // Mark model as corrupted - next request will trigger safe mode
        this.modelCorrupted = true;

        // Return error response instead of garbage
        throw new Error(`Adapter produced garbage output. Adapters ${adaptersAppliedThisRequest.join(', ')} have been blocklisted. Model will reload in safe mode on next request.`);
      }

      const usage: UsageMetrics = {
        inputTokens: Math.ceil(prompt.length / 4), // rough estimate
        outputTokens: result.tokens,
        totalTokens: Math.ceil(prompt.length / 4) + result.tokens,
        estimatedCost: 0, // local = free
      };

      const routing: RoutingInfo = {
        provider: this.providerId,
        isLocal: true,
        routingReason: 'explicit_provider',
        adaptersApplied: adaptersAppliedThisRequest,
        modelRequested: request.model || 'llama3.2:3b',
      };

      return {
        text: result.text,
        finishReason: 'stop',
        model: request.model || 'llama3.2:3b',
        provider: this.providerId,
        usage,
        responseTime,
        requestId,
        routing,
      };
    } catch (err) {
      console.error(`[CandleGrpcAdapter] Generate failed:`, err);
      throw err;
    } finally {
      // Always release the lock
      this.inFlight = false;
    }
  }

  /**
   * Format chat messages using Llama 3.2 chat template
   *
   * Llama 3.2 uses special tokens:
   * - <|begin_of_text|> at the start
   * - <|start_header_id|>role<|end_header_id|> before each message
   * - <|eot_id|> at the end of each message
   *
   * Example:
   * <|begin_of_text|><|start_header_id|>system<|end_header_id|>
   * You are a helpful assistant.<|eot_id|><|start_header_id|>user<|end_header_id|>
   * What is 2+2?<|eot_id|><|start_header_id|>assistant<|end_header_id|>
   */
  private formatMessagesAsLlama32(request: TextGenerationRequest): string {
    if (!request.messages || request.messages.length === 0) {
      return '';
    }

    const parts: string[] = ['<|begin_of_text|>'];

    for (const msg of request.messages) {
      const role = msg.role === 'assistant' ? 'assistant' : msg.role === 'system' ? 'system' : 'user';
      parts.push(`<|start_header_id|>${role}<|end_header_id|>\n\n${msg.content}<|eot_id|>`);
    }

    // Add the assistant header to prompt the model to generate
    parts.push('<|start_header_id|>assistant<|end_header_id|>\n\n');

    return parts.join('');
  }
}
