/**
 * Genome Dataset Prepare Command - Server Implementation
 *
 * Queries chat_messages for a persona's conversations, builds training examples
 * via TrainingDatasetBuilder, exports to JSONL, saves to genome datasets directory.
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeDatasetPrepareParams, GenomeDatasetPrepareResult } from '../shared/GenomeDatasetPrepareTypes';
import { createGenomeDatasetPrepareResultFromParams } from '../shared/GenomeDatasetPrepareTypes';
import { TrainingDatasetBuilder } from '@system/genome/fine-tuning/server/TrainingDatasetBuilder';

export class GenomeDatasetPrepareServerCommand extends CommandBase<GenomeDatasetPrepareParams, GenomeDatasetPrepareResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/dataset-prepare', context, subpath, commander);
  }

  async execute(params: GenomeDatasetPrepareParams): Promise<GenomeDatasetPrepareResult> {
    const { personaId, personaName, roomId } = params;
    const traitType = params.traitType ?? 'conversational';
    const minMessages = params.minMessages ?? 10;
    const maxMessages = params.maxMessages ?? 500;

    console.log(`🧬 DATASET PREPARE: persona=${personaName}, room=${roomId}, trait=${traitType}`);

    if (!personaId) {
      throw new ValidationError('personaId', 'Missing required parameter. See genome/dataset-prepare README.');
    }
    if (!personaName) {
      throw new ValidationError('personaName', 'Missing required parameter. See genome/dataset-prepare README.');
    }
    if (!roomId) {
      throw new ValidationError('roomId', 'Missing required parameter. See genome/dataset-prepare README.');
    }

    // 1. Build dataset from conversation history
    const builder = new TrainingDatasetBuilder({
      minMessages,
      maxMessages,
      minMessageLength: 10,
      excludeSystemMessages: true,
      includeOwnMessages: true,
      includeOtherPersonas: true,
    });

    const result = await builder.buildFromConversation(personaId, personaName, roomId, traitType);

    if (!result.success || !result.dataset) {
      return createGenomeDatasetPrepareResultFromParams(params, {
        success: false,
        error: result.error ?? 'Failed to build dataset',
        datasetPath: '',
        exampleCount: 0,
        personaId,
        traitType,
      });
    }

    // 2. Validate dataset quality
    const validation = TrainingDatasetBuilder.validateDataset(result.dataset);
    if (!validation.valid) {
      return createGenomeDatasetPrepareResultFromParams(params, {
        success: false,
        error: `Dataset validation failed: ${validation.errors.join('; ')}`,
        datasetPath: '',
        exampleCount: 0,
        personaId,
        traitType,
      });
    }

    // 3. Export to JSONL
    const jsonl = TrainingDatasetBuilder.exportToJSONL(result.dataset);

    // 4. Save to datasets directory
    const datasetsDir = path.resolve('.continuum/genome/datasets');
    await fs.promises.mkdir(datasetsDir, { recursive: true });

    const safeName = personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-');
    const timestamp = Date.now();
    const filename = `${safeName}-${traitType}-${timestamp}.jsonl`;
    const datasetPath = path.join(datasetsDir, filename);

    await fs.promises.writeFile(datasetPath, jsonl, 'utf-8');

    console.log(`✅ DATASET PREPARE: ${result.dataset.examples.length} examples → ${datasetPath}`);

    return createGenomeDatasetPrepareResultFromParams(params, {
      success: true,
      datasetPath,
      exampleCount: result.dataset.examples.length,
      personaId,
      traitType,
    });
  }
}
