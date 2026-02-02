/**
 * Code Coordination Stream - File-level MUTEX for multi-agent coding
 *
 * Extends BaseCoordinationStream to coordinate coding agents:
 * - File-level locking: multiple agents CAN work in parallel if they touch different files
 * - Conflict detection: overlapping file claims are detected and resolved
 * - Lock release: automatic on step completion or plan finalization
 *
 * RTOS analogy:
 * - Each file is a MUTEX — only one agent can hold it
 * - The coordination stream manages MUTEX acquisition/release
 * - Agents broadcast their target files as "thoughts"
 * - The decision grants non-overlapping claims, defers the rest
 *
 * Config differences from Chat:
 * - maxResponders: 5 (more parallel coding workers)
 * - intentionWindowMs: 3000ms (coding needs more coordination time)
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  BaseCoordinationStream,
  type BaseThought,
  type BaseDecision,
  type BaseStream,
} from '../shared/BaseCoordinationStream';

// ────────────────────────────────────────────────────────────
// Domain-specific types
// ────────────────────────────────────────────────────────────

/**
 * Code-specific thought — a persona's claim to work on specific files.
 */
export interface CodeThought extends BaseThought {
  /** Plan this thought relates to */
  planId: UUID;

  /** Files this agent intends to modify */
  targetFiles: string[];

  /** Which plan steps this agent intends to execute */
  stepNumbers: number[];
}

/**
 * Code-specific decision — file lock assignments and conflict report.
 */
export interface CodeDecision extends BaseDecision {
  /** Plan this decision relates to */
  planId: UUID;

  /** File → persona ID mapping of granted locks */
  fileLocks: Map<string, UUID>;

  /** Files that were claimed by multiple agents (conflict detected) */
  conflicts: string[];
}

/**
 * Code-specific stream state.
 */
export interface CodeStream extends BaseStream<CodeThought> {
  /** Plan being coordinated */
  planId: UUID;

  /** Current file locks: file path → persona holding the lock */
  fileLocks: Map<string, UUID>;
}

// ────────────────────────────────────────────────────────────
// Implementation
// ────────────────────────────────────────────────────────────

export class CodeCoordinationStream extends BaseCoordinationStream<CodeThought, CodeDecision, CodeStream> {

  /** Global file locks across all streams (prevents cross-plan conflicts) */
  private _globalFileLocks = new Map<string, UUID>();

  constructor() {
    super({
      intentionWindowMs: 3000,   // 3 seconds — coding needs more coordination time
      maxResponders: 5,          // Up to 5 parallel coding agents
      enableLogging: true,
      cleanupIntervalMs: 60000,  // 1 minute — coding streams live longer
    });
  }

  // ════════════════════════════════════════════════════════════
  // ABSTRACT METHOD IMPLEMENTATIONS
  // ════════════════════════════════════════════════════════════

  protected getDomainName(): string {
    return 'Code';
  }

  protected createStream(eventId: string, contextId: UUID): CodeStream {
    const maxResponders = this.getMaxResponders();

    return {
      eventId,
      contextId,
      phase: 'gathering',
      thoughts: [],
      considerations: new Map(),
      startTime: Date.now(),
      availableSlots: maxResponders,
      claimedBy: new Set(),

      // Code-specific
      planId: contextId, // contextId IS the planId for coding
      fileLocks: new Map(),
    };
  }

  protected convertDecision(baseDecision: BaseDecision, stream: CodeStream): CodeDecision {
    // Collect all conflicts: files claimed by multiple personas
    const fileClaims = new Map<string, UUID[]>();
    for (const thought of stream.thoughts) {
      if (thought.type === 'claiming') {
        for (const file of thought.targetFiles) {
          const existing = fileClaims.get(file) ?? [];
          existing.push(thought.personaId);
          fileClaims.set(file, existing);
        }
      }
    }

    const conflicts: string[] = [];
    for (const [file, claimants] of fileClaims) {
      if (claimants.length > 1) {
        conflicts.push(file);
      }
    }

    return {
      ...baseDecision,
      planId: stream.planId,
      fileLocks: new Map(stream.fileLocks),
      conflicts,
    };
  }

  protected getEventLogContext(eventId: string): string {
    return `plan ${eventId.slice(0, 8)}`;
  }

  // ════════════════════════════════════════════════════════════
  // HOOK OVERRIDES
  // ════════════════════════════════════════════════════════════

  /**
   * Validate a claim: check that the persona's target files are not already locked
   * by another persona (either in this stream or globally).
   */
  protected onClaim(stream: CodeStream, thought: CodeThought): boolean {
    for (const file of thought.targetFiles) {
      // Check global locks (cross-plan)
      const globalHolder = this._globalFileLocks.get(file);
      if (globalHolder && globalHolder !== thought.personaId) {
        this.log(`Claim rejected: ${file} globally locked by ${globalHolder.slice(0, 8)}`);
        return false;
      }

      // Check stream-level locks (within same plan)
      const streamHolder = stream.fileLocks.get(file);
      if (streamHolder && streamHolder !== thought.personaId) {
        this.log(`Claim rejected: ${file} locked by ${streamHolder.slice(0, 8)} in stream`);
        return false;
      }
    }

    // Acquire locks for all target files
    for (const file of thought.targetFiles) {
      stream.fileLocks.set(file, thought.personaId);
      this._globalFileLocks.set(file, thought.personaId);
    }

    return true;
  }

  /**
   * After decision: log file lock summary.
   */
  protected onDecisionMade(stream: CodeStream, decision: CodeDecision): void {
    if (decision.conflicts.length > 0) {
      this.log(`Conflicts detected: ${decision.conflicts.join(', ')}`);
    }
    this.log(`File locks: ${stream.fileLocks.size} files locked across ${decision.granted.length} agents`);
  }

  /**
   * Coding tasks are often single-agent — decide immediately if only one thought.
   * For multi-agent, wait for the intention window.
   */
  protected canDecideEarly(stream: CodeStream): boolean {
    // If only one claimer and no one else is expected, decide immediately
    if (stream.thoughts.length >= 1 && stream.claimedBy.size >= 1) {
      // But wait if we might get more thoughts
      const elapsed = Date.now() - stream.startTime;
      if (elapsed > 1000) return true; // 1s grace period
    }
    return stream.thoughts.length >= 5; // Max parallel agents
  }

  /**
   * Coding streams use deterministic slot allocation (not probabilistic).
   * All available agents get a slot (up to maxResponders).
   */
  protected getMaxResponders(): number {
    return this.config.maxResponders; // Deterministic: 5
  }

  /**
   * Coding streams live longer — plans take time to execute.
   */
  protected getStreamMaxAge(stream: CodeStream): number {
    if (stream.phase === 'decided') return 30000;   // 30s after decision
    return 300000;                                    // 5 min for gathering
  }

  // ════════════════════════════════════════════════════════════
  // PUBLIC CODE-SPECIFIC API
  // ════════════════════════════════════════════════════════════

  /**
   * Broadcast a coding thought for file-level coordination.
   */
  async broadcastCodeThought(
    planId: UUID,
    thought: CodeThought,
  ): Promise<void> {
    thought.planId = planId;
    await this.broadcastThought(planId, planId, thought);
  }

  /**
   * Wait for a coding coordination decision.
   */
  async waitForCodeDecision(planId: UUID, timeoutMs?: number): Promise<CodeDecision | null> {
    return this.waitForDecision(planId, timeoutMs ?? 5000);
  }

  /**
   * Check if persona can work on specific files within a plan.
   */
  async canWorkOnFiles(personaId: UUID, planId: UUID, files: string[]): Promise<boolean> {
    const stream = this.getStream(planId);
    if (!stream) return true; // No coordination active — allow

    for (const file of files) {
      const holder = stream.fileLocks.get(file);
      if (holder && holder !== personaId) {
        return false;
      }
    }
    return true;
  }

  /**
   * Release file locks held by a persona (called after step/plan completion).
   */
  releaseLocks(personaId: UUID, planId?: UUID): void {
    // Release global locks
    for (const [file, holder] of Array.from(this._globalFileLocks.entries())) {
      if (holder === personaId) {
        this._globalFileLocks.delete(file);
      }
    }

    // Release stream-level locks
    if (planId) {
      const stream = this.getStream(planId);
      if (stream) {
        for (const [file, holder] of Array.from(stream.fileLocks.entries())) {
          if (holder === personaId) {
            stream.fileLocks.delete(file);
          }
        }
      }
    } else {
      // Release from all streams
      for (const stream of this.streams.values()) {
        for (const [file, holder] of Array.from(stream.fileLocks.entries())) {
          if (holder === personaId) {
            stream.fileLocks.delete(file);
          }
        }
      }
    }

    this.log(`Released locks for persona ${personaId.slice(0, 8)}`);
  }

  /**
   * Get all files currently locked and who holds them.
   */
  get globalFileLocks(): ReadonlyMap<string, UUID> {
    return this._globalFileLocks;
  }

  /**
   * Check if a specific file is locked.
   */
  isFileLocked(filePath: string): boolean {
    return this._globalFileLocks.has(filePath);
  }

  /**
   * Get the persona holding a lock on a file (if any).
   */
  lockHolder(filePath: string): UUID | undefined {
    return this._globalFileLocks.get(filePath);
  }

  /**
   * Override shutdown to clear global locks.
   */
  override shutdown(): void {
    this._globalFileLocks.clear();
    super.shutdown();
  }
}

// ════════════════════════════════════════════════════════════
// SINGLETON PATTERN
// ════════════════════════════════════════════════════════════

let codeCoordinatorInstance: CodeCoordinationStream | null = null;

/**
 * Get global code coordinator instance.
 */
export function getCodeCoordinator(): CodeCoordinationStream {
  if (!codeCoordinatorInstance) {
    codeCoordinatorInstance = new CodeCoordinationStream();
  }
  return codeCoordinatorInstance;
}

/**
 * Reset code coordinator (for testing).
 */
export function resetCodeCoordinator(): void {
  if (codeCoordinatorInstance) {
    codeCoordinatorInstance.shutdown();
    codeCoordinatorInstance = null;
  }
}
