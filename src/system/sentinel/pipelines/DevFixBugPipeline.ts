/**
 * DevFixBugPipeline — Diagnose and fix a bug with collaborative verification
 *
 * Flow:
 *   0: CodingAgent — Investigate the bug (read code, reproduce, find root cause)
 *   1: Emit — Post diagnosis to chat for team input
 *   2: Watch — Wait for team confirmation of approach (timeout=proceed)
 *   3: CodingAgent — Implement the fix
 *   4: Shell — Build
 *   5: Shell — Run tests (focus on regression)
 *   6: Condition — All passed?
 *     then: Emit success to chat
 *     else: CodingAgent retry with test failures
 *   7: Shell — Git commit
 *
 * Key: Diagnosis BEFORE fix. Team reviews diagnosis to avoid fixing symptoms.
 *
 * Workspace design:
 * - When repoPath is set, CodingAgent steps get proper git worktree isolation
 * - Shell steps for build/test/commit use workspace directory via interpolation
 * - No manual `git checkout -b` — WorkspaceStrategy handles branch creation
 *
 * Usage:
 *   const pipeline = buildDevFixBugPipeline({
 *     bug: "Users can't upload avatars larger than 2MB",
 *     personaId: "...",
 *     personaName: "CodeReview AI",
 *     cwd: "/path/to/project",
 *     repoPath: "/path/to/project",
 *   });
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

export interface DevFixBugConfig {
  /** Bug description (natural language, error message, or issue link) */
  bug: string;
  /** Persona executing the fix */
  personaId: string;
  /** Persona display name */
  personaName: string;
  /** Project working directory */
  cwd: string;
  /** Git repo path — enables workspace isolation via project worktree */
  repoPath?: string;
  /** Chat room for collaborative checkpoints */
  roomId?: string;
  /** Provider for CodingAgent */
  codingProvider?: string;
  /** Model for CodingAgent */
  codingModel?: string;
  /** Max budget per attempt in USD */
  maxBudgetUsd?: number;
  /** Max turns per CodingAgent call */
  maxTurns?: number;
  /** Build command (null to skip) */
  buildCommand?: string | null;
  /** Test command (null to skip) */
  testCommand?: string | null;
  /** Seconds to wait for team to review diagnosis (0=skip) */
  diagnosisReviewTimeoutSecs?: number;
  /** Skip collaborative checkpoints */
  autonomous?: boolean;
  /** Capture training data */
  captureTraining?: boolean;
}

export function buildDevFixBugPipeline(config: DevFixBugConfig): Pipeline {
  const {
    bug,
    personaId,
    personaName,
    cwd,
    repoPath,
    roomId = 'general',
    codingProvider = 'claude-code',
    codingModel = 'sonnet',
    maxBudgetUsd = 3.0,
    maxTurns = 25,
    buildCommand = 'npm run build:ts 2>&1 | tail -30',
    testCommand = 'npm test 2>&1 | tail -50',
    diagnosisReviewTimeoutSecs = 60,
    autonomous = false,
    captureTraining = true,
  } = config;

  const runId = `bugfix-${personaId.slice(0, 8)}-${Date.now().toString(36)}`;
  const bugSummary = bug.length > 120 ? bug.slice(0, 120) + '...' : bug;
  const taskSlug = `bugfix-${Date.now().toString(36)}`;
  const hasWorkspace = !!repoPath;

  // CodingAgent step for diagnosis is step 0 — shell steps reference its data.workspaceDir
  const workDir = hasWorkspace ? '{{steps.0.data.workspaceDir}}' : cwd;

  const codingAgentWorkspaceProps = hasWorkspace
    ? { repoPath, taskSlug }
    : { workingDir: cwd };

  const steps: PipelineStep[] = [
    // Step 0: Investigate — read code, find root cause, DON'T fix yet
    {
      type: 'codingagent',
      prompt: [
        '=== BUG REPORT ===',
        bug,
        '',
        'INVESTIGATE ONLY — DO NOT FIX YET.',
        '',
        '1. Search the codebase for relevant code',
        '2. Identify the root cause (not symptoms)',
        '3. Check if there are tests covering this behavior',
        '4. Write a diagnosis summary explaining:',
        '   - What the root cause is',
        '   - Which files/functions are involved',
        '   - What the fix should be',
        '   - What risks the fix might introduce',
        '',
        'Output your diagnosis as a clear summary at the end.',
      ].join('\n'),
      provider: codingProvider,
      model: codingModel,
      maxTurns: 15,
      maxBudgetUsd: Math.min(maxBudgetUsd, 1.5),
      permissionMode: 'bypassPermissions',
      captureTraining,
      personaId,
      ...codingAgentWorkspaceProps,
    },

    // Step 1-2: Collaborative diagnosis review (if not autonomous)
    ...(autonomous ? [] : [
      {
        type: 'command',
        command: 'collaboration/chat/send',
        params: {
          room: roomId,
          message: [
            `**${personaName}** diagnosed a bug:`,
            `> ${bugSummary}`,
            '',
            '**Root Cause Analysis:**',
            '{{steps.0.output}}',
            '',
            `Proceeding with fix in ${diagnosisReviewTimeoutSecs}s unless objected.`,
          ].join('\n'),
        },
      } as PipelineStep,

      {
        type: 'watch',
        event: `dev:${runId}:diagnosis-approved`,
        timeoutSecs: diagnosisReviewTimeoutSecs,
      } as PipelineStep,
    ]),

    // Step 3: Implement the fix
    {
      type: 'codingagent',
      prompt: [
        '=== BUG TO FIX ===',
        bug,
        '',
        '=== YOUR DIAGNOSIS ===',
        '{{steps.0.output}}',
        '',
        'Now implement the fix based on your diagnosis.',
        'Guidelines:',
        '- Fix the ROOT CAUSE, not the symptom',
        '- Add or update tests to prevent regression',
        '- Verify the fix compiles and tests pass',
        '- Minimal changes — don\'t refactor unrelated code',
      ].join('\n'),
      provider: codingProvider,
      model: codingModel,
      maxTurns,
      maxBudgetUsd,
      permissionMode: 'bypassPermissions',
      captureTraining,
      personaId,
      ...codingAgentWorkspaceProps,
    },

    // Build/test verification steps
    ...buildBugVerificationSteps(config, workDir, runId, bugSummary, autonomous, roomId),

    // Git commit — only modified/new files (avoid committing unrelated changes)
    {
      type: 'shell',
      cmd: [
        'git add -u',
        `git diff --cached --quiet && echo "Nothing to commit" || git commit -m "$(cat <<'COMMITMSG'`,
        `fix: ${bugSummary}`,
        '',
        `Root cause identified and fixed by ${personaName}.`,
        'COMMITMSG',
        `)"`,
      ].join('\n'),
      workingDir: workDir,
      timeoutSecs: 30,
      allowFailure: true,
    },

    // Complete
    {
      type: 'emit',
      event: `dev:${runId}:complete`,
      payload: {
        runId,
        bug: bugSummary,
        personaId,
        personaName,
      },
    },
  ];

  return {
    name: `${personaName}: fix ${bugSummary}`,
    steps,
    workingDir: cwd,
    timeoutSecs: 2400,
    inputs: {
      runId,
      bug,
      personaId,
      personaName,
      roomId,
    },
  };
}

function buildBugVerificationSteps(
  config: DevFixBugConfig,
  workDir: string,
  runId: string,
  bugSummary: string,
  autonomous: boolean,
  roomId: string,
): PipelineStep[] {
  const {
    personaName = '',
    buildCommand = 'npm run build:ts 2>&1 | tail -30',
    testCommand = 'npm test 2>&1 | tail -50',
  } = config;

  const steps: PipelineStep[] = [];

  // Build step (skip if null)
  if (buildCommand !== null) {
    steps.push({
      type: 'shell',
      cmd: buildCommand,
      workingDir: workDir,
      timeoutSecs: 180,
      allowFailure: true,
    });
  }

  // Test step (skip if null)
  if (testCommand !== null) {
    steps.push({
      type: 'shell',
      cmd: testCommand,
      workingDir: workDir,
      timeoutSecs: 300,
      allowFailure: true,
    });
  }

  // Condition: check build result (only if we built)
  if (buildCommand !== null) {
    const buildCheckOffset = testCommand !== null ? -2 : -1;
    steps.push({
      type: 'condition',
      if: `{{steps[${buildCheckOffset}].exitCode}} == 0`,
      then: [
        ...(autonomous ? [] : [{
          type: 'command',
          command: 'collaboration/chat/send',
          params: {
            room: roomId,
            message: `**${personaName}** fixed: ${bugSummary}\n\nBuild: Passed`,
          },
        } as PipelineStep]),
      ],
      else: buildBugfixRetrySteps(config, workDir, runId, bugSummary),
    });
  }

  return steps;
}

function buildBugfixRetrySteps(
  config: DevFixBugConfig,
  workDir: string,
  _runId: string,
  _bugSummary: string,
): PipelineStep[] {
  const {
    personaId,
    repoPath,
    codingProvider = 'claude-code',
    codingModel = 'sonnet',
    maxBudgetUsd = 3.0,
    captureTraining = true,
    buildCommand = 'npm run build:ts 2>&1 | tail -30',
  } = config;

  const taskSlug = `bugfix-${Date.now().toString(36)}`;
  const hasWorkspace = !!repoPath;

  const steps: PipelineStep[] = [
    {
      type: 'codingagent',
      prompt: [
        'The fix failed to compile. Fix these errors:',
        '',
        '{{steps[-1].output}}',
        '',
        'Fix all compilation errors without introducing new ones.',
      ].join('\n'),
      provider: codingProvider,
      model: codingModel,
      maxTurns: 15,
      maxBudgetUsd: Math.min(maxBudgetUsd, 1.5),
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

  return steps;
}
