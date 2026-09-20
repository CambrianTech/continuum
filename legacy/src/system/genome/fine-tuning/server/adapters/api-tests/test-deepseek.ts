#!/usr/bin/env node
/**
 * DeepSeek Fine-Tuning API Test
 *
 * DeepSeek uses OpenAI-compatible API - just different endpoint and pricing.
 * This test extends OpenAIFineTuningTest with 95% code reuse.
 *
 * Usage:
 *   npx tsx system/genome/fine-tuning/server/adapters/api-tests/test-deepseek.ts
 *
 * Requirements:
 *   - DEEPSEEK_API_KEY in environment
 *   - /tmp/test-training-minimal.jsonl (training data)
 *
 * Key Differences from OpenAI:
 *   - API Base: https://api.deepseek.com/v1 (not openai.com)
 *   - Cost: 27x cheaper than OpenAI ($0.00015 vs $0.00405 per example)
 *   - Otherwise identical workflow (OpenAI-compatible)
 */

import { OpenAIFineTuningTest } from './test-openai';
import { TestConfig } from './BaseRemoteAPITest';

// ============================================================================
// DeepSeek API Test (Extends OpenAI)
// ============================================================================

class DeepSeekFineTuningTest extends OpenAIFineTuningTest {
  // Everything inherited from OpenAIFineTuningTest!
  // Just override config values in constructor

  // No method overrides needed - 100% API compatible with OpenAI
}

// ============================================================================
// Main Execution
// ============================================================================

async function main(): Promise<void> {
  // Configuration (only differences from OpenAI)
  const config: TestConfig = {
    apiKey: process.env.DEEPSEEK_API_KEY || '',
    apiBase: 'https://api.deepseek.com/v1', // ← Only difference!
    baseModel: 'deepseek-chat', // ← And model name
    epochs: 1,
    trainingFile: '/tmp/test-training-minimal.jsonl',
    providerId: 'deepseek',
  };

  console.log('💡 DeepSeek is 27x cheaper than OpenAI!');
  console.log('   OpenAI: $0.00405 per example');
  console.log('   DeepSeek: $0.00015 per example');
  console.log('');

  // Run test (using inherited OpenAI implementation)
  const test = new DeepSeekFineTuningTest(config);
  const result = await test.runTest();

  // Exit with appropriate code
  process.exit(result.success ? 0 : 1);
}

// Run if executed directly
if (require.main === module) {
  main().catch((error) => {
    console.error('Unhandled error:', error);
    process.exit(1);
  });
}

// Export for reuse
export { DeepSeekFineTuningTest };
