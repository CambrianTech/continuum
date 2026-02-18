/**
 * Test Dataset Building from Real Chat Data (Phase 7.1)
 *
 * Purpose: Build training dataset from actual chat conversations in the system.
 * Philosophy: "lets get on with real data, wow" - use actual conversation history
 *
 * Usage:
 *   npx tsx system/genome/fine-tuning/server/test-dataset-from-chat.ts
 *
 * What This Tests:
 * - TrainingDatasetBuilder with real chat messages
 * - Dataset quality validation
 * - JSONL export format
 * - Statistics tracking
 * - Ready for fine-tuning with any provider (DeepSeek, OpenAI, Unsloth)
 */

import { TrainingDatasetBuilder } from './TrainingDatasetBuilder';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { DataListParams } from '../../../daemons/data-daemon/shared/DataTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../../daemons/data-daemon/shared/DataTypes';

/**
 * Main test function
 */
async function main(): Promise<void> {
  console.log('🧬 TrainingDatasetBuilder Test with Real Chat Data');
  console.log('===================================================\n');

  try {
    // Step 1: Find the general room
    console.log('📋 Step 1: Finding general chat room...');

    const roomsResult = await DataDaemon.list({
      collection: COLLECTIONS.ROOMS,
      filter: { uniqueId: 'general' },
      limit: 1
    } as DataListParams);

    if (!roomsResult.success || !roomsResult.items || roomsResult.items.length === 0) {
      throw new Error('Could not find general room');
    }

    const room = roomsResult.items[0];
    const roomId = room.id as UUID;
    console.log(`✅ Found room: ${room.name} (${roomId})\n`);

    // Step 2: Find a PersonaUser (any AI)
    console.log('📋 Step 2: Finding PersonaUser for training...');

    const usersResult = await DataDaemon.list({
      collection: COLLECTIONS.USERS,
      filter: { category: 'ai' },
      limit: 1
    } as DataListParams);

    if (!usersResult.success || !usersResult.items || usersResult.items.length === 0) {
      throw new Error('Could not find any AI users');
    }

    const persona = usersResult.items[0];
    const personaId = persona.id as UUID;
    const personaName = persona.displayName || 'AI Assistant';
    console.log(`✅ Found PersonaUser: ${personaName} (${personaId})\n`);

    // Step 3: Initialize TrainingDatasetBuilder
    console.log('📋 Step 3: Initializing TrainingDatasetBuilder...');
    const builder = new TrainingDatasetBuilder({
      maxMessages: 50,  // Get last 50 messages
      minMessages: 10,  // Need at least 10 for training
      minMessageLength: 10,  // Filter very short messages
      requirePersonaInConversation: true  // Ensure AI participated
    });
    console.log('✅ TrainingDatasetBuilder initialized\n');

    // Step 4: Build dataset from real chat
    console.log('📋 Step 4: Building training dataset from chat history...');
    console.log(`   Room: ${room.name}`);
    console.log(`   Persona: ${personaName}`);
    console.log(`   Trait Type: conversational\n`);

    const result = await builder.buildFromConversation(
      personaId,
      personaName,
      roomId,
      'conversational'
    );

    if (!result.success) {
      throw new Error(`Failed to build dataset: ${result.error}`);
    }

    if (!result.dataset) {
      throw new Error('Dataset is undefined despite success=true');
    }

    const dataset = result.dataset;
    console.log(`✅ Dataset built successfully!`);
    console.log(`   Examples: ${dataset.examples.length}`);
    console.log(`   Total tokens (estimated): ${dataset.examples.length * 150} tokens\n`);

    // Step 5: Show dataset statistics
    console.log('📊 Step 5: Dataset Statistics...');
    console.log(`   Source: Chat conversations in ${room.name}`);
    console.log(`   Persona: ${personaName}`);
    console.log(`   Trait Type: ${dataset.metadata.traitType}`);
    console.log(`   Total Examples: ${dataset.metadata.totalExamples}`);
    console.log(`   Created: ${new Date(dataset.metadata.createdAt).toISOString()}\n`);

    // Step 6: Show first example
    if (dataset.examples.length > 0) {
      console.log('📄 Step 6: First Training Example Preview...');
      const firstExample = dataset.examples[0];
      console.log(`   Messages in example: ${firstExample.messages.length}`);

      firstExample.messages.forEach((msg, idx) => {
        const preview = msg.content.substring(0, 80);
        console.log(`   [${idx + 1}] ${msg.role}: ${preview}${msg.content.length > 80 ? '...' : ''}`);
      });
      console.log();
    }

    // Step 7: Export to JSONL
    console.log('📋 Step 7: Exporting dataset to JSONL format...');
    const jsonl = TrainingDatasetBuilder.exportToJSONL(dataset);
    const lines = jsonl.trim().split('\n');
    console.log(`✅ Exported ${lines.length} JSONL lines`);
    console.log(`   First line preview: ${lines[0].substring(0, 100)}...\n`);

    // Step 8: Validate dataset quality
    console.log('📋 Step 8: Validating dataset quality...');
    const validation = TrainingDatasetBuilder.validateDataset(dataset);

    if (!validation.isValid) {
      console.log(`⚠️  Validation warnings:`);
      validation.warnings.forEach(warning => {
        console.log(`   - ${warning}`);
      });
    } else {
      console.log(`✅ Dataset passes all quality checks`);
    }
    console.log();

    // Step 9: Show training cost estimates
    console.log('💰 Step 9: Training Cost Estimates...');
    const exampleCount = dataset.examples.length;
    const epochs = 3;

    // DeepSeek pricing
    const deepseekCostPerExample = 0.00015;
    const deepseekCost = exampleCount * deepseekCostPerExample;

    // OpenAI pricing
    const openaiCostPerExample = 0.00405;
    const openaiCost = exampleCount * openaiCostPerExample;

    // Unsloth (free)
    const unslothCost = 0;

    console.log(`   For ${exampleCount} examples × ${epochs} epochs:`);
    console.log(`   - DeepSeek API:  $${deepseekCost.toFixed(4)} (most affordable)`);
    console.log(`   - OpenAI API:    $${openaiCost.toFixed(4)} (premium)`);
    console.log(`   - Unsloth Local: $${unslothCost.toFixed(4)} (free, requires GPU)\n`);

    // Step 10: Show training time estimates
    console.log('⏱️  Step 10: Training Time Estimates...');

    // DeepSeek time
    const deepseekTimeMs = exampleCount * epochs * 1000;
    const deepseekTimeSec = (deepseekTimeMs / 1000).toFixed(1);

    // OpenAI time
    const openaiTimeMs = exampleCount * epochs * 800;
    const openaiTimeSec = (openaiTimeMs / 1000).toFixed(1);

    // Unsloth time
    const unslothTimeMs = exampleCount * epochs * 25;
    const unslothTimeSec = (unslothTimeMs / 1000).toFixed(1);

    console.log(`   For ${exampleCount} examples × ${epochs} epochs:`);
    console.log(`   - DeepSeek API:  ~${deepseekTimeSec}s`);
    console.log(`   - OpenAI API:    ~${openaiTimeSec}s`);
    console.log(`   - Unsloth Local: ~${unslothTimeSec}s (fastest!)\n`);

    // Success summary
    console.log('✅ TrainingDatasetBuilder Test with Real Data: SUCCESS');
    console.log('======================================================');
    console.log('✓ Found chat room and PersonaUser');
    console.log('✓ Built dataset from real conversation history');
    console.log(`✓ Created ${dataset.examples.length} training examples`);
    console.log('✓ Exported to JSONL format');
    console.log('✓ Dataset quality validated');
    console.log('✓ Cost and time estimates calculated');
    console.log();
    console.log('Next Steps:');
    console.log('1. Set DEEPSEEK_API_KEY environment variable');
    console.log('2. Run DeepSeekLoRAAdapter with this dataset');
    console.log('3. Train a fine-tuned model on real conversation data');
    console.log('4. PersonaUser gains new conversational skills!');

  } catch (error) {
    console.error('❌ TrainingDatasetBuilder Test FAILED');
    console.error('=====================================');
    if (error instanceof Error) {
      console.error(`Error: ${error.message}`);
      console.error(`Stack: ${error.stack}`);
    } else {
      console.error(`Error: ${String(error)}`);
    }
    process.exit(1);
  }
}

// Run main if executed directly
if (require.main === module) {
  main().catch((error) => {
    console.error('Fatal error:', error);
    process.exit(1);
  });
}

// Export for testing
export { main };
