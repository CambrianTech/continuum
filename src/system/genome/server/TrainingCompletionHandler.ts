/**
 * TrainingCompletionHandler — Processes completed training sentinels
 *
 * When genome/train runs in async mode, it returns a handle immediately.
 * This handler subscribes to sentinel completion events for training-type
 * sentinels and runs the post-training workflow:
 *
 *   1. Read training output from sentinel logs
 *   2. Parse metrics (final loss, epochs, etc.)
 *   3. Move adapter from temp output to genome storage
 *   4. Create GenomeLayerEntity in database
 *   5. Emit genome:training:complete event
 *
 * The handler is initialized at server startup alongside SentinelEscalationService.
 */

import { Events } from '../../core/shared/Events';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import { AdapterPackage } from './AdapterPackage';
import { GenomeLayerEntity } from '../entities/GenomeLayerEntity';
import { DataCreate } from '../../../commands/data/create/shared/DataCreateTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Metadata stored when async training starts — needed to complete post-training work.
 */
export interface TrainingCompletionContext {
  handle: string;
  personaId: UUID;
  personaName: string;
  traitType: string;
  baseModel: string;
  rank: number;
  epochs: number;
  exampleCount: number;
  outputDir: string;
  datasetPath: string;
  configPath: string;
  startTime: number;
}

/**
 * In-memory registry of pending training completions.
 * Maps sentinel handle → training context.
 */
const pendingTrainings = new Map<string, TrainingCompletionContext>();

/**
 * Register an async training for completion handling.
 * Called by genome/train when running in async mode.
 */
export function registerTrainingCompletion(context: TrainingCompletionContext): void {
  pendingTrainings.set(context.handle, context);
  console.log(`[TrainingCompletion] Registered ${context.handle} for ${context.personaName}/${context.traitType}`);
}

/**
 * Initialize the training completion handler.
 * Subscribes to sentinel completion events for training-type sentinels.
 */
export function initializeTrainingCompletionHandler(): void {
  // Listen for training sentinel completions via the event bridge
  Events.subscribe('sentinel:complete', async (payload: any) => {
    if (payload.type !== 'training') return;

    const handle = payload.handle;
    const ctx = pendingTrainings.get(handle);
    if (!ctx) return; // Not an async training we're tracking

    try {
      await handleTrainingComplete(ctx, payload);
    } catch (err) {
      console.error(`[TrainingCompletion] Failed for ${handle}: ${err}`);
      Events.emit('genome:training:error', {
        handle,
        personaId: ctx.personaId,
        personaName: ctx.personaName,
        traitType: ctx.traitType,
        error: String(err),
      });
    } finally {
      pendingTrainings.delete(handle);
      cleanupTempFiles(ctx.configPath, ctx.datasetPath);
    }
  });

  // Listen for training failures
  Events.subscribe('sentinel:error', async (payload: any) => {
    if (payload.type !== 'training') return;

    const handle = payload.handle;
    const ctx = pendingTrainings.get(handle);
    if (!ctx) return;

    console.error(`[TrainingCompletion] Training ${handle} failed: ${payload.error}`);
    Events.emit('genome:training:error', {
      handle,
      personaId: ctx.personaId,
      personaName: ctx.personaName,
      traitType: ctx.traitType,
      error: payload.error ?? 'Training process failed',
      exitCode: payload.exitCode,
    });

    pendingTrainings.delete(handle);
    cleanupTempFiles(ctx.configPath, ctx.datasetPath);
  });

  console.log('[TrainingCompletion] Initialized — listening for training sentinel completions');
}

/**
 * Handle a successfully completed training sentinel.
 */
async function handleTrainingComplete(
  ctx: TrainingCompletionContext,
  payload: any,
): Promise<void> {
  const { handle, personaId, personaName, traitType, baseModel, rank, epochs, exampleCount, outputDir, startTime } = ctx;
  const trainingTime = Date.now() - startTime;

  console.log(`[TrainingCompletion] Processing ${handle} (${personaName}/${traitType})`);

  // 1. Read training output from sentinel logs
  const client = RustCoreIPCClient.getInstance();
  const logs = await client.sentinelLogsTail(handle, 'combined', 10000);

  // 2. Parse final loss from output
  let finalLoss = 0.5;
  const lossMatch = logs.content.match(/Final loss: ([\d.]+)/);
  if (lossMatch) {
    finalLoss = parseFloat(lossMatch[1]);
  }

  // 3. Build training metadata
  const trainingMetadata = {
    epochs,
    loss: finalLoss,
    performance: 0,
    trainingDuration: trainingTime,
    datasetHash: `examples:${exampleCount}`,
  };

  // 4. Move adapter to genome storage
  const adaptersDir = path.join('.continuum', 'genome', 'adapters');
  await fs.promises.mkdir(adaptersDir, { recursive: true });

  const adapterName = `${personaName.replace(/\s+/g, '-')}-${traitType}-${Date.now()}`;
  const adapterPath = path.join(adaptersDir, adapterName);
  await fs.promises.mkdir(adapterPath, { recursive: true });

  // Copy from temp output directory
  await copyDirRecursive(outputDir, adapterPath);

  // Calculate size and hash
  const sizeMB = await AdapterPackage.calculateSizeMB(adapterPath);
  const contentHash = await AdapterPackage.calculateContentHash(adapterPath);

  // Build and write manifest
  const manifest = AdapterPackage.buildManifest({
    adapterPath,
    personaId,
    personaName,
    traitType,
    baseModel,
    rank,
    sizeMB,
    contentHash,
    trainingMetadata,
  });
  await AdapterPackage.writeManifest(adapterPath, manifest);

  // 5. Create GenomeLayerEntity
  let layerId: UUID | undefined;
  try {
    const entity = AdapterPackage.toGenomeLayerEntity(manifest, adapterPath);
    await DataCreate.execute({
      collection: GenomeLayerEntity.collection,
      data: entity,
    });
    layerId = entity.id;
    console.log(`[TrainingCompletion] GenomeLayerEntity created: ${layerId}`);
  } catch (err) {
    console.warn(`[TrainingCompletion] Failed to persist GenomeLayerEntity: ${err}`);
  }

  // 6. Emit completion event — widgets and services subscribe to this
  Events.emit('genome:training:complete', {
    handle,
    personaId,
    personaName,
    traitType,
    adapterPath,
    layerId,
    sentinelHandle: handle,
    metrics: {
      finalLoss,
      trainingTime,
      examplesProcessed: exampleCount,
      epochs,
    },
  });

  console.log(`[TrainingCompletion] ${handle} complete: adapter=${adapterPath}, loss=${finalLoss}, time=${(trainingTime / 1000).toFixed(1)}s`);

  // 7. Clean up temp output directory
  try {
    await fs.promises.rm(outputDir, { recursive: true, force: true });
  } catch {
    // Non-critical
  }
}

/**
 * Copy directory contents recursively.
 */
async function copyDirRecursive(src: string, dest: string): Promise<void> {
  const entries = await fs.promises.readdir(src, { withFileTypes: true });
  for (const entry of entries) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      await fs.promises.mkdir(destPath, { recursive: true });
      await copyDirRecursive(srcPath, destPath);
    } else {
      await fs.promises.copyFile(srcPath, destPath);
    }
  }
}

/**
 * Clean up temporary files from training setup.
 */
async function cleanupTempFiles(...paths: string[]): Promise<void> {
  for (const filePath of paths) {
    try {
      const stats = await fs.promises.stat(filePath);
      if (stats.isDirectory()) {
        await fs.promises.rm(filePath, { recursive: true, force: true });
      } else {
        await fs.promises.unlink(filePath);
      }
    } catch {
      // File already cleaned up or doesn't exist
    }
  }
}
