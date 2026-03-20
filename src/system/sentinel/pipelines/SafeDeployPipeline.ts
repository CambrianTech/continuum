/**
 * SafeDeployPipeline — Internal sentinel template for safe deployment.
 *
 * Orchestrates the same flow as scripts/safe-deploy.sh but from WITHIN the
 * sentinel pipeline engine. Used when personas trigger self-deploy after
 * code changes.
 *
 * Steps:
 *   0: Shell — npm run build:ts (compile check — fail fast)
 *   1: Shell — git tag safe/pre-deploy --force (mark rollback point)
 *   2: Shell — npm stop
 *   3: Shell — npm start (deploy)
 *   4: Loop  — Poll ./jtag ping until healthy (max iterations from timeout)
 *   5: Condition — healthy?
 *     then: tag safe/latest, log success, emit deploy:success
 *     else: npm stop, revert to pre-deploy, restart, verify, emit deploy:failed
 *
 * Usage:
 *   const pipeline = buildSafeDeployPipeline({
 *     cwd: '/path/to/repo/src',
 *     personaId: '...',
 *     personaName: 'Helper AI',
 *   });
 *   await Commands.execute('sentinel/run', { type: 'pipeline', definition: pipeline });
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

export interface SafeDeployConfig {
  /** Working directory (repo src/) */
  cwd: string;
  /** Persona executing the deploy */
  personaId: string;
  /** Persona display name */
  personaName: string;
  /** Max seconds to wait for health (default: 180) */
  healthTimeoutSecs?: number;
  /** Health poll interval in seconds (default: 3) */
  healthPollIntervalSecs?: number;
  /** Require AI personas to be healthy (default: false) */
  requireAiHealthy?: boolean;
  /** Chat room for status updates */
  roomId?: string;
}

export function buildSafeDeployPipeline(config: SafeDeployConfig): Pipeline {
  const {
    cwd,
    personaId,
    personaName,
    healthTimeoutSecs = 180,
    healthPollIntervalSecs = 3,
    requireAiHealthy = false,
    roomId,
  } = config;

  const runId = `deploy-${personaId.slice(0, 8)}-${Date.now().toString(36)}`;
  const maxHealthPolls = Math.ceil(healthTimeoutSecs / healthPollIntervalSecs);

  // Health check script — parses ./jtag ping output
  const healthCheckCmd = [
    'OUTPUT=$(./jtag ping 2>/dev/null) || { echo "unhealthy"; exit 0; }',
    'if echo "$OUTPUT" | grep -q \'"success":true\' || echo "$OUTPUT" | grep -q \'"success": true\'; then',
    ...(requireAiHealthy
      ? [
          '  if echo "$OUTPUT" | grep -q \'"healthy"\'; then',
          '    echo "healthy"',
          '  else',
          '    echo "unhealthy"',
          '  fi',
        ]
      : ['  echo "healthy"']),
    'else',
    '  echo "unhealthy"',
    'fi',
  ].join('\n');

  const steps: PipelineStep[] = [];

  // ─── Step 0: Compile check — fail fast ──────────────────────────────
  steps.push({
    type: 'shell',
    cmd: 'npm run build:ts 2>&1 | tail -30',
    workingDir: cwd,
    timeoutSecs: 180,
  });

  // ─── Step 1: Mark rollback point ────────────────────────────────────
  steps.push({
    type: 'shell',
    cmd: 'git tag -f safe/pre-deploy HEAD && echo "Tagged safe/pre-deploy → $(git rev-parse --short HEAD)"',
    workingDir: cwd,
    timeoutSecs: 10,
  });

  // ─── Step 2: Notify chat (if configured) ────────────────────────────
  if (roomId) {
    steps.push({
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: `**${personaName}** is deploying... Build passed. Starting deployment.`,
      },
    });
  }

  // ─── Step 3: Stop current system ────────────────────────────────────
  steps.push({
    type: 'shell',
    cmd: 'npm stop 2>/dev/null || bash scripts/system-stop.sh 2>/dev/null || true && sleep 2',
    workingDir: cwd,
    timeoutSecs: 30,
    allowFailure: true,
  });

  // ─── Step 4: Deploy ─────────────────────────────────────────────────
  steps.push({
    type: 'shell',
    cmd: 'npm start 2>&1 | tail -10',
    workingDir: cwd,
    timeoutSecs: 300, // npm start takes ~140s
  });

  // ─── Step 5: Health check loop ──────────────────────────────────────
  steps.push({
    type: 'loop',
    until: '{{steps[-1].output}} contains "healthy"',
    maxIterations: maxHealthPolls,
    steps: [
      {
        type: 'shell',
        cmd: healthCheckCmd,
        workingDir: cwd,
        timeoutSecs: 15,
        allowFailure: true,
      },
      // Wait before next poll (skip if already healthy)
      {
        type: 'condition',
        if: '{{steps[-1].output}} contains "unhealthy"',
        then: [
          {
            type: 'shell',
            cmd: `sleep ${healthPollIntervalSecs}`,
            timeoutSecs: healthPollIntervalSecs + 5,
          },
        ],
      },
    ],
  });

  // ─── Step 6: Check final health result ──────────────────────────────
  // After the loop, run one final check to get definitive status
  steps.push({
    type: 'shell',
    cmd: healthCheckCmd,
    workingDir: cwd,
    timeoutSecs: 15,
    allowFailure: true,
  });

  // ─── Step 7: Branch on health ───────────────────────────────────────
  steps.push({
    type: 'condition',
    if: '{{steps[-1].output}} contains "healthy"',
    then: buildSuccessSteps(config, runId),
    else: buildRollbackSteps(config, runId),
  });

  return {
    name: `${personaName}: safe deploy`,
    steps,
    workingDir: cwd,
    timeoutSecs: healthTimeoutSecs + 600, // health timeout + buffer
    inputs: {
      runId,
      personaId,
      personaName,
    },
  };
}

/**
 * Steps executed when health check passes.
 */
function buildSuccessSteps(config: SafeDeployConfig, runId: string): PipelineStep[] {
  const { cwd, personaName, roomId } = config;
  const steps: PipelineStep[] = [];

  // Tag as known-good
  steps.push({
    type: 'shell',
    cmd: [
      'git tag -f safe/latest HEAD',
      'git tag "safe/$(date +%Y%m%d-%H%M%S)" HEAD 2>/dev/null || true',
      'echo "Tagged safe/latest → $(git rev-parse --short HEAD)"',
    ].join('\n'),
    workingDir: cwd,
    timeoutSecs: 10,
  });

  // Log to deploy history
  steps.push({
    type: 'shell',
    cmd: [
      'mkdir -p ~/.continuum/deploys',
      `echo '{"timestamp":"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'","commit":"'$(git rev-parse HEAD)'","status":"success","deployedBy":"${personaName}"}' >> ~/.continuum/deploys/history.jsonl`,
    ].join('\n'),
    workingDir: cwd,
    timeoutSecs: 5,
  });

  // Notify chat
  if (roomId) {
    steps.push({
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: `**${personaName}**: Deploy **successful**. Tagged safe/latest.`,
      },
    });
  }

  // Emit success event
  steps.push({
    type: 'emit',
    event: `deploy:${runId}:success`,
    payload: {
      runId,
      personaName: config.personaName,
      personaId: config.personaId,
      status: 'success',
    },
  });

  return steps;
}

/**
 * Steps executed when health check fails — revert and restart.
 */
function buildRollbackSteps(config: SafeDeployConfig, runId: string): PipelineStep[] {
  const { cwd, personaName, healthTimeoutSecs = 180, roomId } = config;
  const steps: PipelineStep[] = [];

  // Notify chat about failure
  if (roomId) {
    steps.push({
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: `**${personaName}**: Deploy **failed** health check. Rolling back...`,
      },
    });
  }

  // Stop the broken deployment
  steps.push({
    type: 'shell',
    cmd: 'npm stop 2>/dev/null || bash scripts/system-stop.sh 2>/dev/null || true && sleep 2',
    workingDir: cwd,
    timeoutSecs: 30,
    allowFailure: true,
  });

  // Revert to pre-deploy tag
  steps.push({
    type: 'shell',
    cmd: [
      'echo "Reverting to safe/pre-deploy..."',
      'git checkout safe/pre-deploy 2>/dev/null || git checkout safe/latest 2>/dev/null || echo "No safe tag to revert to"',
      'echo "Now at $(git rev-parse --short HEAD)"',
    ].join('\n'),
    workingDir: cwd,
    timeoutSecs: 15,
    allowFailure: true,
  });

  // Restart with reverted code
  steps.push({
    type: 'shell',
    cmd: 'npm start 2>&1 | tail -10',
    workingDir: cwd,
    timeoutSecs: 300,
    allowFailure: true,
  });

  // Wait for recovery health check
  const healthCheckCmd = [
    'OUTPUT=$(./jtag ping 2>/dev/null) || { echo "unhealthy"; exit 0; }',
    'if echo "$OUTPUT" | grep -q \'"success":true\' || echo "$OUTPUT" | grep -q \'"success": true\'; then',
    '  echo "healthy"',
    'else',
    '  echo "unhealthy"',
    'fi',
  ].join('\n');

  const recoveryPolls = Math.ceil(healthTimeoutSecs / 3);
  steps.push({
    type: 'loop',
    until: '{{steps[-1].output}} contains "healthy"',
    maxIterations: recoveryPolls,
    steps: [
      {
        type: 'shell',
        cmd: healthCheckCmd,
        workingDir: cwd,
        timeoutSecs: 15,
        allowFailure: true,
      },
      {
        type: 'condition',
        if: '{{steps[-1].output}} contains "unhealthy"',
        then: [
          {
            type: 'shell',
            cmd: 'sleep 3',
            timeoutSecs: 8,
          },
        ],
      },
    ],
  });

  // Log failure to deploy history
  steps.push({
    type: 'shell',
    cmd: [
      'mkdir -p ~/.continuum/deploys',
      `echo '{"timestamp":"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'","commit":"'$(git rev-parse HEAD)'","status":"failed","reason":"health_check_timeout","deployedBy":"${personaName}","revertedTo":"'$(git rev-parse safe/pre-deploy 2>/dev/null || echo unknown)'"}' >> ~/.continuum/deploys/history.jsonl`,
    ].join('\n'),
    workingDir: cwd,
    timeoutSecs: 5,
    allowFailure: true,
  });

  // Notify recovery status
  if (roomId) {
    steps.push({
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: roomId,
        message: `**${personaName}**: Rolled back to safe/pre-deploy. System recovery {{steps[-2].output}}.`,
      },
    });
  }

  // Emit failure event
  steps.push({
    type: 'emit',
    event: `deploy:${runId}:failed`,
    payload: {
      runId,
      personaName: config.personaName,
      personaId: config.personaId,
      status: 'failed',
      reason: 'health_check_timeout',
    },
  });

  return steps;
}
