#!/usr/bin/env tsx
/**
 * Direct Adapter Diagnostic Test
 *
 * Tests each AI provider adapter directly without coordination system interference.
 * This is a simple "hello world" test for each provider.
 */

import { initializeSecrets, getSecret } from '../../system/secrets/SecretManager';

interface AdapterTest {
  name: string;
  keyName: string;
  adapterPath: string;
  defaultModel: string;
}

const ADAPTERS: AdapterTest[] = [
  { name: 'DeepSeek', keyName: 'DEEPSEEK_API_KEY', adapterPath: '../../daemons/ai-provider-daemon/adapters/deepseek/shared/DeepSeekAdapter', defaultModel: 'deepseek-chat' },
  { name: 'Groq', keyName: 'GROQ_API_KEY', adapterPath: '../../daemons/ai-provider-daemon/adapters/groq/shared/GroqAdapter', defaultModel: 'llama-3.1-8b-instant' },
  { name: 'OpenAI', keyName: 'OPENAI_API_KEY', adapterPath: '../../daemons/ai-provider-daemon/adapters/openai/shared/OpenAIAdapter', defaultModel: 'gpt-4' },
  { name: 'Anthropic', keyName: 'ANTHROPIC_API_KEY', adapterPath: '../../daemons/ai-provider-daemon/adapters/anthropic/shared/AnthropicAdapter', defaultModel: 'claude-3-5-sonnet-20241022' },
  { name: 'Together', keyName: 'TOGETHER_API_KEY', adapterPath: '../../daemons/ai-provider-daemon/adapters/together/shared/TogetherAIAdapter', defaultModel: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo' },
  { name: 'Fireworks', keyName: 'FIREWORKS_API_KEY', adapterPath: '../../daemons/ai-provider-daemon/adapters/fireworks/shared/FireworksAdapter', defaultModel: 'accounts/fireworks/models/llama-v3p1-70b-instruct' },
];

async function testAdapter(config: AdapterTest): Promise<void> {
  console.log(`\n🧪 Testing ${config.name}...`);
  console.log(`${'='.repeat(50)}`);

  try {
    // Check if API key is configured
    const apiKey = await getSecret(config.keyName);
    if (!apiKey) {
      console.log(`⏭️  Skipping ${config.name} - API key not configured`);
      return;
    }
    console.log(`✅ API key found for ${config.name}`);

    // Load adapter
    const module = await import(config.adapterPath);
    const AdapterClass = module[`${config.name}Adapter`];
    const adapter = new AdapterClass(apiKey);
    console.log(`✅ ${config.name} adapter instantiated`);

    // Initialize adapter
    await adapter.initialize();
    console.log(`✅ ${config.name} adapter initialized`);

    // Test text generation with simple prompt
    const request = {
      messages: [
        { role: 'user' as const, content: 'Say hello in one sentence.' }
      ],
      model: config.defaultModel,
      temperature: 0.7,
      maxTokens: 50
    };

    console.log(`🤖 Requesting generation from ${config.name}...`);
    const response = await adapter.generateText(request);

    if (response.success && response.text) {
      console.log(`✅ ${config.name} responded successfully!`);
      console.log(`📝 Response: "${response.text}"`);
      console.log(`⏱️  Time: ${response.timing?.totalTime}ms`);
      console.log(`💰 Tokens: ${response.usage?.totalTokens || 'N/A'}`);
    } else {
      console.log(`❌ ${config.name} failed: ${response.error}`);
    }

  } catch (error) {
    console.error(`❌ ${config.name} error:`, error instanceof Error ? error.message : String(error));
  }
}

async function main(): Promise<void> {
  console.log('🚀 AI Provider Adapter Diagnostics');
  console.log('=' .repeat(50));

  // Initialize secrets
  await initializeSecrets();
  console.log('✅ Secrets initialized\n');

  // Test each adapter
  for (const adapter of ADAPTERS) {
    await testAdapter(adapter);
  }

  console.log('\n' + '='.repeat(50));
  console.log('✅ Diagnostic tests complete!');
}

// Run if called directly
if (require.main === module) {
  main().catch(console.error);
}
