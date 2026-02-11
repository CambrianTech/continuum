/**
 * SentinelExecutionLog - Complete record of what a sentinel did
 *
 * When a sentinel completes (success, failure, timeout), we need to know:
 * 1. WHAT it did - every action, every file change, every LLM decision
 * 2. WHERE it did it - branch, worktree, file paths
 * 3. HOW TO MODIFY - resume, revert, continue from where it stopped
 *
 * This is critical for:
 * - Personas reviewing sentinel work
 * - Users approving before merge
 * - Debugging failed runs
 * - Learning from mistakes
 *
 * STREAMING: Events are emitted as actions happen (like a stack trace).
 * Callers can subscribe to get real-time updates, or "join" to get current state.
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import type { BuildError, BuildAttempt } from './BuildSentinel';
import type { WorkspaceInfo } from './SentinelWorkspace';

/**
 * Sentinel event types for streaming execution log
 *
 * Callers can subscribe to these to get real-time updates:
 * - sentinel:{handle}:action - Each action as it happens
 * - sentinel:{handle}:file-change - File modifications
 * - sentinel:{handle}:status - Status changes (started, escalated, completed)
 */
export type SentinelEventType = 'action' | 'file-change' | 'status';

export interface SentinelEvent {
  type: SentinelEventType;
  handle: string;
  timestamp: string;
  payload: SentinelAction | FileChange | { status: ExecutionLog['status']; reason?: string };
}

/**
 * Event emitter interface for streaming execution
 * Can be Events.emit or a custom callback
 */
export type SentinelEventEmitter = (event: SentinelEvent) => Promise<void> | void;

/**
 * A single action taken by the sentinel
 */
export interface SentinelAction {
  /** Timestamp of the action */
  timestamp: string;

  /** Action type */
  type: 'build' | 'analyze' | 'fix' | 'llm_query' | 'file_edit' | 'file_create' | 'escalate';

  /** What the sentinel was trying to do */
  intent: string;

  /** The actual operation performed */
  operation?: string;

  /** Result of the action */
  result: 'success' | 'failure' | 'skipped';

  /** Details (file path, command, LLM response, etc.) */
  details?: Record<string, unknown>;

  /** Duration in ms */
  durationMs?: number;

  /**
   * EVIDENCE - The actual proof that this action succeeded/failed
   * For builds: the raw output (stdout + stderr)
   * For fixes: before/after diffs
   * For LLM queries: the actual response
   */
  evidence?: {
    /** Raw output (truncated for large outputs) */
    output?: string;
    /** Full output path if saved to file */
    outputFile?: string;
    /** Before state (for edits) */
    before?: string;
    /** After state (for edits) */
    after?: string;
    /** Verification result (did the fix actually work?) */
    verified?: boolean;
    /** Verification output (proof it worked) */
    verificationOutput?: string;
  };
}

/**
 * A file change made by the sentinel
 */
export interface FileChange {
  /** File path relative to workspace */
  path: string;

  /** Type of change */
  type: 'created' | 'modified' | 'deleted';

  /** The diff (for modified files) */
  diff?: string;

  /** Original content (for modified/deleted) */
  originalContent?: string;

  /** New content (for created/modified) */
  newContent?: string;

  /** Which action caused this change */
  actionIndex: number;
}

/**
 * Complete execution log for a sentinel run
 */
export interface ExecutionLog {
  /** Unique handle for this execution */
  handle: string;

  /** Sentinel type */
  sentinelType: 'build' | 'orchestrate' | 'task' | 'screenshot';

  /** What the sentinel was trying to accomplish */
  goal: string;

  /** Final status */
  status: 'success' | 'failure' | 'timeout' | 'escalated' | 'aborted';

  /** Start time */
  startedAt: string;

  /** End time */
  completedAt: string;

  /** Total duration in ms */
  durationMs: number;

  /** Workspace info (where the work happened) */
  workspace: {
    /** Working directory */
    workingDir: string;

    /** Git branch (if git-based isolation) */
    branch?: string;

    /** Original branch to return to */
    originalBranch?: string;

    /** Whether this is a worktree */
    isWorktree?: boolean;

    /** Worktree path */
    worktreePath?: string;

    /** Repo root */
    repoRoot?: string;
  };

  /** Chronological list of all actions taken */
  actions: SentinelAction[];

  /** All file changes made */
  fileChanges: FileChange[];

  /** Build attempts (for BuildSentinel) */
  buildAttempts?: BuildAttempt[];

  /** Final errors (if failed) */
  finalErrors?: BuildError[];

  /** Escalation reason (if escalated) */
  escalationReason?: string;

  /** Summary for quick review */
  summary: string;

  /** How to continue or modify this work */
  continuationInfo: {
    /** Can this work be resumed? */
    canResume: boolean;

    /** Command to resume */
    resumeCommand?: string;

    /** Can changes be reverted? */
    canRevert: boolean;

    /** Command to revert */
    revertCommand?: string;

    /** Branch to merge (if success) */
    mergeBranch?: string;

    /** Command to merge */
    mergeCommand?: string;

    /** Command to create PR */
    prCommand?: string;
  };
}

/**
 * Builder for creating execution logs
 *
 * Streams events in real-time as actions happen (like a stack trace).
 * Callers can:
 * 1. Pass an event emitter to get real-time updates
 * 2. Call getSnapshot() to get current state at any point
 * 3. Call complete() to get the final log
 */
export class ExecutionLogBuilder {
  private log: Partial<ExecutionLog>;
  private startTime: number;
  private actions: SentinelAction[] = [];
  private fileSnapshots: Map<string, string> = new Map();
  private eventEmitter?: SentinelEventEmitter;

  constructor(
    handle: string,
    sentinelType: ExecutionLog['sentinelType'],
    goal: string,
    eventEmitter?: SentinelEventEmitter
  ) {
    this.startTime = Date.now();
    this.eventEmitter = eventEmitter;
    this.log = {
      handle,
      sentinelType,
      goal,
      startedAt: new Date().toISOString(),
      actions: [],
      fileChanges: [],
      workspace: { workingDir: '' },
      continuationInfo: { canResume: false, canRevert: false },
    };

    // Emit started event
    this.emitStatus('success', 'started');
  }

  /**
   * Get the handle for this execution
   */
  get handle(): string {
    return this.log.handle!;
  }

  /**
   * Emit an event (if emitter is configured)
   */
  private async emitEvent(event: SentinelEvent): Promise<void> {
    if (this.eventEmitter) {
      try {
        await this.eventEmitter(event);
      } catch {
        // Ignore emission errors - don't break the sentinel
      }
    }
  }

  /**
   * Emit a status event
   */
  private emitStatus(status: ExecutionLog['status'], reason?: string): void {
    this.emitEvent({
      type: 'status',
      handle: this.log.handle!,
      timestamp: new Date().toISOString(),
      payload: { status, reason },
    });
  }

  /**
   * Set workspace info
   */
  setWorkspace(info: WorkspaceInfo | { workingDir: string }): this {
    this.log.workspace = {
      workingDir: info.workingDir,
      branch: 'branch' in info ? info.branch : undefined,
      originalBranch: 'originalBranch' in info ? info.originalBranch : undefined,
      isWorktree: 'isWorktree' in info ? info.isWorktree : undefined,
      worktreePath: 'worktreePath' in info ? info.worktreePath : undefined,
    };

    // Try to get repo root
    try {
      this.log.workspace!.repoRoot = execSync('git rev-parse --show-toplevel', {
        cwd: info.workingDir,
        encoding: 'utf-8',
      }).trim();
    } catch {
      // Not a git repo
    }

    return this;
  }

  /**
   * Snapshot a file before modification
   */
  snapshotFile(filePath: string): void {
    try {
      const fullPath = path.isAbsolute(filePath) ? filePath : path.join(this.log.workspace!.workingDir, filePath);
      if (fs.existsSync(fullPath)) {
        this.fileSnapshots.set(filePath, fs.readFileSync(fullPath, 'utf-8'));
      }
    } catch {
      // Ignore errors
    }
  }

  /**
   * Record an action (emits event immediately for streaming)
   */
  recordAction(action: Omit<SentinelAction, 'timestamp'>): void {
    const fullAction: SentinelAction = {
      ...action,
      timestamp: new Date().toISOString(),
    };
    this.actions.push(fullAction);

    // Stream the action immediately
    this.emitEvent({
      type: 'action',
      handle: this.log.handle!,
      timestamp: fullAction.timestamp,
      payload: fullAction,
    });
  }

  /**
   * Record a file change (emits event immediately for streaming)
   */
  recordFileChange(filePath: string, type: FileChange['type']): void {
    const change: FileChange = {
      path: filePath,
      type,
      actionIndex: this.actions.length - 1,
    };

    const originalContent = this.fileSnapshots.get(filePath);
    if (originalContent) {
      change.originalContent = originalContent;
    }

    try {
      const fullPath = path.isAbsolute(filePath) ? filePath : path.join(this.log.workspace!.workingDir, filePath);
      if (type !== 'deleted' && fs.existsSync(fullPath)) {
        change.newContent = fs.readFileSync(fullPath, 'utf-8');
      }

      // Generate diff
      if (originalContent && change.newContent) {
        change.diff = this.generateDiff(originalContent, change.newContent);
      }
    } catch {
      // Ignore errors
    }

    this.log.fileChanges!.push(change);

    // Stream the file change immediately
    this.emitEvent({
      type: 'file-change',
      handle: this.log.handle!,
      timestamp: new Date().toISOString(),
      payload: change,
    });
  }

  /**
   * Get a snapshot of the current execution state
   * (Like reading current stack trace without waiting for completion)
   */
  getSnapshot(): Partial<ExecutionLog> & { inProgress: true } {
    return {
      ...this.log,
      inProgress: true,
      durationMs: Date.now() - this.startTime,
      actions: [...this.actions],
      fileChanges: [...this.log.fileChanges!],
      summary: this.generateSummary(),
    };
  }

  /**
   * Complete the log (emits final status event)
   */
  complete(
    status: ExecutionLog['status'],
    options?: {
      buildAttempts?: BuildAttempt[];
      finalErrors?: BuildError[];
      escalationReason?: string;
    }
  ): ExecutionLog {
    const completedAt = new Date().toISOString();
    const durationMs = Date.now() - this.startTime;

    this.log.status = status;
    this.log.completedAt = completedAt;
    this.log.durationMs = durationMs;
    this.log.actions = this.actions;

    if (options?.buildAttempts) this.log.buildAttempts = options.buildAttempts;
    if (options?.finalErrors) this.log.finalErrors = options.finalErrors;
    if (options?.escalationReason) this.log.escalationReason = options.escalationReason;

    // Generate summary
    this.log.summary = this.generateSummary();

    // Generate continuation info
    this.log.continuationInfo = this.generateContinuationInfo(status);

    // Emit completion event
    this.emitStatus(status, options?.escalationReason);

    return this.log as ExecutionLog;
  }

  private generateSummary(): string {
    const { status, actions, fileChanges, buildAttempts, durationMs } = this.log;

    const parts: string[] = [];

    parts.push(`Status: ${status?.toUpperCase() || 'IN_PROGRESS'}`);
    parts.push(`Duration: ${((durationMs ?? (Date.now() - this.startTime)) / 1000).toFixed(1)}s`);
    parts.push(`Actions: ${this.actions.length}`);
    parts.push(`File changes: ${fileChanges?.length || 0}`);

    if (buildAttempts) {
      parts.push(`Build attempts: ${buildAttempts.length}`);
    }

    return parts.join(' | ');
  }

  private generateContinuationInfo(status: ExecutionLog['status']): ExecutionLog['continuationInfo'] {
    const { workspace, handle } = this.log;
    const info: ExecutionLog['continuationInfo'] = {
      canResume: false,
      canRevert: false,
    };

    if (workspace?.branch && workspace.branch !== workspace.originalBranch) {
      // Work was done on a separate branch
      info.canRevert = true;
      info.revertCommand = `git checkout ${workspace.originalBranch} && git branch -D ${workspace.branch}`;

      if (status === 'success') {
        info.mergeBranch = workspace.branch;
        info.mergeCommand = `git checkout ${workspace.originalBranch} && git merge ${workspace.branch}`;
        info.prCommand = `git push -u origin ${workspace.branch} && gh pr create --head ${workspace.branch}`;
      }

      // Can resume if not success
      if (status !== 'success') {
        info.canResume = true;
        info.resumeCommand = `./jtag sentinel/run --type=build --workingDir="${workspace.workingDir}" --useLLM=true`;
      }
    }

    return info;
  }

  private generateDiff(original: string, modified: string): string {
    const originalLines = original.split('\n');
    const modifiedLines = modified.split('\n');
    const diff: string[] = [];

    // Simple line-by-line diff
    const maxLines = Math.max(originalLines.length, modifiedLines.length);
    for (let i = 0; i < maxLines; i++) {
      const orig = originalLines[i];
      const mod = modifiedLines[i];

      if (orig === undefined) {
        diff.push(`+${i + 1}: ${mod}`);
      } else if (mod === undefined) {
        diff.push(`-${i + 1}: ${orig}`);
      } else if (orig !== mod) {
        diff.push(`-${i + 1}: ${orig}`);
        diff.push(`+${i + 1}: ${mod}`);
      }
    }

    return diff.join('\n');
  }
}

/**
 * Format execution log for display
 */
export function formatExecutionLog(log: ExecutionLog): string {
  const lines: string[] = [];

  lines.push('='.repeat(62));
  lines.push(`SENTINEL EXECUTION LOG: ${log.handle}`);
  lines.push('='.repeat(62));
  lines.push(`Type: ${log.sentinelType}`);
  lines.push(`Goal: ${log.goal}`);
  lines.push(`Status: ${log.status.toUpperCase()}`);
  lines.push(`Duration: ${(log.durationMs / 1000).toFixed(1)}s`);
  lines.push('-'.repeat(62));

  // Workspace
  lines.push('WORKSPACE');
  lines.push(`  Dir: ${log.workspace.workingDir}`);
  if (log.workspace.branch) {
    lines.push(`  Branch: ${log.workspace.branch}`);
  }

  // Actions with evidence
  lines.push('-'.repeat(62));
  lines.push(`ACTIONS (${log.actions.length})`);
  for (const action of log.actions.slice(-5)) {
    const status = action.result === 'success' ? '[OK]' : action.result === 'failure' ? '[FAIL]' : '[SKIP]';
    lines.push(`  ${status} ${action.type}: ${action.intent}`);

    // Show evidence if present (THE PROOF)
    if (action.evidence) {
      if (action.evidence.verificationOutput) {
        lines.push(`      PROOF: ${action.evidence.verificationOutput}`);
      }
      if (action.evidence.output && action.evidence.output.length < 200) {
        const indented = action.evidence.output.split('\n').map(l => `      | ${l}`).join('\n');
        lines.push(indented);
      } else if (action.evidence.output) {
        const firstLines = action.evidence.output.split('\n').slice(0, 3).map(l => `      | ${l}`).join('\n');
        lines.push(firstLines);
        lines.push(`      | ... (${action.evidence.output.split('\n').length - 3} more lines)`);
      }
    }
  }
  if (log.actions.length > 5) {
    lines.push(`  ... and ${log.actions.length - 5} more`);
  }

  // File changes
  if (log.fileChanges.length > 0) {
    lines.push('-'.repeat(62));
    lines.push(`FILE CHANGES (${log.fileChanges.length})`);
    for (const change of log.fileChanges) {
      const icon = change.type === 'created' ? '+' : change.type === 'modified' ? '~' : '-';
      lines.push(`  ${icon} ${change.path}`);
    }
  }

  // Continuation
  lines.push('-'.repeat(62));
  lines.push('CONTINUATION OPTIONS');
  if (log.continuationInfo.canRevert) {
    lines.push(`  [REVERT] ${log.continuationInfo.revertCommand}`);
  }
  if (log.continuationInfo.canResume) {
    lines.push('  [RESUME] Run sentinel again on the same branch');
  }
  if (log.continuationInfo.mergeBranch) {
    lines.push(`  [MERGE] ${log.continuationInfo.mergeCommand}`);
  }
  if (log.continuationInfo.prCommand) {
    lines.push('  [PR] Create pull request for review');
  }

  lines.push('='.repeat(62));

  return lines.join('\n');
}

/**
 * Save execution log to file
 */
export function saveExecutionLog(log: ExecutionLog, outputDir?: string): string {
  const dir = outputDir || '/tmp/sentinel-logs';
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }

  const filename = `sentinel-${log.handle}-${log.status}.json`;
  const filepath = path.join(dir, filename);

  fs.writeFileSync(filepath, JSON.stringify(log, null, 2));
  return filepath;
}

/**
 * Create an event emitter that uses the Events system
 * Emits events in format: sentinel:{handle}:{type}
 *
 * @example
 * const emitter = createEventsEmitter();
 * const log = new ExecutionLogBuilder('my-sentinel', 'build', 'Fix errors', emitter);
 *
 * // Elsewhere, subscribe to real-time updates:
 * Events.subscribe('sentinel:my-sentinel:action', (event) => { ... });
 * Events.subscribe('sentinel:my-sentinel:status', (event) => { ... });
 */
export function createEventsEmitter(): SentinelEventEmitter {
  return async (event: SentinelEvent) => {
    // Dynamic import to avoid circular dependencies
    const { Events } = await import('../core/shared/Events');
    const eventName = `sentinel:${event.handle}:${event.type}`;
    await Events.emit(eventName, event);
  };
}

/**
 * Subscribe to all events from a specific sentinel handle
 *
 * @example
 * const unsubscribe = subscribeSentinelEvents('my-sentinel', (event) => {
 *   console.log(`[${event.type}]`, event.payload);
 * });
 */
export async function subscribeSentinelEvents(
  handle: string,
  callback: (event: SentinelEvent) => void
): Promise<() => void> {
  const { Events } = await import('../core/shared/Events');
  const unsubscribers: (() => void)[] = [];

  // Subscribe to all event types
  for (const type of ['action', 'file-change', 'status'] as SentinelEventType[]) {
    const unsub = await Events.subscribe(`sentinel:${handle}:${type}`, callback);
    unsubscribers.push(unsub);
  }

  return () => unsubscribers.forEach(unsub => unsub());
}

/**
 * In-memory registry of active execution logs for "join" functionality
 * Like how you'd join a Unix process to get its status
 */
const activeExecutions = new Map<string, ExecutionLogBuilder>();

/**
 * Register an execution log for external access (like Unix process table)
 */
export function registerExecution(log: ExecutionLogBuilder): void {
  activeExecutions.set(log.handle, log);
}

/**
 * Unregister an execution (called on completion)
 */
export function unregisterExecution(handle: string): void {
  activeExecutions.delete(handle);
}

/**
 * Get current snapshot of an active execution (like joining a process)
 * Returns undefined if execution not found or already completed
 */
export function getExecutionSnapshot(handle: string): ReturnType<ExecutionLogBuilder['getSnapshot']> | undefined {
  const log = activeExecutions.get(handle);
  return log?.getSnapshot();
}

/**
 * List all active execution handles
 */
export function listActiveExecutions(): string[] {
  return Array.from(activeExecutions.keys());
}
