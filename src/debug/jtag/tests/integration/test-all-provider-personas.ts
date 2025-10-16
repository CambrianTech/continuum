#!/usr/bin/env tsx
/**
 * Test All PersonaUser Cloud Providers
 *
 * Directly tests each PersonaUser's ability to call their cloud provider API
 * and generate responses. Bypasses ThoughtStreamCoordinator to test API connectivity.
 */

import { initializeSecrets } from '../../system/secrets/SecretManager';
import { AIProviderDaemon } from '../../daemons/ai-provider-daemon/shared/AIProviderDaemon';

interface ProviderTest {
  name: string;
  provider: string;
  model: string;
}

const PROVIDERS: ProviderTest[] = [
  { name: 'DeepSeek', provider: 'deepseek', model: 'deepseek-chat' },
  { name: 'Groq Lightning', provider: 'groq', model: 'llama-3.1-8b-instant' },
  { name: 'Claude Assistant', provider: 'anthropic', model: 'claude-3-5-sonnet-20241022' },
  { name: 'GPT Assistant', provider: 'openai', model: 'gpt-4' },
  { name: 'Grok', provider: 'xai', model: 'grok-beta' },
  { name: 'Together Assistant', provider: 'together', model: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo' },
  { name: 'Fireworks AI', provider: 'fireworks', model: 'accounts/fireworks/models/deepseek-v3p1' },
  { name: 'Local Assistant', provider: 'ollama', model: 'llama3.2:3b' },
];

async function testProvider(config: ProviderTest, daemon: AIProviderDaemon): Promise<void> {
  const start = Date.now();

  try {
    console.log(`\n🧪 Testing ${config.name}...`);
    console.log(`   Provider: ${config.provider}, Model: ${config.model}`);

    const request = {
      provider: config.provider,
      model: config.model,
      messages: [
        { role: 'user' as const, content: 'Say hello in one sentence.' }
      ],
      temperature: 0.7,
      maxTokens: 50
    };

    const response = await daemon.generateText(request);
    const elapsed = Date.now() - start;

    if (response.success && response.text) {
      console.log(`   ✅ ${config.name}: "${response.text}" (${elapsed}ms)`);
      return;
    } else {
      console.log(`   ❌ ${config.name}: ${response.error || 'Unknown error'} (${elapsed}ms)`);
    }
  } catch (error) {
    const elapsed = Date.now() - start;
    console.error(`   ❌ ${config.name}: ${error instanceof Error ? error.message : String(error)} (${elapsed}ms)`);
  }
}

async function main(): Promise<void> {
  console.log('🚀 Testing All PersonaUser Cloud Providers');
  console.log('='.repeat(60));

  // Initialize secrets
  await initializeSecrets();
  console.log('✅ Secrets initialized\n');

  // Initialize AIProviderDaemon
  const daemon = new AIProviderDaemon();
  await daemon.initialize();
  console.log('✅ AIProviderDaemon initialized\n');

  const results: { provider: string; success: boolean; time: number }[] = [];

  // Test each provider
  for (const provider of PROVIDERS) {
    const start = Date.now();
    try {
      await testProvider(provider, daemon);
      results.push({ provider: provider.name, success: true, time: Date.now() - start });
    } catch (error) {
      results.push({ provider: provider.name, success: false, time: Date.now() - start });
    }
  }

  // Summary
  console.log('\n' + '='.repeat(60));
  console.log('📊 Summary:');
  const working = results.filter(r => r.success);
  const failed = results.filter(r => !r.success);
  console.log(`✅ Working: ${working.length}/${results.length}`);
  console.log(`❌ Failed: ${failed.length}/${results.length}`);

  if (working.length > 0) {
    console.log('\n✅ Working providers:');
    working.forEach(r => console.log(`   - ${r.provider} (${r.time}ms)`));
  }

  if (failed.length > 0) {
    console.log('\n❌ Failed providers:');
    failed.forEach(r => console.log(`   - ${r.provider}`));
  }
}

// Run if called directly
if (require.main === module) {
  main().catch(console.error);
}
