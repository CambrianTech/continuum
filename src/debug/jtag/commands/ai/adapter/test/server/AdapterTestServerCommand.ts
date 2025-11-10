/**
 * AI Adapter Test Command - Server Implementation
 * ================================================
 *
 * Self-diagnostic command that validates adapter capabilities.
 * Tests each adapter's declared capabilities with real API calls.
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type {
  AdapterTestParams,
  AdapterTestResult,
  AllAdaptersTestResult,
  CapabilityTestResult,
  AsyncTestResult,
} from '../shared/AdapterTestTypes';
import type { AIProviderAdapter, ModelCapability } from '../../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { AIProviderDaemon } from '../../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import { Commands } from '../../../../../system/core/shared/Commands';
import type { DataCreateParams, DataUpdateParams, DataCreateResult } from '../../../../../daemons/data-daemon/shared/DataTypes';
import { TestExecutionEntity } from '../../../../../daemons/data-daemon/shared/entities/TestExecutionEntity';
import { Events } from '../../../../../system/core/shared/Events';
import { generateUUID } from '../../../../../system/core/types/CrossPlatformUUID';

export class AdapterTestServerCommand extends CommandBase<AdapterTestParams, AsyncTestResult> {
  constructor(
    context: JTAGContext,
    subpath: string,
    commander: ICommandDaemon
  ) {
    super('ai/adapter/test', context, subpath, commander);
  }

  /**
   * Execute - returns UUID immediately, runs tests in background
   */
  async execute(params: AdapterTestParams): Promise<AsyncTestResult> {
    console.log('🧪 Starting AI adapter test (async mode)...', params);

    // Validate params
    if (!params.adapter && !params.all) {
      throw new Error('Must specify --adapter or --all');
    }

    // Generate test ID
    const testId = generateUUID();

    // Create initial test execution entity
    const execution: TestExecutionEntity = new TestExecutionEntity();
    execution.id = testId;
    execution.adapterName = params.adapter ?? 'all';
    execution.modelName = params.model;
    execution.capability = params.capability;
    execution.fullTest = params.full ?? false;
    execution.status = 'queued';
    execution.progress = {
      totalCapabilities: 0,
      completedCapabilities: 0,
      percentComplete: 0,
    };
    execution.testResults = [];
    execution.available = false;
    execution.healthy = false;
    execution.declaredCapabilities = [];
    execution.summary = {
      total: 0,
      passed: 0,
      failed: 0,
      skipped: 0,
    };

    // Save to database using data/create command
    const createResult = await Commands.execute<DataCreateParams<TestExecutionEntity>, DataCreateResult>('data/create', {
      collection: TestExecutionEntity.collection,
      data: execution,
      id: testId,
    });

    if (!createResult.success) {
      throw new Error(`Failed to create test execution: ${createResult.error?.message ?? 'Unknown error'}`);
    }

    console.log(`✅ Test execution ${testId} queued`);

    // Start background execution (non-blocking)
    setImmediate(() => {
      this.executeTestsInBackground(testId, params).catch(async (error) => {
        console.error(`❌ Background test execution failed: ${error}`);
        // Update database status to failed so test isn't stuck in 'queued'
        try {
          await this.updateTestStatus(testId, {
            status: 'failed',
            error: error instanceof Error ? error.message : String(error)
          });
        } catch (updateError) {
          console.error(`❌ Failed to update test status after error: ${updateError}`);
        }
      });
    });

    // Return handle immediately
    return transformPayload(params, {
      testId,
      status: 'queued' as const,
      message: `Test execution ${testId} started. Use 'ai/adapter/test/status --testId=${testId}' to monitor progress.`,
    }) as AsyncTestResult;
  }

  /**
   * Background execution - updates database with progress
   */
  private async executeTestsInBackground(testId: string, params: AdapterTestParams): Promise<void> {
    try {
      // Update status to 'running'
      await this.updateTestStatus(testId, {
        status: 'running',
        startedAt: new Date(),
      });

      console.log(`🏃 Test ${testId} running...`);

      // Test all adapters or specific adapter
      if (params.all) {
        await this.testAllAdaptersBackground(testId, params);
      } else if (params.adapter) {
        await this.testAdapterBackground(testId, params.adapter, params);
      }

      // Mark as completed
      await this.updateTestStatus(testId, {
        status: 'completed',
        completedAt: new Date(),
      });

      console.log(`✅ Test ${testId} completed`);

      // Emit completion event
      Events.emit('test:execution:completed', { testId });

    } catch (error) {
      console.error(`❌ Test ${testId} failed:`, error);

      // Mark as failed
      await this.updateTestStatus(testId, {
        status: 'failed',
        error: error instanceof Error ? error.message : String(error),
        completedAt: new Date(),
      });

      // Emit failure event
      Events.emit('test:execution:failed', { testId, error });
    }
  }

  /**
   * Helper to update test status in database
   */
  private async updateTestStatus(testId: string, updates: Partial<TestExecutionEntity>): Promise<void> {
    await Commands.execute<DataUpdateParams<TestExecutionEntity>, CommandResult>('data/update', {
      collection: TestExecutionEntity.collection,
      id: testId,
      data: {
        ...updates,
        updatedAt: new Date(),
      },
    });
  }

  /**
   * Test all registered adapters (background version)
   */
  private async testAllAdaptersBackground(testId: string, params: AdapterTestParams): Promise<void> {
    const adapters = AIProviderDaemon.getAllAdapters();
    let completed = 0;
    const total = adapters.size;

    for (const [providerId] of adapters) {
      try {
        await this.testAdapterBackground(testId, providerId, params);
      } catch (error) {
        console.log(`❌ Failed to test adapter ${providerId}:`, error);
      }

      completed++;
      await this.updateTestStatus(testId, {
        progress: {
          totalCapabilities: total,
          completedCapabilities: completed,
          percentComplete: (completed / total) * 100,
        },
      });

      // Emit progress event
      Events.emit('test:execution:progress', {
        testId,
        progress: (completed / total) * 100,
      });
    }
  }

  /**
   * Test a specific adapter (background version)
   */
  private async testAdapterBackground(testId: string, adapterName: string, params: AdapterTestParams): Promise<void> {
    const result = await this.testAdapter(adapterName, params);

    // Update database with results
    await this.updateTestStatus(testId, {
      available: result.available,
      healthy: result.healthy,
      declaredCapabilities: result.declaredCapabilities as string[],
      testResults: result.testResults,
      models: result.models,
      summary: result.summary,
      performance: result.performance,
    });
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
