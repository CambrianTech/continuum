/**
 * Sentinel Adapter - Pre-trained Model Integration (HTTP)
 * ========================================================
 *
 * Adapter for Sentinel-AI server running local models.
 * Uses HTTP API (like Ollama) instead of shell execution.
 *
 * Features:
 * - Models stay loaded in memory (fast inference)
 * - Multiple model support (gpt2, phi-2, etc.)
 * - Non-blocking HTTP requests
 * - Health checks and graceful degradation
 *
 * Server: http://localhost:11435
 */

import type {
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ModelInfo,
  ModelCapability,
} from '../../../shared/AIProviderTypesV2';
import type { ChatMessage } from '../../../shared/PromptFormatters';
import { BaseAIProviderAdapter } from '../../../shared/BaseAIProviderAdapter';
import { truncateMessages } from '../../../shared/PromptFormatters';

// Helper function for token estimation
function estimateTokenCount(text: string): number {
  return Math.ceil(text.length / 4); // Rough approximation: 1 token ≈ 4 characters
}
import { spawn, type ChildProcess } from 'child_process';
import * as path from 'path';

interface SentinelGenerateRequest {
  model: string;
  prompt: string;
  system?: string;
  temperature?: number;
  num_predict?: number;
  stream?: boolean;
}

interface SentinelGenerateResponse {
  model: string;
  response: string;
  done: boolean;
  context?: number[];
  total_duration?: number;
  load_duration?: number;
  prompt_eval_duration?: number;
  eval_duration?: number;
}

interface SentinelModelInfo {
  name: string;
  size: string;
  family: string;
  modified_at: string;
}

/**
 * Sentinel Adapter - HTTP client for Sentinel server
 */
export class SentinelAdapter extends BaseAIProviderAdapter {
  readonly providerId = 'sentinel';
  readonly providerName = 'Sentinel AI';
  readonly supportedCapabilities: ModelCapability[] = ['text-generation', 'chat'];

  private readonly apiEndpoint: string;
  private readonly serverPort: string;
  private healthCache: { status: HealthStatus; timestamp: number } | null = null;
  private readonly healthCacheTTL = 5000; // 5 seconds
  private serverProcess: ChildProcess | null = null;
  private serverStarting = false;

  constructor() {
    super();

    // Get port from environment or use default
    this.serverPort = process.env.SENTINEL_PORT || '11435';
    this.apiEndpoint = `http://127.0.0.1:${this.serverPort}`;

    console.log('🧬 Sentinel Adapter initialized (HTTP mode)');
    console.log(`   Endpoint: ${this.apiEndpoint}`);
  }

  /**
   * Initialize Sentinel server (auto-start if needed)
   */
  protected async initializeProvider(): Promise<void> {
    console.log('🧬 Sentinel: Initializing provider...');

    // Check if server is already running
    const health = await this.healthCheck();

    if (health.status === 'healthy') {
      console.log('✅ Sentinel: Server already running');
      return;
    }

    // Try to auto-start the server
    console.log('🚀 Sentinel: Server not found, attempting auto-start...');
    await this.startServer();

    // Wait for server to be ready
    const maxWaitTime = 30000; // 30 seconds
    const startTime = Date.now();

    while (Date.now() - startTime < maxWaitTime) {
      await new Promise(resolve => setTimeout(resolve, 2000));

      const checkHealth = await this.healthCheck();
      if (checkHealth.status === 'healthy') {
        console.log('✅ Sentinel: Server started and ready');
        return;
      }
    }

    throw new Error(
      'Sentinel server failed to start. Please start manually: cd /Volumes/FlashGordon/cambrian/sentinel-ai && ./server/start_server.sh'
    );
  }

  /**
   * Restart Sentinel server
   */
  protected async restartProvider(): Promise<void> {
    console.log('🔄 Sentinel: Restarting server...');

    // Kill existing process if we have one
    if (this.serverProcess && !this.serverProcess.killed) {
      this.serverProcess.kill();
      await new Promise(resolve => setTimeout(resolve, 2000));
    }

    // Start fresh
    await this.startServer();
  }

  /**
   * Start the Sentinel server
   */
  private async startServer(): Promise<void> {
    if (this.serverStarting) {
      console.log('⏳ Sentinel: Server already starting, waiting...');
      return;
    }

    this.serverStarting = true;

    try {
      // Get path to sentinel-ai project
      const sentinelPath = process.env.SENTINEL_PATH || '/Volumes/FlashGordon/cambrian/sentinel-ai';
      const startScript = path.join(sentinelPath, 'server', 'start_server.sh');

      console.log(`🧬 Sentinel: Starting server from ${sentinelPath}...`);

      // Start server in background
      this.serverProcess = spawn('bash', [startScript], {
        cwd: sentinelPath,
        detached: true,
        stdio: 'ignore',
        env: {
          ...process.env,
          SENTINEL_PORT: this.serverPort,
        },
      });

      // Allow process to run independently
      this.serverProcess.unref();

      console.log(`🧬 Sentinel: Server process spawned (PID: ${this.serverProcess.pid})`);
    } catch (error) {
      console.error(`❌ Sentinel: Failed to start server: ${error}`);
      throw error;
    } finally {
      this.serverStarting = false;
    }
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const requestId = `sentinel-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const startTime = Date.now();

    console.log(`🧬 Sentinel: Generating text (model: ${request.model ?? 'gpt2'})`);

    try {
      // Get model context window (GPT-2 = 1024 tokens)
      const contextWindow = this.getContextWindowForModel(request.model || 'gpt2');

      // Convert AIProviderTypesV2 messages to PromptFormatters messages
      const formatterMessages: ChatMessage[] = request.messages.map(msg => ({
        role: msg.role,
        content: typeof msg.content === 'string'
          ? msg.content
          : msg.content.map(part => part.type === 'text' ? part.text : `[${part.type}]`).join(' ')
      }));

      // Truncate messages to fit in context window (leave room for response)
      const maxInputTokens = Math.floor(contextWindow * 0.7); // Use 70% for input
      const truncatedMessages = truncateMessages(formatterMessages, maxInputTokens, 'base');

      // Determine prompt format based on intelligence level (PersonaUser property)
      // 1-30: Base models (GPT-2) - simple "Q: A:" format
      // 31+: Instruction-tuned models (phi-2, etc.) - full chat format
      const intelligenceLevel = request.intelligenceLevel ?? 15; // Default to base model if not specified
      const isBaseModel = intelligenceLevel <= 30;

      let prompt = '';
      let systemPrompt: string | undefined;

      if (isBaseModel) {
        // Base models (GPT-2, DistilGPT-2): Use simple conversational completion
        // These models do text completion, not instruction following
        // Just pass the cleaned message through for natural continuation

        // Get just the last user message
        const lastUserMessage = truncatedMessages.reverse().find(m => m.role === 'user');
        if (lastUserMessage) {
          let content = typeof lastUserMessage.content === 'string' ? lastUserMessage.content : '';

          // Strip timestamp prefix like "[02:08] Joel: "
          content = content.replace(/^\[\d{2}:\d{2}\]\s+\w+:\s+/, '');

          // Strip @mentions like "@sentinel "
          content = content.replace(/@\w+\s+/, '');

          // Simple conversational format - let GPT-2 naturally continue the text
          // Works better than "Q: A:" format for conversational responses
          prompt = `${content} `;
        }
      } else {
        // Instruction-tuned models (phi-2, future chat models): Use full chat history
        for (const message of truncatedMessages) {
          const content = typeof message.content === 'string' ? message.content : '';

          if (message.role === 'system') {
            systemPrompt = content;
          } else if (message.role === 'user') {
            prompt += `Human: ${content}\n\n`;
          } else if (message.role === 'assistant') {
            prompt += `Assistant: ${content}\n\n`;
          }
        }

        if (!prompt.endsWith('Assistant:')) {
          prompt += 'Assistant:';
        }
      }

      console.log(`🧬 Sentinel: Prompt preview (first 200 chars): ${prompt.substring(0, 200)}...`);
      console.log(`🧬 Sentinel: System prompt: ${systemPrompt || request.systemPrompt || 'none'}`);

      // Build request
      const sentinelRequest: SentinelGenerateRequest = {
        model: request.model || 'gpt2',
        prompt,
        system: systemPrompt || request.systemPrompt,
        temperature: request.temperature ?? 0.7,
        num_predict: request.maxTokens ?? 150,
        stream: false,
      };

      // Make HTTP request
      const response = await fetch(`${this.apiEndpoint}/api/generate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(sentinelRequest),
        signal: AbortSignal.timeout(60000), // 60 second timeout
      });

      if (!response.ok) {
        throw new Error(`Sentinel server returned ${response.status}`);
      }

      const result: SentinelGenerateResponse = await response.json();
      const responseTime = Date.now() - startTime;

      // Calculate usage
      const inputTokens = estimateTokenCount(prompt + (systemPrompt || ''));
      const outputTokens = estimateTokenCount(result.response);

      console.log(`✅ Sentinel: Generated ${result.response.length} chars in ${responseTime}ms`);

      return {
        text: result.response,
        finishReason: result.done ? 'stop' : 'length',
        model: result.model,
        provider: this.providerId,
        responseTime,
        requestId,
        usage: {
          inputTokens,
          outputTokens,
          totalTokens: inputTokens + outputTokens,
          estimatedCost: 0, // Sentinel is free
        },
      };
    } catch (error) {
      const duration = Date.now() - startTime;
      console.error(`❌ Sentinel: Generation failed after ${duration}ms`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);
      throw error;
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    // Check cache
    if (this.healthCache && Date.now() - this.healthCache.timestamp < this.healthCacheTTL) {
      return this.healthCache.status;
    }

    console.log(`🔍 Sentinel Health: Running check...`);
    const startTime = Date.now();

    try {
      const response = await fetch(`${this.apiEndpoint}/api/health`, {
        method: 'GET',
        signal: AbortSignal.timeout(5000),
      });

      const responseTime = Date.now() - startTime;

      if (!response.ok) {
        const status: HealthStatus = {
          status: 'degraded',
          apiAvailable: false,
          responseTime,
          errorRate: 1.0,
          lastChecked: Date.now(),
        };
        this.healthCache = { status, timestamp: Date.now() };
        return status;
      }

      const health = await response.json();

      const status: HealthStatus = {
        status: 'healthy',
        apiAvailable: true,
        responseTime,
        errorRate: 0,
        lastChecked: Date.now(),
      };

      this.healthCache = { status, timestamp: Date.now() };
      console.log(`✅ Sentinel Health: healthy (${responseTime}ms)`);

      return status;
    } catch (error) {
      const responseTime = Date.now() - startTime;

      const status: HealthStatus = {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime,
        errorRate: 1.0,
        lastChecked: Date.now(),
      };

      this.healthCache = { status, timestamp: Date.now() };
      console.log(`❌ Sentinel Health: unhealthy`);

      return status;
    }
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    try {
      const response = await fetch(`${this.apiEndpoint}/api/tags`, {
        method: 'GET',
        signal: AbortSignal.timeout(5000),
      });

      if (!response.ok) {
        console.warn(`Failed to fetch Sentinel models: ${response.status}`);
        return this.getDefaultModels();
      }

      const data: { models: SentinelModelInfo[] } = await response.json();

      return data.models.map((model) => ({
        id: model.name,
        name: model.name,
        provider: this.providerId,
        capabilities: this.supportedCapabilities,
        contextWindow: this.getContextWindowForModel(model.name),
        maxOutputTokens: 2048,
        costPer1kTokens: { input: 0, output: 0 },
        supportsStreaming: false,
        supportsFunctions: false,
      }));
    } catch (error) {
      console.warn(`Failed to fetch Sentinel models: ${error}`);
      return this.getDefaultModels();
    }
  }

  private getDefaultModels(): ModelInfo[] {
    return [
      {
        id: 'gpt2',
        name: 'GPT-2 (124M)',
        provider: this.providerId,
        capabilities: this.supportedCapabilities,
        contextWindow: 1024,
        maxOutputTokens: 1024,
        costPer1kTokens: { input: 0, output: 0 },
        supportsStreaming: false,
        supportsFunctions: false,
      },
      {
        id: 'distilgpt2',
        name: 'DistilGPT-2 (82M)',
        provider: this.providerId,
        capabilities: this.supportedCapabilities,
        contextWindow: 1024,
        maxOutputTokens: 1024,
        costPer1kTokens: { input: 0, output: 0 },
        supportsStreaming: false,
        supportsFunctions: false,
      },
    ];
  }

  private getContextWindowForModel(modelName: string): number {
    if (modelName.includes('phi')) return 2048;
    if (modelName.includes('gpt2')) return 1024;
    return 2048; // Default
  }
}
