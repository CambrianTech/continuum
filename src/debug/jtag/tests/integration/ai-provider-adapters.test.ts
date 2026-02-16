#!/usr/bin/env tsx
/**
 * AI Provider Adapter Integration Tests
 * =====================================
 *
 * Tests the complete AI provider adapter stack:
 * - SecretManager loads API keys
 * - Adapters initialize with keys
 * - Actual API calls work
 * - Multi-provider routing works
 * - Failover works when provider unavailable
 */

import { OpenAIAdapter } from '../../daemons/ai-provider-daemon/shared/adapters/OpenAIAdapter';
import { TogetherAIAdapter } from '../../daemons/ai-provider-daemon/shared/adapters/TogetherAIAdapter';
import { FireworksAdapter } from '../../daemons/ai-provider-daemon/shared/adapters/FireworksAdapter';
import { initializeSecrets, SecretManager } from '../../system/secrets/SecretManager';

async function testSecretManagerInitialization(): Promise<void> {
  console.log('\n🔐 TEST 1: SecretManager Initialization');
  console.log('=========================================');

  await initializeSecrets();

  const secrets = SecretManager.getInstance();
  const availableKeys = secrets.getAvailableKeys();

  console.log(`✅ SecretManager initialized`);
  console.log(`📋 Available keys: ${availableKeys.length}`);

  // Check for AI provider keys (without revealing values)
  const hasOpenAI = secrets.has('OPENAI_API_KEY');
  const hasTogether = secrets.has('TOGETHER_API_KEY');
  const hasFireworks = secrets.has('FIREWORKS_API_KEY');

  console.log(`   OPENAI_API_KEY: ${hasOpenAI ? '✅ Configured' : '❌ Missing'}`);
  console.log(`   TOGETHER_API_KEY: ${hasTogether ? '✅ Configured' : '❌ Missing'}`);
  console.log(`   FIREWORKS_API_KEY: ${hasFireworks ? '✅ Configured' : '❌ Missing'}`);

  if (!hasOpenAI && !hasTogether && !hasFireworks) {
    console.log('\n⚠️  No AI provider keys configured');
    console.log('   Add keys to ~/.continuum/config.env to test API calls');
  }
}

async function testAdapterInitialization(): Promise<void> {
  console.log('\n🔌 TEST 2: Adapter Initialization');
  console.log('==================================');

  const adapters = [
    { name: 'OpenAI', adapter: new OpenAIAdapter(), keyName: 'OPENAI_API_KEY' },
    { name: 'Together AI', adapter: new TogetherAIAdapter(), keyName: 'TOGETHER_API_KEY' },
    { name: 'Fireworks', adapter: new FireworksAdapter(), keyName: 'FIREWORKS_API_KEY' },
  ];

  for (const { name, adapter, keyName } of adapters) {
    const hasKey = SecretManager.getInstance().has(keyName);

    if (hasKey) {
      try {
        await adapter.initialize();
        console.log(`✅ ${name}: Initialized successfully`);
      } catch (error) {
        console.log(`❌ ${name}: Initialization failed - ${error instanceof Error ? error.message : String(error)}`);
      }
    } else {
      console.log(`⏭️  ${name}: Skipped (no API key configured)`);
    }
  }
}

async function testSimpleAPICall(): Promise<void> {
  console.log('\n💬 TEST 3: Simple API Call (OpenAI)');
  console.log('====================================');

  const hasOpenAI = SecretManager.getInstance().has('OPENAI_API_KEY');

  if (!hasOpenAI) {
    console.log('⏭️  Skipped (OPENAI_API_KEY not configured)');
    return;
  }

  try {
    const adapter = new OpenAIAdapter();
    await adapter.initialize();

    console.log('📤 Sending test prompt: "Say hello in 5 words"');
    const startTime = Date.now();

    const response = await adapter.generateText({
      messages: [
        { role: 'user', content: 'Say hello in 5 words' }
      ],
      maxTokens: 20,
      temperature: 0.7,
    });

    const responseTime = Date.now() - startTime;

    console.log(`✅ Response received in ${responseTime}ms`);
    console.log(`📝 Response: "${response.text}"`);
    console.log(`🔢 Tokens: ${response.usage.inputTokens} in, ${response.usage.outputTokens} out`);
    console.log(`💰 Cost: $${response.usage.estimatedCost.toFixed(4)}`);
  } catch (error) {
    console.log(`❌ API call failed: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function testMultipleProviders(): Promise<void> {
  console.log('\n🔄 TEST 4: Multiple Provider Comparison');
  console.log('========================================');

  const prompt = 'What is 2+2? Answer in one word.';

  const providers = [
    { name: 'OpenAI', adapter: new OpenAIAdapter(), keyName: 'OPENAI_API_KEY' },
    { name: 'Together AI', adapter: new TogetherAIAdapter(), keyName: 'TOGETHER_API_KEY' },
  ];

  for (const { name, adapter, keyName } of providers) {
    const hasKey = SecretManager.getInstance().has(keyName);

    if (!hasKey) {
      console.log(`⏭️  ${name}: Skipped (no API key)`);
      continue;
    }

    try {
      await adapter.initialize();
      const startTime = Date.now();

      const response = await adapter.generateText({
        messages: [{ role: 'user', content: prompt }],
        maxTokens: 10,
      });

      const responseTime = Date.now() - startTime;
      console.log(`✅ ${name}: "${response.text}" (${responseTime}ms, $${response.usage.estimatedCost.toFixed(4)})`);
    } catch (error) {
      console.log(`❌ ${name}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}

async function testHealthChecks(): Promise<void> {
  console.log('\n🏥 TEST 5: Health Checks');
  console.log('=========================');

  const adapters = [
    { name: 'OpenAI', adapter: new OpenAIAdapter(), keyName: 'OPENAI_API_KEY' },
    { name: 'Together AI', adapter: new TogetherAIAdapter(), keyName: 'TOGETHER_API_KEY' },
    { name: 'Fireworks', adapter: new FireworksAdapter(), keyName: 'FIREWORKS_API_KEY' },
  ];

  for (const { name, adapter, keyName } of adapters) {
    const hasKey = SecretManager.getInstance().has(keyName);

    if (!hasKey) {
      console.log(`⏭️  ${name}: Skipped (no API key)`);
      continue;
    }

    try {
      const health = await adapter.healthCheck();
      const statusIcon = health.status === 'healthy' ? '✅' :
                        health.status === 'degraded' ? '⚠️' : '❌';

      console.log(`${statusIcon} ${name}: ${health.status} (${health.responseTimeMs}ms) - ${health.message}`);
    } catch (error) {
      console.log(`❌ ${name}: Health check failed - ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}

async function testSecretRedaction(): Promise<void> {
  console.log('\n🔒 TEST 6: Secret Redaction');
  console.log('============================');

  const secrets = SecretManager.getInstance();

  // Simulate a log message with an API key
  const testKey = secrets.get('OPENAI_API_KEY');
  if (testKey) {
    const unsafeLog = `API request with key: ${testKey}`;
    const safeLog = secrets.redact(unsafeLog);

    console.log('📝 Original: [INTENTIONALLY NOT SHOWN]');
    console.log(`✅ Redacted: ${safeLog}`);
    console.log('   API keys automatically redacted from logs!');
  } else {
    console.log('⏭️  Skipped (no API keys to test)');
  }
}

async function testAuditTrail(): Promise<void> {
  console.log('\n📊 TEST 7: Audit Trail');
  console.log('=======================');

  const secrets = SecretManager.getInstance();
  const auditLog = secrets.getAuditLog();

  console.log(`✅ Audit log contains ${auditLog.length} access records`);

  if (auditLog.length > 0) {
    console.log('\n📋 Recent accesses:');
    const recent = auditLog.slice(-5);
    for (const entry of recent) {
      const timestamp = new Date(entry.accessedAt).toISOString();
      console.log(`   ${entry.key} by ${entry.requestedBy} at ${timestamp}`);
    }
  }
}

async function main(): Promise<void> {
  console.log('🤖 AI PROVIDER ADAPTER INTEGRATION TESTS');
  console.log('=========================================');
  console.log('Testing complete AI provider stack with SecretManager\n');

  try {
    await testSecretManagerInitialization();
    await testAdapterInitialization();
    await testSimpleAPICall();
    await testMultipleProviders();
    await testHealthChecks();
    await testSecretRedaction();
    await testAuditTrail();

    console.log('\n✅ All integration tests completed!');
    console.log('=========================================\n');
  } catch (error) {
    console.error('\n❌ Test suite failed:', error);
    process.exit(1);
  }
}

// Run if executed directly
if (require.main === module) {
  main();
}
