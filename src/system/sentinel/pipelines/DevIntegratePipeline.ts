/**
 * DevIntegratePipeline — Merge persona branches into a feature/integration branch
 *
 * The same workflow every dev team uses:
 *   1. Individual developers (personas) work on their own branches
 *   2. Integration branch collects their work
 *   3. Each merge gets build+tested
 *   4. Conflicts get resolved by a CodingAgent (sees both sides)
 *   5. Final integrated result gets full test suite
 *   6. Report to chat — ready for PR or merge to main
 *
 * Strategy: Shell script does the sequential merging (fast, handles clean merges).
 * If ANY merge has conflicts, it stops and hands off to CodingAgent for resolution.
 * After resolution, the shell script continues with remaining branches.
 * This is a while loop: merge → check → resolve → repeat until all merged.
 *
 * Usage:
 *   const pipeline = buildDevIntegratePipeline({
 *     cwd: "/path/to/project",
 *     featureBranch: "feature/auth-overhaul",
 *     branches: ["ai/helper/auth-middleware", "ai/teacher/auth-tests"],
 *     personaId: "...",
 *     personaName: "Integration Lead",
 *   });
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

export interface DevIntegrateConfig {
  /** Project working directory */
  cwd: string;
  /** Git repo path — for future workspace support */
  repoPath?: string;
  /** Target integration branch name (created from baseBranch if doesn't exist) */
  featureBranch: string;
  /** Specific branches to merge (if omitted, discovers all ai/* branches) */
  branches?: string[];
  /** Base branch (default: main) */
  baseBranch?: string;
  /** Persona orchestrating the integration */
  personaId: string;
  /** Persona display name */
  personaName: string;
  /** Chat room for progress reporting */
  roomId?: string;
  /** Build command (null to skip) */
  buildCommand?: string | null;
  /** Test command (null to skip) */
  testCommand?: string | null;
  /** CodingAgent provider for conflict resolution */
  codingProvider?: string;
  /** CodingAgent model for conflict resolution */
  codingModel?: string;
  /** Max budget per conflict resolution attempt */
  maxBudgetUsd?: number;
  /** Skip chat reporting */
  autonomous?: boolean;
}

export function buildDevIntegratePipeline(config: DevIntegrateConfig): Pipeline {
  const {
    cwd,
    featureBranch,
    branches,
    baseBranch = 'main',
    personaId,
    personaName,
    roomId = 'general',
    buildCommand: rawBuildCommand = 'npm run build:ts 2>&1 | tail -30',
    testCommand: rawTestCommand = 'npm test 2>&1 | tail -50',
    codingProvider = 'claude-code',
    codingModel = 'sonnet',
    maxBudgetUsd = 3.0,
    autonomous = false,
  } = config;

  const buildCommand = rawBuildCommand === null ? null : rawBuildCommand;
  const testCommand = rawTestCommand === null ? null : rawTestCommand;
  const runId = `integrate-${personaId.slice(0, 8)}-${Date.now().toString(36)}`;

  // Build the branch list expression for the shell
  const branchListCmd = branches
    ? `echo "${branches.join('\n')}"`
    : `git branch --list "ai/*" --format="%(refname:short)" | grep -v "^$"`;

  const steps: PipelineStep[] = [
    // Step 0: Discover branches + create/checkout integration branch
    {
      type: 'shell',
      cmd: [
        `cd "${cwd}"`,
        `git fetch origin ${baseBranch} 2>/dev/null || true`,
        // Create or checkout integration branch
        `git checkout ${featureBranch} 2>/dev/null || git checkout -b ${featureBranch} origin/${baseBranch} 2>/dev/null || git checkout -b ${featureBranch} ${baseBranch}`,
        `echo "Integration branch: ${featureBranch}"`,
        `echo "Base: ${baseBranch}"`,
        `echo ""`,
        `echo "=== Branches to merge ==="`,
        branchListCmd,
      ].join('\n'),
      timeoutSecs: 30,
      workingDir: cwd,
    },

    // Step 1: Report start (if not autonomous)
    ...(!autonomous ? [{
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: [
          `**${personaName}** is starting integration on \`${featureBranch}\`:`,
          '',
          '{{steps.0.output}}',
          '',
          'Will merge each branch sequentially, resolving conflicts as needed.',
        ].join('\n'),
      },
    } as PipelineStep] : []),

    // Step 2: Sequential merge — attempt all branches in one shell script.
    // Exits with code 0 if all clean, non-zero if any conflicts remain.
    {
      type: 'shell',
      cmd: buildMergeScript(cwd, featureBranch, branchListCmd),
      timeoutSecs: 120,
      workingDir: cwd,
      allowFailure: true,
    },

    // Step 3: Check if conflicts were detected (exit code non-zero from step 2)
    {
      type: 'condition',
      if: '{{steps.2.exitCode}} != 0',
      then: [
        // Get conflict details for CodingAgent
        {
          type: 'shell',
          cmd: [
            `cd "${cwd}"`,
            `echo "=== Conflicting files ==="`,
            `git diff --name-only --diff-filter=U 2>/dev/null || echo "(none)"`,
            `echo ""`,
            `echo "=== Conflict diff ==="`,
            `git diff 2>/dev/null | head -200`,
            `echo ""`,
            `echo "=== Merge output ==="`,
            `echo "{{steps.2.output}}"`,
          ].join('\n'),
          timeoutSecs: 15,
          workingDir: cwd,
          allowFailure: true,
        },

        // CodingAgent resolves all conflicts
        {
          type: 'codingagent',
          prompt: [
            `Merge conflicts need resolution on the integration branch "${featureBranch}".`,
            '',
            'Conflict details:',
            '{{steps.3.then.0.output}}',
            '',
            'Instructions:',
            '1. Open each conflicting file',
            '2. Resolve ALL conflict markers (<<<<<<< / ======= / >>>>>>>)',
            '3. Keep the intent of BOTH sides where possible',
            '4. Stage resolved files with `git add <file>`',
            '5. Complete the merge with `git commit --no-edit`',
            '',
            'Do NOT create new branches or reset. Just resolve conflicts in place.',
          ].join('\n'),
          provider: codingProvider,
          model: codingModel,
          workingDir: cwd,
          maxBudgetUsd,
          maxTurns: 20,
        },

        // After CodingAgent resolves, try remaining branches
        {
          type: 'shell',
          cmd: buildMergeScript(cwd, featureBranch, branchListCmd),
          timeoutSecs: 120,
          workingDir: cwd,
          allowFailure: true,
        },
      ],
    },
  ];

  // Step 4+: Final build verification on integrated branch
  if (buildCommand !== null) {
    steps.push({
      type: 'shell',
      cmd: buildCommand,
      timeoutSecs: 120,
      workingDir: cwd,
      allowFailure: true,
    });
  }

  if (testCommand !== null) {
    steps.push({
      type: 'shell',
      cmd: testCommand,
      timeoutSecs: 300,
      workingDir: cwd,
      allowFailure: true,
    });
  }

  // Summary step
  const summaryStepIdx = steps.length;
  steps.push({
    type: 'shell',
    cmd: [
      `cd "${cwd}"`,
      `echo "=== Integration Summary ==="`,
      `echo "Branch: ${featureBranch}"`,
      `echo ""`,
      `echo "Commits integrated:"`,
      `git log --oneline ${baseBranch}..${featureBranch} 2>/dev/null || echo "(no new commits)"`,
      `echo ""`,
      `echo "Files changed:"`,
      `git diff --stat ${baseBranch}...${featureBranch} 2>/dev/null || echo "(none)"`,
    ].join('\n'),
    timeoutSecs: 15,
    workingDir: cwd,
  });

  // Report to chat
  if (!autonomous) {
    steps.push({
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: [
          `**${personaName}** completed integration on \`${featureBranch}\`:`,
          '',
          `{{steps.${summaryStepIdx}.output}}`,
          '',
          `Ready for review or merge to \`${baseBranch}\`.`,
        ].join('\n'),
      },
    });
  }

  // Emit completion
  steps.push({
    type: 'emit',
    event: `dev:${runId}:integrate-complete`,
    payload: {
      featureBranch,
      baseBranch,
      personaId,
      personaName,
    },
  });

  return {
    name: `dev/integrate — ${featureBranch}`,
    steps,
    workingDir: cwd,
    timeoutSecs: 1800,
    inputs: {
      featureBranch,
      baseBranch,
    },
  };
}

/**
 * Build a shell script that sequentially merges all discovered branches.
 *
 * For each branch:
 *   - Skip empty, non-existent, or already-merged branches
 *   - Attempt git merge --no-edit
 *   - If conflict detected, exit with code 1 (CodingAgent takes over)
 *   - If clean, continue to next branch
 *
 * This runs FAST for clean merges (pure shell, no IPC overhead).
 * Only escalates to CodingAgent when human-like judgment is needed.
 */
function buildMergeScript(cwd: string, featureBranch: string, branchListCmd: string): string {
  return [
    `cd "${cwd}"`,
    `git checkout ${featureBranch}`,
    `CONFLICT=0`,
    `MERGED=0`,
    `SKIPPED=0`,
    ``,
    `for BRANCH in $(${branchListCmd}); do`,
    `  # Skip empty`,
    `  [ -z "$BRANCH" ] && continue`,
    ``,
    `  # Skip if branch doesn't exist`,
    `  if ! git rev-parse --verify "$BRANCH" >/dev/null 2>&1; then`,
    `    echo "SKIP (not found): $BRANCH"`,
    `    SKIPPED=$((SKIPPED + 1))`,
    `    continue`,
    `  fi`,
    ``,
    `  # Skip if already merged`,
    `  if git merge-base --is-ancestor "$BRANCH" HEAD 2>/dev/null; then`,
    `    echo "SKIP (already merged): $BRANCH"`,
    `    SKIPPED=$((SKIPPED + 1))`,
    `    continue`,
    `  fi`,
    ``,
    `  # Attempt merge`,
    `  echo "Merging: $BRANCH"`,
    `  if git merge "$BRANCH" --no-edit 2>&1; then`,
    `    echo "OK: $BRANCH merged cleanly"`,
    `    MERGED=$((MERGED + 1))`,
    `  else`,
    `    echo "CONFLICT in: $BRANCH"`,
    `    CONFLICT=1`,
    `    break`,
    `  fi`,
    `done`,
    ``,
    `echo ""`,
    `echo "Merged: $MERGED, Skipped: $SKIPPED, Conflict: $CONFLICT"`,
    `exit $CONFLICT`,
  ].join('\n');
}
