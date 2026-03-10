/**
 * Genome Adapter Update Metrics Command - Server Implementation
 *
 * Reads an adapter's manifest.json, merges in phenotype validation results,
 * and writes the updated manifest back to disk. This persists quality gate
 * results so genome/layers can compute accurate maturity scores.
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeAdapterUpdateMetricsParams, GenomeAdapterUpdateMetricsResult } from '../shared/GenomeAdapterUpdateMetricsTypes';
import { createGenomeAdapterUpdateMetricsResultFromParams } from '../shared/GenomeAdapterUpdateMetricsTypes';

export class GenomeAdapterUpdateMetricsServerCommand extends CommandBase<GenomeAdapterUpdateMetricsParams, GenomeAdapterUpdateMetricsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-update-metrics', context, subpath, commander);
  }

  async execute(params: GenomeAdapterUpdateMetricsParams): Promise<GenomeAdapterUpdateMetricsResult> {
    const { adapterPath, phenotypeScore, phenotypeImprovement } = params;

    if (!adapterPath || adapterPath.trim() === '') {
      throw new ValidationError('adapterPath', 'Missing required parameter: adapterPath');
    }

    const manifestPath = path.join(adapterPath, 'manifest.json');
    if (!fs.existsSync(manifestPath)) {
      throw new ValidationError('adapterPath', `No manifest.json found at: ${adapterPath}`);
    }

    // Read current manifest
    const manifestRaw = await fs.promises.readFile(manifestPath, 'utf-8');
    const manifest = JSON.parse(manifestRaw);

    // Ensure trainingMetadata exists
    if (!manifest.trainingMetadata) {
      manifest.trainingMetadata = {};
    }

    // Merge phenotype metrics
    if (phenotypeScore != null) {
      manifest.trainingMetadata.phenotypeScore = phenotypeScore;
    }
    if (phenotypeImprovement != null) {
      manifest.trainingMetadata.phenotypeImprovement = phenotypeImprovement;
    }

    // Write back
    await fs.promises.writeFile(manifestPath, JSON.stringify(manifest, null, 2), 'utf-8');

    return createGenomeAdapterUpdateMetricsResultFromParams(params, {
      success: true,
      name: manifest.name,
    });
  }
}
