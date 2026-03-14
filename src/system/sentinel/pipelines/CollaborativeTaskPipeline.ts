/**
 * CollaborativeTaskPipeline — AIs do real coding work from chat
 *
 * When a persona is asked to do coding work in chat, this pipeline:
 *   1. CodingAgent — Claude Code implements the task
 *   2. Shell — Verify it compiles (npm run build:ts)
 *   3. Condition — Did it compile?
 *     then: Report success to chat
 *     else: CodingAgent retry with error context → re-verify → report
 *
 * Training data is captured automatically (captureTraining=true).
 * Every task makes the persona better at coding over time.
 *
 * This is the core of the evolving collaborative system:
 * Task → Sentinel → Claude Code → Verify → Training Data → LoRA → Better Next Time
 *
 * Usage from persona tool call:
 *   sentinel/run --type=pipeline --definition=<JSON>
 *
 * Usage from TypeScript:
 *   const pipeline = buildCollaborativeTaskPipeline({ task: "Fix the bug", ... });
 *   await SentinelRun.execute({ type: 'pipeline', definition: JSON.stringify(pipeline) });
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

export interface CollaborativeTaskConfig {
  /** The coding task description */
  task: string;
  /** Persona executing the task */
  personaId: string;
  /** Persona display name (for chat reporting) */
  personaName: string;
  /** Working directory */
  cwd: string;
  /** Room to report results to */
  roomId?: string;
  /** Max budget per attempt in USD */
  maxBudgetUsd?: number;
  /** Max conversation turns */
  maxTurns?: number;
  /** Model to use (default: sonnet) */
  model?: string;
}

/**
 * Build a collaborative task pipeline from configuration.
 *
 * Returns a Pipeline JSON that the Rust sentinel executor runs.
 * The pipeline is self-contained — it executes, verifies, retries, and reports.
 */
export function buildCollaborativeTaskPipeline(config: CollaborativeTaskConfig): Pipeline {
  const {
    task,
    personaId,
    personaName,
    cwd,
    roomId = 'general',
    maxBudgetUsd = 2.0,
    maxTurns = 25,
    model = 'sonnet',
  } = config;

  const taskSummary = task.length > 100 ? task.slice(0, 100) + '...' : task;

  const steps: PipelineStep[] = [
    // Step 0: Execute the coding task via Claude Code
    {
      type: 'codingagent',
      prompt: task,
      model,
      maxTurns,
      maxBudgetUsd,
      permissionMode: 'bypassPermissions',
      captureTraining: true,
      personaId,
      workingDir: cwd,
    },

    // Step 1: Verify compilation
    {
      type: 'shell',
      cmd: 'npm run build:ts 2>&1 | tail -10',
      workingDir: cwd,
      timeoutSecs: 120,
      allowFailure: true,
    },

    // Step 2: Check compilation result, retry or report
    {
      type: 'condition',
      if: '{{steps[1].exitCode}} == 0',
      then: [
        // Success — report to chat
        {
          type: 'command',
          command: 'collaboration/chat/send',
          params: {
            room: roomId,
            message: `Done: ${taskSummary}\n\nCompilation passed. Training data captured.`,
          },
        },
      ],
      else: [
        // Compilation failed — retry with error context
        {
          type: 'codingagent',
          prompt: `The previous change failed to compile. Fix these errors:\n\n{{steps[1].stdout}}\n\nOriginal task: ${task}`,
          model,
          maxTurns: 15,
          maxBudgetUsd: 1.0,
          permissionMode: 'bypassPermissions',
          captureTraining: true,
          personaId,
          workingDir: cwd,
        },

        // Re-verify
        {
          type: 'shell',
          cmd: 'npm run build:ts 2>&1 | tail -10',
          workingDir: cwd,
          timeoutSecs: 120,
          allowFailure: true,
        },

        // Report final result
        {
          type: 'command',
          command: 'collaboration/chat/send',
          params: {
            room: roomId,
            message: `Task: ${taskSummary}\n\nRetried after compilation failure. Training data captured.`,
          },
        },
      ],
    },
  ];

  return {
    name: `${personaName}: ${taskSummary}`,
    steps,
    workingDir: cwd,
    inputs: {
      task,
      personaId,
      personaName,
      roomId,
    },
  };
}
