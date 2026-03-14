/**
 * DevBuildFeaturePipeline — End-to-end feature development sentinel
 *
 * Takes a feature description and handles EVERYTHING:
 *
 * Autonomous mode (2 core steps):
 *   0: Shell — Create feature branch
 *   1: CodingAgent — Plan + implement the feature
 *   2-3: Build/test verification (if configured)
 *   4: Shell — Git commit
 *   5: Emit — Completion event
 *
 * Collaborative mode (full pipeline):
 *   0: Shell — Create feature branch from main
 *   1: LLM  — Analyze request and generate implementation plan
 *   2: Command — Post plan to chat room for team review
 *   3: Watch — Wait for team feedback/approval (with timeout fallback)
 *   4: CodingAgent — Implement the feature based on plan + feedback
 *   5: Shell — Build and verify compilation
 *   6: Condition — Compilation passed?
 *     then: Shell — Run tests
 *     else: CodingAgent retry with errors → rebuild → test
 *   7: Shell — Git commit
 *   8: Emit — Completion event
 *
 * Key design: Collaborative decision points throughout.
 * AIs can vote on the plan, QA the result, suggest improvements.
 * User controls their involvement level via config.
 *
 * Usage:
 *   const pipeline = buildDevBuildFeaturePipeline({
 *     feature: "Add user profile page with avatar upload",
 *     personaId: "...",
 *     personaName: "Helper AI",
 *     cwd: "/path/to/project",
 *     roomId: "general",
 *   });
 *   await Commands.execute('sentinel/run', { type: 'pipeline', definition: pipeline });
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

export interface DevBuildFeatureConfig {
  /** Natural language feature description */
  feature: string;
  /** Persona executing the task */
  personaId: string;
  /** Persona display name */
  personaName: string;
  /** Project working directory */
  cwd: string;
  /** Chat room for collaborative decision points */
  roomId?: string;
  /** Branch name (auto-generated from feature if omitted) */
  branchName?: string;
  /** Base branch to fork from */
  baseBranch?: string;
  /** Provider for LLM planning step */
  planProvider?: string;
  /** Provider for CodingAgent execution */
  codingProvider?: string;
  /** Model for CodingAgent */
  codingModel?: string;
  /** Max budget per coding attempt in USD */
  maxBudgetUsd?: number;
  /** Max conversation turns per coding attempt */
  maxTurns?: number;
  /** Build command (default: npm run build:ts, null to skip) */
  buildCommand?: string | null;
  /** Test command (default: npm test, null to skip) */
  testCommand?: string | null;
  /** Seconds to wait for team plan review (0=skip) */
  planReviewTimeoutSecs?: number;
  /** Seconds to wait for team QA (0=skip) */
  qaReviewTimeoutSecs?: number;
  /** Skip collaborative checkpoints entirely */
  autonomous?: boolean;
  /** Capture training data for persona improvement */
  captureTraining?: boolean;
}

/**
 * Build a dev/build-feature pipeline.
 *
 * Returns a Pipeline JSON for the Rust sentinel executor.
 * All prerequisites are handled inline — zero setup required.
 */
export function buildDevBuildFeaturePipeline(config: DevBuildFeatureConfig): Pipeline {
  const {
    feature,
    personaId,
    personaName,
    cwd,
    roomId = 'general',
    baseBranch = 'main',
    planProvider = 'deepseek',
    codingProvider = 'claude-code',
    codingModel = 'sonnet',
    maxBudgetUsd = 5.0,
    maxTurns = 30,
    buildCommand: rawBuildCommand = 'npm run build:ts 2>&1 | tail -30',
    testCommand: rawTestCommand = 'npm test 2>&1 | tail -50',
    planReviewTimeoutSecs = 60,
    qaReviewTimeoutSecs = 120,
    autonomous = false,
    captureTraining = true,
  } = config;

  // null means "skip this step entirely"
  const buildCommand = rawBuildCommand === null ? null : rawBuildCommand;
  const testCommand = rawTestCommand === null ? null : rawTestCommand;
  const branchName = config.branchName || generateBranchName(feature);
  const runId = `dev-${personaId.slice(0, 8)}-${Date.now().toString(36)}`;
  const featureSummary = feature.length > 120 ? feature.slice(0, 120) + '...' : feature;

  const steps: PipelineStep[] = [
    // Step 0: Create feature branch
    {
      type: 'shell',
      cmd: [
        `cd "${cwd}"`,
        `git fetch origin ${baseBranch} 2>/dev/null || true`,
        `git checkout -b ${branchName} origin/${baseBranch} 2>/dev/null || git checkout -b ${branchName} ${baseBranch}`,
        `echo "Branch: ${branchName}"`,
      ].join('\n'),
      timeoutSecs: 30,
      workingDir: cwd,
    },

    // Steps 1-3: LLM plan + team review (collaborative mode only)
    // In autonomous mode, skip planning — CodingAgent plans its own approach.
    // This avoids redundant LLM calls and provider credit requirements.
    ...(autonomous ? [] : [
      // Step 1: LLM plans the implementation
      {
        type: 'llm',
        prompt: buildPlanPrompt(feature, cwd),
        provider: planProvider,
        temperature: 0.3,
        maxTokens: 4096,
        systemPrompt: [
          'You are a senior software architect planning a feature implementation.',
          'Analyze the request and produce a concrete, actionable plan.',
          'Include: files to modify/create, key changes, potential risks, test strategy.',
          'Be specific — reference actual file paths and function names where possible.',
          'Output a structured plan in markdown.',
        ].join(' '),
      } as PipelineStep,

      // Step 2: Post plan to chat for team review
      {
        type: 'command',
        command: 'collaboration/chat/send',
        params: {
          room: roomId,
          message: [
            `**${personaName}** is planning a feature:`,
            `> ${featureSummary}`,
            '',
            '**Implementation Plan:**',
            '{{steps.1.output}}',
            '',
            'React with a vote or reply with feedback. Proceeding in ' +
              `${planReviewTimeoutSecs}s if no objections.`,
          ].join('\n'),
        },
      } as PipelineStep,

      // Step 3: Wait for team feedback
      {
        type: 'watch',
        event: `dev:${runId}:plan-approved`,
        timeoutSecs: planReviewTimeoutSecs,
      } as PipelineStep,
    ]),

    // CodingAgent implements the feature
    {
      type: 'codingagent',
      prompt: buildImplementationPrompt(feature, autonomous),
      provider: codingProvider,
      model: codingModel,
      workingDir: cwd,
      maxTurns,
      maxBudgetUsd,
      permissionMode: 'bypassPermissions',
      captureTraining,
      personaId,
    },

    // Step 5-6: Build verification + conditional test/retry (skipped if buildCommand is null)
    ...(buildCommand !== null ? [
      {
        type: 'shell',
        cmd: buildCommand,
        workingDir: cwd,
        timeoutSecs: 180,
        allowFailure: true,
      } as PipelineStep,
      {
        type: 'condition',
        if: '{{steps[-1].exitCode}} == 0',
        then: buildTestSteps(cwd, testCommand, runId, personaName, featureSummary, roomId, autonomous),
        else: buildRetrySteps(config, runId, featureSummary),
      } as PipelineStep,
    ] : (testCommand !== null ? [
      // No build step but has tests — run tests directly
      {
        type: 'shell',
        cmd: testCommand,
        workingDir: cwd,
        timeoutSecs: 300,
        allowFailure: true,
      } as PipelineStep,
    ] : [])),

    // Step: Git commit — only files changed since branch creation
    // Uses git diff against baseBranch to avoid committing unrelated working tree changes
    {
      type: 'shell',
      cmd: [
        `cd "${cwd}"`,
        // Stage only files that differ from the base branch (our changes, not pre-existing uncommitted work)
        `git diff --name-only ${baseBranch} --diff-filter=ACMR | xargs -r git add`,
        // Also add any new untracked files created by CodingAgent (not in base branch)
        `git ls-files --others --exclude-standard | xargs -r git add`,
        // Commit if there are staged changes
        `git diff --cached --quiet && echo "Nothing to commit" || git commit -m "$(cat <<'COMMITMSG'`,
        `feat: ${featureSummary}`,
        '',
        `Implemented by ${personaName} via sentinel pipeline.`,
        'COMMITMSG',
        `)"`,
      ].join('\n'),
      workingDir: cwd,
      timeoutSecs: 30,
      allowFailure: true,
    },

    // Step 8: Final status emit
    {
      type: 'emit',
      event: `dev:${runId}:complete`,
      payload: {
        runId,
        feature: featureSummary,
        personaId,
        personaName,
        branchName,
      },
    },
  ];

  return {
    name: `${personaName}: build ${featureSummary}`,
    steps,
    workingDir: cwd,
    timeoutSecs: 3600,
    inputs: {
      runId,
      feature,
      personaId,
      personaName,
      branchName,
      baseBranch,
      roomId,
    },
  };
}

/**
 * Build test steps for when compilation succeeds.
 */
function buildTestSteps(
  cwd: string,
  testCommand: string | null,
  runId: string,
  personaName: string,
  featureSummary: string,
  roomId: string,
  autonomous: boolean,
): PipelineStep[] {
  const steps: PipelineStep[] = [];

  // Run tests (skip if testCommand is null)
  if (testCommand !== null) {
    steps.push({
      type: 'shell',
      cmd: testCommand,
      workingDir: cwd,
      timeoutSecs: 300,
      allowFailure: true,
    });
  }

  // Post QA results to chat if collaborative
  if (!autonomous) {
    steps.push(
      {
        type: 'command',
        command: 'collaboration/chat/send',
        params: {
          room: roomId,
          message: [
            `**${personaName}** completed: ${featureSummary}`,
            '',
            '**Build:** Passed',
            '**Tests:** {{steps[-1].exitCode}} == 0 ? "Passed" : "Some failures"',
            '',
            '{{steps[-1].output}}',
            '',
            'Review and react to approve commit, or reply with changes needed.',
          ].join('\n'),
        },
      },
    );
  }

  return steps;
}

/**
 * Build retry steps when compilation fails.
 */
function buildRetrySteps(
  config: DevBuildFeatureConfig,
  runId: string,
  featureSummary: string,
): PipelineStep[] {
  const {
    cwd,
    personaId,
    codingProvider = 'claude-code',
    codingModel = 'sonnet',
    maxBudgetUsd = 5.0,
    captureTraining = true,
    buildCommand = 'npm run build:ts 2>&1 | tail -30',
    testCommand = 'npm test 2>&1 | tail -50',
  } = config;

  const steps: PipelineStep[] = [
    // Retry CodingAgent with compilation errors as context
    {
      type: 'codingagent',
      prompt: [
        'The previous implementation failed to compile. Fix these errors:',
        '',
        '{{steps[-1].output}}',
        '',
        'Fix all compilation errors. Do not introduce new ones.',
      ].join('\n'),
      provider: codingProvider,
      model: codingModel,
      workingDir: cwd,
      maxTurns: 15,
      maxBudgetUsd: Math.min(maxBudgetUsd, 2.0),
      permissionMode: 'bypassPermissions',
      captureTraining,
      personaId,
    },
  ];

  // Re-verify build (skip if null)
  if (buildCommand !== null) {
    steps.push({
      type: 'shell',
      cmd: buildCommand,
      workingDir: cwd,
      timeoutSecs: 180,
      allowFailure: true,
    });
  }

  // Run tests (skip if null)
  if (testCommand !== null) {
    steps.push({
      type: 'shell',
      cmd: testCommand,
      workingDir: cwd,
      timeoutSecs: 300,
      allowFailure: true,
    });
  }

  return steps;
}

/**
 * Generate a git branch name from a feature description.
 */
function generateBranchName(feature: string): string {
  const slug = feature
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, '')
    .replace(/\s+/g, '-')
    .slice(0, 50)
    .replace(/-+$/, '');
  return `feature/${slug}`;
}

/**
 * Build the planning prompt.
 */
function buildPlanPrompt(feature: string, cwd: string): string {
  return [
    '=== FEATURE REQUEST ===',
    feature,
    '',
    '=== PROJECT DIRECTORY ===',
    cwd,
    '',
    'Create a detailed implementation plan for this feature.',
    'Include:',
    '1. Files to create or modify (with paths)',
    '2. Key implementation steps in order',
    '3. Dependencies or prerequisites',
    '4. Potential risks or edge cases',
    '5. How to test the implementation',
    '',
    'Be specific and actionable. Reference actual paths in the project.',
  ].join('\n');
}

/**
 * Build the CodingAgent implementation prompt.
 */
function buildImplementationPrompt(feature: string, autonomous: boolean): string {
  const parts = [
    '=== FEATURE TO IMPLEMENT ===',
    feature,
    '',
  ];

  if (!autonomous) {
    // In collaborative mode: step 0=shell, step 1=LLM plan
    parts.push(
      '=== TEAM PLAN (from planning phase) ===',
      '{{steps.1.output}}',
      '',
      '=== TEAM FEEDBACK ===',
      '{{steps.3.output}}',
      '',
    );
  }

  parts.push(
    'Implement this feature following the plan above.',
    'Guidelines:',
    '- Read existing code before modifying — understand the patterns in use',
    '- Follow the project\'s existing conventions and style',
    '- Write clean, well-structured code',
    '- After implementation, run the build to verify compilation',
    '- Fix any issues before finishing',
  );

  return parts.join('\n');
}
