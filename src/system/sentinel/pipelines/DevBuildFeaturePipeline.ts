/**
 * DevBuildFeaturePipeline — End-to-end feature development sentinel
 *
 * Takes a feature description and handles EVERYTHING:
 *
 * Autonomous mode (workspace-aware):
 *   0: CodingAgent — Plan + implement the feature (workspace auto-created with branch)
 *   1-2: Build/test verification (if configured)
 *   3: Shell — Git commit (inside workspace/worktree)
 *   4: Emit — Completion event
 *
 * Collaborative mode (full pipeline):
 *   0: LLM  — Analyze request and generate implementation plan
 *   1: Command — Post plan to chat room for team review
 *   2: Watch — Wait for team feedback/approval (with timeout fallback)
 *   3: CodingAgent — Implement the feature based on plan + feedback
 *   4: Shell — Build and verify compilation
 *   5: Condition — Compilation passed?
 *     then: Shell — Run tests
 *     else: CodingAgent retry with errors → rebuild → test
 *   6: Shell — Git commit
 *   7: Emit — Completion event
 *
 * Workspace design:
 * - When repoPath is set, CodingAgent creates a project worktree automatically
 * - Branch creation handled by WorkspaceStrategy — no shell git checkout needed
 * - Shell steps for build/test/commit use {{steps.N.data.workspaceDir}} interpolation
 * - 5 personas can work on the same repo concurrently without conflicts
 *
 * Usage:
 *   const pipeline = buildDevBuildFeaturePipeline({
 *     feature: "Add user profile page with avatar upload",
 *     personaId: "...",
 *     personaName: "Helper AI",
 *     cwd: "/path/to/project",
 *     repoPath: "/path/to/project",  // enables git isolation
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
  /** Project working directory (fallback when repoPath not set) */
  cwd: string;
  /** Git repo path — enables workspace isolation via project worktree */
  repoPath?: string;
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
    repoPath,
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
  const taskSlug = config.branchName || generateBranchSlug(feature);
  const runId = `dev-${personaId.slice(0, 8)}-${Date.now().toString(36)}`;
  const featureSummary = feature.length > 120 ? feature.slice(0, 120) + '...' : feature;

  // When repoPath is set, workspace creates the branch. Otherwise we need a shell step.
  const hasWorkspace = !!repoPath;

  // The working directory for shell steps:
  // - With workspace: use {{steps.N.data.workspaceDir}} (resolved by CodingAgent step result)
  // - Without workspace: use raw cwd
  // We compute the CodingAgent step index to build the interpolation expression.
  const codingAgentStepIndex = autonomous ? 0 : 3;
  const workDir = hasWorkspace ? `{{steps.${codingAgentStepIndex}.data.workspaceDir}}` : cwd;

  const steps: PipelineStep[] = [];

  // ─── Collaborative mode: planning + review ──────────────────────
  if (!autonomous) {
    // Step 0: LLM plans the implementation
    steps.push({
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
    });

    // Step 1: Post plan to chat for team review
    steps.push({
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: [
          `**${personaName}** is planning a feature:`,
          `> ${featureSummary}`,
          '',
          '**Implementation Plan:**',
          '{{steps.0.output}}',
          '',
          'React with a vote or reply with feedback. Proceeding in ' +
            `${planReviewTimeoutSecs}s if no objections.`,
        ].join('\n'),
      },
    });

    // Step 2: Wait for team feedback
    steps.push({
      type: 'watch',
      event: `dev:${runId}:plan-approved`,
      timeoutSecs: planReviewTimeoutSecs,
    });
  }

  // ─── CodingAgent implements the feature ──────────────────────────
  const codingAgentStep: PipelineStep = {
    type: 'codingagent',
    prompt: buildImplementationPrompt(feature, autonomous),
    provider: codingProvider,
    model: codingModel,
    maxTurns,
    maxBudgetUsd,
    permissionMode: 'bypassPermissions',
    captureTraining,
    personaId,
    // Workspace: repoPath triggers proper git worktree isolation
    ...(hasWorkspace
      ? { repoPath, taskSlug }
      : { workingDir: cwd }),
  };
  steps.push(codingAgentStep);

  // ─── Build verification + conditional test/retry ──────────────────
  if (buildCommand !== null) {
    steps.push(
      {
        type: 'shell',
        cmd: buildCommand,
        workingDir: workDir,
        timeoutSecs: 180,
        allowFailure: true,
      },
      {
        type: 'condition',
        if: '{{steps[-1].exitCode}} == 0',
        then: buildTestSteps(workDir, testCommand, runId, personaName, featureSummary, roomId, autonomous),
        else: buildRetrySteps(config, workDir, runId, featureSummary),
      },
    );
  } else if (testCommand !== null) {
    // No build step but has tests — run tests directly
    steps.push({
      type: 'shell',
      cmd: testCommand,
      workingDir: workDir,
      timeoutSecs: 300,
      allowFailure: true,
    });
  }

  // ─── Git commit ──────────────────────────────────────────────────
  steps.push({
    type: 'shell',
    cmd: [
      // Stage only our changes
      `git add -u`,
      `git ls-files --others --exclude-standard | xargs -r git add`,
      // Commit if there are staged changes
      `git diff --cached --quiet && echo "Nothing to commit" || git commit -m "$(cat <<'COMMITMSG'`,
      `feat: ${featureSummary}`,
      '',
      `Implemented by ${personaName} via sentinel pipeline.`,
      'COMMITMSG',
      `)"`,
    ].join('\n'),
    workingDir: workDir,
    timeoutSecs: 30,
    allowFailure: true,
  });

  // ─── Completion event ────────────────────────────────────────────
  steps.push({
    type: 'emit',
    event: `dev:${runId}:complete`,
    payload: {
      runId,
      feature: featureSummary,
      personaId,
      personaName,
      taskSlug,
    },
  });

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
      taskSlug,
      baseBranch,
      roomId,
    },
  };
}

/**
 * Build test steps for when compilation succeeds.
 */
function buildTestSteps(
  workDir: string,
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
      workingDir: workDir,
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
  workDir: string,
  _runId: string,
  _featureSummary: string,
): PipelineStep[] {
  const {
    personaId,
    repoPath,
    codingProvider = 'claude-code',
    codingModel = 'sonnet',
    maxBudgetUsd = 5.0,
    captureTraining = true,
    buildCommand = 'npm run build:ts 2>&1 | tail -30',
    testCommand = 'npm test 2>&1 | tail -50',
  } = config;

  const taskSlug = config.branchName || generateBranchSlug(config.feature);
  const hasWorkspace = !!repoPath;

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
      maxTurns: 15,
      maxBudgetUsd: Math.min(maxBudgetUsd, 2.0),
      permissionMode: 'bypassPermissions',
      captureTraining,
      personaId,
      ...(hasWorkspace
        ? { repoPath, taskSlug }
        : { workingDir: workDir }),
    },
  ];

  // Re-verify build (skip if null)
  if (buildCommand !== null) {
    steps.push({
      type: 'shell',
      cmd: buildCommand,
      workingDir: workDir,
      timeoutSecs: 180,
      allowFailure: true,
    });
  }

  // Run tests (skip if null)
  if (testCommand !== null) {
    steps.push({
      type: 'shell',
      cmd: testCommand,
      workingDir: workDir,
      timeoutSecs: 300,
      allowFailure: true,
    });
  }

  return steps;
}

/**
 * Generate a task slug from a feature description.
 */
function generateBranchSlug(feature: string): string {
  return feature
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, '')
    .replace(/\s+/g, '-')
    .slice(0, 50)
    .replace(/-+$/, '');
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
    // In collaborative mode: step 0=LLM plan
    parts.push(
      '=== TEAM PLAN (from planning phase) ===',
      '{{steps.0.output}}',
      '',
      '=== TEAM FEEDBACK ===',
      '{{steps.2.output}}',
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
