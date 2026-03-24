/**
 * TrainingStepBridge — Parses per-step training metrics from peft-train.py stdout
 *
 * Real-time path: sentinel captures stdout → this bridge parses → emits Events → widgets update
 * Persistence: peft-train.py writes TensorBoard tfevents via `report_to="tensorboard"`
 *
 * This bridge does NOT write to the database or to disk.
 * Step data persistence is handled by the Trainer itself (tfevents in output_dir/runs/).
 * This bridge is purely for real-time event emission.
 *
 * JSON formats from peft-train.py StepMetricsCallback:
 *   Step:       {"event":"step","step":42,"loss":0.234,"lr":0.0001,"epoch":1.5,"memMb":3200}
 *   Checkpoint: {"event":"checkpoint","step":4700,"path":"/path/to/checkpoint-4700","sizeMb":1200}
 */

import { Events } from '../../core/shared/Events';
import {
  AI_LEARNING_EVENTS,
  type AITrainingStepEventData,
} from '../../events/shared/AILearningEvents';

// ── JSON shapes from peft-train.py ──────────────────────────────────────

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

interface PeftCheckpointJson {
  event: 'checkpoint';
  step: number;
  path: string;
  sizeMb?: number;
}

type PeftJsonLine = PeftStepJson | PeftCheckpointJson;

// ── Context ─────────────────────────────────────────────────────────────

interface TrainingContext {
  personaId: string;
  personaName: string;
  domain: string;
}

const activeContexts = new Map<string, TrainingContext>();

// ── Public API ──────────────────────────────────────────────────────────

export function registerTrainingStepContext(handle: string, ctx: TrainingContext): void {
  activeContexts.set(handle, ctx);
  console.log(`[TrainingStepBridge] Registered ${handle}: ${ctx.personaName}/${ctx.domain}`);
}

export function unregisterTrainingStepContext(handle: string): void {
  activeContexts.delete(handle);
}

// ── Parsing ─────────────────────────────────────────────────────────────

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

export const parseStepLine = parseStdoutLine;

// ── Processing ──────────────────────────────────────────────────────────

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
}

function processCheckpoint(handle: string, checkpoint: PeftCheckpointJson): void {
  const ctx = activeContexts.get(handle);
  console.log(`[TrainingStepBridge] Checkpoint: step=${checkpoint.step}, path=${checkpoint.path}`);

  Events.emit(AI_LEARNING_EVENTS.TRAINING_STEP, {
    personaId: ctx?.personaId ?? 'unknown',
    personaName: ctx?.personaName ?? 'Unknown',
    domain: ctx?.domain ?? 'unknown',
    timestamp: Date.now(),
    step: checkpoint.step,
    loss: 0,
    learningRate: 0,
  });
}

// ── Initialization ──────────────────────────────────────────────────────

export function initializeTrainingStepBridge(): void {
  Events.subscribe('sentinel:stdout', (payload: { handle: string; line: string }) => {
    processTrainingStdoutLine(payload.handle, payload.line);
  });

  Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, (data: { personaId: string }) => {
    for (const [handle, ctx] of activeContexts) {
      if (ctx.personaId === data.personaId) {
        unregisterTrainingStepContext(handle);
        break;
      }
    }
  });

  Events.subscribe(AI_LEARNING_EVENTS.TRAINING_ERROR, (data: { personaId: string }) => {
    for (const [handle, ctx] of activeContexts) {
      if (ctx.personaId === data.personaId) {
        unregisterTrainingStepContext(handle);
        break;
      }
    }
  });

  console.log('[TrainingStepBridge] Initialized — events only, persistence via TensorBoard tfevents');
}
