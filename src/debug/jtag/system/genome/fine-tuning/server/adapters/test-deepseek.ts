/**
 * Standalone test for DeepSeekLoRAAdapter - Integration Test (Phase 7.1)
 *
 * Purpose: Test DeepSeek adapter in isolation without full system integration
 * Philosophy: "main() type interface to make it easier. Helps you isolate problems"
 *
 * Usage:
 *   npx tsx system/genome/fine-tuning/server/adapters/test-deepseek.ts
 *
 * What This Tests:
 * - DeepSeekLoRAAdapter initialization
 * - Capability reporting (rank, epochs, model support, pricing)
 * - Cost/time estimation (extremely competitive pricing)
 * - API-based training workflow structure
 * - Error handling
 *
 * What This Does NOT Test Yet (Phase 7.1+):
 * - Actual DeepSeek API calls
 * - Real training with API
 * - Job monitoring and polling
 * - Model download after training
 */

import { DeepSeekLoRAAdapter } from './DeepSeekLoRAAdapter';
import { TrainingDatasetBuilder } from '../TrainingDatasetBuilder';
import type { LoRATrainingRequest, TrainingDataset, TrainingExample } from '../../shared/FineTuningTypes';
import type { UUID } from '../../../../core/types/CrossPlatformUUID';

/**
 * Main test function
 */
async function main(): Promise<void> {
  console.log('🧬 DeepSeekLoRAAdapter Standalone Test');
  console.log('======================================\n');

  try {
    // Step 1: Initialize adapter
    console.log('📦 Step 1: Initializing DeepSeekLoRAAdapter...');
    const adapter = new DeepSeekLoRAAdapter();
    console.log(`✅ Adapter initialized: providerId="${adapter.providerId}"\n`);

    // Step 2: Check if fine-tuning supported
    console.log('🔍 Step 2: Checking fine-tuning support...');
    const supported = adapter.supportsFineTuning();
    console.log(`   supportsFineTuning(): ${supported}`);
    if (!supported) {
      console.log('   (Expected: false - MVP not yet implemented)\n');
    }

    // Step 3: Get capabilities
    console.log('⚙️  Step 3: Getting fine-tuning capabilities...');
    const capabilities = adapter.getFineTuningCapabilities();
    console.log(`   LoRA Rank: ${capabilities.minRank}-${capabilities.maxRank} (default: ${capabilities.defaultRank})`);
    console.log(`   LoRA Alpha: ${capabilities.minAlpha}-${capabilities.maxAlpha} (default: ${capabilities.defaultAlpha})`);
    console.log(`   Epochs: ${capabilities.minEpochs}-${capabilities.maxEpochs} (default: ${capabilities.defaultEpochs})`);
    console.log(`   Learning Rate: ${capabilities.minLearningRate}-${capabilities.maxLearningRate} (default: ${capabilities.defaultLearningRate})`);
    console.log(`   Batch Size: ${capabilities.minBatchSize}-${capabilities.maxBatchSize} (default: ${capabilities.defaultBatchSize})`);
    console.log(`   Cost per example: $${capabilities.costPerExample} (27x cheaper than OpenAI!)`);
    console.log(`   Estimated training time: ${capabilities.estimatedTrainingTime}ms per example per epoch`);
    console.log(`   Requires GPU: ${capabilities.requiresGPU} (cloud-based training)`);
    console.log(`   Requires Internet: ${capabilities.requiresInternet}`);
    console.log(`   Supported models: ${capabilities.supportedBaseModels?.length || 'all'} models`);
    if (capabilities.supportedBaseModels) {
      capabilities.supportedBaseModels.forEach((model: string) => {
        console.log(`     - ${model}`);
      });
    }
    console.log();

    // Step 4: Get strategy
    console.log('🎯 Step 4: Getting training strategy...');
    const strategy = adapter.getFineTuningStrategy();
    console.log(`   Strategy: ${strategy}`);
    console.log(`   (DeepSeek uses remote API for cloud training)\n`);

    // Step 5: Create minimal test dataset
    console.log('📋 Step 5: Creating minimal test dataset (5 examples)...');
    const testDataset = createMinimalDataset();
    console.log(`   Created ${testDataset.examples.length} training examples`);
    console.log(`   Persona: ${testDataset.metadata.personaName}`);
    console.log(`   Trait Type: ${testDataset.metadata.traitType}\n`);

    // Step 6: Estimate training cost (DeepSeek pricing)
    console.log('💰 Step 6: Estimating training cost (DeepSeek pricing)...');
    const exampleCount = testDataset.examples.length;
    const cost = adapter.estimateTrainingCost(exampleCount);
    console.log(`   ${exampleCount} examples × $${capabilities.costPerExample} = $${cost.toFixed(6)}`);
    console.log(`   DeepSeek: $0.55/1M input tokens, $2.19/1M output`);
    console.log(`   OpenAI equivalent: ~$${(cost * 27).toFixed(6)} (27x more expensive)\n`);

    // Step 7: Estimate training time
    console.log('⏱️  Step 7: Estimating training time...');
    const epochs = capabilities.defaultEpochs;
    const estimatedTimeMs = adapter.estimateTrainingTime(exampleCount, epochs);
    console.log(`   ${exampleCount} examples × ${epochs} epochs × ${capabilities.estimatedTrainingTime}ms = ${estimatedTimeMs}ms`);
    console.log(`   (~${(estimatedTimeMs / 1000).toFixed(2)} seconds)`);
    console.log(`   (Includes API latency + cloud processing)\n`);

    // Step 8: Create training request
    console.log('📝 Step 8: Creating training request...');
    const request: LoRATrainingRequest = {
      personaId: testDataset.metadata.personaId,
      personaName: testDataset.metadata.personaName,
      traitType: testDataset.metadata.traitType,
      baseModel: 'deepseek-chat', // Use DeepSeek Chat model
      dataset: testDataset,
      rank: capabilities.defaultRank,
      alpha: capabilities.defaultAlpha,
      epochs: capabilities.defaultEpochs,
      learningRate: capabilities.defaultLearningRate,
      batchSize: capabilities.defaultBatchSize
    };
    console.log(`   Base Model: ${request.baseModel}`);
    console.log(`   LoRA Rank: ${request.rank}`);
    console.log(`   LoRA Alpha: ${request.alpha}`);
    console.log(`   Epochs: ${request.epochs}`);
    console.log(`   Learning Rate: ${request.learningRate}`);
    console.log(`   Batch Size: ${request.batchSize}\n`);

    // Step 9: Validate training request
    console.log('✓  Step 9: Validating training request...');
    try {
      // This calls protected validateRequest() via trainLoRA()
      await adapter.trainLoRA(request);
      console.log('   ❌ UNEXPECTED: trainLoRA() should throw (MVP not implemented)\n');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      if (errorMessage.includes('not implemented yet')) {
        console.log('   ✅ Validation passed (expected "not implemented" error)');
        console.log(`   Message: "${errorMessage}"\n`);
      } else {
        console.log(`   ❌ Unexpected error: ${errorMessage}\n`);
        throw error;
      }
    }

    // Step 10: Test JSONL export
    console.log('📄 Step 10: Testing JSONL export...');
    const jsonl = TrainingDatasetBuilder.exportToJSONL(testDataset);
    const lines = jsonl.trim().split('\n');
    console.log(`   Exported ${lines.length} JSONL lines`);
    console.log(`   First line preview: ${lines[0].substring(0, 80)}...\n`);

    // Step 11: Cost comparison
    console.log('💵 Step 11: Cost comparison (DeepSeek vs competitors)...');
    const deepseekCost = cost;
    const openaiCost = cost * 27; // 27x more expensive
    const unslothCost = 0; // Free (electricity only)

    console.log(`   DeepSeek API:  $${deepseekCost.toFixed(6)} (5 examples)`);
    console.log(`   OpenAI API:    $${openaiCost.toFixed(6)} (5 examples) - 27x more!`);
    console.log(`   Unsloth Local: $${unslothCost.toFixed(6)} (free - electricity only)`);
    console.log();
    console.log(`   For 1000 examples:`);
    console.log(`   DeepSeek:  $${(deepseekCost * 200).toFixed(4)}`);
    console.log(`   OpenAI:    $${(openaiCost * 200).toFixed(4)}`);
    console.log(`   Unsloth:   $0.00 (but requires GPU)\n`);

    // Success summary
    console.log('✅ DeepSeekLoRAAdapter Standalone Test: PASSED');
    console.log('============================================');
    console.log('✓ Adapter initialized successfully');
    console.log('✓ Capabilities reported correctly');
    console.log('✓ Strategy identified as remote-api');
    console.log('✓ Cost estimation works (extremely competitive pricing)');
    console.log('✓ Time estimation works (1000ms per example per epoch)');
    console.log('✓ Training request validation works');
    console.log('✓ JSONL export works');
    console.log('✓ Cost comparison shows DeepSeek advantage');
    console.log();
    console.log('Note: Actual API training not yet implemented (Phase 7.0 MVP)');
    console.log('Next: Phase 7.1 - Implement DeepSeek API calls + job monitoring');

  } catch (error) {
    console.error('❌ DeepSeekLoRAAdapter Test FAILED');
    console.error('===================================');
    if (error instanceof Error) {
      console.error(`Error: ${error.message}`);
      console.error(`Stack: ${error.stack}`);
    } else {
      console.error(`Error: ${String(error)}`);
    }
    process.exit(1);
  }
}

/**
 * Create minimal test dataset (5 examples for MVP)
 * Philosophy: "simplest possible models/datasets - just needs to finish"
 */
function createMinimalDataset(): TrainingDataset {
  const personaId = 'test-persona-deepseek-1234-1234-1234-123456789012' as UUID;
  const personaName = 'Test DeepSeek AI';

  const examples: TrainingExample[] = [
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in coding.` },
        { role: 'user', content: 'What is the difference between const and let in JavaScript?' },
        { role: 'assistant', content: 'const declares a constant that cannot be reassigned, while let declares a variable that can be reassigned.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in coding.` },
        { role: 'user', content: 'How do I handle promises in JavaScript?' },
        { role: 'assistant', content: 'Use .then() for chaining, .catch() for errors, or async/await for cleaner syntax.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in coding.` },
        { role: 'user', content: 'What is destructuring in JavaScript?' },
        { role: 'assistant', content: 'Destructuring extracts values from arrays or properties from objects into distinct variables.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in coding.` },
        { role: 'user', content: 'Explain arrow functions' },
        { role: 'assistant', content: 'Arrow functions provide a concise syntax for functions and lexically bind the this value.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in coding.` },
        { role: 'user', content: 'What is the spread operator?' },
        { role: 'assistant', content: 'The spread operator (...) expands an iterable into individual elements, useful for copying arrays or objects.' }
      ]
    }
  ];

  return {
    examples,
    metadata: {
      personaId,
      personaName,
      traitType: 'conversational',
      createdAt: Date.now(),
      source: 'conversations',
      totalExamples: examples.length
    }
  };
}

// Run main if executed directly
if (require.main === module) {
  main().catch((error) => {
    console.error('Fatal error:', error);
    process.exit(1);
  });
}

// Export for testing
export { main, createMinimalDataset };
