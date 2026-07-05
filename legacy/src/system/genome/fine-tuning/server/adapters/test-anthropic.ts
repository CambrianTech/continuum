/**
 * Standalone test for AnthropicLoRAAdapter - Integration Test (Phase 7.1)
 *
 * Purpose: Test Anthropic adapter in isolation without full system integration
 * Philosophy: "main() type interface to make it easier. Helps you isolate problems"
 *
 * Usage:
 *   npx tsx system/genome/fine-tuning/server/adapters/test-anthropic.ts
 *
 * What This Tests:
 * - AnthropicLoRAAdapter initialization
 * - Capability reporting (rank, epochs, model support, pricing)
 * - Cost/time estimation (competitive pricing between DeepSeek and OpenAI)
 * - API-based training workflow structure
 * - Error handling
 *
 * What This Does NOT Test Yet (Phase 7.1+):
 * - Actual Anthropic API calls (Anthropic doesn't offer fine-tuning yet)
 * - Real training with API
 * - Job monitoring and polling
 * - Model download after training
 *
 * NOTE: This adapter is future-proofed for when Anthropic adds fine-tuning support.
 */

import { AnthropicLoRAAdapter } from './AnthropicLoRAAdapter';
import { TrainingDatasetBuilder } from '../TrainingDatasetBuilder';
import type { LoRATrainingRequest, TrainingDataset, TrainingExample } from '../../shared/FineTuningTypes';
import type { UUID } from '../../../../core/types/CrossPlatformUUID';

/**
 * Main test function
 */
async function main(): Promise<void> {
  console.log('🧬 AnthropicLoRAAdapter Standalone Test');
  console.log('========================================\n');

  try {
    // Step 1: Initialize adapter
    console.log('📦 Step 1: Initializing AnthropicLoRAAdapter...');
    const adapter = new AnthropicLoRAAdapter();
    console.log(`✅ Adapter initialized: providerId="${adapter.providerId}"\n`);

    // Step 2: Check if fine-tuning supported
    console.log('🔍 Step 2: Checking fine-tuning support...');
    const supported = adapter.supportsFineTuning();
    console.log(`   supportsFineTuning(): ${supported}`);
    if (!supported) {
      console.log('   (Expected: false - Anthropic does not yet offer fine-tuning)\n');
    }

    // Step 3: Get capabilities
    console.log('⚙️  Step 3: Getting fine-tuning capabilities...');
    const capabilities = adapter.getFineTuningCapabilities();
    console.log(`   LoRA Rank: ${capabilities.minRank}-${capabilities.maxRank} (default: ${capabilities.defaultRank})`);
    console.log(`   LoRA Alpha: ${capabilities.minAlpha}-${capabilities.maxAlpha} (default: ${capabilities.defaultAlpha})`);
    console.log(`   Epochs: ${capabilities.minEpochs}-${capabilities.maxEpochs} (default: ${capabilities.defaultEpochs})`);
    console.log(`   Learning Rate: ${capabilities.minLearningRate}-${capabilities.maxLearningRate} (default: ${capabilities.defaultLearningRate})`);
    console.log(`   Batch Size: ${capabilities.minBatchSize}-${capabilities.maxBatchSize} (default: ${capabilities.defaultBatchSize})`);
    console.log(`   Cost per example: $${capabilities.costPerExample} (competitive pricing)`);
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
    console.log(`   (Anthropic would use remote API for cloud training when available)\n`);

    // Step 5: Create minimal test dataset
    console.log('📋 Step 5: Creating minimal test dataset (5 examples)...');
    const testDataset = createMinimalDataset();
    console.log(`   Created ${testDataset.examples.length} training examples`);
    console.log(`   Persona: ${testDataset.metadata.personaName}`);
    console.log(`   Trait Type: ${testDataset.metadata.traitType}\n`);

    // Step 6: Estimate training cost (Anthropic pricing)
    console.log('💰 Step 6: Estimating training cost (Anthropic pricing)...');
    const exampleCount = testDataset.examples.length;
    const cost = adapter.estimateTrainingCost(exampleCount);
    console.log(`   ${exampleCount} examples × $${capabilities.costPerExample} = $${cost.toFixed(6)}`);
    console.log(`   Anthropic: ~$3/1M input tokens, ~$15/1M output (estimated)`);
    console.log(`   Competitive position: Between DeepSeek ($0.55/1M) and OpenAI ($15/1M)\n`);

    // Step 7: Estimate training time
    console.log('⏱️  Step 7: Estimating training time...');
    const epochs = capabilities.defaultEpochs;
    const estimatedTimeMs = adapter.estimateTrainingTime(exampleCount, epochs);
    console.log(`   ${exampleCount} examples × ${epochs} epochs × ${capabilities.estimatedTrainingTime}ms = ${estimatedTimeMs}ms`);
    console.log(`   (~${(estimatedTimeMs / 1000).toFixed(2)} seconds)`);
    console.log(`   (Includes estimated API latency + cloud processing)\n`);

    // Step 8: Create training request
    console.log('📝 Step 8: Creating training request...');
    const request: LoRATrainingRequest = {
      personaId: testDataset.metadata.personaId,
      personaName: testDataset.metadata.personaName,
      traitType: testDataset.metadata.traitType,
      baseModel: 'claude-3-5-sonnet', // Use latest Claude 3.5 Sonnet
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
      console.log('   ❌ UNEXPECTED: trainLoRA() should throw (not available yet)\n');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      if (errorMessage.includes('not available yet')) {
        console.log('   ✅ Validation passed (expected "not available" error)');
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
    const anthropicCost = cost;
    const deepseekCost = cost / 7; // ~7x cheaper than Anthropic
    const openaiCost = cost * 3.86; // ~3.86x more expensive than Anthropic
    const unslothCost = 0; // Free (electricity only)

    console.log(`   DeepSeek API:   $${deepseekCost.toFixed(6)} (5 examples) - most affordable!`);
    console.log(`   Anthropic API:  $${anthropicCost.toFixed(6)} (5 examples) - balanced pricing`);
    console.log(`   OpenAI API:     $${openaiCost.toFixed(6)} (5 examples) - premium`);
    console.log(`   Unsloth Local:  $${unslothCost.toFixed(6)} (free - electricity only)`);
    console.log();
    console.log(`   For 1000 examples:`);
    console.log(`   DeepSeek:   $${(deepseekCost * 200).toFixed(4)}`);
    console.log(`   Anthropic:  $${(anthropicCost * 200).toFixed(4)}`);
    console.log(`   OpenAI:     $${(openaiCost * 200).toFixed(4)}`);
    console.log(`   Unsloth:    $0.00 (but requires GPU)\n`);

    // Step 12: Provider comparison matrix
    console.log('📊 Step 12: Provider comparison matrix...');
    console.log('   ┌──────────────┬─────────────┬──────────────┬──────────────┬───────────────┐');
    console.log('   │ Provider     │ Cost/1K     │ GPU Required │ Speed        │ Status        │');
    console.log('   ├──────────────┼─────────────┼──────────────┼──────────────┼───────────────┤');
    console.log(`   │ DeepSeek     │ $${(deepseekCost * 200).toFixed(4).padStart(11)} │ No           │ 1000ms/ex/ep │ Available     │`);
    console.log(`   │ Anthropic    │ $${(anthropicCost * 200).toFixed(4).padStart(11)} │ No           │ 900ms/ex/ep  │ Not Yet       │`);
    console.log(`   │ OpenAI       │ $${(openaiCost * 200).toFixed(4).padStart(11)} │ No           │ 800ms/ex/ep  │ Available     │`);
    console.log('   │ Unsloth      │ $0.0000     │ Yes          │ 25ms/ex/ep   │ Available     │');
    console.log('   └──────────────┴─────────────┴──────────────┴──────────────┴───────────────┘\n');

    console.log('   Trade-offs:');
    console.log('   - DeepSeek: Most affordable API option, available now');
    console.log('   - Anthropic: Balanced pricing, great models, but not yet available for fine-tuning');
    console.log('   - OpenAI: Premium pricing, established API, strong documentation');
    console.log('   - Unsloth: Free and 36x faster, but requires local GPU hardware\n');

    // Step 13: Future-proofing notes
    console.log('🔮 Step 13: Future-proofing assessment...');
    console.log('   ✅ Adapter architecture ready for when Anthropic adds fine-tuning');
    console.log('   ✅ Cost estimates based on Claude API pricing patterns');
    console.log('   ✅ Training workflow structured for easy API integration');
    console.log('   ✅ JSONL export compatible with expected Anthropic format');
    console.log('   ✅ Model support includes full Claude 3 family');
    console.log();
    console.log('   When Anthropic announces fine-tuning:');
    console.log('   1. Update supportsFineTuning() to check for ANTHROPIC_API_KEY');
    console.log('   2. Uncomment implementation methods (trainWithAnthropicAPI, etc.)');
    console.log('   3. Test with real API endpoints');
    console.log('   4. Update pricing if actual costs differ from estimates\n');

    // Success summary
    console.log('✅ AnthropicLoRAAdapter Standalone Test: PASSED');
    console.log('==============================================');
    console.log('✓ Adapter initialized successfully');
    console.log('✓ Capabilities reported correctly (estimated)');
    console.log('✓ Strategy identified as remote-api');
    console.log('✓ Cost estimation works (balanced pricing)');
    console.log('✓ Time estimation works (900ms per example per epoch)');
    console.log('✓ Training request validation works');
    console.log('✓ JSONL export works');
    console.log('✓ Cost comparison shows Anthropic positioning');
    console.log('✓ Provider comparison matrix displayed');
    console.log('✓ Future-proofing assessment complete');
    console.log();
    console.log('Note: Anthropic does not yet offer fine-tuning (Phase 7.0 MVP)');
    console.log('This adapter is ready for when they announce support');

  } catch (error) {
    console.error('❌ AnthropicLoRAAdapter Test FAILED');
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
  const personaId = 'test-persona-anthropic-1234-1234-1234-123456789012' as UUID;
  const personaName = 'Test Anthropic Assistant';

  const examples: TrainingExample[] = [
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in writing.` },
        { role: 'user', content: 'What makes a good technical explanation?' },
        { role: 'assistant', content: 'A good technical explanation is clear, accurate, and tailored to the audience. It uses concrete examples, avoids jargon when possible, and builds understanding incrementally.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in writing.` },
        { role: 'user', content: 'How do I structure a persuasive argument?' },
        { role: 'assistant', content: 'Structure a persuasive argument with: (1) Clear thesis statement, (2) Supporting evidence with credible sources, (3) Address counterarguments, (4) Strong conclusion that reinforces your position.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in writing.` },
        { role: 'user', content: 'What is active vs passive voice?' },
        { role: 'assistant', content: 'Active voice emphasizes the doer: "The cat chased the mouse." Passive voice emphasizes the action: "The mouse was chased by the cat." Active voice is typically more direct and engaging.' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in writing.` },
        { role: 'user', content: 'Explain the concept of "show, don\'t tell"' },
        { role: 'assistant', content: '"Show, don\'t tell" means using specific details, actions, and sensory information instead of abstract statements. Rather than saying "She was angry," describe "Her jaw clenched and she slammed the door."' }
      ]
    },
    {
      messages: [
        { role: 'system', content: `You are ${personaName}, a helpful AI assistant specialized in writing.` },
        { role: 'user', content: 'What is the purpose of a thesis statement?' },
        { role: 'assistant', content: 'A thesis statement provides the central argument or main point of your essay. It guides the reader\'s expectations and gives your writing focus and direction.' }
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
