#!/usr/bin/env node
/**
 * Together AI Fine-Tuning API Test
 *
 * Together uses OpenAI-compatible API - just different endpoint.
 * This test extends OpenAIFineTuningTest for code reuse.
 *
 * Usage:
 *   npx tsx system/genome/fine-tuning/server/adapters/api-tests/test-together.ts
 *
 * Requirements:
 *   - TOGETHER_API_KEY in environment
 *   - /tmp/test-training-minimal.jsonl (training data)
 *
 * Key Differences from OpenAI:
 *   - API Base: https://api.together.xyz/v1 (not openai.com)
 *   - Different base models (Meta Llama, Mistral, etc.)
 *   - Otherwise OpenAI-compatible workflow
 */

import { OpenAIFineTuningTest } from './test-openai';
import { TestConfig } from './BaseRemoteAPITest';

// ============================================================================
// Together API Test (Extends OpenAI)
// ============================================================================

class TogetherFineTuningTest extends OpenAIFineTuningTest {
  // Everything inherited from OpenAIFineTuningTest!
  // Together AI is fully OpenAI-compatible

  // No method overrides needed
}

// ============================================================================
// Main Execution
// ============================================================================

async function main(): Promise<void> {
  // Configuration (only differences from OpenAI)
  const config: TestConfig = {
    apiKey: process.env.TOGETHER_API_KEY || '',
    apiBase: 'https://api.together.xyz/v1', // ← Different endpoint
    baseModel: 'meta-llama/Llama-3-8b-chat-hf', // ← Example Together model
    epochs: 1,
    trainingFile: '/tmp/test-training-minimal.jsonl',
    providerId: 'together',
  };

  console.log('🤝 Together AI is OpenAI-compatible!');
  console.log('   Supports Meta Llama, Mistral, Mixtral, and more');
  console.log('');

  // Run test (using inherited OpenAI implementation)
  const test = new TogetherFineTuningTest(config);
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
export { TogetherFineTuningTest };
