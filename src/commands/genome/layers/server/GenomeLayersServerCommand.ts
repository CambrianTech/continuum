/**
 * Genome Layers Command - Server Implementation
 *
 * Queries AdapterStore for a persona's real LoRA adapter stack.
 * Returns actual adapter info — no fakes, no demos.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomeLayersParams, GenomeLayersResult, GenomeLayerInfo } from '../shared/GenomeLayersTypes';
import { createGenomeLayersResultFromParams } from '../shared/GenomeLayersTypes';
import { AdapterStore, type DiscoveredAdapter } from '../../../../system/genome/server/AdapterStore';
import * as fs from 'fs';
import * as path from 'path';

/** Max adapters before fitness saturates at 1.0 */
const MAX_EXPECTED_ADAPTERS = 10;

/**
 * Compute layer maturity (0.0–1.0) from real training data.
 *
 * | Criterion              | Points | Condition                        |
 * |------------------------|--------|----------------------------------|
 * | Weights exist          | +0.20  | hasWeights === true               |
 * | Training converged     | +0.20  | finalLoss < 1.0                  |
 * | Deep convergence       | +0.10  | finalLoss < 0.5                  |
 * | Adequate data          | +0.15  | examples >= 20                   |
 * | Rich data              | +0.10  | examples >= 50                   |
 * | Phenotype validated    | +0.15  | phenotypeScore > 0               |
 * | Quality gate passed    | +0.10  | phenotypeImprovement >= 5        |
 */
function computeMaturity(hasWeights: boolean, metrics?: GenomeLayerInfo['trainingMetrics']): number {
  let score = 0;

  if (hasWeights) score += 0.20;
  if (!metrics) return score;

  if (metrics.finalLoss < 1.0) score += 0.20;
  if (metrics.finalLoss < 0.5) score += 0.10;
  if (metrics.examplesProcessed >= 20) score += 0.15;
  if (metrics.examplesProcessed >= 50) score += 0.10;
  if (metrics.phenotypeScore != null && metrics.phenotypeScore > 0) score += 0.15;
  if (metrics.phenotypeImprovement != null && metrics.phenotypeImprovement >= 5) score += 0.10;

  return Math.min(1.0, score);
}

/**
 * Read training_metrics.json from adapter directory (written by peft-train.py).
 * Falls back to manifest.trainingMetadata if the file doesn't exist.
 */
function readTrainingMetrics(adapter: DiscoveredAdapter): GenomeLayerInfo['trainingMetrics'] | undefined {
  const manifest = adapter.manifest;

  // Try structured metrics file first (Phase 1A contract)
  const metricsPath = path.join(adapter.dirPath, 'training_metrics.json');
  try {
    if (fs.existsSync(metricsPath)) {
      const raw = JSON.parse(fs.readFileSync(metricsPath, 'utf-8'));
      return {
        finalLoss: raw.finalLoss ?? 0,
        epochs: raw.epochs ?? 0,
        examplesProcessed: raw.examplesProcessed ?? 0,
        trainingDurationMs: (raw.trainRuntime ?? 0) * 1000,
        lossHistory: raw.lossHistory,
        phenotypeScore: manifest.trainingMetadata?.phenotypeScore,
        phenotypeImprovement: manifest.trainingMetadata?.phenotypeImprovement,
      };
    }
  } catch {
    // Fall through to manifest
  }

  // Fallback: use manifest.trainingMetadata
  const tm = manifest.trainingMetadata;
  if (!tm) return undefined;

  return {
    finalLoss: tm.loss ?? 0,
    epochs: tm.epochs ?? 0,
    examplesProcessed: tm.examplesProcessed ?? 0,
    trainingDurationMs: tm.trainingDuration ?? 0,
    lossHistory: tm.lossHistory,
    phenotypeScore: tm.phenotypeScore,
    phenotypeImprovement: tm.phenotypeImprovement,
  };
}

export class GenomeLayersServerCommand extends CommandBase<GenomeLayersParams, GenomeLayersResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-layers', context, subpath, commander);
  }

  async execute(params: GenomeLayersParams): Promise<GenomeLayersResult> {
    try {
      const { personaId, personaName } = params;

      if (!personaId) {
        return createGenomeLayersResultFromParams(params, {
          success: false,
          error: 'personaId is required'
        });
      }

      const adapters = AdapterStore.discoverForPersona(personaId, personaName);

      const layers: GenomeLayerInfo[] = adapters.map(a => {
        const trainingMetrics = readTrainingMetrics(a);
        const maturity = computeMaturity(a.hasWeights, trainingMetrics);

        return {
          name: a.manifest.name,
          domain: a.manifest.traitType,
          hasWeights: a.hasWeights,
          baseModel: a.manifest.baseModel,
          createdAt: a.manifest.createdAt,
          sizeMB: a.manifest.sizeMB,
          trainingMetrics,
          maturity,
        };
      });

      const trainedCount = adapters.filter(a => a.hasWeights).length;
      const fitness = Math.min(1.0, trainedCount / MAX_EXPECTED_ADAPTERS);

      return createGenomeLayersResultFromParams(params, {
        success: true,
        layers,
        fitness
      });
    } catch (error) {
      return createGenomeLayersResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
