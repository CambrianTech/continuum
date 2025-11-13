/**
 * Genome Train Server Command
 *
 * Integrates fine-tuning into the live system - REAL UTILITY!
 * Philosophy: "Past simple integration tests into real utility"
 *
 * Usage: ./jtag genome/train --personaId=<uuid> --provider=unsloth --roomId=<uuid>
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomeTrainParams, GenomeTrainResult } from '../shared/GenomeTrainTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

// Dataset builder
import { TrainingDatasetBuilder } from '../../../../system/genome/fine-tuning/server/TrainingDatasetBuilder';

// Adapters
import { PEFTLoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/PEFTLoRAAdapter';
import { OllamaLoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/OllamaLoRAAdapter';
import { DeepSeekLoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/DeepSeekLoRAAdapter';
import { OpenAILoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/OpenAILoRAAdapter';
import { TogetherLoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/TogetherLoRAAdapter';
import { FireworksLoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/FireworksLoRAAdapter';
import { AnthropicLoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/AnthropicLoRAAdapter';
import type { BaseLoRATrainer } from '../../../../system/genome/fine-tuning/shared/BaseLoRATrainer';

// Data access
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';
import { UserEntity } from '../../../../system/data/entities/UserEntity';

/**
 * Genome Train Server Command
 *
 * Orchestrates the complete fine-tuning pipeline:
 * 1. Load PersonaUser data
 * 2. Extract training data from chat conversations
 * 3. Build training dataset
 * 4. Show cost/time estimates
 * 5. Execute training with selected provider
 * 6. Save adapter to genome storage
 * 7. Return results to user
 */
export class GenomeTrainServerCommand extends CommandBase<GenomeTrainParams, GenomeTrainResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-train', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeTrainResult> {
    const trainParams = params as GenomeTrainParams;
    console.log('🧬 GENOME TRAIN: Starting fine-tuning integration');
    console.log(`   PersonaUser: ${trainParams.personaId.slice(0, 8)}...`);
    console.log(`   Provider: ${trainParams.provider}`);
    console.log(`   Room: ${trainParams.roomId ? trainParams.roomId.slice(0, 8) + '...' : 'ALL'}`);

    try {
      // Step 1: Load PersonaUser data
      const persona = await this.loadPersonaUser(trainParams.personaId);
      if (!persona) {
        return transformPayload(params, {
          success: false,
          error: `PersonaUser not found: ${trainParams.personaId}`
        });
      }

      console.log(`   Persona name: ${persona.displayName}`);

      // Step 2: Load or build training dataset
      let dataset;
      let datasetStats;

      if (trainParams.datasetPath) {
        // PATHWAY 1: Load dataset from file
        console.log(`📁 GENOME TRAIN: Loading dataset from file: ${trainParams.datasetPath}`);
        const loadResult = await this.loadDatasetFromFile(
          trainParams.datasetPath,
          trainParams.personaId,
          persona.displayName ?? 'AI Assistant',
          trainParams.traitType ?? 'conversational'
        );

        if (!loadResult.success || !loadResult.dataset) {
          return transformPayload(params, {
            success: false,
            error: loadResult.error ?? 'Failed to load dataset from file',
            dataset: {
              totalMessages: 0,
              exampleCount: 0,
              messagesFiltered: 0
            }
          });
        }

        dataset = loadResult.dataset;
        datasetStats = {
          totalMessages: dataset.examples.length,
          exampleCount: dataset.examples.length,
          messagesFiltered: 0
        };
        console.log(`✅ Dataset loaded: ${dataset.examples.length} training examples`);

      } else {
        // PATHWAY 2: Extract from chat history (original behavior)
        const roomIds = await this.determineRooms(trainParams.personaId, trainParams.roomId);
        if (roomIds.length === 0) {
          return transformPayload(params, {
            success: false,
            error: 'No rooms found for this PersonaUser (or provide --datasetPath instead)'
          });
        }

        console.log(`   Training from ${roomIds.length} room(s)`);

        const builder = new TrainingDatasetBuilder({
          maxMessages: trainParams.maxMessages ?? 50,
          minMessages: trainParams.minMessages ?? 10,
          minMessageLength: 10
        });

        console.log('🔧 GENOME TRAIN: Building training dataset from chat history...');

        const datasetResult = await builder.buildFromConversation(
          trainParams.personaId,
          persona.displayName ?? 'AI Assistant',
          roomIds[0], // Use first room for now (TODO: support multiple rooms)
          trainParams.traitType ?? 'conversational'
        );

        if (!datasetResult.success || !datasetResult.dataset) {
          return transformPayload(params, {
            success: false,
            error: datasetResult.error ?? 'Failed to build training dataset',
            dataset: {
              totalMessages: 0,
              exampleCount: 0,
              messagesFiltered: 0
            }
          });
        }

        dataset = datasetResult.dataset;
        datasetStats = {
          totalMessages: datasetResult.stats?.messagesProcessed ?? 0,
          exampleCount: dataset.examples.length,
          messagesFiltered: datasetResult.stats?.messagesFiltered ?? 0
        };
        console.log(`✅ Dataset built: ${dataset.examples.length} training examples`);
      }

      // Step 4: Get adapter for provider
      const adapter = this.getAdapter(trainParams.provider);
      if (!adapter) {
        return transformPayload(params, {
          success: false,
          error: `Unknown provider: ${trainParams.provider}`
        });
      }

      // Check if adapter supports fine-tuning
      if (!adapter.supportsFineTuning()) {
        return transformPayload(params, {
          success: false,
          error: `Provider ${trainParams.provider} not available (check API keys or dependencies)`
        });
      }

      // Step 5: Show cost/time estimates
      const capabilities = adapter.getFineTuningCapabilities();
      const epochs = trainParams.epochs ?? capabilities.defaultEpochs ?? 3;  // Guaranteed to be number with fallback
      const cost = adapter.estimateTrainingCost(dataset.examples.length);
      const time = adapter.estimateTrainingTime(dataset.examples.length, epochs);

      const estimates = {
        cost,
        time,
        exampleCount: dataset.examples.length
      };

      console.log('💰 Cost estimate: $' + cost.toFixed(4));
      console.log('⏱️  Time estimate: ' + (time / 1000).toFixed(1) + 's');

      if (trainParams.showEstimates !== false) {
        console.log('📊 Training estimates:');
        console.log(`   Examples: ${dataset.examples.length}`);
        console.log(`   Epochs: ${epochs}`);
        console.log(`   Cost: $${cost.toFixed(4)}`);
        console.log(`   Time: ~${(time / 1000).toFixed(1)}s`);
      }

      // Step 6: Dry run? Return estimates only
      // CLI params come as strings, so check for both boolean and string 'true'/'false'
      const dryRunValue = String(trainParams.dryRun);
      const isDryRun = dryRunValue === 'true' || trainParams.dryRun === true;

      if (isDryRun) {
        console.log('🔍 DRY RUN: Skipping actual training');
        return transformPayload(params, {
          success: true,
          estimates,
          dataset: datasetStats
        });
      }

      // Step 7: Execute training
      console.log('🚀 GENOME TRAIN: Starting training...');

      const trainingResult = await adapter.trainLoRA({
        personaId: trainParams.personaId,
        personaName: persona.displayName ?? 'AI Assistant',
        traitType: trainParams.traitType ?? 'conversational',
        baseModel: trainParams.baseModel ?? (capabilities.supportedBaseModels?.[0] ?? 'llama3.2:3b'),
        dataset,
        rank: trainParams.rank ?? capabilities.defaultRank,
        alpha: trainParams.alpha ?? capabilities.defaultAlpha,
        epochs,
        learningRate: trainParams.learningRate ?? capabilities.defaultLearningRate,
        batchSize: trainParams.batchSize ?? capabilities.defaultBatchSize
      });

      if (!trainingResult.success) {
        return transformPayload(params, {
          success: false,
          error: trainingResult.error ?? 'Training failed',
          estimates,
          dataset: datasetStats
        });
      }

      // Step 8: Success!
      console.log('✅ GENOME TRAIN: Training complete!');
      if (trainingResult.modelId) {
        console.log(`   Model ID: ${trainingResult.modelId}`);
      }
      if (trainingResult.modelPath) {
        console.log(`   Adapter path: ${trainingResult.modelPath}`);
      }

      return transformPayload(params, {
        success: true,
        modelId: trainingResult.modelId,
        adapterPath: trainingResult.modelPath,
        metrics: trainingResult.metrics ? {
          trainingTime: trainingResult.metrics.trainingTime ?? 0,
          finalLoss: trainingResult.metrics.finalLoss ?? 0.5,
          examplesProcessed: trainingResult.metrics.examplesProcessed ?? dataset.examples.length,
          epochs: trainingResult.metrics.epochs ?? epochs
        } : undefined,
        estimates,
        dataset: datasetStats
      });

    } catch (error) {
      console.error('❌ GENOME TRAIN: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  /**
   * Load PersonaUser data from database
   */
  private async loadPersonaUser(personaId: UUID): Promise<{ id: UUID; displayName: string } | null> {
    const result = await DataDaemon.query<UserEntity>({
      collection: COLLECTIONS.USERS,
      filter: { id: personaId },
      limit: 1
    });

    if (!result.success || !result.data || result.data.length === 0) {
      return null;
    }

    const record = result.data[0];  // DataRecord<UserEntity>
    const user = record.data;  // UserEntity with displayName property
    return {
      id: user.id as UUID,
      displayName: user.displayName ?? 'AI Assistant'
    };
  }

  /**
   * Load training dataset from JSONL file or SQLite database
   */
  private async loadDatasetFromFile(
    filePath: string,
    personaId: UUID,
    personaName: string,
    traitType: string
  ): Promise<{ success: boolean; dataset?: any; error?: string }> {
    try {
      const fs = await import('fs');
      const path = await import('path');

      // Resolve path (support relative paths)
      const resolvedPath = path.resolve(filePath);

      // Check if file exists
      if (!fs.existsSync(resolvedPath)) {
        return {
          success: false,
          error: `Dataset file not found: ${resolvedPath}`
        };
      }

      // Detect file type by extension
      const ext = path.extname(resolvedPath).toLowerCase();

      if (ext === '.db') {
        // Load from SQLite database
        console.log('📦 Loading dataset from SQLite database (buffered)...');
        return await this.loadDatasetFromDatabase(resolvedPath, personaId, personaName, traitType);
      } else {
        // Load from JSONL file (legacy support)
        console.log('📄 Loading dataset from JSONL file (in-memory)...');
        return await this.loadDatasetFromJSONL(resolvedPath, personaId, personaName, traitType);
      }

    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Load training dataset from SQLite database (memory-efficient, buffered)
   */
  private async loadDatasetFromDatabase(
    dbPath: string,
    personaId: UUID,
    personaName: string,
    traitType: string
  ): Promise<{ success: boolean; dataset?: any; error?: string }> {
    const { Commands } = await import('../../../../system/core/shared/Commands');

    let dbHandle = 'default';

    try {
      // Step 1: Open database with read-only mode
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const openResult = await Commands.execute('data/open', {
        adapter: 'sqlite',
        config: {
          path: dbPath,
          mode: 'readonly'
        }
      } as any) as any;

      if (!openResult.success || !openResult.dbHandle) {
        return {
          success: false,
          error: `Failed to open database: ${openResult.error ?? 'Unknown error'}`
        };
      }

      dbHandle = openResult.dbHandle;
      console.log(`✅ Database opened with handle: ${dbHandle}`);

      // Step 2: Query all training examples (buffered via data/list)
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const listResult = await Commands.execute('data/list', {
        collection: 'training_examples',
        dbHandle,
        limit: 10000  // Large limit for training datasets
      } as any) as any;

      if (!listResult.success || !listResult.data) {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        await Commands.execute('data/close', { dbHandle } as any);
        return {
          success: false,
          error: `Failed to query training examples: ${listResult.error ?? 'Unknown error'}`
        };
      }

      const records = listResult.data;
      console.log(`📚 Loaded ${records.length} training examples from database`);

      // Step 3: Convert TrainingExampleEntity to training format
      const examples = records.map((record: any) => ({
        messages: record.messages
      }));

      // Step 4: Close database handle
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      await Commands.execute('data/close', { dbHandle } as any);
      console.log(`🔒 Database closed (handle: ${dbHandle})`);

      // Step 5: Build TrainingDataset
      const dataset = {
        examples,
        metadata: {
          personaId,
          personaName,
          traitType,
          createdAt: Date.now(),
          source: 'database' as const,
          totalExamples: examples.length
        }
      };

      return {
        success: true,
        dataset
      };

    } catch (error) {
      // Clean up handle on error
      if (dbHandle !== 'default') {
        try {
          const { Commands } = await import('../../../../system/core/shared/Commands');
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          await Commands.execute('data/close', { dbHandle } as any);
        } catch (closeError) {
          console.error('Failed to close database after error:', closeError);
        }
      }

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Load training dataset from JSONL file (legacy support, loads into memory)
   */
  private async loadDatasetFromJSONL(
    filePath: string,
    personaId: UUID,
    personaName: string,
    traitType: string
  ): Promise<{ success: boolean; dataset?: any; error?: string }> {
    try {
      const fs = await import('fs');

      // Read JSONL file
      const fileContent = fs.readFileSync(filePath, 'utf-8');
      const lines = fileContent.split('\n').filter(line => line.trim().length > 0);

      // Parse each line as JSON
      const examples = [];
      for (let i = 0; i < lines.length; i++) {
        try {
          const parsed = JSON.parse(lines[i]);
          examples.push(parsed);
        } catch (error) {
          console.warn(`⚠️  Skipping malformed JSON on line ${i + 1}: ${error}`);
        }
      }

      if (examples.length === 0) {
        return {
          success: false,
          error: 'No valid training examples found in dataset file'
        };
      }

      // Build TrainingDataset
      const dataset = {
        examples,
        metadata: {
          personaId,
          personaName,
          traitType,
          createdAt: Date.now(),
          source: 'file' as const,
          totalExamples: examples.length
        }
      };

      return {
        success: true,
        dataset
      };

    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Determine which room(s) to extract training data from
   */
  private async determineRooms(personaId: UUID, roomId?: UUID): Promise<UUID[]> {
    // If roomId specified, use that
    if (roomId) {
      return [roomId];
    }

    // Otherwise, find all rooms this PersonaUser participates in
    // TODO: Implement room membership query
    // For now, just return empty array (user must specify roomId)
    console.log('⚠️  No roomId specified - user must provide --roomId parameter');
    return [];
  }

  /**
   * Get adapter for provider
   */
  private getAdapter(provider: string): BaseLoRATrainer | null {
    switch (provider) {
      case 'peft':
      case 'unsloth': // Legacy alias
        return new PEFTLoRAAdapter();
      case 'ollama':
      case 'llama-cpp': // Alias
        return new OllamaLoRAAdapter();
      case 'deepseek':
        return new DeepSeekLoRAAdapter();
      case 'openai':
        return new OpenAILoRAAdapter();
      case 'together':
        return new TogetherLoRAAdapter();
      case 'fireworks':
        return new FireworksLoRAAdapter();
      case 'anthropic':
        return new AnthropicLoRAAdapter();
      default:
        return null;
    }
  }
}
