/**
 * TrainingStepBridge — Parses per-step training metrics from peft-train.py stdout
 *
 * Sentinel captures stdout from the training process. This bridge:
 *   1. Subscribes to sentinel stdout events for training-type sentinels
 *   2. Parses structured JSON lines ({"event":"step"|"checkpoint", ...})
 *   3. Re-emits as AI_LEARNING_EVENTS for widget consumption
 *   4. Persists progress to TrainingJobEntity in database (crash-safe)
 *
 * Zero polling — entirely event-driven from stdout capture.
 *
 * JSON formats from peft-train.py:
 *   Step:       {"event":"step","step":42,"loss":0.234,"lr":0.0001,"epoch":1.5,"memMb":3200}
 *   Checkpoint: {"event":"checkpoint","step":4700,"path":"/path/to/checkpoint-4700","sizeMb":1200}
 */

import { Events } from '../../core/shared/Events';
import {
  AI_LEARNING_EVENTS,
  type AITrainingStepEventData,
} from '../../events/shared/AILearningEvents';
import { DataUpdate } from '../../../commands/data/update/shared/DataUpdateTypes';
import { TrainingJobEntity } from '../entities/TrainingJobEntity';

// ── JSON shapes from peft-train.py ──────────────────────────────────────

/** Step metrics line */
interface PeftStepJson {
  event: 'step';
  step: number;
  loss: number;
  lr: number;
  tokenAccuracy?: number;
  memMb?: number;
  epoch?: number;
  gradNorm?: number;
}

/** Checkpoint saved line */
interface PeftCheckpointJson {
  event: 'checkpoint';
  step: number;
  path: string;
  sizeMb?: number;
}

type PeftJsonLine = PeftStepJson | PeftCheckpointJson;

// ── Context ─────────────────────────────────────────────────────────────

/** Context needed to enrich step events with persona info */
interface TrainingContext {
  personaId: string;
  personaName: string;
  domain: string;
  jobId?: string; // TrainingJobEntity.id for DB persistence
}

/**
 * Active training contexts keyed by sentinel handle.
 * Also backed by TrainingJobEntity in database for crash recovery.
 */
const activeContexts = new Map<string, TrainingContext>();

/** Throttle DB writes — persist every N steps (avoid DB thrash on fast training) */
const DB_PERSIST_INTERVAL = 10;
const lastPersistStep = new Map<string, number>();

// ── Public API ──────────────────────────────────────────────────────────

/**
 * Register a training context so step events can be enriched with persona info.
 * Called by genome/train when a training process starts.
 */
export function registerTrainingStepContext(handle: string, ctx: TrainingContext): void {
  activeContexts.set(handle, ctx);
  lastPersistStep.set(handle, 0);
  console.log(`[TrainingStepBridge] Registered context for ${handle}: ${ctx.personaName}/${ctx.domain} (jobId=${ctx.jobId ?? 'none'})`);
}

/**
 * Unregister a training context when training completes or errors.
 */
export function unregisterTrainingStepContext(handle: string): void {
  activeContexts.delete(handle);
  lastPersistStep.delete(handle);
}

// ── Parsing ─────────────────────────────────────────────────────────────

/**
 * Parse a single stdout line. Returns parsed data or null if not a recognized event.
 */
export function parseStdoutLine(line: string): PeftJsonLine | null {
  const trimmed = line.trim();
  if (!trimmed.startsWith('{')) return null;

  try {
    const parsed = JSON.parse(trimmed);
    if (parsed.event === 'step' && typeof parsed.step === 'number' && typeof parsed.loss === 'number') {
      return parsed as PeftStepJson;
    }
    if (parsed.event === 'checkpoint' && typeof parsed.step === 'number' && typeof parsed.path === 'string') {
      return parsed as PeftCheckpointJson;
    }
    return null;
  } catch {
    return null;
  }
}

// Keep old name as alias for backward compat
export const parseStepLine = parseStdoutLine;

// ── Processing ──────────────────────────────────────────────────────────

/**
 * Process a stdout line from a training sentinel.
 * Handles both step metrics and checkpoint events.
 */
export function processTrainingStdoutLine(handle: string, line: string): void {
  const parsed = parseStdoutLine(line);
  if (!parsed) return;

  if (parsed.event === 'step') {
    processStep(handle, parsed);
  } else if (parsed.event === 'checkpoint') {
    processCheckpoint(handle, parsed);
  }
}

function processStep(handle: string, step: PeftStepJson): void {
  const ctx = activeContexts.get(handle);

  // Emit event (widgets consume this for real-time charting)
  const eventData: AITrainingStepEventData = {
    personaId: ctx?.personaId ?? 'unknown',
    personaName: ctx?.personaName ?? 'Unknown',
    domain: ctx?.domain ?? 'unknown',
    timestamp: Date.now(),
    step: step.step,
    loss: step.loss,
    learningRate: step.lr,
    tokenAccuracy: step.tokenAccuracy,
    memoryMb: step.memMb,
    epoch: step.epoch,
    gradNorm: step.gradNorm,
  };
  Events.emit(AI_LEARNING_EVENTS.TRAINING_STEP, eventData);

  // Persist to DB (throttled — every N steps to avoid thrash)
  if (ctx?.jobId) {
    const lastStep = lastPersistStep.get(handle) ?? 0;
    if (step.step - lastStep >= DB_PERSIST_INTERVAL || step.step <= 1) {
      lastPersistStep.set(handle, step.step);
      persistStepProgress(ctx.jobId, step).catch(err => {
        console.error(`[TrainingStepBridge] DB persist failed for step ${step.step}:`, err);
      });
    }
  }
}

function processCheckpoint(handle: string, checkpoint: PeftCheckpointJson): void {
  const ctx = activeContexts.get(handle);
  console.log(`[TrainingStepBridge] Checkpoint saved: step=${checkpoint.step}, path=${checkpoint.path}`);

  // Emit checkpoint event
  Events.emit(AI_LEARNING_EVENTS.TRAINING_STEP, {
    personaId: ctx?.personaId ?? 'unknown',
    personaName: ctx?.personaName ?? 'Unknown',
    domain: ctx?.domain ?? 'unknown',
    timestamp: Date.now(),
    step: checkpoint.step,
    loss: 0, // Checkpoint doesn't carry loss
    learningRate: 0,
  });

  // Persist checkpoint to DB (always — checkpoints are infrequent and critical)
  if (ctx?.jobId) {
    persistCheckpoint(ctx.jobId, checkpoint).catch(err => {
      console.error(`[TrainingStepBridge] Checkpoint persist failed:`, err);
    });
  }
}

// ── DB Persistence ──────────────────────────────────────────────────────

async function persistStepProgress(jobId: string, step: PeftStepJson): Promise<void> {
  try {
    await DataUpdate.execute({
      collection: TrainingJobEntity.collection,
      id: jobId,
      data: {
        currentStep: step.step,
        currentLoss: step.loss,
        currentLearningRate: step.lr,
        currentEpoch: step.epoch ?? 0,
        // lossHistory is appended in-memory by TrainingJobEntity.recordStep(),
        // but for DB we batch it via the full entity update at completion.
        // Per-step DB writes only update scalars for crash recovery.
      },
    });
  } catch (err) {
    // Non-fatal — step progress is best-effort. The next step will try again.
    console.warn(`[TrainingStepBridge] Step persist failed (non-fatal): ${err}`);
  }
}

async function persistCheckpoint(jobId: string, checkpoint: PeftCheckpointJson): Promise<void> {
  try {
    await DataUpdate.execute({
      collection: TrainingJobEntity.collection,
      id: jobId,
      data: {
        status: 'checkpointed',
        latestCheckpointPath: checkpoint.path,
        latestCheckpointStep: checkpoint.step,
        latestCheckpointAt: new Date(Date.now()),
      },
    });
  } catch (err) {
    console.error(`[TrainingStepBridge] Checkpoint persist failed: ${err}`);
  }
}

// ── Initialization ──────────────────────────────────────────────────────

/**
 * Initialize the TrainingStepBridge.
 * Subscribes to sentinel stdout events and filters for training step metrics.
 */
export function initializeTrainingStepBridge(): void {
  // Sentinel emits 'sentinel:stdout' for each captured line
  Events.subscribe('sentinel:stdout', (payload: { handle: string; line: string }) => {
    processTrainingStdoutLine(payload.handle, payload.line);
  });

  // Clean up context when training completes or errors
  Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, (data: { personaId: string }) => {
    for (const [handle, ctx] of activeContexts) {
      if (ctx.personaId === data.personaId) {
        activeContexts.delete(handle);
        lastPersistStep.delete(handle);
        break;
      }
    }
  });

  Events.subscribe(AI_LEARNING_EVENTS.TRAINING_ERROR, (data: { personaId: string }) => {
    for (const [handle, ctx] of activeContexts) {
      if (ctx.personaId === data.personaId) {
        activeContexts.delete(handle);
        lastPersistStep.delete(handle);
        break;
      }
    }
  });

  console.log('[TrainingStepBridge] Initialized — listening for sentinel:stdout with DB persistence');
}
