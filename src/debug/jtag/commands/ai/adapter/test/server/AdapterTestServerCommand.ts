/**
 * AI Adapter Test Command - Server Implementation
 * ================================================
 *
 * Self-diagnostic command that validates adapter capabilities.
 * Tests each adapter's declared capabilities with real API calls.
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type {
  AdapterTestParams,
  AdapterTestResult,
  AllAdaptersTestResult,
  CapabilityTestResult,
} from '../shared/AdapterTestTypes';
import type { AIProviderAdapter, ModelCapability } from '../../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { AIProviderDaemon } from '../../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';

export class AdapterTestServerCommand extends CommandBase<AdapterTestParams, AdapterTestResult | AllAdaptersTestResult> {
  constructor(
    context: JTAGContext,
    subpath: string,
    commander: ICommandDaemon
  ) {
    super('ai/adapter/test', context, subpath, commander);
  }
  async execute(params: AdapterTestParams): Promise<AdapterTestResult | AllAdaptersTestResult> {
    console.log('🧪 Running AI adapter diagnostics...', params);

    // Test all adapters if --all flag is set
    if (params.all) {
      const result = await this.testAllAdapters(params);
      return transformPayload(params, result) as AllAdaptersTestResult;
    }

    // Test specific adapter
    if (!params.adapter) {
      throw new Error('Must specify --adapter or --all');
    }

    const result = await this.testAdapter(params.adapter, params);
    return transformPayload(params, result) as AdapterTestResult;
  }

  /**
   * Test all registered adapters
   */
  private async testAllAdapters(
    params: AdapterTestParams
  ): Promise<Omit<AllAdaptersTestResult, 'context' | 'sessionId'>> {
    console.log('Testing all registered adapters...');

    // Get all registered adapters from AIProviderDaemon
    const adapters = AIProviderDaemon.getAllAdapters();
    const results: Array<Omit<AdapterTestResult, 'context' | 'sessionId'>> = [];

    for (const [providerId] of adapters) {
      try {
        const result = await this.testAdapter(providerId, params);
        results.push(result);
      } catch (error) {
        console.log(`❌ Failed to test adapter ${providerId}:`, error);
        results.push({
          adapter: providerId,
          available: false,
          healthy: false,
          declaredCapabilities: [],
          testResults: [],
          summary: {
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
          },
        });
      }
    }

    const healthyCount = results.filter(r => r.healthy).length;

    return {
      adapters: results as AdapterTestResult[],
      summary: {
        totalAdapters: results.length,
        healthyAdapters: healthyCount,
        unhealthyAdapters: results.length - healthyCount,
      },
    };
  }

  /**
   * Test a specific adapter
   */
  private async testAdapter(
    adapterName: string,
    params: AdapterTestParams
  ): Promise<Omit<AdapterTestResult, 'context' | 'sessionId'>> {
    console.log(`🔍 Testing adapter: ${adapterName}`);

    const startTime = Date.now();

    // TODO: Get adapter instance from AIProviderDaemon
    // For now, create mock result structure
    const adapter = await this.getAdapter(adapterName);

    if (!adapter) {
      return {
        adapter: adapterName,
        available: false,
        healthy: false,
        declaredCapabilities: [],
        testResults: [],
        summary: {
          total: 0,
          passed: 0,
          failed: 0,
          skipped: 0,
        },
      };
    }

    // Run health check
    const healthCheckStart = Date.now();
    const health = await adapter.healthCheck();
    const healthCheckTime = Date.now() - healthCheckStart;

    console.log(`Health check: ${health.status} (${healthCheckTime}ms)`);

    // Get declared capabilities
    const capabilities = adapter.supportedCapabilities;
    console.log(`Declared capabilities: ${capabilities.join(', ')}`);

    // Get available models
    const models = await adapter.getAvailableModels();
    console.log(`Available models: ${models.length}`);

    // Test capabilities
    const testResults: CapabilityTestResult[] = [];
    const capabilityToTest = params.capability
      ? [params.capability]
      : capabilities;

    for (const capability of capabilityToTest) {
      const testResult = await this.testCapability(
        adapter,
        capability,
        params.model,
        params.full ?? false
      );
      testResults.push(testResult);
    }

    const totalTestTime = Date.now() - startTime;
    const passed = testResults.filter(r => r.success).length;
    const failed = testResults.filter(r => r.tested && !r.success).length;
    const skipped = testResults.filter(r => !r.tested).length;

    return {
      adapter: adapterName,
      available: true,
      healthy: health.status === 'healthy',
      declaredCapabilities: capabilities,
      testResults,
      models: models.map(m => m.id),
      summary: {
        total: testResults.length,
        passed,
        failed,
        skipped,
      },
      performance: {
        healthCheckTime,
        totalTestTime,
      },
    };
  }

  /**
   * Test a specific capability
   */
  private async testCapability(
    adapter: AIProviderAdapter,
    capability: ModelCapability,
    model: string | undefined,
    _fullTest: boolean
  ): Promise<CapabilityTestResult> {
    console.log(`  Testing capability: ${capability}`);

    const result: CapabilityTestResult = {
      capability,
      supported: adapter.supportedCapabilities.includes(capability),
      tested: false,
    };

    if (!result.supported) {
      console.log(`  ⏭️  Skipped (not supported)`);
      return result;
    }

    result.tested = true;
    const startTime = Date.now();

    try {
      switch (capability) {
        case 'text-generation':
        case 'chat':
          await this.testTextGeneration(adapter, model);
          break;

        case 'embeddings':
          await this.testEmbeddings(adapter, model);
          break;

        case 'image-generation':
          await this.testImageGeneration(adapter, model);
          break;

        case 'audio-generation':
          await this.testAudioGeneration(adapter, model);
          break;

        case 'audio-transcription':
          await this.testAudioTranscription(adapter, model);
          break;

        default:
          console.log(`  ⚠️  No test implemented for ${capability}`);
          result.tested = false;
          return result;
      }

      result.success = true;
      result.responseTime = Date.now() - startTime;
      console.log(`  ✅ Passed (${result.responseTime}ms)`);
    } catch (error) {
      result.success = false;
      result.error = error instanceof Error ? error.message : String(error);
      result.responseTime = Date.now() - startTime;
      console.log(`  ❌ Failed: ${result.error}`);
    }

    return result;
  }

  /**
   * Test text generation capability
   */
  private async testTextGeneration(
    adapter: AIProviderAdapter,
    model: string | undefined
  ): Promise<void> {
    if (!adapter.generateText) {
      throw new Error('generateText method not implemented');
    }

    const response = await adapter.generateText({
      messages: [
        { role: 'user', content: 'Say "test successful" and nothing else.' },
      ],
      model,
      maxTokens: 10,
      temperature: 0,
    });

    if (!response.text || response.text.length === 0) {
      throw new Error('Empty response from text generation');
    }
  }

  /**
   * Test embeddings capability
   */
  private async testEmbeddings(
    adapter: AIProviderAdapter,
    model: string | undefined
  ): Promise<void> {
    if (!adapter.createEmbedding) {
      throw new Error('createEmbedding method not implemented');
    }

    const response = await adapter.createEmbedding({
      input: 'test embedding',
      model,
    });

    if (!response.embeddings || response.embeddings.length === 0) {
      throw new Error('Empty embeddings response');
    }

    if (response.embeddings[0].length === 0) {
      throw new Error('Empty embedding vector');
    }
  }

  /**
   * Test image generation capability
   */
  private async testImageGeneration(
    adapter: AIProviderAdapter,
    model: string | undefined
  ): Promise<void> {
    if (!adapter.generateImage) {
      throw new Error('generateImage method not implemented');
    }

    const response = await adapter.generateImage({
      prompt: 'A simple test image',
      model,
      size: '256x256',
      n: 1,
    });

    if (!response.images || response.images.length === 0) {
      throw new Error('No images generated');
    }
  }

  /**
   * Test audio generation capability
   */
  private async testAudioGeneration(
    adapter: AIProviderAdapter,
    model: string | undefined
  ): Promise<void> {
    if (!adapter.generateAudio) {
      throw new Error('generateAudio method not implemented');
    }

    const response = await adapter.generateAudio({
      text: 'Test',
      model,
    });

    if (!response.audio) {
      throw new Error('No audio generated');
    }
  }

  /**
   * Test audio transcription capability
   */
  private async testAudioTranscription(
    adapter: AIProviderAdapter,
    _model: string | undefined
  ): Promise<void> {
    if (!adapter.transcribeAudio) {
      throw new Error('transcribeAudio method not implemented');
    }

    // Would need actual audio data here - skip for now
    throw new Error('Audio transcription test requires audio data');
  }

  /**
   * Get adapter instance from AIProviderDaemon
   */
  private async getAdapter(adapterName: string): Promise<AIProviderAdapter | null> {
    return AIProviderDaemon.getAdapter(adapterName);
  }
}
