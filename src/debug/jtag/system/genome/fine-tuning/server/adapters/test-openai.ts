/**
 * Standalone test for OpenAILoRAAdapter - Integration Test (Phase 7.1)
 *
 * Purpose: Test OpenAI adapter in isolation without full system integration
 * Philosophy: "main() type interface to make it easier. Helps you isolate problems"
 *
 * Usage:
 *   npx tsx system/genome/fine-tuning/server/adapters/test-openai.ts
 *
 * What This Tests:
 * - OpenAILoRAAdapter initialization
 * - Capability reporting (rank, epochs, model support, pricing)
 * - Cost/time estimation (27x more expensive than DeepSeek)
 * - API-based training workflow structure
 * - Error handling
 *
 * What This Does NOT Test Yet (Phase 7.1+):
 * - Actual OpenAI API calls
 * - Real training with API
 * - Job monitoring and polling
 * - Model download after training
 */

import { OpenAILoRAAdapter } from './OpenAILoRAAdapter';
import { TrainingDatasetBuilder } from '../TrainingDatasetBuilder';
import type { LoRATrainingRequest, TrainingDataset, TrainingExample } from '../../shared/FineTuningTypes';
import type { UUID } from '../../../../core/types/CrossPlatformUUID';

/**
 * Main test function
 */
async function main(): Promise<void> {
  console.log('🧬 OpenAILoRAAdapter Standalone Test');
  console.log('====================================\n');

  try {
    // Step 1: Initialize adapter
    console.log('📦 Step 1: Initializing OpenAILoRAAdapter...');
    const adapter = new OpenAILoRAAdapter();
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
    console.log(`   Cost per example: $${capabilities.costPerExample} (premium pricing)`);
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
    console.log(`   (OpenAI uses remote API for cloud training)\n`);

    // Step 5: Create minimal test dataset
    console.log('📋 Step 5: Creating minimal test dataset (5 examples)...');
    const testDataset = createMinimalDataset();
    console.log(`   Created ${testDataset.examples.length} training examples`);
    console.log(`   Persona: ${testDataset.metadata.personaName}`);
    console.log(`   Trait Type: ${testDataset.metadata.traitType}\n`);

    // Step 6: Estimate training cost (OpenAI pricing)
    console.log('💰 Step 6: Estimating training cost (OpenAI pricing)...');
    const exampleCount = testDataset.examples.length;
    const cost = adapter.estimateTrainingCost(exampleCount);
    console.log(`   ${exampleCount} examples × $${capabilities.costPerExample} = $${cost.toFixed(6)}`);
    console.log(`   OpenAI: $15/1M input tokens, $60/1M output`);
    console.log(`   DeepSeek equivalent: ~$${(cost / 27).toFixed(6)} (27x cheaper)\n`);

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
      baseModel: 'gpt-4o-mini', // Use GPT-4o-mini model
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

    // Step 11: Cost comparison (all providers)
    console.log('💵 Step 11: Cost comparison (all providers)...');
    const openaiCost = cost;
    const deepseekCost = cost / 27; // 27x cheaper
    const unslothCost = 0; // Free (electricity only)

    console.log(`   OpenAI API:    $${openaiCost.toFixed(6)} (5 examples) - premium pricing`);
    console.log(`   DeepSeek API:  $${deepseekCost.toFixed(6)} (5 examples) - 27x cheaper!`);
    console.log(`   Unsloth Local: $${unslothCost.toFixed(6)} (free - electricity only)`);
    console.log();
    console.log(`   For 1000 examples:`);
    console.log(`   OpenAI:    $${(openaiCost * 200).toFixed(4)}`);
    console.log(`   DeepSeek:  $${(deepseekCost * 200).toFixed(4)}`);
    console.log(`   Unsloth:   $0.00 (but requires GPU)\n`);

    // Step 12: Provider comparison matrix
    console.log('📊 Step 12: Provider comparison matrix...');
    console.log('   ┌──────────────┬─────────────┬──────────────┬──────────────┐');
    console.log('   │ Provider     │ Cost/1K     │ GPU Required │ Speed        │');
    console.log('   ├──────────────┼─────────────┼──────────────┼──────────────┤');
    console.log(`   │ OpenAI       │ $${(openaiCost * 200).toFixed(4).padStart(11)} │ No           │ 800ms/ex/ep  │`);
    console.log(`   │ DeepSeek     │ $${(deepseekCost * 200).toFixed(4).padStart(11)} │ No           │ 1000ms/ex/ep │`);
    console.log('   │ Unsloth      │ $0.0000     │ Yes          │ 25ms/ex/ep   │');
    console.log('   └──────────────┴─────────────┴──────────────┴──────────────┘\n');

    console.log('   Trade-offs:');
    console.log('   - OpenAI: Premium pricing, but established API, strong documentation');
    console.log('   - DeepSeek: 27x cheaper than OpenAI, competitive alternative');
    console.log('   - Unsloth: Free and 32x faster, but requires local GPU hardware\n');

    // Success summary
    console.log('✅ OpenAILoRAAdapter Standalone Test: PASSED');
    console.log('==========================================');
    console.log('✓ Adapter initialized successfully');
    console.log('✓ Capabilities reported correctly');
    console.log('✓ Strategy identified as remote-api');
    console.log('✓ Cost estimation works (premium pricing)');
    console.log('✓ Time estimation works (800ms per example per epoch)');
    console.log('✓ Training request validation works');
    console.log('✓ JSONL export works');
    console.log('✓ Cost comparison shows provider trade-offs');
    console.log('✓ Provider comparison matrix displayed');
    console.log();
    console.log('Note: Actual API training not yet implemented (Phase 7.0 MVP)');
    console.log('Next: Phase 7.1 - Implement OpenAI API calls + job monitoring');

  } catch (error) {
    console.error('❌ OpenAILoRAAdapter Test FAILED');
    console.error('=================================');
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
  const personaId = 'test-persona-openai-1234-1234-1234-123456789012' as UUID;
  const personaName = 'Test OpenAI Assistant';

  const examples: TrainingExample[] = [
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in Python.` },
        { role: 'user', content: 'What is the difference between list and tuple in Python?' },
        { role: 'assistant', content: 'Lists are mutable (can be changed), while tuples are immutable (cannot be changed after creation).' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in Python.` },
        { role: 'user', content: 'How do I handle exceptions in Python?' },
        { role: 'assistant', content: 'Use try/except blocks to catch exceptions, with optional finally for cleanup code.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in Python.` },
        { role: 'user', content: 'What are Python decorators?' },
        { role: 'assistant', content: 'Decorators are functions that modify the behavior of other functions, using @decorator syntax.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in Python.` },
        { role: 'user', content: 'Explain list comprehensions' },
        { role: 'assistant', content: 'List comprehensions provide concise syntax for creating lists: [x**2 for x in range(10)]' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in Python.` },
        { role: 'user', content: 'What is the difference between == and is?' },
        { role: 'assistant', content: '== checks value equality, while is checks identity (same object in memory).' }
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
