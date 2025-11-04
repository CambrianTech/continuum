#!/usr/bin/env npx tsx
/**
 * Sentinel Generation Integration Test
 *
 * Tests that the Sentinel adapter can actually generate responses with gpt2.
 * This is the "tool in the toolbox" for debugging Sentinel issues.
 */

import { SentinelAdapter } from '../../daemons/ai-provider-daemon/adapters/sentinel/shared/SentinelAdapter';
import type { TextGenerationRequest } from '../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

async function testSentinelGeneration() {
  console.log('🧪 Testing Sentinel Generation with GPT-2\n');
  console.log('=' .repeat(80));

  // Create adapter directly
  const adapter = new SentinelAdapter();

  // Test message
  const testPrompt = "Hello, how are you?";

  console.log(`\n📝 Test Prompt: "${testPrompt}"\n`);
  console.log('Generating response...\n');

  const request: TextGenerationRequest = {
    model: 'gpt2',
    messages: [
      {
        role: 'user',
        content: testPrompt
      }
    ],
    temperature: 0.7,
    maxTokens: 150
  };

  try {
    const startTime = Date.now();

    // Generate text using adapter
    const response = await adapter.generateText(request);

    const duration = Date.now() - startTime;

    console.log('\n');
    console.log('=' .repeat(80));
    console.log('\n✅ Generation Complete!\n');
    console.log(`📊 Statistics:`);
    console.log(`   - Duration: ${duration}ms`);
    console.log(`   - Total length: ${response.text.length} characters`);
    console.log(`   - Response: "${response.text.trim()}"`);

    if (response.usage) {
      console.log(`   - Prompt tokens: ${response.usage.promptTokens}`);
      console.log(`   - Completion tokens: ${response.usage.completionTokens}`);
      console.log(`   - Total tokens: ${response.usage.totalTokens}`);
    }

    console.log('\n' + '='.repeat(80));

    // Validate response
    if (response.text.length === 0) {
      console.error('\n❌ FAIL: Empty response generated');
      process.exit(1);
    }

    if (response.text.length < 10) {
      console.error('\n⚠️  WARNING: Very short response (< 10 chars)');
    }

    // Check for repetitive garbage (like "or. or. or.")
    const tokens = response.text.trim().split(/\s+/);
    const uniqueTokens = new Set(tokens);
    const repetitionRatio = tokens.length / uniqueTokens.size;

    console.log(`\n🔍 Quality Check:`);
    console.log(`   - Unique tokens: ${uniqueTokens.size} / ${tokens.length}`);
    console.log(`   - Repetition ratio: ${repetitionRatio.toFixed(2)}x`);

    if (repetitionRatio > 3) {
      console.log('   ⚠️  High repetition detected (might be using greedy decoding)');
    } else {
      console.log('   ✅ Reasonable token diversity');
    }

    console.log('\n🎉 Sentinel generation test PASSED!');
    console.log('🔧 Tool verified and ready for future use.\n');

  } catch (error) {
    console.error('\n❌ Test FAILED with error:');
    console.error(error);
    process.exit(1);
  }
}

// Run the test
testSentinelGeneration().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});
