/**
 * DemoPipeline — Sentinel pipeline where Claude Code builds real software
 *
 * This pipeline proves end-to-end: Claude Code builds a real project milestone by
 * milestone, every interaction is captured as training data, then LoRA-trains a
 * local persona and validates improvement.
 *
 * Step flow:
 *   0: Shell — Create temp working dir, copy scaffold, npm install
 *   1: Loop (milestones):
 *     loop.0: Shell — Read milestone spec from project.json
 *     loop.1: CodingAgent — Build the feature (captureTraining=true)
 *     loop.2: Shell — Run deterministic tests
 *     loop.3: Condition — Tests passed?
 *       then: Emit milestone:passed
 *       else: CodingAgent retry with test output as feedback → re-run tests
 *     loop.4: Emit milestone:complete {index, passed, attempts}
 *   2: Command — genome/training-export (accumulator → JSONL file)
 *   3: Command — genome/train (JSONL → LoRA adapter)
 *   4: Emit — demo:complete {layerId, examples, loss}
 *
 * Key design decisions:
 * - Uses `codingagent` step type (Rust-native, wraps Claude Code SDK)
 * - Tests are deterministic shell steps (no LLM grading bias)
 * - Retry uses condition + nested codingagent (test failures as context)
 * - Training happens AFTER all milestones (batch, not per-milestone)
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { DemoPipelineConfig } from './DemoTypes';
import { demoEvent } from './DemoTypes';

/**
 * Build a demo pipeline from a project configuration.
 *
 * The returned Pipeline is JSON-serializable and dispatched to the Rust
 * sentinel executor via `sentinel/run` with type='pipeline'.
 */
export function buildDemoPipeline(config: DemoPipelineConfig): Pipeline {
  const {
    projectDir,
    project,
    personaId,
    personaName,
    baseModel,
    maxRetries,
    maxBudgetPerMilestone,
    maxTurnsPerMilestone,
    provider,
    training,
  } = config;

  // Unique run ID for event scoping (derived from persona + timestamp at build time)
  const runId = `${personaId.slice(0, 8)}-${Date.now().toString(36)}`;

  const steps: PipelineStep[] = [
    // Step 0: Create working directory from scaffold, install dependencies
    buildSetupStep(projectDir),

    // Step 1: Milestone loop — CodingAgent builds each feature, tests verify
    {
      type: 'loop',
      count: project.milestones.length,
      steps: buildMilestoneLoopSteps(config, runId),
    },

    // Step 2: Export captured interactions from accumulator to JSONL file
    {
      type: 'command',
      command: 'genome/training-export',
      params: {
        personaId,
        personaName,
        domain: 'coding',
      },
    },

    // Step 3: Train LoRA on exported dataset
    {
      type: 'command',
      command: 'genome/train',
      params: {
        personaId,
        personaName,
        traitType: `demo-${project.name}-${runId}`,
        datasetPath: '{{steps.2.data.datasetPath}}',
        baseModel,
        domain: `coding-${project.skill}`,
        rank: training.rank,
        epochs: training.epochs,
        learningRate: training.learningRate,
        batchSize: training.batchSize,
      },
    },

    // Step 4: Emit demo:complete with aggregate results
    {
      type: 'emit',
      event: demoEvent(runId, 'demo:complete'),
      payload: {
        runId,
        projectName: project.name,
        milestonesTotal: project.milestones.length,
        personaId,
        personaName,
        trainingLayerId: '{{steps.3.data.layerId}}',
        trainingExamples: '{{steps.2.data.exampleCount}}',
        trainingLoss: '{{steps.3.data.metrics.finalLoss}}',
      },
    },
  ];

  return {
    name: `demo-${project.name}-${personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    steps,
    workingDir: projectDir,
    timeoutSecs: 3600, // 1 hour max for full demo
    inputs: {
      runId,
      projectDir,
      projectName: project.name,
      personaId,
      personaName,
      baseModel,
      provider,
    },
  };
}

/**
 * Step 0: Create temp working directory, copy scaffold + tests, install deps.
 */
function buildSetupStep(projectDir: string): PipelineStep {
  return {
    type: 'shell',
    cmd: [
      `WORKDIR=$(mktemp -d)`,
      // Info goes to stderr — stdout is ONLY the path (used by {{steps.0.output}})
      `echo "Setting up working directory: $WORKDIR" >&2`,
      // Copy scaffold (provides package.json, tsconfig.json)
      `cp -r "${projectDir}/scaffold/"* "$WORKDIR/"`,
      // Copy test files
      `mkdir -p "$WORKDIR/tests"`,
      `cp -r "${projectDir}/tests/"* "$WORKDIR/tests/"`,
      // Install dependencies (output to stderr to keep stdout clean)
      `cd "$WORKDIR"`,
      `npm install --silent 1>&2 2>&1`,
      // Output ONLY the working directory path (no trailing newline issues)
      `printf '%s' "$WORKDIR"`,
    ].join('\n'),
    timeoutSecs: 120,
  };
}

/**
 * Build the per-milestone loop steps.
 *
 * Each iteration:
 *   0: Shell — Read milestone spec from project.json
 *   1: CodingAgent — Build the feature (first attempt)
 *   2: Shell — Run tests
 *   3: Condition — passed? → emit passed, else → retry
 *   4: Emit — milestone:complete
 */
function buildMilestoneLoopSteps(config: DemoPipelineConfig, runId: string): PipelineStep[] {
  const {
    projectDir,
    project,
    personaId,
    maxRetries,
    maxBudgetPerMilestone,
    maxTurnsPerMilestone,
    provider,
  } = config;

  return [
    // loop.0: Read the milestone spec (parsed by the CodingAgent as context)
    {
      type: 'shell',
      cmd: [
        `MILESTONE_IDX={{input.iteration}}`,
        // Extract this milestone's spec as JSON for the CodingAgent prompt
        `node -e "`,
        `const fs = require('fs');`,
        `const proj = JSON.parse(fs.readFileSync('${projectDir}/project.json', 'utf8'));`,
        `const m = proj.milestones[$MILESTONE_IDX];`,
        `console.log(JSON.stringify(m, null, 2));`,
        `"`,
      ].join('\n'),
      timeoutSecs: 10,
    },

    // loop.1: CodingAgent — Claude Code builds this milestone
    buildCodingAgentStep({
      prompt: buildFirstAttemptPrompt(project.name),
      provider,
      personaId,
      maxTurns: maxTurnsPerMilestone,
      maxBudget: maxBudgetPerMilestone,
    }),

    // loop.2: Shell — Run the milestone's deterministic tests
    buildTestRunnerStep(projectDir),

    // loop.3: Condition — did tests pass? (exit code 0 = pass)
    {
      type: 'condition',
      if: '{{loop.2.exitCode == 0}}',
      then: [
        // Tests passed on first try
        {
          type: 'emit',
          event: demoEvent(runId, 'milestone:passed'),
          payload: {
            milestoneIndex: '{{input.iteration}}',
            attempts: 1,
            testOutput: '{{loop.2.output}}',
          },
        },
      ],
      else: buildRetrySteps(config, runId),
    },

    // loop.4: Emit milestone:complete (always runs)
    {
      type: 'emit',
      event: demoEvent(runId, 'milestone:complete'),
      payload: {
        milestoneIndex: '{{input.iteration}}',
        runId,
      },
    },
  ];
}

/**
 * Build CodingAgent retry steps when first attempt fails.
 *
 * Structure: CodingAgent (with test failures as context) → Shell (re-run tests) → Condition
 * This is a fixed retry (not a loop) since we use condition nesting.
 * For maxRetries > 1, we nest additional retry layers.
 */
function buildRetrySteps(config: DemoPipelineConfig, runId: string): PipelineStep[] {
  const {
    projectDir,
    project,
    personaId,
    maxBudgetPerMilestone,
    maxTurnsPerMilestone,
    provider,
  } = config;

  // Retry 1: CodingAgent with test failure context → re-run tests → check
  const retry1Steps: PipelineStep[] = [
    // CodingAgent retry with test failures as feedback
    buildCodingAgentStep({
      prompt: buildRetryPrompt(project.name, 1),
      provider,
      personaId,
      maxTurns: maxTurnsPerMilestone,
      maxBudget: maxBudgetPerMilestone,
    }),

    // Re-run tests
    buildTestRunnerStep(projectDir),

    // Check again
    {
      type: 'condition',
      if: '{{steps[-1].exitCode == 0}}',
      then: [
        {
          type: 'emit',
          event: demoEvent(runId, 'milestone:passed'),
          payload: {
            milestoneIndex: '{{input.iteration}}',
            attempts: 2,
            testOutput: '{{steps[-1].output}}',
          },
        },
      ],
      else: buildRetry2Steps(config, runId),
    },
  ];

  return retry1Steps;
}

/**
 * Build retry 2 steps (final attempt).
 */
function buildRetry2Steps(config: DemoPipelineConfig, runId: string): PipelineStep[] {
  const {
    projectDir,
    project,
    personaId,
    maxRetries,
    maxBudgetPerMilestone,
    maxTurnsPerMilestone,
    provider,
  } = config;

  // If maxRetries < 2, just emit failure
  if (maxRetries < 2) {
    return [
      {
        type: 'emit',
        event: demoEvent(runId, 'milestone:failed'),
        payload: {
          milestoneIndex: '{{input.iteration}}',
          attempts: 2,
        },
      },
    ];
  }

  return [
    // Final CodingAgent attempt
    buildCodingAgentStep({
      prompt: buildRetryPrompt(project.name, 2),
      provider,
      personaId,
      maxTurns: maxTurnsPerMilestone,
      maxBudget: maxBudgetPerMilestone,
    }),

    // Final test run
    buildTestRunnerStep(projectDir),

    // Final check
    {
      type: 'condition',
      if: '{{steps[-1].exitCode == 0}}',
      then: [
        {
          type: 'emit',
          event: demoEvent(runId, 'milestone:passed'),
          payload: {
            milestoneIndex: '{{input.iteration}}',
            attempts: 3,
            testOutput: '{{steps[-1].output}}',
          },
        },
      ],
      else: [
        {
          type: 'emit',
          event: demoEvent(runId, 'milestone:failed'),
          payload: {
            milestoneIndex: '{{input.iteration}}',
            attempts: 3,
          },
        },
      ],
    },
  ];
}

/**
 * Build a CodingAgent step for a milestone.
 */
function buildCodingAgentStep(opts: {
  prompt: string;
  provider: string;
  personaId: string;
  maxTurns: number;
  maxBudget: number;
}): PipelineStep {
  return {
    type: 'codingagent',
    prompt: opts.prompt,
    provider: opts.provider,
    // workingDir from {{steps.0.output}} (setup step outputs only the path)
    workingDir: '{{steps.0.output}}',
    maxTurns: opts.maxTurns,
    maxBudgetUsd: opts.maxBudget,
    captureTraining: true,
    personaId: opts.personaId,
    permissionMode: 'bypassPermissions',
  };
}

/**
 * Build a shell step that runs the current milestone's test file.
 */
function buildTestRunnerStep(projectDir: string): PipelineStep {
  return {
    type: 'shell',
    cmd: [
      // {{steps.0.output}} is now a clean path (no extra lines)
      `cd "{{steps.0.output}}"`,
      `MILESTONE_IDX={{input.iteration}}`,
      // Read the test command from project.json
      `TEST_CMD=$(node -e "`,
      `const fs = require('fs');`,
      `const proj = JSON.parse(fs.readFileSync('${projectDir}/project.json', 'utf8'));`,
      `const m = proj.milestones[$MILESTONE_IDX];`,
      `console.log(m.testCommand || 'npx tsx ' + m.testFile);`,
      `")`,
      `echo "Running: $TEST_CMD"`,
      `$TEST_CMD 2>&1`,
    ].join('\n'),
    timeoutSecs: 60,
    // allowFailure: test failures should be handled by the condition step, not kill the loop
    allowFailure: true,
  };
}

/**
 * Build the CodingAgent prompt for a first attempt at a milestone.
 */
function buildFirstAttemptPrompt(projectName: string): string {
  return [
    `You are building a ${projectName} project step by step.`,
    '',
    '=== CURRENT MILESTONE ===',
    '{{loop.0.output}}',
    '',
    'Your task:',
    '1. Read the existing code in the working directory to understand current state',
    '2. Implement the requirements described in the milestone above',
    '3. Build on existing code — do NOT rewrite files that already work',
    '4. After writing code, run the milestone tests to verify:',
    '   {{loop.0.output.testCommand}}',
    '',
    'IMPORTANT:',
    '- The project scaffold already has package.json and dependencies installed',
    '- Tests are in the tests/ directory and are pre-written — do NOT modify them',
    '- If tests reference src/index.ts exporting an `app`, make sure your Express app is exported',
    '- All code goes in the src/ directory',
    '- Preserve working functionality from previous milestones',
  ].join('\n');
}

/**
 * Build the CodingAgent prompt for a retry attempt.
 */
function buildRetryPrompt(projectName: string, retryNumber: number): string {
  return [
    `You are building a ${projectName} project. This is retry attempt #${retryNumber}.`,
    '',
    '=== MILESTONE REQUIREMENTS ===',
    '{{loop.0.output}}',
    '',
    '=== PREVIOUS TEST FAILURES ===',
    '{{steps[-1].output}}',
    '',
    'The tests above FAILED. Fix the implementation:',
    '1. Read the failing test output carefully',
    '2. Identify what went wrong (missing routes, wrong status codes, bad response format, etc.)',
    '3. Fix the code to make ALL tests pass',
    '4. Run the tests again to verify your fix',
    '',
    'IMPORTANT:',
    '- Do NOT modify the test files in tests/',
    '- Focus on fixing the implementation in src/',
    '- Make sure previous milestone tests still pass',
  ].join('\n');
}
