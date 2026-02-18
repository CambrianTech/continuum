/**
 * Coding Types - Shared type definitions for the coding system
 *
 * Defines the data structures for:
 * - Security & risk levels for workspace operations
 * - Model selection by task complexity
 * - Coding actions that map to code/* commands
 * - Coding tasks that describe work to be done
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// ============================================================================
// Security & Risk
// ============================================================================

/**
 * Risk level for coding operations.
 * Determines security tier and oversight requirements.
 */
export type RiskLevel = 'low' | 'medium' | 'high' | 'critical';

/**
 * Security tier that governs which tools are available.
 * Higher tiers require more oversight.
 */
export type SecurityTierLevel = 'discovery' | 'read' | 'write' | 'system';

// ============================================================================
// Model Selection
// ============================================================================

/**
 * Task types that determine which model tier to use.
 * Higher-capability models for planning, cheaper models for quick fixes.
 */
export type CodingTaskType =
  | 'planning'       // Architecture, task decomposition — needs best reasoning
  | 'generation'     // Writing new code — needs strong coding ability
  | 'editing'        // Modifying existing code — needs strong coding ability
  | 'review'         // Code review, analysis — any frontier model
  | 'quick-fix'      // Small fixes, typos — fast and cheap
  | 'discovery';     // Exploring codebase structure — fast and cheap

/**
 * Model tier configuration for a specific task type.
 * CodingModelSelector maps CodingTaskType → CodingModelTier.
 */
export interface CodingModelTier {
  readonly taskType: CodingTaskType;
  readonly provider: string;
  readonly model: string;
  readonly temperature: number;
  readonly maxTokens: number;
  readonly description: string;
}

// ============================================================================
// Coding Actions
// ============================================================================

/**
 * Actions a coding operation can perform.
 * Each maps to a code/* command.
 */
export type CodingAction =
  | 'discover'   // code/tree — explore structure
  | 'search'     // code/search — find patterns
  | 'read'       // code/read — read file contents
  | 'write'      // code/write — create/overwrite file
  | 'edit'       // code/edit — partial edit
  | 'diff'       // code/diff — preview changes
  | 'undo'       // code/undo — revert changes
  | 'verify'     // code/verify — build/test verification
  | 'commit'     // code/git — stage and commit changes
  | 'report';    // Meta: summarize what was done

// ============================================================================
// Coding Task
// ============================================================================

/**
 * A coding task describes what needs to be done in a workspace.
 * Used by the coding activity to drive agent work.
 */
export interface CodingTask {
  /** Unique task ID */
  readonly id: UUID;

  /** Persona executing this task */
  readonly personaId: UUID;

  /** Human-readable task description */
  readonly description: string;

  /** Task type for model selection */
  readonly taskType: CodingTaskType;

  /** Room/context this task originated from */
  readonly contextId?: UUID;

  /** Files already known to be relevant (hints for discovery) */
  readonly relevantFiles?: string[];

  /** Maximum execution time in milliseconds */
  readonly maxDurationMs?: number;

  /**
   * Workspace handle — identifies which Rust workspace to use for code/* operations.
   * Defaults to personaId (general persona workspace).
   */
  readonly workspaceHandle?: string;

  /**
   * Workspace mode for this task:
   * - 'sandbox': Isolated directory under .continuum/personas/{id}/workspace/ (default)
   * - 'worktree': Git worktree on continuum repo with sparse checkout
   * - 'project': Git worktree on any external git repo with persona identity
   */
  readonly workspaceMode?: 'sandbox' | 'worktree' | 'project';

  /** Absolute path to git repo on disk (project mode) */
  readonly repoPath?: string;

  /** Paths to sparse-checkout when using worktree mode */
  readonly sparsePaths?: string[];

  /** When the task was created */
  readonly createdAt: number;
}
