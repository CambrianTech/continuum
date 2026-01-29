/**
 * Training Import Command - Server Implementation
 *
 * Imports JSONL training data into SQLite database:
 * 1. Opens training database with data/open
 * 2. Reads JSONL file line by line
 * 3. Creates TrainingExampleEntity for each line using data/create with dbHandle
 * 4. Tracks statistics (total examples, tokens, duration)
 * 5. Returns dbHandle for MLX training
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { TrainingImportParams, TrainingImportResult } from '../shared/TrainingImportTypes';
import { createTrainingImportResultFromParams } from '../shared/TrainingImportTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataOpenResult } from '../../../data/open/shared/DataOpenTypes';
import type { DataCreateParams, DataCreateResult } from '../../../data/create/shared/DataCreateTypes';
import type { DataCloseResult } from '../../../data/close/shared/DataCloseTypes';
import type { DbHandle } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import type { TrainingMessage } from '../../../../daemons/data-daemon/shared/entities/TrainingExampleEntity';
import { DATABASE_PATHS } from '../../../../system/data/config/DatabaseConfig';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import * as fs from 'fs';
import * as readline from 'readline';

interface JSONLLine {
  messages: TrainingMessage[];
  [key: string]: unknown;
}

export class TrainingImportServerCommand extends CommandBase<TrainingImportParams, TrainingImportResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('training/import', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<TrainingImportResult> {
    const importParams = params as TrainingImportParams;
    const startTime = Date.now();
    let dbHandle: DbHandle = 'default';
    let dbPath = '';

    try {
      // Step 1: Verify JSONL file exists
      if (!fs.existsSync(importParams.jsonlPath)) {
        return createTrainingImportResultFromParams(importParams, {
          success: false,
          dbHandle: 'default',
          dbPath: '',
          stats: {
            totalExamples: 0,
            importedExamples: 0,
            skippedExamples: 0,
            totalTokens: 0,
            averageLength: 0,
            durationMs: 0
          },
          error: `JSONL file not found: ${importParams.jsonlPath}`
        });
      }

      // Step 2: Determine output database path
      dbPath = importParams.outputPath ?? this.getDefaultOutputPath(importParams);
      console.log(`📚 TRAINING IMPORT: Opening database at ${dbPath}...`);

      // Step 3: Open training database
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const openResult = await Commands.execute(DATA_COMMANDS.OPEN, {
        adapter: 'sqlite',
        config: {
          path: dbPath,
          mode: 'create'
        }
      } as any) as DataOpenResult;

      if (!openResult.success || !openResult.dbHandle) {
        return createTrainingImportResultFromParams(importParams, {
          success: false,
          dbHandle: 'default',
          dbPath,
          stats: {
            totalExamples: 0,
            importedExamples: 0,
            skippedExamples: 0,
            totalTokens: 0,
            averageLength: 0,
            durationMs: Date.now() - startTime
          },
          error: `Failed to open database: ${openResult.error ?? 'Unknown error'}`
        });
      }

      dbHandle = openResult.dbHandle;
      console.log(`✅ Database opened with handle: ${dbHandle}`);

      // Step 4: Import JSONL data
      console.log(`📖 TRAINING IMPORT: Reading ${importParams.jsonlPath}...`);
      const stats = await this.importJSONL(importParams, dbHandle);

      // Step 5: Optionally create indices for faster training queries
      if (importParams.createIndices !== false) {
        console.log('🔧 Creating database indices for training optimization...');
        // Indices are created automatically by SqliteStorageAdapter during schema creation
      }

      const durationMs = Date.now() - startTime;
      console.log(`✅ TRAINING IMPORT: Completed in ${durationMs}ms`);
      console.log(`   Total examples: ${stats.totalExamples}`);
      console.log(`   Imported: ${stats.importedExamples}`);
      console.log(`   Total tokens: ${stats.totalTokens}`);
      console.log(`   Average length: ${stats.averageLength.toFixed(2)}`);
      console.log(`   Skipped: ${stats.skippedExamples}`);

      return createTrainingImportResultFromParams(importParams, {
        success: true,
        dbHandle,
        dbPath,
        stats: {
          ...stats,
          durationMs
        }
      });

    } catch (error) {
      // Close database if opened
      if (dbHandle !== 'default') {
        try {
          await Commands.execute<DataCloseResult>(DATA_COMMANDS.CLOSE, { dbHandle });
        } catch (closeError) {
          console.error('Failed to close database after error:', closeError);
        }
      }

      return createTrainingImportResultFromParams(importParams, {
        success: false,
        dbHandle: 'default',
        dbPath,
        stats: {
          totalExamples: 0,
          importedExamples: 0,
          skippedExamples: 0,
          totalTokens: 0,
          averageLength: 0,
          durationMs: Date.now() - startTime
        },
        error: `Import failed: ${error instanceof Error ? error.message : String(error)}`
      });
    }
  }

  /**
   * Import JSONL file into database
   */
  private async importJSONL(
    params: TrainingImportParams,
    dbHandle: DbHandle
  ): Promise<{
    totalExamples: number;
    importedExamples: number;
    totalTokens: number;
    averageLength: number;
    skippedExamples: number;
  }> {
    let totalExamples = 0;
    let importedExamples = 0;
    let totalTokens = 0;
    let skippedExamples = 0;
    let processedCount = 0;

    const batchSize = params.batchSize ?? 100;
    const maxExamples = params.maxExamples ?? Infinity;

    const fileStream = fs.createReadStream(params.jsonlPath);
    const rl = readline.createInterface({
      input: fileStream,
      crlfDelay: Infinity
    });

    for await (const line of rl) {
      if (!line.trim()) continue;
      if (processedCount >= maxExamples) break;

      try {
        const data = JSON.parse(line) as JSONLLine;

        // Validate messages array
        if (!data.messages || !Array.isArray(data.messages) || data.messages.length === 0) {
          skippedExamples++;
          continue;
        }

        // Calculate token count (rough approximation)
        const messageCount = data.messages.length;
        const totalContent = data.messages.map(m => m.content).join(' ');
        const tokenCount = Math.ceil(totalContent.length / 4); // Rough token estimate

        // Create TrainingExampleEntity (pass partial data, DataDaemon constructs full entity)
        const createResult = await Commands.execute<DataCreateParams, DataCreateResult>(DATA_COMMANDS.CREATE, {
          collection: 'training_examples',
          data: {
            messages: data.messages,
            messageCount,
            totalTokens: tokenCount,
            metadata: {
              source: params.datasetName,
              targetSkill: params.targetSkill,
              importedAt: new Date().toISOString()
            }
          },
          dbHandle
        });

        if (!createResult.success) {
          console.warn(`Failed to create example ${processedCount + 1}: ${createResult.error}`);
          skippedExamples++;
          continue;
        }

        totalExamples++;
        importedExamples++;
        totalTokens += tokenCount;
        processedCount++;

        // Emit progress events for UI progress bars (every 100 rows)
        const progressInterval = 100;
        if (processedCount % progressInterval === 0) {
          console.log(`   Imported ${processedCount} examples (${totalTokens} tokens)...`);
          // Future: Events.emit('training:import:progress', {
          //   dbHandle,
          //   processedCount,
          //   totalTokens,
          //   percentage: maxExamples !== Infinity ? (processedCount / maxExamples) * 100 : undefined
          // });
        }

      } catch (error) {
        console.warn(`Failed to parse line ${processedCount + 1}: ${error}`);
        skippedExamples++;
      }
    }

    const averageLength = importedExamples > 0 ? totalTokens / importedExamples : 0;

    return {
      totalExamples,
      importedExamples,
      totalTokens,
      averageLength,
      skippedExamples
    };
  }

  /**
   * Generate default output path from parameters
   * Uses DATASETS_DIR environment variable or DATABASE_PATHS.DATASETS_DIR constant
   */
  private getDefaultOutputPath(params: TrainingImportParams): string {
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const safeName = params.datasetName.replace(/[^a-z0-9-]/gi, '-').toLowerCase();
    const datasetsDir = process.env.DATASETS_DIR || DATABASE_PATHS.DATASETS_DIR;
    return `${datasetsDir}/prepared/${safeName}-${timestamp}.sqlite`;
  }
}
