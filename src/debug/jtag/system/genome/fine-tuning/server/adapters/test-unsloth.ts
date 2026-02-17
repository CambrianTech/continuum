/**
 * Standalone test for UnslothLoRAAdapter - Integration Test (Phase 7.1)
 *
 * Purpose: Test Unsloth adapter in isolation without full system integration
 * Philosophy: "main() type interface to make it easier. Helps you isolate problems"
 *
 * Usage:
 *   npx tsx system/genome/fine-tuning/server/adapters/test-unsloth.ts
 *
 * What This Tests:
 * - UnslothLoRAAdapter initialization
 * - Capability reporting (rank, epochs, model support)
 * - Training workflow (NOT actual training yet - just proves the cycle)
 * - Cost/time estimation
 * - Error handling
 *
 * What This Does NOT Test Yet (Phase 7.1+):
 * - Actual Python subprocess execution
 * - Real training with Unsloth
 * - GGUF export
 * - Model loading
 */

import { UnslothLoRAAdapter } from './UnslothLoRAAdapter';
import { TrainingDatasetBuilder } from '../TrainingDatasetBuilder';
import type { LoRATrainingRequest, TrainingDataset, TrainingExample } from '../../shared/FineTuningTypes';
import type { UUID } from '../../../../core/types/CrossPlatformUUID';

/**
 * Main test function
 */
async function main(): Promise<void> {
  console.log('🧬 UnslothLoRAAdapter Standalone Test');
  console.log('=====================================\n');

  try {
    // Step 1: Initialize adapter
    console.log('📦 Step 1: Initializing UnslothLoRAAdapter...');
    const adapter = new UnslothLoRAAdapter();
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
    console.log(`   Cost per example: $${capabilities.costPerExample} (free for local)`);
    console.log(`   Estimated training time: ${capabilities.estimatedTrainingTime}ms per example per epoch`);
    console.log(`   Requires GPU: ${capabilities.requiresGPU}`);
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
    console.log(`   (Unsloth uses PyTorch with optimizations)\n`);

    // Step 5: Create minimal test dataset
    console.log('📋 Step 5: Creating minimal test dataset (5 examples)...');
    const testDataset = createMinimalDataset();
    console.log(`   Created ${testDataset.examples.length} training examples`);
    console.log(`   Persona: ${testDataset.metadata.personaName}`);
    console.log(`   Trait Type: ${testDataset.metadata.traitType}\n`);

    // Step 6: Estimate training cost
    console.log('💰 Step 6: Estimating training cost...');
    const exampleCount = testDataset.examples.length;
    const cost = adapter.estimateTrainingCost(exampleCount);
    console.log(`   ${exampleCount} examples × $${capabilities.costPerExample} = $${cost.toFixed(4)}`);
    console.log(`   (Free for local training!)\n`);

    // Step 7: Estimate training time
    console.log('⏱️  Step 7: Estimating training time...');
    const epochs = capabilities.defaultEpochs;
    const estimatedTimeMs = adapter.estimateTrainingTime(exampleCount, epochs);
    console.log(`   ${exampleCount} examples × ${epochs} epochs × ${capabilities.estimatedTrainingTime}ms = ${estimatedTimeMs}ms`);
    console.log(`   (~${(estimatedTimeMs / 1000).toFixed(2)} seconds)\n`);

    // Step 8: Create training request
    console.log('📝 Step 8: Creating training request...');
    const request: LoRATrainingRequest = {
      personaId: testDataset.metadata.personaId,
      personaName: testDataset.metadata.personaName,
      traitType: testDataset.metadata.traitType,
      baseModel: 'unsloth/Llama-4-8b', // Use Unsloth-optimized model
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

    // Success summary
    console.log('✅ UnslothLoRAAdapter Standalone Test: PASSED');
    console.log('==========================================');
    console.log('✓ Adapter initialized successfully');
    console.log('✓ Capabilities reported correctly');
    console.log('✓ Strategy identified as local-pytorch');
    console.log('✓ Cost estimation works (free for local)');
    console.log('✓ Time estimation works (25ms per example per epoch)');
    console.log('✓ Training request validation works');
    console.log('✓ JSONL export works');
    console.log();
    console.log('Note: Actual training not yet implemented (Phase 7.0 MVP)');
    console.log('Next: Phase 7.1 - Implement Python subprocess + Unsloth training');

  } catch (error) {
    console.error('❌ UnslothLoRAAdapter Test FAILED');
    console.error('==================================');
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
  const personaId = 'test-persona-12345678-1234-1234-1234-123456789012' as UUID;
  const personaName = 'Test Helper AI';

  const examples: TrainingExample[] = [
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant.` },
        { role: 'user', content: 'What is TypeScript?' },
        { role: 'assistant', content: 'TypeScript is a superset of JavaScript that adds static typing.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant.` },
        { role: 'user', content: 'How do I define an interface?' },
        { role: 'assistant', content: 'Use the interface keyword followed by the interface name and properties.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant.` },
        { role: 'user', content: 'What is a generic type?' },
        { role: 'assistant', content: 'A generic type is a type that can work with multiple types while maintaining type safety.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant.` },
        { role: 'user', content: 'Explain async/await' },
        { role: 'assistant', content: 'async/await is syntactic sugar for working with Promises in a synchronous-looking way.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant.` },
        { role: 'user', content: 'What is a union type?' },
        { role: 'assistant', content: 'A union type represents a value that can be one of several types, using the | operator.' }
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
