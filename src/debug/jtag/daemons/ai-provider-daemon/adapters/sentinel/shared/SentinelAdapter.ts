/**
 * Sentinel Adapter - Pre-trained Model Integration
 * =================================================
 *
 * Adapter for Sentinel-AI pre-trained models (TinyLlama, Phi-2, CodeLlama, etc.)
 * Provides local inference with models from the Sentinel-AI model zoo.
 *
 * Features:
 * - Text generation with pre-trained transformers
 * - No external API dependencies
 * - Privacy-first (models run locally)
 * - Adaptive architecture support (future)
 *
 * Python Bridge: /Volumes/FlashGordon/cambrian/sentinel-ai/scripts/continuum_inference.py
 */

import type {
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ModelInfo,
  ModelCapability,
} from '../../../shared/AIProviderTypesV2';
import { BaseAIProviderAdapter } from '../../../shared/BaseAIProviderAdapter';
import { truncateMessages } from '../../../shared/PromptFormatters';
import { getSecret } from '../../../../../system/secrets/SecretManager';
import { exec } from 'child_process';
import { promisify } from 'util';
import { writeFile, unlink } from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

const execAsync = promisify(exec);

/**
 * Sentinel Adapter - Executes Python scripts to load and run pre-trained models
 */
export class SentinelAdapter extends BaseAIProviderAdapter {
  readonly providerId = 'sentinel';
  readonly providerName = 'Sentinel AI';
  readonly supportedCapabilities: ModelCapability[] = ['text-generation', 'chat'];

  // Paths for Sentinel Python environment
  private readonly pythonWrapper = 'experiments/run_with_continuum_python.sh';
  private readonly inferenceScript = 'scripts/chat_inference.py';
  private readonly sentinelPath: string;

  constructor() {
    super();

    // Get Sentinel path from config (required)
    const configPath = getSecret('SENTINEL_PATH', 'SentinelAdapter');
    if (!configPath) {
      throw new Error(
        'SENTINEL_PATH not configured. Please add it to ~/.continuum/config.env:\n' +
        'SENTINEL_PATH=/path/to/sentinel-ai'
      );
    }

    this.sentinelPath = configPath;

    // Clear Python bytecode cache on startup to ensure fresh code
    this.clearPythonCache();

    console.log('🧬 Sentinel Adapter initialized');
    console.log(`   Path: ${this.sentinelPath}`);
    console.log(`   Script: ${this.inferenceScript}`);
  }

  /**
   * Clear Python bytecode cache (.pyc files and __pycache__ directories)
   * This ensures we always run the latest Python code, not cached bytecode
   */
  private clearPythonCache(): void {
    try {
      // Run asynchronously in background - don't block initialization
      execAsync(`find "${this.sentinelPath}" -type f -name "*.pyc" -delete && find "${this.sentinelPath}" -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null`, {
        timeout: 5000
      }).catch(() => {
        // Ignore errors - cache clearing is best-effort
      });
    } catch {
      // Ignore errors - cache clearing is best-effort
    }
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const requestId = `sentinel-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const startTime = Date.now();

    console.log(`🧬 Sentinel: Generating text (model: ${request.model ?? 'distilgpt2'})`);

    // Get model info to find context window
    const model = request.model ?? 'distilgpt2';
    const availableModels = await this.getAvailableModels();
    const modelInfo = availableModels.find(m => m.id === model);
    const contextWindow = modelInfo?.contextWindow ?? 1024; // Default to 1024 for GPT-2

    // Truncate messages to fit context window (uses 'base' format for GPT-2 models)
    const truncatedMessages = truncateMessages(request.messages, contextWindow, 'base');

    // Write messages to temp file to avoid shell escaping issues
    const tmpFile = path.join(os.tmpdir(), `sentinel-messages-${requestId}.json`);

    try {
      // Write truncated messages to temp file
      await writeFile(tmpFile, JSON.stringify(truncatedMessages), 'utf-8');

      const model = request.model ?? 'distilgpt2';
      const temperature = request.temperature ?? 0.7;
      const maxTokens = request.maxTokens ?? 150;

      // Execute Python script with file path instead of inline JSON (using environment wrapper)
      const command = `cd "${this.sentinelPath}" && ${this.pythonWrapper} "${this.inferenceScript}" --model "${model}" --messages-file "${tmpFile}" --temperature ${temperature} --max-tokens ${maxTokens}`;

      console.log(`🔧 Executing: ${this.pythonWrapper} ${this.inferenceScript} --model ${model} --messages-file ${tmpFile}`);

      const { stdout, stderr } = await execAsync(command, {
        timeout: 120000, // 2 minutes
        maxBuffer: 10 * 1024 * 1024 // 10MB
      });

      // Filter out common Python warnings and informational messages
      // Only treat stderr as an error if it contains actual error indicators
      const errorIndicators = ['Error:', 'Exception:', 'Traceback', 'FAILED', 'CRITICAL'];
      const hasRealError = stderr && errorIndicators.some(indicator => stderr.includes(indicator));

      if (stderr && !hasRealError) {
        // Log informational stderr for debugging but don't treat as error
        console.log(`🐍 Sentinel debug output: ${stderr.substring(0, 200)}`);
      } else if (hasRealError) {
        console.error(`❌ Sentinel error in stderr: ${stderr.substring(0, 500)}`);
      }

      // Parse JSON response from Python
      const response = JSON.parse(stdout.trim());

      if (response.error) {
        throw new Error(`Python error: ${response.error}`);
      }

      // Calculate usage metrics
      const promptTokens = request.messages.reduce(
        (sum, msg) => {
          const content = typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content);
          return sum + Math.ceil(content.length / 4);
        },
        0
      );
      const completionTokens = Math.ceil(response.text.length / 4);

      const result: TextGenerationResponse = {
        text: response.text,
        model: response.metadata?.model ?? model,
        provider: this.providerId,
        finishReason: 'stop',
        usage: {
          inputTokens: promptTokens,
          outputTokens: completionTokens,
          totalTokens: promptTokens + completionTokens
        },
        responseTime: Date.now() - startTime,
        requestId
      };

      console.log(`✅ Sentinel: Generated ${completionTokens} tokens in ${result.responseTime}ms`);

      // Log if using stub (for debugging)
      if (response.metadata?.stub) {
        console.log(`ℹ️  Sentinel: Using stub response (swap to real inference when ready)`);
      }

      return result;

    } catch (error) {
      const duration = Date.now() - startTime;
      console.error(`❌ Sentinel: Generation failed after ${duration}ms:`, error);

      throw new Error(
        `Sentinel generation failed: ${error instanceof Error ? error.message : 'Unknown error'}`
      );
    } finally {
      // Clean up temp file
      try {
        await unlink(tmpFile);
      } catch {
        // Ignore cleanup errors
      }
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      // Check if Python script exists
      const startTime = Date.now();
      const { stdout } = await execAsync(`test -f "${this.inferenceScript}" && echo "ok"`);
      const isHealthy = stdout.trim() === 'ok';

      return {
        status: isHealthy ? 'healthy' : 'unhealthy',
        apiAvailable: isHealthy,
        responseTime: Date.now() - startTime,
        errorRate: isHealthy ? 0 : 1,
        lastChecked: Date.now()
      };
    } catch {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime: 0,
        errorRate: 1,
        lastChecked: Date.now()
      };
    }
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    return [
      {
        id: 'distilgpt2',
        name: 'DistilGPT2 (Default)',
        provider: this.providerId,
        capabilities: ['text-generation'],
        contextWindow: 1024,
        maxOutputTokens: 256,
        supportsStreaming: false,
        supportsFunctions: false
      },
      {
        id: 'gpt2',
        name: 'GPT-2',
        provider: this.providerId,
        capabilities: ['text-generation'],
        contextWindow: 1024,
        maxOutputTokens: 256,
        supportsStreaming: false,
        supportsFunctions: false
      },
      {
        id: 'phi-2',
        name: 'Microsoft Phi-2 2.7B',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 2048,
        maxOutputTokens: 512,
        supportsStreaming: false,
        supportsFunctions: false
      },
      {
        id: 'codellama-7b',
        name: 'CodeLlama 7B Instruct',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 4096,
        maxOutputTokens: 1024,
        supportsStreaming: false,
        supportsFunctions: false
      }
    ];
  }

  protected async restartProvider(): Promise<void> {
    console.log('🔄 Sentinel: Restart not needed (stateless Python scripts)');
    // Sentinel is stateless - each request starts a new Python process
    // Nothing to restart
  }
}
