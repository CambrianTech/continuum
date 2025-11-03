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

  private readonly sentinelPath = '/Volumes/FlashGordon/cambrian/sentinel-ai';
  private readonly pythonWrapper = 'experiments/run_with_continuum_python.sh';

  // For now, use stub for fast testing
  // Once proven, swap to real inference: 'scripts/continuum_inference.py'
  private readonly inferenceScript = '/tmp/test_sentinel_stub.py';

  constructor() {
    super();
    console.log('🧬 Sentinel Adapter initialized');
    console.log(`   Path: ${this.sentinelPath}`);
    console.log(`   Script: ${this.inferenceScript}`);
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const requestId = `sentinel-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const startTime = Date.now();

    console.log(`🧬 Sentinel: Generating text (model: ${request.model ?? 'tinyllama-chat'})`);

    // Write messages to temp file to avoid shell escaping issues
    const tmpFile = path.join(os.tmpdir(), `sentinel-messages-${requestId}.json`);

    try {
      // Write messages to temp file
      await writeFile(tmpFile, JSON.stringify(request.messages), 'utf-8');

      const model = request.model ?? 'tinyllama-chat';
      const temperature = request.temperature ?? 0.7;
      const maxTokens = request.maxTokens ?? 150;

      // Execute Python script with file path instead of inline JSON
      const command = `cd "${this.sentinelPath}" && python3 "${this.inferenceScript}" --model "${model}" --messages-file "${tmpFile}" --temperature ${temperature} --max-tokens ${maxTokens}`;

      console.log(`🔧 Executing: python3 ${this.inferenceScript} --model ${model} --messages-file ${tmpFile}`);

      const { stdout, stderr } = await execAsync(command, {
        timeout: 120000, // 2 minutes
        maxBuffer: 10 * 1024 * 1024 // 10MB
      });

      if (stderr && !stderr.includes('FutureWarning')) {
        console.warn(`⚠️  Sentinel stderr: ${stderr.substring(0, 200)}`);
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
        id: 'tinyllama-chat',
        name: 'TinyLlama 1.1B Chat',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 2048,
        maxOutputTokens: 512,
        supportsStreaming: false,
        supportsFunctions: false
      },
      {
        id: 'distilgpt2',
        name: 'DistilGPT2',
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
