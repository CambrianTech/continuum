#!/usr/bin/env npx tsx
/**
 * Sentinel Adapter Integration Test
 * ==================================
 *
 * Tests the SentinelAdapter with the actual Sentinel HTTP server.
 * Verifies:
 * - Auto-start functionality
 * - Text generation
 * - Model caching
 * - Health checks
 */

import { SentinelAdapter } from '../../daemons/ai-provider-daemon/adapters/sentinel/shared/SentinelAdapter';
import type { TextGenerationRequest } from '../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

async function runTests() {
  console.log('🧬 Sentinel Adapter Integration Test');
  console.log('=====================================\n');

  const adapter = new SentinelAdapter();
  let testsFailed = 0;
  let testsPassed = 0;

  // Test 1: Health Check
  try {
    console.log('🔍 Test 1: Health check...');
    const health = await adapter.healthCheck();

    if (health.status !== 'healthy') {
      throw new Error(`Expected status 'healthy', got '${health.status}'`);
    }
    if (!health.lastChecked) {
      throw new Error('Expected lastChecked to be defined');
    }

    console.log(`✅ Health check passed: ${health.status}`);
    console.log(`   Response time: ${health.responseTime}ms\n`);
    testsPassed++;
  } catch (error) {
    console.error(`❌ Health check failed: ${error}`);
    testsFailed++;
  }

  // Test 2: List Available Models
  try {
    console.log('🔍 Test 2: List available models...');
    const models = await adapter.getAvailableModels();

    if (!Array.isArray(models)) {
      throw new Error('Expected models to be an array');
    }
    if (models.length === 0) {
      throw new Error('Expected at least one model');
    }

    const modelIds = models.map(m => m.id);
    if (!modelIds.includes('gpt2')) {
      throw new Error('Expected gpt2 model to be available');
    }

    console.log(`✅ Found ${models.length} models: ${modelIds.join(', ')}\n`);
    testsPassed++;
  } catch (error) {
    console.error(`❌ List models failed: ${error}\n`);
    testsFailed++;
  }

  // Test 3: Basic Text Generation
  try {
    console.log('🔍 Test 3: Basic text generation...');
    const request: TextGenerationRequest = {
      messages: [
        {
          role: 'user',
          content: 'Hello, my name is',
        },
      ],
      model: 'gpt2',
      maxTokens: 15,
      temperature: 0.7,
    };

    const startTime = Date.now();
    const response = await adapter.generateText(request);
    const elapsed = Date.now() - startTime;

    if (!response.text) {
      throw new Error('Expected text to be defined');
    }
    if (response.text.length === 0) {
      throw new Error('Expected text to have content');
    }
    if (response.model !== 'gpt2') {
      throw new Error(`Expected model 'gpt2', got '${response.model}'`);
    }
    if (response.provider !== 'sentinel') {
      throw new Error(`Expected provider 'sentinel', got '${response.provider}'`);
    }
    if (!response.usage) {
      throw new Error('Expected usage to be defined');
    }
    if (response.usage.inputTokens <= 0) {
      throw new Error('Expected inputTokens > 0');
    }
    if (response.usage.outputTokens <= 0) {
      throw new Error('Expected outputTokens > 0');
    }

    console.log(`✅ Generated text: "${response.text}"`);
    console.log(`   Model: ${response.model}`);
    console.log(`   Tokens: ${response.usage.inputTokens} in, ${response.usage.outputTokens} out`);
    console.log(`   Time: ${elapsed}ms\n`);
    testsPassed++;
  } catch (error) {
    console.error(`❌ Basic generation failed: ${error}\n`);
    testsFailed++;
  }

  // Test 4: Generation with System Prompt
  try {
    console.log('🔍 Test 4: Generation with system prompt...');
    const request: TextGenerationRequest = {
      messages: [
        {
          role: 'user',
          content: 'What is 2+2?',
        },
      ],
      systemPrompt: 'You are a helpful math assistant.',
      model: 'gpt2',
      maxTokens: 20,
      temperature: 0.7,
    };

    const response = await adapter.generateText(request);

    if (!response.text) {
      throw new Error('Expected text to be defined');
    }
    if (response.text.length === 0) {
      throw new Error('Expected text to have content');
    }

    console.log(`✅ Generated with system prompt: "${response.text}"\n`);
    testsPassed++;
  } catch (error) {
    console.error(`❌ System prompt generation failed: ${error}\n`);
    testsFailed++;
  }

  // Test 5: Model Caching
  try {
    console.log('🔍 Test 5: Model caching (faster subsequent requests)...');
    const request: TextGenerationRequest = {
      messages: [
        {
          role: 'user',
          content: 'Test prompt for caching',
        },
      ],
      model: 'gpt2',
      maxTokens: 10,
      temperature: 0.7,
    };

    // First request (may need to load model)
    const start1 = Date.now();
    const response1 = await adapter.generateText(request);
    const time1 = Date.now() - start1;

    // Second request (model cached)
    const start2 = Date.now();
    const response2 = await adapter.generateText(request);
    const time2 = Date.now() - start2;

    if (!response1.text) {
      throw new Error('Expected first response text to be defined');
    }
    if (!response2.text) {
      throw new Error('Expected second response text to be defined');
    }

    const speedup = time1 / time2;
    console.log(`✅ Caching verified:`);
    console.log(`   First request: ${time1}ms`);
    console.log(`   Second request: ${time2}ms`);
    console.log(`   Speedup: ${speedup.toFixed(1)}x\n`);

    // If first request was slow (>2s), second should be much faster
    if (time1 > 2000 && time2 >= time1 * 0.5) {
      throw new Error('Expected caching to provide significant speedup');
    }

    testsPassed++;
  } catch (error) {
    console.error(`❌ Caching test failed: ${error}\n`);
    testsFailed++;
  }

  // Test 6: Error Handling
  try {
    console.log('🔍 Test 6: Error handling (invalid model)...');
    const request: TextGenerationRequest = {
      messages: [
        {
          role: 'user',
          content: 'Test',
        },
      ],
      model: 'non-existent-model',
      maxTokens: 10,
    };

    let errorThrown = false;
    try {
      await adapter.generateText(request);
    } catch (error) {
      errorThrown = true;
    }

    if (!errorThrown) {
      throw new Error('Expected invalid model to throw error');
    }

    console.log('✅ Invalid model handled gracefully\n');
    testsPassed++;
  } catch (error) {
    console.error(`❌ Error handling test failed: ${error}\n`);
    testsFailed++;
  }

  // Test 7: Deterministic Output
  try {
    console.log('🔍 Test 7: Deterministic output (temperature=0)...');
    const request: TextGenerationRequest = {
      messages: [
        {
          role: 'user',
          content: 'The capital of France is',
        },
      ],
      model: 'gpt2',
      maxTokens: 5,
      temperature: 0.0, // Deterministic
    };

    // Generate twice
    const response1 = await adapter.generateText(request);
    const response2 = await adapter.generateText(request);

    // With temperature=0, results should be identical
    if (response1.text !== response2.text) {
      throw new Error(
        `Expected deterministic output, got different results:\n  1: "${response1.text}"\n  2: "${response2.text}"`
      );
    }

    console.log(`✅ Deterministic output verified: "${response1.text}"\n`);
    testsPassed++;
  } catch (error) {
    console.error(`❌ Deterministic test failed: ${error}\n`);
    testsFailed++;
  }

  // Summary
  console.log('=====================================');
  console.log(`Tests Passed: ${testsPassed}`);
  console.log(`Tests Failed: ${testsFailed}`);

  if (testsFailed > 0) {
    console.log('\n❌ Some tests failed');
    process.exit(1);
  } else {
    console.log('\n✅ All tests passed!');
    process.exit(0);
  }
}

// Run tests
runTests().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
