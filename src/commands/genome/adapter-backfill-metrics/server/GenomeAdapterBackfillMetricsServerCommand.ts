/**
 * Genome Adapter Backfill Metrics Command - Server Implementation
 *
 * Scans all existing adapters and backfills training metrics from:
 * 1. training_metrics.json (written by peft-train.py Phase 1A)
 * 2. checkpoint trainer_state.json (HuggingFace Trainer output)
 * 3. manifest.json trainingMetadata (existing but incomplete)
 *
 * Writes structured training_metrics.json alongside each adapter and
 * updates the manifest with loss history and example counts.
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type {
  GenomeAdapterBackfillMetricsParams,
  GenomeAdapterBackfillMetricsResult,
  BackfillResult,
} from '../shared/GenomeAdapterBackfillMetricsTypes';
import { createGenomeAdapterBackfillMetricsResultFromParams } from '../shared/GenomeAdapterBackfillMetricsTypes';
import { AdapterStore } from '@system/genome/server/AdapterStore';

export class GenomeAdapterBackfillMetricsServerCommand extends CommandBase<GenomeAdapterBackfillMetricsParams, GenomeAdapterBackfillMetricsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-backfill-metrics', context, subpath, commander);
  }

  async execute(params: GenomeAdapterBackfillMetricsParams): Promise<GenomeAdapterBackfillMetricsResult> {
    const missingOnly = params.missingOnly !== false;
    const adapters = AdapterStore.discoverAll();
    const results: BackfillResult[] = [];
    let updated = 0;

    for (const adapter of adapters) {
      const metricsPath = path.join(adapter.dirPath, 'training_metrics.json');
      const manifestPath = path.join(adapter.dirPath, 'manifest.json');

      // Skip if already has training_metrics.json and missingOnly
      if (missingOnly && fs.existsSync(metricsPath)) {
        results.push({ name: adapter.manifest.name, updated: false, reason: 'already has metrics' });
        continue;
      }

      // Try to extract metrics from trainer_state.json in checkpoints
      const extracted = this._extractFromCheckpoints(adapter.dirPath);
      const manifestMetrics = adapter.manifest.trainingMetadata;

      if (!extracted && !manifestMetrics) {
        results.push({ name: adapter.manifest.name, updated: false, reason: 'no metrics source found' });
        continue;
      }

      // Build training_metrics.json from best available source
      const metrics = {
        finalLoss: extracted?.finalLoss ?? manifestMetrics?.loss ?? 0,
        trainRuntime: extracted?.trainRuntime ?? (manifestMetrics?.trainingDuration ? manifestMetrics.trainingDuration / 1000 : 0),
        epochs: extracted?.epochs ?? manifestMetrics?.epochs ?? 0,
        examplesProcessed: manifestMetrics?.examplesProcessed ?? 0,
        lossHistory: extracted?.lossHistory ?? manifestMetrics?.lossHistory ?? [],
      };

      // Write training_metrics.json
      await fs.promises.writeFile(metricsPath, JSON.stringify(metrics, null, 2), 'utf-8');

      // Update manifest with enriched data
      if (manifestMetrics) {
        const manifest = JSON.parse(await fs.promises.readFile(manifestPath, 'utf-8'));
        if (!manifest.trainingMetadata) manifest.trainingMetadata = {};
        if (metrics.lossHistory.length > 0) manifest.trainingMetadata.lossHistory = metrics.lossHistory;
        if (metrics.examplesProcessed > 0) manifest.trainingMetadata.examplesProcessed = metrics.examplesProcessed;
        if (metrics.trainRuntime > 0) manifest.trainingMetadata.trainRuntime = metrics.trainRuntime;
        await fs.promises.writeFile(manifestPath, JSON.stringify(manifest, null, 2), 'utf-8');
      }

      results.push({
        name: adapter.manifest.name,
        updated: true,
        finalLoss: metrics.finalLoss,
        epochs: metrics.epochs,
      });
      updated++;
    }

    return createGenomeAdapterBackfillMetricsResultFromParams(params, {
      success: true,
      scanned: adapters.length,
      updated,
      results,
    });
  }

  /**
   * Extract training metrics from HuggingFace checkpoint trainer_state.json.
   * Scans all checkpoint-* dirs, picks the latest one.
   */
  private _extractFromCheckpoints(adapterDir: string): { finalLoss: number; trainRuntime: number; epochs: number; lossHistory: number[] } | null {
    try {
      const entries = fs.readdirSync(adapterDir, { withFileTypes: true });
      const checkpoints = entries
        .filter(e => e.isDirectory() && e.name.startsWith('checkpoint-'))
        .map(e => path.join(adapterDir, e.name))
        .sort(); // Ascending by step number

      if (checkpoints.length === 0) return null;

      // Use the latest checkpoint
      const latestCheckpoint = checkpoints[checkpoints.length - 1];
      const trainerStatePath = path.join(latestCheckpoint, 'trainer_state.json');

      if (!fs.existsSync(trainerStatePath)) return null;

      const state = JSON.parse(fs.readFileSync(trainerStatePath, 'utf-8'));

      // Extract loss history from log_history
      const lossHistory: number[] = [];
      if (Array.isArray(state.log_history)) {
        for (const entry of state.log_history) {
          if (typeof entry.loss === 'number') {
            lossHistory.push(Math.round(entry.loss * 10000) / 10000);
          }
        }
      }

      return {
        finalLoss: lossHistory.length > 0 ? lossHistory[lossHistory.length - 1] : 0,
        trainRuntime: state.log_history?.find((e: Record<string, unknown>) => typeof e.train_runtime === 'number')?.train_runtime ?? 0,
        epochs: state.epoch ?? 0,
        lossHistory,
      };
    } catch {
      return null;
    }
  }
}
