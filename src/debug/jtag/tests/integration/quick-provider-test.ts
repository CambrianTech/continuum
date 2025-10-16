#!/usr/bin/env tsx
/**
 * Quick Provider Test - Fast inference demo for all providers
 *
 * Each provider gets 10 seconds max to respond with "Hello!" - if they timeout, move on.
 */

import { initializeSecrets, getSecret } from '../../system/secrets/SecretManager';

const TIMEOUT_MS = 10000; // 10 second timeout per provider

interface TestResult {
  provider: string;
  success: boolean;
  response?: string;
  time?: number;
  error?: string;
}

async function testWithTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  providerName: string
): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) =>
      setTimeout(() => reject(new Error(`${providerName} timed out after ${timeoutMs}ms`)), timeoutMs)
    )
  ]);
}

async function testProvider(
  name: string,
  keyName: string,
  adapterPath: string,
  model: string
): Promise<TestResult> {
  try {
    const apiKey = await getSecret(keyName);
    if (!apiKey) {
      return { provider: name, success: false, error: 'No API key' };
    }

    // Load and initialize adapter
    const module = await import(adapterPath);
    const AdapterClass = module[`${name}Adapter`];
    const adapter = new AdapterClass(apiKey);
    await adapter.initialize();

    // Test inference with timeout
    const startTime = Date.now();
    const response = await testWithTimeout(
      adapter.generateText({
        messages: [{ role: 'user' as const, content: 'Say hello in 5 words or less.' }],
        model,
        temperature: 0.7,
        maxTokens: 20
      }),
      TIMEOUT_MS,
      name
    );

    const time = Date.now() - startTime;

    // Successful responses have a text field; failed responses have an error field
    if (response.text) {
      return {
        provider: name,
        success: true,
        response: response.text.trim(),
        time
      };
    } else {
      return {
        provider: name,
        success: false,
        error: response.error || `Unexpected response format: ${JSON.stringify(response, null, 2)}`
      };
    }
  } catch (error) {
    return {
      provider: name,
      success: false,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

async function main(): Promise<void> {
  console.log('🚀 Quick Provider Inference Test');
  console.log('='.repeat(60));

  await initializeSecrets();
  console.log('✅ Secrets loaded\n');

  const providers = [
    { name: 'DeepSeek', keyName: 'DEEPSEEK_API_KEY', path: '../../daemons/ai-provider-daemon/adapters/deepseek/shared/DeepSeekAdapter', model: 'deepseek-chat' },
    { name: 'Groq', keyName: 'GROQ_API_KEY', path: '../../daemons/ai-provider-daemon/adapters/groq/shared/GroqAdapter', model: 'llama-3.1-8b-instant' },
    { name: 'XAI', keyName: 'XAI_API_KEY', path: '../../daemons/ai-provider-daemon/adapters/xai/shared/XAIAdapter', model: 'grok-4' },
    { name: 'OpenAI', keyName: 'OPENAI_API_KEY', path: '../../daemons/ai-provider-daemon/adapters/openai/shared/OpenAIAdapter', model: 'gpt-4' },
    { name: 'Anthropic', keyName: 'ANTHROPIC_API_KEY', path: '../../daemons/ai-provider-daemon/adapters/anthropic/shared/AnthropicAdapter', model: 'claude-3-5-sonnet-20241022' },
    { name: 'Fireworks', keyName: 'FIREWORKS_API_KEY', path: '../../daemons/ai-provider-daemon/adapters/fireworks/shared/FireworksAdapter', model: 'accounts/fireworks/models/deepseek-v3p1' },
  ];

  const results: TestResult[] = [];

  for (const provider of providers) {
    console.log(`Testing ${provider.name}...`);
    const result = await testProvider(provider.name, provider.keyName, provider.path, provider.model);
    results.push(result);

    if (result.success) {
      console.log(`✅ ${result.provider}: "${result.response}" (${result.time}ms)\n`);
    } else {
      console.log(`❌ ${result.provider}: ${result.error}\n`);
    }
  }

  // Summary
  console.log('='.repeat(60));
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
    failed.forEach(r => console.log(`   - ${r.provider}: ${r.error}`));
  }
}

if (require.main === module) {
  main().catch(console.error);
}
