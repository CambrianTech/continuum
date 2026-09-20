/**
 * Genome Training Export Command - Server Implementation
 *
 * Consumes accumulated training data from a PersonaUser's TrainingDataAccumulator,
 * converts to JSONL format, writes to disk, and returns the path for genome/train.
 *
 * This bridges the gap between the in-memory capture buffer (populated by
 * sentinel/coding-agent with captureTraining=true) and genome/train which
 * requires a JSONL file on disk.
 */

import * as fs from 'fs';
import * as path from 'path';
import { SystemPaths } from '../../../../system/core/config/SystemPaths';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeTrainingExportParams, GenomeTrainingExportResult } from '../shared/GenomeTrainingExportTypes';
import { createGenomeTrainingExportResultFromParams } from '../shared/GenomeTrainingExportTypes';
import { UserDaemonServer } from '@daemons/user-daemon/server/UserDaemonServer';
import { PersonaUser } from '@system/user/server/PersonaUser';
import type { TrainingExample as AccumulatorExample } from '@system/user/server/modules/TrainingDataAccumulator';

export class GenomeTrainingExportServerCommand extends CommandBase<GenomeTrainingExportParams, GenomeTrainingExportResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/training-export', context, subpath, commander);
  }

  async execute(params: GenomeTrainingExportParams): Promise<GenomeTrainingExportResult> {
    const { personaId, personaName, domain, outputPath } = params;

    if (!personaId) {
      throw new ValidationError('personaId', 'Missing required parameter. See genome/training-export README.');
    }
    if (!personaName) {
      throw new ValidationError('personaName', 'Missing required parameter. See genome/training-export README.');
    }
    if (!domain) {
      throw new ValidationError('domain', 'Missing required parameter. See genome/training-export README.');
    }

    console.log(`🧬 TRAINING EXPORT: persona=${personaName}, domain=${domain}`);

    // 1. Get PersonaUser instance
    const userDaemon = UserDaemonServer.getInstance();
    if (!userDaemon) {
      return createGenomeTrainingExportResultFromParams(params, {
        success: false,
        error: 'UserDaemon not initialized',
        datasetPath: '',
        exampleCount: 0,
        domain,
      });
    }

    const baseUser = userDaemon.getPersonaUser(personaId);
    if (!baseUser || !(baseUser instanceof PersonaUser)) {
      return createGenomeTrainingExportResultFromParams(params, {
        success: false,
        error: `PersonaUser not found: ${personaId}`,
        datasetPath: '',
        exampleCount: 0,
        domain,
      });
    }

    const personaUser = baseUser as PersonaUser;

    // 2. Consume accumulated training data
    const accExamples = await personaUser.trainingAccumulator.consumeTrainingData(domain);

    if (accExamples.length === 0) {
      return createGenomeTrainingExportResultFromParams(params, {
        success: false,
        error: `No accumulated training data for domain "${domain}". Ensure CodingAgent steps ran with captureTraining=true.`,
        datasetPath: '',
        exampleCount: 0,
        domain,
      });
    }

    // 3. Convert accumulator examples to JSONL training format
    const jsonlLines = accExamples.map(ex => JSON.stringify({
      messages: [
        { role: 'user', content: ex.input },
        { role: 'assistant', content: ex.output },
      ],
    }));
    const jsonlContent = jsonlLines.join('\n');

    // 4. Write to disk
    const datasetPath = outputPath || this.generatePath(personaName, domain);
    const dir = path.dirname(datasetPath);
    await fs.promises.mkdir(dir, { recursive: true });
    await fs.promises.writeFile(datasetPath, jsonlContent, 'utf-8');

    console.log(`✅ TRAINING EXPORT: ${accExamples.length} examples → ${datasetPath}`);

    return createGenomeTrainingExportResultFromParams(params, {
      success: true,
      datasetPath,
      exampleCount: accExamples.length,
      domain,
    });
  }

  private generatePath(personaName: string, domain: string): string {
    const safeName = personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-');
    const safeDomain = domain.replace(/[^a-z0-9-]+/g, '-');
    const timestamp = Date.now();
    const filename = `${safeName}-${safeDomain}-${timestamp}.jsonl`;
    return path.join(SystemPaths.datasets.root, filename);
  }
}
