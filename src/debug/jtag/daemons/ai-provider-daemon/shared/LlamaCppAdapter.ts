/**
 * LlamaCpp Adapter - Native llama.cpp Bindings
 * ============================================
 *
 * Adapter for native llama.cpp inference via node-llama-cpp.
 * Provides true parallel inference without HTTP bottlenecks.
 *
 * Features:
 * - Native C++ bindings (no HTTP overhead)
 * - True parallel inference across worker processes
 * - Uses downloaded GGUF models
 * - Metal/CUDA/Vulkan acceleration support
 * - Privacy-first (data never leaves machine)
 *
 * Architecture:
 * - Each worker loads its own model instance
 * - Workers run in parallel without blocking
 * - No shared state between workers
 */

import type {
  AIProviderAdapter,
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ProviderConfiguration,
  UsageMetrics,
  ModelInfo,
  ModelCapability,
} from './AIProviderTypesV2';
import {
  chatMessagesToPrompt,
  AIProviderError,
} from './AIProviderTypesV2';

// Helper functions
function createRequestId(): string {
  return `llamacpp-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

function estimateTokenCount(text: string): number {
  return Math.ceil(text.length / 4); // Rough approximation: 1 token ≈ 4 characters
}
import { getLlama, LlamaChatSession, type Llama, type LlamaModel, type LlamaContext } from 'node-llama-cpp';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

/**
 * Resolve model name to GGUF file path from local model storage
 */
function resolveLocalModelPath(modelName: string): string | null {
  const modelsDir = path.join(os.homedir(), '.continuum', 'models');
  const manifestPath = path.join(modelsDir, 'manifests', 'library', modelName.replace(':', '/'));

  if (!fs.existsSync(manifestPath)) {
    return null;
  }

  try {
    const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'));
    const modelLayer = manifest.layers?.find((layer: { mediaType?: string; digest?: string }) =>
      layer.mediaType === 'application/vnd.gguf.model'
    );

    if (!modelLayer) {
      return null;
    }

    const digest = modelLayer.digest.replace('sha256:', 'sha256-');
    const blobPath = path.join(modelsDir, 'blobs', digest);

    return fs.existsSync(blobPath) ? blobPath : null;
  } catch (error) {
    // Silent failure - caller will handle missing model
    return null;
  }
}

export class LlamaCppAdapter implements AIProviderAdapter {
  readonly providerId = 'llama-cpp';
  readonly providerName = 'LlamaCpp';
  readonly supportedCapabilities: ModelCapability[] = ['text-generation', 'chat'];

  private config: ProviderConfiguration;
  private llama: Llama | null = null;
  private model: LlamaModel | null = null;
  private context: LlamaContext | null = null;
  private session: LlamaChatSession | null = null;
  private currentModelName: string | null = null;
  private log: (message: string) => void;

  constructor(config?: Partial<ProviderConfiguration>, logger?: (message: string) => void) {
    this.log = logger || console.log.bind(console);
    this.config = {
      apiEndpoint: '', // Not used for native bindings
      timeout: 60000, // Longer timeout for native inference
      retryAttempts: 1,
      retryDelay: 1000,
      defaultModel: 'llama3.2:3b',
      defaultTemperature: 0.7,
      logRequests: true,
      ...config,
    };
  }

  async initialize(): Promise<void> {
    this.log(`🤖 ${this.providerName}: Initializing native llama.cpp bindings...`);

    try {
      this.llama = await getLlama();
      this.log(`✅ ${this.providerName}: llama.cpp bindings loaded`);

      // Pre-load default model
      const modelPath = resolveLocalModelPath(this.config.defaultModel);
      if (modelPath) {
        this.log(`🔄 ${this.providerName}: Pre-loading default model ${this.config.defaultModel}...`);
        await this.loadModel(this.config.defaultModel);
        this.log(`✅ ${this.providerName}: Default model loaded and ready`);
      } else {
        this.log(`⚠️  ${this.providerName}: Default model ${this.config.defaultModel} not found`);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      throw new AIProviderError(
        `Failed to initialize llama.cpp: ${errorMsg}`,
        'adapter',
        'LLAMACPP_INIT_FAILED',
        { error: errorMsg }
      );
    }
  }

  async shutdown(): Promise<void> {
    this.log(`🔄 ${this.providerName}: Shutting down...`);

    // Cleanup resources
    this.session = null;
    this.context = null;
    this.model = null;
    this.llama = null;
    this.currentModelName = null;

    this.log(`✅ ${this.providerName}: Shutdown complete`);
  }

  private async loadModel(modelName: string): Promise<void> {
    // Skip if already loaded
    if (this.currentModelName === modelName && this.model && this.context && this.session) {
      return;
    }

    const modelPath = resolveLocalModelPath(modelName);
    if (!modelPath) {
      throw new AIProviderError(
        `Model ${modelName} not found in local storage`,
        'adapter',
        'MODEL_NOT_FOUND',
        { modelName }
      );
    }

    if (this.config.logRequests) {
      this.log(`🔄 ${this.providerName}: Loading model from ${modelPath}...`);
    }

    try {
      if (!this.llama) {
        throw new Error('llama.cpp not initialized');
      }

      this.model = await this.llama.loadModel({ modelPath });
      this.context = await this.model.createContext();
      this.session = new LlamaChatSession({
        contextSequence: this.context.getSequence()
      });
      this.currentModelName = modelName;

      if (this.config.logRequests) {
        this.log(`✅ ${this.providerName}: Model ${modelName} loaded successfully`);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      throw new AIProviderError(
        `Failed to load model ${modelName}: ${errorMsg}`,
        'adapter',
        'MODEL_LOAD_FAILED',
        { modelName, modelPath, error: errorMsg }
      );
    }
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const requestId = request.requestId || createRequestId();

    try {
      const modelName = request.model || this.config.defaultModel;

      // Load model if needed
      await this.loadModel(modelName);

      if (!this.session) {
        throw new Error('Chat session not initialized');
      }

      // Convert chat messages to prompt
      const { prompt } = chatMessagesToPrompt(request.messages);

      if (this.config.logRequests) {
        this.log(`🤖 ${this.providerName}: Generating text with model ${modelName}`);
        this.log(`   Request ID: ${requestId}`);
        this.log(`   Prompt length: ${prompt.length} chars`);
      }

      // Generate response using native llama.cpp
      const response = await this.session.prompt(prompt, {
        temperature: request.temperature ?? this.config.defaultTemperature,
        maxTokens: request.maxTokens,
      });

      const responseTime = Date.now() - startTime;

      // Calculate usage metrics
      const usage: UsageMetrics = {
        inputTokens: estimateTokenCount(prompt),
        outputTokens: estimateTokenCount(response),
        totalTokens: 0,
        estimatedCost: 0, // Local inference is free
      };
      usage.totalTokens = usage.inputTokens + usage.outputTokens;

      if (this.config.logRequests) {
        this.log(`✅ ${this.providerName}: Generated response in ${responseTime}ms`);
        this.log(`   Output length: ${response.length} chars`);
        this.log(`   Tokens: ${usage.inputTokens} in, ${usage.outputTokens} out`);
      }

      return {
        text: response,
        finishReason: 'stop',
        model: modelName,
        provider: this.providerId,
        usage,
        responseTime,
        requestId,
      };
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      this.log(`❌ ${this.providerName}: Text generation failed: ${errorMsg}`);

      throw new AIProviderError(
        `Text generation failed: ${errorMsg}`,
        'adapter',
        'GENERATION_FAILED',
        { requestId, error: errorMsg }
      );
    }
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    const modelsDir = path.join(os.homedir(), '.continuum', 'models', 'manifests', 'library');

    if (!fs.existsSync(modelsDir)) {
      return [];
    }

    const models: ModelInfo[] = [];

    try {
      const entries = fs.readdirSync(modelsDir, { withFileTypes: true });

      for (const entry of entries) {
        if (entry.isDirectory()) {
          const modelName = entry.name;
          const variantsDir = path.join(modelsDir, modelName);
          const variants = fs.readdirSync(variantsDir);

          for (const variant of variants) {
            const fullName = `${modelName}:${variant}`;
            models.push({
              id: fullName,
              name: fullName,
              provider: this.providerId,
              capabilities: this.supportedCapabilities,
              contextWindow: 4096,
              supportsStreaming: false,
              supportsFunctions: false
            });
          }
        }
      }
    } catch (error) {
      this.log(`Failed to list models: ${error}`);
    }

    return models;
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const models = await this.getAvailableModels();

      return {
        status: models.length > 0 ? 'healthy' : 'degraded',
        apiAvailable: this.llama !== null,
        responseTime: 0,
        errorRate: 0,
        lastChecked: Date.now(),
        message: models.length > 0 
          ? `${models.length} models available` 
          : 'No models found',
      };
    } catch (error) {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime: 0,
        errorRate: 1,
        lastChecked: Date.now(),
        message: error instanceof Error ? error.message : 'Health check failed',
      };
    }
  }
}
