/**
 * BaseLoRATrainerServer - Server-side base class with database orchestration
 *
 * This class provides universal handle-pattern orchestration for ALL providers:
 * - Remote APIs (OpenAI, Together, Fireworks)
 * - Local training (Ollama, MLX, PEFT)
 * - Weird APIs (provider-specific quirks)
 *
 * Architecture:
 * 1. trainLoRA() - Calls _startTraining(), persists handle, returns immediately
 * 2. checkStatus() - Loads session, calls _queryStatus(), updates database
 * 3. Subclasses implement TWO primitives: _startTraining() and _queryStatus()
 *
 * Benefits:
 * - Non-blocking: Everything returns immediately
 * - Crash-proof: Handles persisted in database
 * - Universal: Works for any provider type
 * - Simple: Subclasses just implement 2 methods
 */

/* eslint-disable @typescript-eslint/prefer-nullish-coalescing */

import { BaseLoRATrainer } from '../shared/BaseLoRATrainer';
import type {
  LoRATrainingRequest,
  LoRATrainingResult,
  TrainingHandle,
  TrainingStatus
} from '../shared/FineTuningTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DataCreateResult } from '../../../../commands/data/create/shared/DataCreateTypes';
import type { DataReadResult } from '../../../../commands/data/read/shared/DataReadTypes';
import type { TrainingSessionEntity } from '../../../../system/data/entities/TrainingSessionEntity';
import { DATA_COMMANDS } from '../../../../commands/data/shared/DataCommandConstants';

/**
 * Server-side base class with database operations
 *
 * Subclasses implement _startTraining() and _queryStatus().
 * Base class handles all database persistence and orchestration.
 */
export abstract class BaseLoRATrainerServer extends BaseLoRATrainer {
  /**
   * Start training and return handle immediately
   *
   * Subclasses implement this to:
   * 1. Upload/prepare data
   * 2. Start training job/process
   * 3. Return handle with jobId and any provider-specific metadata
   *
   * Examples:
   * - OpenAI: Upload file, create job, return { jobId, fileId }
   * - Ollama: Spawn process, return { jobId: pid.toString(), processId: pid }
   * - Fireworks: Upload dataset, create job, return { jobId, datasetName }
   *
   * This method should be FAST (< 30 seconds). Don't wait for training to complete!
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  protected abstract _startTraining(
    request: LoRATrainingRequest
  ): Promise<TrainingHandle>;
  /* eslint-enable @typescript-eslint/naming-convention */

  /**
   * Query current status from provider
   *
   * Subclasses implement this to:
   * 1. Load provider-specific data from session.metadata
   * 2. Query provider API or check process status
   * 3. Return current status
   *
   * Examples:
   * - OpenAI: GET /v1/fine_tuning/jobs/{jobId}, map status
   * - Ollama: Check process running, read progress file
   * - Fireworks: GET /v1/accounts/{accountId}/jobs/{jobId}
   *
   * This method should be FAST (< 5 seconds). It's called frequently!
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  protected abstract _queryStatus(
    sessionId: UUID,
    providerJobId: string,
    metadata: Record<string, unknown>
  ): Promise<TrainingStatus>;
  /* eslint-enable @typescript-eslint/naming-convention */

  // ==========================================================================
  // Protected Helper Methods (Available to Subclasses)
  // ==========================================================================

  /**
   * Export dataset to standard JSONL format
   *
   * This is the unified JSONL exporter for ALL providers.
   * Uses OpenAI Chat Completions format: {"messages": [...]}
   *
   * CRITICAL: Only includes messages field, strips metadata to avoid validation errors
   *
   * @param dataset - Training dataset with examples
   * @param outputPath - Where to write the JSONL file
   * @returns Path to the exported JSONL file
   */
  protected async exportDatasetToJSONL(
    dataset: import('../shared/FineTuningTypes').TrainingDataset,
    outputPath: string
  ): Promise<string> {
    const fs = await import('fs');
    const path = await import('path');

    // Ensure directory exists
    const dir = path.dirname(outputPath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    // Convert to JSONL (only messages field, strip metadata)
    const lines = (dataset.examples ?? []).map(example => {
      return JSON.stringify({ messages: example.messages });
    });

    fs.writeFileSync(outputPath, lines.join('\n'), 'utf-8');
    return outputPath;
  }

  // ==========================================================================
  // Public API Methods
  // ==========================================================================

  /**
   * Universal trainLoRA implementation
   *
   * Orchestrates the async pattern:
   * 1. Validate request
   * 2. Start training (calls subclass _startTraining())
   * 3. Persist session to database
   * 4. Return immediately with session ID
   *
   * NO BLOCKING! Returns in seconds, not minutes.
   */
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // 1. Validate request
    this.validateRequest(request);

    this.log('info', `🚀 ${this.providerId}: Starting training (async pattern)...`);
    const startTime = Date.now();

    try {
      // 2. Start training (subclass primitive - should be fast!)
      const handle = await this._startTraining(request);

      // 3. Persist session to database
      const sessionId = await this._persistSession(request, handle);

      // 4. Return immediately
      const elapsed = Date.now() - startTime;
      this.log('info', `✅ ${this.providerId}: Training started in ${elapsed}ms`);
      this.log('info', `   Session ID: ${sessionId}`);
      this.log('info', `   Provider Job ID: ${handle.jobId}`);
      this.log('info', `   Use checkStatus(sessionId) to monitor progress`);

      return {
        success: true,
        modelId: sessionId,  // Session ID serves as model ID for now
        metrics: {
          epochs: request.epochs ?? 3,
          trainingTime: elapsed,
          examplesProcessed: request.dataset.examples.length
        }
      };
    } catch (error) {
      const elapsed = Date.now() - startTime;
      this.log('error', `❌ ${this.providerId}: Training failed in ${elapsed}ms:`, error);

      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        errorDetails: error
      };
    }
  }

  /**
   * Universal checkStatus implementation
   *
   * Orchestrates status checking:
   * 1. Load session from database
   * 2. Query provider (calls subclass _queryStatus())
   * 3. Update database if status changed
   * 4. Return current status
   *
   * FAST query - no blocking!
   */
  async checkStatus(sessionId: UUID): Promise<TrainingStatus> {
    try {
      // 1. Load session from database
      const session = await this._loadSession(sessionId);

      // 2. Query provider (subclass primitive)
      const status = await this._queryStatus(
        sessionId,
        session.providerJobId,
        session.metadata
      );

      // 3. Update database if status changed
      if (status.status !== session.status) {
        await this._updateSession(sessionId, status);
      }

      // 4. Return current status
      return status;
    } catch (error) {
      this.log('error', `❌ ${this.providerId}: checkStatus failed:`, error);

      return {
        status: 'failed',
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  /**
   * Persist session to database
   *
   * Creates TrainingSessionEntity with handle information.
   * Session survives server restarts.
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  private async _persistSession(
    request: LoRATrainingRequest,
    handle: TrainingHandle
  ): Promise<UUID> {
    // Import Commands here (server-side only)
    /* eslint-disable @typescript-eslint/naming-convention */
    const { Commands } = await import('../../../../system/core/shared/Commands');
    /* eslint-enable @typescript-eslint/naming-convention */

    // Create session entity
    const rawResult = await Commands.execute(DATA_COMMANDS.CREATE, {
      collection: 'training_sessions',
      data: {
        personaId: request.personaId,
        providerJobId: handle.jobId,
        provider: this.providerId as 'openai' | 'deepseek' | 'peft' | 'mlx' | 'anthropic',
        status: 'running',
        baseModel: request.baseModel,
        datasetHandle: handle.fileId || handle.datasetName || `dataset-${handle.jobId}`, // Provider's file/dataset ID
        hyperparameters: {
          learningRate: request.learningRate ?? 0.0001,
          batchSize: request.batchSize ?? 4,
          epochs: request.epochs ?? 3,
          rank: request.rank ?? 16,
          alpha: request.alpha ?? 32,
          targetModules: ['q_proj', 'v_proj']  // Default modules
        },
        providerConfig: {
          provider: this.providerId as 'openai' | 'deepseek' | 'peft' | 'mlx' | 'anthropic'
        },
        outputDir: request.outputPath ?? `/tmp/training-${handle.jobId}`,
        startedAt: Date.now(),
        completedAt: null,
        finalCheckpointPath: null,
        error: null,
        estimatedDuration: this.estimateTrainingTime(
          request.dataset.examples.length,
          request.epochs ?? 3
        ),
        description: `Training ${request.personaName} on ${request.baseModel}`,
        metadata: {
          ...handle.metadata,
          fileId: handle.fileId,
          datasetName: handle.datasetName,
          processId: handle.processId,
          traitType: request.traitType
        }
      }
    } as never);

    const result = rawResult as DataCreateResult<TrainingSessionEntity>;
    return result.data!.id;
  }
  /* eslint-enable @typescript-eslint/naming-convention */

  /**
   * Load session from database
   *
   * Returns session data with handle information.
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  private async _loadSession(sessionId: UUID): Promise<{
    providerJobId: string;
    status: string;
    metadata: Record<string, unknown>;
  }> {
    // Import Commands here (server-side only)
    /* eslint-disable @typescript-eslint/naming-convention */
    const { Commands } = await import('../../../../system/core/shared/Commands');
    /* eslint-enable @typescript-eslint/naming-convention */

    const rawResult = await Commands.execute(DATA_COMMANDS.READ, {
      collection: 'training_sessions',
      id: sessionId
    } as never);

    const result = rawResult as DataReadResult<TrainingSessionEntity>;
    return {
      providerJobId: result.data!.providerJobId as string,
      status: result.data!.status as string,
      metadata: (result.data!.metadata ?? {}) as Record<string, unknown>
    };
  }
  /* eslint-enable @typescript-eslint/naming-convention */

  /**
   * Update session in database
   *
   * Updates status and metadata when training progresses.
   */
  /* eslint-disable @typescript-eslint/naming-convention */
  private async _updateSession(
    sessionId: UUID,
    status: TrainingStatus
  ): Promise<void> {
    // Import Commands here (server-side only)
    /* eslint-disable @typescript-eslint/naming-convention */
    const { Commands } = await import('../../../../system/core/shared/Commands');
    /* eslint-enable @typescript-eslint/naming-convention */

    const updateData: Record<string, unknown> = {
      status: status.status,
      metadata: {
        ...status.metadata,
        progress: status.progress
      }
    };

    // Set completion fields if terminal state
    if (status.status === 'completed') {
      updateData.completedAt = Date.now();
      updateData.finalCheckpointPath = status.modelId;
    } else if (status.status === 'failed') {
      updateData.completedAt = Date.now();
      updateData.error = {
        message: status.error ?? 'Unknown error',
        timestamp: Date.now()
      };
    }

    await Commands.execute(DATA_COMMANDS.UPDATE, {
      collection: 'training_sessions',
      id: sessionId,
      data: updateData
    } as never);
  }
  /* eslint-enable @typescript-eslint/naming-convention */
}
