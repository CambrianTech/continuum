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
import { SystemDaemon } from '../../../../system-daemon/shared/SystemDaemon';

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
    this.serverPort = process.env.SENTINEL_PORT ?? '11435';
    this.apiEndpoint = `http://127.0.0.1:${this.serverPort}`;

    this.log(null, 'info', '🧬 Sentinel Adapter initialized (HTTP mode)');
    this.log(null, 'info', `   Endpoint: ${this.apiEndpoint}`);
  }

  /**
   * Initialize Sentinel server (auto-start if needed)
   *
   * Event-driven: doesn't block on server startup
   * Health monitoring (AdapterHealthMonitor) will detect when server is ready
   */
  protected async initializeProvider(): Promise<void> {
    this.log(null, 'info', '🧬 Sentinel: Initializing provider...');

    // Check if server is already running (non-blocking)
    const health = await this.healthCheck();

    if (health.status === 'healthy') {
      this.log(null, 'info', '✅ Sentinel: Server already running');
      return;
    }

    // Try to auto-start the server (non-blocking)
    this.log(null, 'info', '🚀 Sentinel: Server not found, attempting auto-start...');
    try {
      await this.startServer();

      // Don't wait for startup - AdapterHealthMonitor will detect when ready
      const systemDaemon = SystemDaemon.sharedInstance();
      const startupTimeout = systemDaemon.getSetting('system/adapters/sentinel/startup-timeout') as number ?? 30000;

      this.log(null, 'info', `🧬 Sentinel: Server process spawned, health monitoring will verify readiness (timeout: ${startupTimeout}ms)`);
    } catch (error) {
      this.log(null, 'warn', `⚠️  Sentinel: Auto-start failed: ${error instanceof Error ? error.message : String(error)}`);
      const sentinelPath = process.env.SENTINEL_PATH || './sentinel-ai';
      this.log(null, 'warn', `   Start manually: cd ${sentinelPath} && ./server/start_server.sh`);
      this.log(null, 'warn', '   Sentinel AI will be unavailable until started');
    }
  }

  /**
   * Restart Sentinel server
   *
   * Event-driven: doesn't block on process termination
   * Health monitoring will verify when server is back up
   */
  protected async restartProvider(): Promise<void> {
    this.log(null, 'info', '🔄 Sentinel: Restarting server...');

    // Kill existing process if we have one (non-blocking)
    if (this.serverProcess && !this.serverProcess.killed) {
      this.serverProcess.kill();
      this.log(null, 'info', '🔄 Sentinel: Killed existing process, starting fresh');
    }

    // Start fresh (non-blocking)
    await this.startServer();

    // Don't wait - AdapterHealthMonitor will detect when healthy again
    const systemDaemon = SystemDaemon.sharedInstance();
    const stabilizationDelay = systemDaemon.getSetting('system/adapters/sentinel/restart-stabilization-delay') as number ?? 3000;
    this.log(null, 'info', `🔄 Sentinel: Restart initiated, health monitoring will verify recovery (stabilization: ${stabilizationDelay}ms)`);
  }

  /**
   * Start the Sentinel server
   */
  private async startServer(): Promise<void> {
    if (this.serverStarting) {
      this.log(null, 'info', '⏳ Sentinel: Server already starting, waiting...');
      return;
    }

    this.serverStarting = true;

    try {
      // Get path to sentinel-ai project
      const sentinelPath = process.env.SENTINEL_PATH || './sentinel-ai';
      const startScript = path.join(sentinelPath, 'server', 'start_server.sh');

      this.log(null, 'info', `🧬 Sentinel: Starting server from ${sentinelPath}...`);

      // Start server in background
      this.serverProcess = spawn('/bin/bash', [startScript], {
        cwd: sentinelPath,
        detached: true,
        stdio: 'ignore',
        env: {
          ...process.env,
          SENTINEL_PORT: this.serverPort,
        },
      });

      // Handle spawn errors (e.g., bash not found, script doesn't exist)
      this.serverProcess.on('error', (error) => {
        this.log(null, 'error', `❌ Sentinel: Failed to spawn server process: ${error.message}`);
        this.serverProcess = null;
      });

      // Allow process to run independently
      this.serverProcess.unref();

      this.log(null, 'info', `🧬 Sentinel: Server process spawned (PID: ${this.serverProcess.pid})`);
    } catch (error) {
      this.log(null, 'error', `❌ Sentinel: Failed to start server: ${error}`);
      throw error;
    } finally {
      this.serverStarting = false;
    }
  }

  protected async generateTextImpl(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const requestId = `sentinel-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const startTime = Date.now();

    this.log(request, 'info', `🧬 Sentinel: Generating text (model: ${request.model ?? 'gpt2'})`);

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

      this.log(request, 'debug', `🧬 Sentinel: Prompt preview (first 200 chars): ${prompt.substring(0, 200)}...`);
      this.log(request, 'debug', `🧬 Sentinel: System prompt: ${systemPrompt || request.systemPrompt || 'none'}`);

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

      this.log(request, 'info', `✅ Sentinel: Generated ${result.response.length} chars in ${responseTime}ms`);

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
      this.log(request, 'error', `❌ Sentinel: Generation failed after ${duration}ms`);
      this.log(request, 'error', `   Error: ${error instanceof Error ? error.message : String(error)}`);
      throw error;
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    // Check cache
    if (this.healthCache && Date.now() - this.healthCache.timestamp < this.healthCacheTTL) {
      return this.healthCache.status;
    }

    this.log(null, 'info', `🔍 Sentinel Health: Running check...`);
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
      this.log(null, 'info', `✅ Sentinel Health: healthy (${responseTime}ms)`);

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
      this.log(null, 'error', `❌ Sentinel Health: unhealthy`);

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
        this.log(null, 'warn', `Failed to fetch Sentinel models: ${response.status}`);
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
      this.log(null, 'warn', `Failed to fetch Sentinel models: ${error}`);
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
