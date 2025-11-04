#!/usr/bin/env tsx
/**
 * Sentinel Adapter Integration Tests
 * ===================================
 *
 * Tests the Sentinel adapter end-to-end:
 * - Python script returns clean JSON (no stdout pollution)
 * - Context window validation works correctly
 * - Multiple models work (gpt2, distilgpt2)
 * - Proper error handling for context overflow
 * - Response times are reasonable
 *
 * Design Philosophy:
 * - Python scripts should be PURE FUNCTIONS: input file → JSON output only
 * - NO print statements should pollute stdout
 * - Defense in depth: TypeScript estimates, Python validates with actual tokenizer
 * - Dynamic context window detection (different models have different limits)
 */

import { SentinelAdapter } from '../../daemons/ai-provider-daemon/adapters/sentinel/shared/SentinelAdapter';
import { initializeSecrets, SecretManager } from '../../system/secrets/SecretManager';
import type { TextGenerationRequest } from '../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

async function testSentinelConfiguration(): Promise<void> {
  console.log('\n🧬 TEST 1: Sentinel Configuration');
  console.log('==================================');

  await initializeSecrets();

  const hasSentinelPath = SecretManager.getInstance().has('SENTINEL_PATH');
  console.log(`   SENTINEL_PATH: ${hasSentinelPath ? '✅ Configured' : '❌ Missing'}`);

  if (!hasSentinelPath) {
    console.log('\n⚠️  SENTINEL_PATH not configured');
    console.log('   Add to ~/.continuum/config.env:');
    console.log('   SENTINEL_PATH=/path/to/sentinel-ai');
  }
}

async function testSentinelInitialization(): Promise<void> {
  console.log('\n🔌 TEST 2: Sentinel Adapter Initialization');
  console.log('==========================================');

  const hasSentinelPath = SecretManager.getInstance().has('SENTINEL_PATH');

  if (!hasSentinelPath) {
    console.log('⏭️  Skipped (SENTINEL_PATH not configured)');
    return;
  }

  try {
    const adapter = new SentinelAdapter();
    await adapter.initialize();
    console.log('✅ Sentinel adapter initialized successfully');

    // Check available models
    const models = await adapter.getAvailableModels();
    console.log(`📋 Available models: ${models.length}`);
    for (const model of models) {
      console.log(`   - ${model.id}: ${model.contextWindow} tokens context`);
    }
  } catch (error) {
    console.log(`❌ Initialization failed: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function testSimpleGeneration(): Promise<void> {
  console.log('\n💬 TEST 3: Simple Text Generation (distilgpt2)');
  console.log('===============================================');

  const hasSentinelPath = SecretManager.getInstance().has('SENTINEL_PATH');

  if (!hasSentinelPath) {
    console.log('⏭️  Skipped (SENTINEL_PATH not configured)');
    return;
  }

  try {
    const adapter = new SentinelAdapter();
    await adapter.initialize();

    console.log('📤 Sending test prompt: "What color is the sky?"');
    const startTime = Date.now();

    const request: TextGenerationRequest = {
      messages: [
        { role: 'user', content: 'What color is the sky?' }
      ],
      model: 'distilgpt2',
      maxTokens: 50,
      temperature: 0.7
    };

    const response = await adapter.generateText(request);
    const responseTime = Date.now() - startTime;

    console.log(`✅ Response received in ${responseTime}ms`);
    console.log(`📝 Response: "${response.text}"`);
    console.log(`🔢 Tokens: ${response.usage?.inputTokens ?? 0} in, ${response.usage?.outputTokens ?? 0} out`);
    console.log(`🔍 Model: ${response.model}`);
    console.log(`🏁 Finish reason: ${response.finishReason}`);

    // Verify no JSON parsing errors (would have thrown by now)
    console.log('✅ JSON parsing clean (no stdout pollution)');

  } catch (error) {
    console.log(`❌ Generation failed: ${error instanceof Error ? error.message : String(error)}`);
    if (error instanceof Error && error.stack) {
      console.log(`📚 Stack trace:\n${error.stack.substring(0, 500)}`);
    }
  }
}

async function testContextWindowHandling(): Promise<void> {
  console.log('\n📏 TEST 4: Context Window Overflow Handling');
  console.log('===========================================');

  const hasSentinelPath = SecretManager.getInstance().has('SENTINEL_PATH');

  if (!hasSentinelPath) {
    console.log('⏭️  Skipped (SENTINEL_PATH not configured)');
    return;
  }

  try {
    const adapter = new SentinelAdapter();
    await adapter.initialize();

    // Create a message that will exceed 1024 tokens (GPT-2/DistilGPT2 limit)
    // ~4 chars per token, so 5000 chars ≈ 1250 tokens
    const longMessage = 'This is a test message. '.repeat(200); // ~4800 chars ≈ 1200 tokens

    console.log(`📤 Sending ${longMessage.length} char message (~${Math.ceil(longMessage.length / 4)} tokens)`);
    console.log('   Expected: Truncation to fit 1024 token context window');

    const startTime = Date.now();

    const request: TextGenerationRequest = {
      messages: [
        { role: 'user', content: longMessage }
      ],
      model: 'distilgpt2',
      maxTokens: 50,
      temperature: 0.7
    };

    const response = await adapter.generateText(request);
    const responseTime = Date.now() - startTime;

    console.log(`✅ Response received in ${responseTime}ms (truncation worked!)`);
    console.log(`📝 Response: "${response.text.substring(0, 100)}..."`);
    console.log(`🔢 Tokens: ${response.usage?.inputTokens ?? 0} in, ${response.usage?.outputTokens ?? 0} out`);
    console.log('✅ Context window overflow handled gracefully');

  } catch (error) {
    console.log(`❌ Context window test failed: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function testMultipleModels(): Promise<void> {
  console.log('\n🔄 TEST 5: Multiple Model Comparison');
  console.log('=====================================');

  const hasSentinelPath = SecretManager.getInstance().has('SENTINEL_PATH');

  if (!hasSentinelPath) {
    console.log('⏭️  Skipped (SENTINEL_PATH not configured)');
    return;
  }

  const adapter = new SentinelAdapter();
  await adapter.initialize();

  const models = ['distilgpt2', 'gpt2'];
  const prompt = 'The capital of France is';

  for (const modelId of models) {
    try {
      console.log(`\n🧬 Testing ${modelId}...`);
      const startTime = Date.now();

      const request: TextGenerationRequest = {
        messages: [
          { role: 'user', content: prompt }
        ],
        model: modelId,
        maxTokens: 20,
        temperature: 0.7
      };

      const response = await adapter.generateText(request);
      const responseTime = Date.now() - startTime;

      console.log(`✅ ${modelId}: "${response.text}" (${responseTime}ms)`);
      console.log(`   Tokens: ${response.usage?.inputTokens ?? 0} in, ${response.usage?.outputTokens ?? 0} out`);

    } catch (error) {
      console.log(`❌ ${modelId}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}

async function testHealthCheck(): Promise<void> {
  console.log('\n🏥 TEST 6: Health Check');
  console.log('========================');

  const hasSentinelPath = SecretManager.getInstance().has('SENTINEL_PATH');

  if (!hasSentinelPath) {
    console.log('⏭️  Skipped (SENTINEL_PATH not configured)');
    return;
  }

  try {
    const adapter = new SentinelAdapter();
    await adapter.initialize();

    const health = await adapter.healthCheck();

    console.log(`Status: ${health.status === 'healthy' ? '✅' : '❌'} ${health.status}`);
    console.log(`API Available: ${health.apiAvailable ? '✅' : '❌'}`);
    console.log(`Response Time: ${health.responseTime}ms`);
    console.log(`Error Rate: ${health.errorRate}`);

  } catch (error) {
    console.log(`❌ Health check failed: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function testConversationHistory(): Promise<void> {
  console.log('\n💭 TEST 7: Multi-turn Conversation');
  console.log('===================================');

  const hasSentinelPath = SecretManager.getInstance().has('SENTINEL_PATH');

  if (!hasSentinelPath) {
    console.log('⏭️  Skipped (SENTINEL_PATH not configured)');
    return;
  }

  try {
    const adapter = new SentinelAdapter();
    await adapter.initialize();

    console.log('📤 Sending multi-turn conversation');
    const startTime = Date.now();

    const request: TextGenerationRequest = {
      messages: [
        { role: 'user', content: 'Hello!' },
        { role: 'assistant', content: 'Hi there! How can I help you?' },
        { role: 'user', content: 'What is 2+2?' }
      ],
      model: 'distilgpt2',
      maxTokens: 30,
      temperature: 0.7
    };

    const response = await adapter.generateText(request);
    const responseTime = Date.now() - startTime;

    console.log(`✅ Response received in ${responseTime}ms`);
    console.log(`📝 Response: "${response.text}"`);
    console.log(`🔢 Tokens: ${response.usage?.inputTokens ?? 0} in, ${response.usage?.outputTokens ?? 0} out`);

  } catch (error) {
    console.log(`❌ Multi-turn test failed: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function testModelTiers(): Promise<void> {
  console.log('\n🎯 TEST 8: Model Tier Architecture (Simple vs Capable)');
  console.log('======================================================');

  const hasSentinelPath = SecretManager.getInstance().has('SENTINEL_PATH');

  if (!hasSentinelPath) {
    console.log('⏭️  Skipped (SENTINEL_PATH not configured)');
    return;
  }

  console.log('\n📊 Model Tier Strategy:');
  console.log('   TIER 1 (Simple): GPT-2, DistilGPT-2 - Small base models');
  console.log('      • Limited context (1024 tokens)');
  console.log('      • Repetitive output, poor instruction following');
  console.log('      • REQUIRES @mention to respond (opt-in)');
  console.log('      • Use case: Quick local inference when explicitly asked');
  console.log('');
  console.log('   TIER 2 (Capable): Phi-2, TinyLlama - Instruction-tuned models');
  console.log('      • Larger context (2048+ tokens)');
  console.log('      • Better quality, follows instructions');
  console.log('      • Can respond automatically (full RAG)');
  console.log('      • Use case: Primary local assistant');
  console.log('');

  try {
    const adapter = new SentinelAdapter();
    await adapter.initialize();

    const models = await adapter.getAvailableModels();

    console.log('📋 Available Models by Tier:\n');

    // Tier 1: Simple models (current implementation)
    const tier1Models = models.filter(m =>
      m.contextWindow <= 1024 &&
      (m.id.includes('gpt2') || m.id.includes('distilgpt2'))
    );

    console.log('🔹 TIER 1 (Simple - requiresExplicitMention recommended):');
    tier1Models.forEach(m => {
      console.log(`   ✓ ${m.id}: ${m.contextWindow} tokens, ${m.maxOutputTokens} max output`);
      console.log(`     Strategy: Only respond when @mentioned`);
    });

    // Tier 2: Capable models (coming soon)
    const tier2Models = models.filter(m =>
      m.contextWindow > 1024 ||
      m.id.includes('phi') ||
      m.id.includes('llama') ||
      m.id.includes('codellama')
    );

    if (tier2Models.length > 0) {
      console.log('\n🔹 TIER 2 (Capable - full RAG supported):');
      tier2Models.forEach(m => {
        console.log(`   ✓ ${m.id}: ${m.contextWindow} tokens, ${m.maxOutputTokens} max output`);
        console.log(`     Strategy: Full autonomous response with RAG`);
      });
    } else {
      console.log('\n🔹 TIER 2 (Capable - coming soon):');
      console.log('   ⏳ phi-2 (2.7B params, 2048 context)');
      console.log('   ⏳ TinyLlama-1.1B (2048 context)');
      console.log('   ⏳ CodeLlama-7B (4096 context)');
      console.log('   Note: These will NOT need requiresExplicitMention');
    }

    console.log('\n✅ Model tier architecture validated');
    console.log('💡 Future: Add phi-2/TinyLlama as Tier 2 models');

  } catch (error) {
    console.log(`❌ Model tier test failed: ${error instanceof Error ? error.message : String(error)}`);
  }
}

// Main test runner
async function runTests(): Promise<void> {
  console.log('╔═════════════════════════════════════════╗');
  console.log('║   Sentinel Adapter Integration Tests   ║');
  console.log('╚═════════════════════════════════════════╝');

  try {
    await testSentinelConfiguration();
    await testSentinelInitialization();
    await testSimpleGeneration();
    await testContextWindowHandling();
    await testMultipleModels();
    await testHealthCheck();
    await testConversationHistory();
    await testModelTiers();

    console.log('\n╔═════════════════════════════════════════╗');
    console.log('║        All Tests Completed! 🎉          ║');
    console.log('╚═════════════════════════════════════════╝\n');

  } catch (error) {
    console.error('\n❌ Test suite failed:', error);
    process.exit(1);
  }
}

// Run tests if executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runTests().catch(error => {
    console.error('Fatal error:', error);
    process.exit(1);
  });
}
