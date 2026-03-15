/**
 * PublishPipeline — Publication pipeline for code → PR → CI → merge.
 *
 * Takes a workspace with committed code and handles the full publication lifecycle:
 *   0: Approve — "Here's the diff. Proceed with PR?"
 *   1: Shell — git push origin <branch>
 *   2: Shell — gh pr create
 *   3: Loop — Poll gh pr checks until CI completes (with timeout)
 *   4: Condition — CI passed? → continue. Failed? → emit failure event
 *   5: Approve — "CI passed. Merge?"
 *   6: Shell — gh pr merge --squash
 *   7: Emit — publication:complete event
 *
 * Designed to be appended to DevBuildFeaturePipeline or used standalone.
 *
 * Usage:
 *   const pipeline = buildPublishPipeline({
 *     branch: 'ai/helper/add-health-endpoint',
 *     baseBranch: 'main',
 *     title: 'Add health endpoint',
 *     body: 'Implemented by Helper AI via sentinel pipeline.',
 *     personaId: '...',
 *     personaName: 'Helper AI',
 *     cwd: '/path/to/repo',
 *   });
 *   await Commands.execute('sentinel/run', { type: 'pipeline', definition: pipeline });
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

export interface PublishConfig {
  /** Git branch to push and create PR from */
  branch: string;
  /** Base branch for the PR (default: main) */
  baseBranch?: string;
  /** PR title */
  title: string;
  /** PR body (markdown) */
  body?: string;
  /** Persona executing the task */
  personaId: string;
  /** Persona display name */
  personaName: string;
  /** Git repo working directory */
  cwd: string;
  /** Remote name (default: origin) */
  remote?: string;
  /** Merge strategy: squash, merge, rebase (default: squash) */
  mergeStrategy?: 'squash' | 'merge' | 'rebase';
  /** Max seconds to wait for CI (default: 1800 = 30min) */
  ciTimeoutSecs?: number;
  /** CI poll interval in seconds (default: 30) */
  ciPollIntervalSecs?: number;
  /** Auto-approve PR creation (skip first approval gate) */
  autoApprovePr?: boolean;
  /** Auto-approve merge (skip second approval gate) */
  autoApproveMerge?: boolean;
  /** Timeout for approval gates in seconds (undefined = wait forever) */
  approvalTimeoutSecs?: number;
  /** Chat room for status updates */
  roomId?: string;
  /** Trigger safe-deploy after successful merge */
  deploy?: boolean;
  /** Override DEPLOY_HEALTH_TIMEOUT for safe-deploy (seconds) */
  deployHealthTimeout?: number;
}

/**
 * Build a publication pipeline.
 *
 * Returns Pipeline JSON steps that handle push → PR → CI → merge.
 * Can be used standalone or appended to other pipelines.
 */
export function buildPublishPipeline(config: PublishConfig): Pipeline {
  const {
    branch,
    baseBranch = 'main',
    title,
    body = '',
    personaId,
    personaName,
    cwd,
    remote = 'origin',
    mergeStrategy = 'squash',
    ciTimeoutSecs = 1800,
    ciPollIntervalSecs = 30,
    autoApprovePr = false,
    autoApproveMerge = false,
    approvalTimeoutSecs,
    roomId,
  } = config;

  const runId = `publish-${personaId.slice(0, 8)}-${Date.now().toString(36)}`;
  const steps: PipelineStep[] = [];

  // ─── Step 0: Show diff and approve PR creation ──────────────────────
  if (!autoApprovePr) {
    // Get the diff for review
    steps.push({
      type: 'shell',
      cmd: `git diff ${baseBranch}...HEAD --stat && echo "---" && git diff ${baseBranch}...HEAD`,
      workingDir: cwd,
      timeoutSecs: 30,
      allowFailure: true,
    });

    steps.push({
      type: 'approve',
      prompt: [
        `**${personaName}** wants to create a PR:`,
        `**Title:** ${title}`,
        `**Branch:** ${branch} → ${baseBranch}`,
        '',
        '**Diff summary:**',
        '{{steps[-1].output}}',
        '',
        'Approve to push and create the PR.',
      ].join('\n'),
      approvers: ['human'],
      ...(approvalTimeoutSecs ? { timeoutSecs: approvalTimeoutSecs } : {}),
    });
  }

  // ─── Step 1: Push branch ────────────────────────────────────────────
  steps.push({
    type: 'shell',
    cmd: `git push -u ${remote} HEAD:${branch}`,
    workingDir: cwd,
    timeoutSecs: 120,
  });

  // ─── Step 2: Create PR ──────────────────────────────────────────────
  const prBody = [
    body,
    '',
    '---',
    `Automated by **${personaName}** via sentinel pipeline.`,
  ].join('\n');

  steps.push({
    type: 'shell',
    cmd: [
      `gh pr create`,
      `--title "${escapeShell(title)}"`,
      `--body "$(cat <<'PRBODY'`,
      prBody,
      `PRBODY`,
      `)"`,
      `--base ${baseBranch}`,
      `--head ${branch}`,
    ].join(' \\\n  '),
    workingDir: cwd,
    timeoutSecs: 60,
  });

  // Notify chat if configured
  if (roomId) {
    steps.push({
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: [
          `**${personaName}** created a PR: **${title}**`,
          '{{steps[-1].output}}',
          'CI running — will report back when done.',
        ].join('\n'),
      },
    });
  }

  // ─── Step 3: Poll CI checks ─────────────────────────────────────────
  const maxCiPolls = Math.ceil(ciTimeoutSecs / ciPollIntervalSecs);

  steps.push({
    type: 'loop',
    until: '{{steps[-1].output}} contains "pass" || {{steps[-1].output}} contains "fail"',
    maxIterations: maxCiPolls,
    steps: [
      {
        type: 'shell',
        cmd: [
          // gh pr checks outputs status. We parse for pass/fail/pending.
          `STATUS=$(gh pr checks --json state --jq '.[].state' 2>/dev/null | sort -u)`,
          `if echo "$STATUS" | grep -qi "failure\\|error"; then echo "fail"; exit 0; fi`,
          `if echo "$STATUS" | grep -qi "pending\\|queued\\|in_progress\\|waiting"; then echo "pending"; exit 0; fi`,
          `if echo "$STATUS" | grep -qi "success\\|pass"; then echo "pass"; exit 0; fi`,
          // No checks configured — treat as pass
          `echo "pass"`,
        ].join('\n'),
        workingDir: cwd,
        timeoutSecs: 60,
        allowFailure: true,
      },
      // Wait before next poll (skip if already passed/failed)
      {
        type: 'condition',
        if: '{{steps[-1].output}} contains "pending"',
        then: [
          {
            type: 'shell',
            cmd: `sleep ${ciPollIntervalSecs}`,
            timeoutSecs: ciPollIntervalSecs + 10,
          },
        ],
      },
    ],
  });

  // ─── Step 4: Check CI result ────────────────────────────────────────
  // Get final CI status after loop
  steps.push({
    type: 'shell',
    cmd: [
      `STATUS=$(gh pr checks --json state,name --jq '.[] | "\\(.name): \\(.state)"' 2>/dev/null)`,
      `echo "$STATUS"`,
      `if echo "$STATUS" | grep -qi "failure\\|error"; then exit 1; fi`,
    ].join('\n'),
    workingDir: cwd,
    timeoutSecs: 60,
    allowFailure: true,
  });

  steps.push({
    type: 'condition',
    if: '{{steps[-1].exitCode}} == 0',
    then: buildMergeSteps(config, runId),
    else: buildCiFailureSteps(config, runId),
  });

  return {
    name: `${personaName}: publish ${title}`,
    steps,
    workingDir: cwd,
    timeoutSecs: ciTimeoutSecs + 600, // CI timeout + buffer for push/merge
    inputs: {
      runId,
      branch,
      baseBranch,
      title,
      personaId,
      personaName,
    },
  };
}

/**
 * Build steps for successful CI → merge flow.
 */
function buildMergeSteps(config: PublishConfig, runId: string): PipelineStep[] {
  const {
    personaName,
    cwd,
    title,
    mergeStrategy = 'squash',
    autoApproveMerge = false,
    approvalTimeoutSecs,
    roomId,
  } = config;

  const steps: PipelineStep[] = [];

  // Approval gate before merge
  if (!autoApproveMerge) {
    steps.push({
      type: 'approve',
      prompt: [
        `**${personaName}**: CI passed for **${title}**.`,
        '',
        'Approve to merge the PR.',
      ].join('\n'),
      approvers: ['human'],
      ...(approvalTimeoutSecs ? { timeoutSecs: approvalTimeoutSecs } : {}),
    });
  }

  // Merge
  steps.push({
    type: 'shell',
    cmd: `gh pr merge --${mergeStrategy} --delete-branch`,
    workingDir: cwd,
    timeoutSecs: 60,
  });

  // Notify chat
  if (roomId) {
    steps.push({
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: `**${personaName}** merged PR: **${title}** (${mergeStrategy})`,
      },
    });
  }

  // Post-merge deploy (if configured)
  if (config.deploy) {
    const baseBranch = config.baseBranch || 'main';
    const deployTimeout = config.deployHealthTimeout || 180;

    // Pull merged code
    steps.push({
      type: 'shell',
      cmd: `git checkout ${baseBranch} && git pull origin ${baseBranch}`,
      workingDir: cwd,
      timeoutSecs: 60,
    });

    // Trigger safe-deploy via external watchdog
    steps.push({
      type: 'shell',
      cmd: `DEPLOY_HEALTH_TIMEOUT=${deployTimeout} bash scripts/safe-deploy.sh`,
      workingDir: cwd,
      timeoutSecs: deployTimeout + 600,
      allowFailure: true,
    });

    // Check deploy result
    steps.push({
      type: 'condition',
      if: '{{steps[-1].exitCode}} == 0',
      then: [
        ...(roomId
          ? [
              {
                type: 'command' as const,
                command: 'collaboration/chat/send',
                params: {
                  room: roomId,
                  message: `**${personaName}**: PR merged and **deployed successfully**.`,
                },
              },
            ]
          : []),
      ],
      else: [
        ...(roomId
          ? [
              {
                type: 'command' as const,
                command: 'collaboration/chat/send',
                params: {
                  room: roomId,
                  message: `**${personaName}**: PR merged but **deploy failed**. Auto-reverted to safe state.`,
                },
              },
            ]
          : []),
      ],
    });
  }

  // Completion event
  steps.push({
    type: 'emit',
    event: `publish:${runId}:complete`,
    payload: {
      runId,
      title,
      personaName: config.personaName,
      personaId: config.personaId,
      merged: true,
      deployed: !!config.deploy,
    },
  });

  return steps;
}

/**
 * Build steps for CI failure handling.
 */
function buildCiFailureSteps(config: PublishConfig, runId: string): PipelineStep[] {
  const { personaName, roomId, title } = config;

  const steps: PipelineStep[] = [];

  // Notify chat
  if (roomId) {
    steps.push({
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: [
          `**${personaName}**: CI **failed** for **${title}**.`,
          '',
          '{{steps[-1].output}}',
          '',
          'PR remains open for manual investigation.',
        ].join('\n'),
      },
    });
  }

  // Failure event
  steps.push({
    type: 'emit',
    event: `publish:${runId}:ci-failed`,
    payload: {
      runId,
      title,
      personaName: config.personaName,
      personaId: config.personaId,
      merged: false,
      reason: 'ci_failed',
    },
  });

  return steps;
}

/**
 * Build publication steps that can be appended to an existing pipeline.
 *
 * Used by DevBuildFeaturePipeline's --publish flag. These steps reference
 * prior step outputs via interpolation for branch name and diff context.
 */
export function buildPublishSteps(config: PublishConfig): PipelineStep[] {
  const pipeline = buildPublishPipeline(config);
  return pipeline.steps;
}

/**
 * Escape shell special characters in a string for safe embedding.
 */
function escapeShell(str: string): string {
  return str.replace(/"/g, '\\"').replace(/\$/g, '\\$').replace(/`/g, '\\`');
}
