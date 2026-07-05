#!/usr/bin/env tsx
/**
 * Big Three AI Provider Integration Tests
 * ========================================
 *
 * Focused testing of the three major AI providers:
 * - OpenAI (GPT-4, DALL-E)
 * - Anthropic (Claude 3.5 Sonnet)
 * - Meta via Together AI (Llama 3.1)
 *
 * Tests:
 * - Real API calls with actual responses
 * - Response quality comparison
 * - Speed and cost comparison
 * - Capability-based routing (text, multimodal)
 */

import { OpenAIAdapter } from '../../daemons/ai-provider-daemon/shared/adapters/OpenAIAdapter';
import { TogetherAIAdapter } from '../../daemons/ai-provider-daemon/shared/adapters/TogetherAIAdapter';
import { AnthropicAdapter } from '../../daemons/ai-provider-daemon/shared/adapters/AnthropicAdapter';
import { initializeSecrets, SecretManager } from '../../system/secrets/SecretManager';
import type { AIProviderAdapter, TextGenerationRequest } from '../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

interface ProviderTest {
  name: string;
  adapter: AIProviderAdapter;
  keyName: string;
  defaultModel: string;
}

interface TestResult {
  provider: string;
  model: string;
  response: string;
  responseTime: number;
  cost: number;
  success: boolean;
  error?: string;
}

async function testTextGeneration(): Promise<void> {
  console.log('\n💬 TEST 1: Text Generation Comparison');
  console.log('=====================================');
  console.log('Prompt: "Explain what makes a great AI assistant in one sentence."\\n');

  const providers: ProviderTest[] = [
    {
      name: 'OpenAI GPT-4',
      adapter: new OpenAIAdapter(),
      keyName: 'OPENAI_API_KEY',
      defaultModel: 'gpt-4-turbo',
    },
    {
      name: 'Anthropic Claude 3.5',
      adapter: new AnthropicAdapter(),
      keyName: 'ANTHROPIC_API_KEY',
      defaultModel: 'claude-3-5-sonnet-20241022',
    },
    {
      name: 'Meta Llama 3.1 (Together AI)',
      adapter: new TogetherAIAdapter(),
      keyName: 'TOGETHER_API_KEY',
      defaultModel: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
    },
  ];

  const results: TestResult[] = [];

  const request: TextGenerationRequest = {
    messages: [
      { role: 'user', content: 'Explain what makes a great AI assistant in one sentence.' },
    ],
    maxTokens: 100,
    temperature: 0.7,
  };

  for (const provider of providers) {
    const hasKey = SecretManager.getInstance().has(provider.keyName);

    if (!hasKey) {
      console.log(`⏭️  ${provider.name}: Skipped (${provider.keyName} not configured)`);
      results.push({
        provider: provider.name,
        model: provider.defaultModel,
        response: 'N/A',
        responseTime: 0,
        cost: 0,
        success: false,
        error: 'API key not configured',
      });
      continue;
    }

    try {
      await provider.adapter.initialize();
      const startTime = Date.now();

      const response = await provider.adapter.generateText!(request);

      const responseTime = Date.now() - startTime;

      console.log(`\\n✅ ${provider.name}:`);
      console.log(`   Model: ${response.model}`);
      console.log(`   Response: "${response.text}"`);
      console.log(`   Time: ${responseTime}ms`);
      console.log(`   Cost: $${response.usage.estimatedCost?.toFixed(4) || '0.0000'}`);
      console.log(`   Tokens: ${response.usage.inputTokens} in, ${response.usage.outputTokens} out`);

      results.push({
        provider: provider.name,
        model: response.model,
        response: response.text,
        responseTime,
        cost: response.usage.estimatedCost || 0,
        success: true,
      });
    } catch (error) {
      console.log(`\\n❌ ${provider.name}: Failed`);
      console.log(`   Error: ${error instanceof Error ? error.message : String(error)}`);

      results.push({
        provider: provider.name,
        model: provider.defaultModel,
        response: 'N/A',
        responseTime: 0,
        cost: 0,
        success: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  // Summary comparison
  console.log('\\n📊 Comparison Summary:');
  console.log('======================');

  const successful = results.filter((r) => r.success);
  if (successful.length > 0) {
    const fastest = successful.reduce((a, b) => (a.responseTime < b.responseTime ? a : b));
    const cheapest = successful.reduce((a, b) => (a.cost < b.cost ? a : b));

    console.log(`⚡ Fastest: ${fastest.provider} (${fastest.responseTime}ms)`);
    console.log(`💰 Cheapest: ${cheapest.provider} ($${cheapest.cost.toFixed(4)})`);
  } else {
    console.log('⚠️  No successful responses to compare');
  }
}

async function testCapabilityRouting(): Promise<void> {
  console.log('\\n🎯 TEST 2: Capability-Based Routing');
  console.log('====================================');

  const providers = [
    { name: 'OpenAI', adapter: new OpenAIAdapter(), keyName: 'OPENAI_API_KEY' },
    { name: 'Anthropic', adapter: new AnthropicAdapter(), keyName: 'ANTHROPIC_API_KEY' },
    { name: 'Together AI', adapter: new TogetherAIAdapter(), keyName: 'TOGETHER_API_KEY' },
  ];

  const capabilities = [
    'text-generation',
    'chat',
    'multimodal',
    'image-generation',
    'embeddings',
  ] as const;

  console.log('\\nCapability Matrix:');
  console.log('==================');

  for (const capability of capabilities) {
    const supporting = providers.filter((p) => {
      const hasKey = SecretManager.getInstance().has(p.keyName);
      return hasKey && p.adapter.supportedCapabilities.includes(capability);
    });

    if (supporting.length > 0) {
      console.log(`✅ ${capability}:`);
      console.log(`   Providers: ${supporting.map((p) => p.name).join(', ')}`);
    } else {
      console.log(`❌ ${capability}: No providers available`);
    }
  }

  console.log('\\n💡 Routing Examples:');
  console.log('   Image generation → OpenAI (DALL-E)');
  console.log('   Multimodal vision → OpenAI (GPT-4V) or Anthropic (Claude 3.5)');
  console.log('   Fast text generation → Together AI (Llama 3.1) or OpenAI');
  console.log('   Long context → Anthropic (Claude 3.5, 200k tokens)');
}

async function testModelComparison(): Promise<void> {
  console.log('\\n🤖 TEST 3: Model Information Comparison');
  console.log('========================================');

  const providers = [
    { name: 'OpenAI', adapter: new OpenAIAdapter(), keyName: 'OPENAI_API_KEY' },
    { name: 'Anthropic', adapter: new AnthropicAdapter(), keyName: 'ANTHROPIC_API_KEY' },
    { name: 'Together AI', adapter: new TogetherAIAdapter(), keyName: 'TOGETHER_API_KEY' },
  ];

  for (const provider of providers) {
    const hasKey = SecretManager.getInstance().has(provider.keyName);

    if (!hasKey) {
      console.log(`\\n⏭️  ${provider.name}: Skipped (no API key)`);
      continue;
    }

    try {
      const models = await provider.adapter.getAvailableModels();

      console.log(`\\n✅ ${provider.name}:`);
      console.log(`   Available models: ${models.length}`);

      // Show top 3 models
      const top3 = models.slice(0, 3);
      for (const model of top3) {
        console.log(`   📋 ${model.name}:`);
        console.log(`      ID: ${model.id}`);
        console.log(`      Context: ${model.contextWindow.toLocaleString()} tokens`);
        console.log(
          `      Cost: $${model.costPer1kTokens?.input.toFixed(4)}/1k in, $${model.costPer1kTokens?.output.toFixed(4)}/1k out`
        );
        console.log(`      Capabilities: ${model.capabilities.join(', ')}`);
      }
    } catch (error) {
      console.log(`\\n❌ ${provider.name}: Failed to get models`);
      console.log(`   Error: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}

async function testHealthChecks(): Promise<void> {
  console.log('\\n🏥 TEST 4: Provider Health Checks');
  console.log('==================================');

  const providers = [
    { name: 'OpenAI', adapter: new OpenAIAdapter(), keyName: 'OPENAI_API_KEY' },
    { name: 'Anthropic', adapter: new AnthropicAdapter(), keyName: 'ANTHROPIC_API_KEY' },
    { name: 'Together AI', adapter: new TogetherAIAdapter(), keyName: 'TOGETHER_API_KEY' },
  ];

  for (const provider of providers) {
    const hasKey = SecretManager.getInstance().has(provider.keyName);

    if (!hasKey) {
      console.log(`⏭️  ${provider.name}: Skipped (no API key)`);
      continue;
    }

    try {
      const health = await provider.adapter.healthCheck();
      const statusIcon =
        health.status === 'healthy' ? '✅' : health.status === 'degraded' ? '⚠️' : '❌';

      console.log(
        `${statusIcon} ${provider.name}: ${health.status} (${health.responseTimeMs}ms) - ${health.message}`
      );
    } catch (error) {
      console.log(
        `❌ ${provider.name}: Health check failed - ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }
}

async function main(): Promise<void> {
  console.log('🏆 BIG THREE AI PROVIDER INTEGRATION TESTS');
  console.log('==========================================');
  console.log('Testing OpenAI (GPT), Anthropic (Claude), Meta (Llama)\\n');

  try {
    // Initialize SecretManager
    await initializeSecrets();

    const secrets = SecretManager.getInstance();
    const hasOpenAI = secrets.has('OPENAI_API_KEY');
    const hasAnthropic = secrets.has('ANTHROPIC_API_KEY');
    const hasTogether = secrets.has('TOGETHER_API_KEY');

    console.log('🔐 API Key Status:');
    console.log(`   OPENAI_API_KEY: ${hasOpenAI ? '✅ Configured' : '❌ Missing'}`);
    console.log(`   ANTHROPIC_API_KEY: ${hasAnthropic ? '✅ Configured' : '❌ Missing'}`);
    console.log(`   TOGETHER_API_KEY: ${hasTogether ? '✅ Configured' : '❌ Missing'}`);

    if (!hasOpenAI && !hasAnthropic && !hasTogether) {
      console.log('\\n⚠️  No API keys configured - add keys to ~/.continuum/config.env');
      console.log('   Example:');
      console.log('   OPENAI_API_KEY=sk-...');
      console.log('   ANTHROPIC_API_KEY=sk-ant-...');
      console.log('   TOGETHER_API_KEY=...');
      return;
    }

    // Run tests
    await testTextGeneration();
    await testCapabilityRouting();
    await testModelComparison();
    await testHealthChecks();

    console.log('\\n✅ Big Three integration tests completed!');
    console.log('==========================================\\n');
  } catch (error) {
    console.error('\\n❌ Test suite failed:', error);
    process.exit(1);
  }
}

// Run if executed directly
if (require.main === module) {
  main();
}
