/**
 * Coding Agent Types - Shared type definitions for the coding agent system
 *
 * Defines the data structures for:
 * - CodingTask: What the agent needs to accomplish
 * - CodingPlan: DAG of steps to accomplish the task
 * - CodingStep: Individual operation in the plan
 * - CodingResult: Outcome of executing a plan
 * - CodingModelTier: Model selection by task complexity
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// ============================================================================
// Security & Risk
// ============================================================================

/**
 * Risk level assessed by PlanFormulator for a coding plan.
 * Determines security tier and whether governance approval is needed.
 */
export type RiskLevel = 'low' | 'medium' | 'high' | 'critical';

/**
 * Security tier that governs which tools a plan can use.
 * Assigned based on risk level; higher tiers require more oversight.
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
// Coding Task
// ============================================================================

/**
 * A coding task is the input to the coding agent system.
 * It describes what needs to be done, who's doing it, and constraints.
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

  /** Maximum execution time in milliseconds (default: 120000) */
  readonly maxDurationMs?: number;

  /** Maximum number of tool calls allowed (default: 15) */
  readonly maxToolCalls?: number;

  /** When the task was created */
  readonly createdAt: number;
}

// ============================================================================
// Coding Plan (DAG of Steps)
// ============================================================================

/**
 * Actions a coding step can perform.
 * Each maps to a code/* command or meta-operation.
 */
export type CodingAction =
  | 'discover'   // code/tree — explore structure
  | 'search'     // code/search — find patterns
  | 'read'       // code/read — read file contents
  | 'write'      // code/write — create/overwrite file
  | 'edit'       // code/edit — partial edit
  | 'diff'       // code/diff — preview changes
  | 'undo'       // code/undo — revert changes
  | 'verify'     // Meta: check results (build, test, read-back)
  | 'report';    // Meta: summarize what was done

/**
 * A single step in a CodingPlan.
 * Steps form a DAG via dependsOn — independent steps can execute in parallel.
 */
export interface CodingStep {
  /** Step number (1-indexed, unique within plan) */
  readonly stepNumber: number;

  /** What this step does */
  readonly action: CodingAction;

  /** Human-readable description of what this step accomplishes */
  readonly description: string;

  /** Files this step will operate on */
  readonly targetFiles: string[];

  /** Which code/* command to execute (e.g., 'code/read', 'code/edit') */
  readonly toolCall: string;

  /** Parameters for the tool call */
  readonly toolParams: Record<string, unknown>;

  /** Steps that must complete before this one (DAG edges) */
  readonly dependsOn: number[];

  /** How to verify this step succeeded */
  readonly verification: string;
}

/**
 * A coding plan is a DAG of CodingSteps produced by the PlanFormulator.
 * The orchestrator executes steps respecting dependency ordering.
 */
export interface CodingPlan {
  /** The task this plan addresses */
  readonly taskId: UUID;

  /** Ordered steps (topologically sorted) */
  readonly steps: CodingStep[];

  /** High-level summary of the approach */
  readonly summary: string;

  /** Estimated total tool calls */
  readonly estimatedToolCalls: number;

  /** Which model generated this plan */
  readonly generatedBy: {
    readonly provider: string;
    readonly model: string;
  };

  /** When the plan was generated */
  readonly generatedAt: number;

  /** Risk level assessed by PlanFormulator */
  readonly riskLevel: RiskLevel;

  /** Why this risk level was assigned */
  readonly riskReason: string;

  /** Minimum security tier required for execution */
  readonly requiredTier: SecurityTierLevel;
}

// ============================================================================
// Step Execution Result
// ============================================================================

export type StepStatus = 'pending' | 'running' | 'completed' | 'failed' | 'skipped';

/**
 * Result of executing a single CodingStep.
 */
export interface StepResult {
  /** Which step */
  readonly stepNumber: number;

  /** Execution status */
  readonly status: StepStatus;

  /** Command output (if any) */
  readonly output?: unknown;

  /** Error message (if failed) */
  readonly error?: string;

  /** Execution time in milliseconds */
  readonly durationMs: number;

  /** Tool call used */
  readonly toolCall: string;
}

// ============================================================================
// Coding Result (Final Output)
// ============================================================================

export type CodingResultStatus = 'completed' | 'partial' | 'failed' | 'budget_exceeded';

/**
 * Final result of executing a coding task.
 */
export interface CodingResult {
  /** The task that was executed */
  readonly taskId: UUID;

  /** Overall status */
  readonly status: CodingResultStatus;

  /** Summary of what was accomplished */
  readonly summary: string;

  /** Results for each step */
  readonly stepResults: StepResult[];

  /** Files that were modified */
  readonly filesModified: string[];

  /** Files that were created */
  readonly filesCreated: string[];

  /** Total tool calls used */
  readonly totalToolCalls: number;

  /** Total execution time in milliseconds */
  readonly totalDurationMs: number;

  /** Change IDs from code/write and code/edit for potential undo */
  readonly changeIds: string[];

  /** Errors encountered */
  readonly errors: string[];
}

// ============================================================================
// Execution Options (Phase 4C: Multi-Agent Coordination)
// ============================================================================

/**
 * Options controlling how a coding plan is executed.
 * Passed to CodeAgentOrchestrator.execute().
 */
export interface ExecutionOptions {
  /** Execute but don't write — report what would happen */
  readonly dryRun?: boolean;

  /** Override the security tier (defaults to plan's requiredTier) */
  readonly securityTier?: SecurityTierLevel;

  /** Enable multi-agent delegation for this execution */
  readonly delegationEnabled?: boolean;
}

// ============================================================================
// Agent Capability (Phase 4C: Multi-Agent Delegation)
// ============================================================================

/**
 * Describes an AI persona's capabilities for coding task delegation.
 * Used by CodeTaskDelegator to match tasks to agents.
 */
export interface AgentCapability {
  /** Persona ID */
  readonly personaId: UUID;

  /** Persona display name */
  readonly name: string;

  /** Coding specialties (e.g., 'typescript', 'testing', 'code-review') */
  readonly specialties: string[];

  /** Current workload fraction (0.0 = idle, 1.0 = fully loaded) */
  readonly currentLoad: number;

  /** Security tier this agent is authorized for */
  readonly securityTier: SecurityTierLevel;
}

// ============================================================================
// Delegation Result (Phase 4C: Multi-Agent Coordination)
// ============================================================================

/**
 * Result of delegating a plan to multiple agents.
 */
export interface DelegationResult {
  /** Parent plan ID */
  readonly parentPlanId: UUID;

  /** Sub-plan IDs created for each agent cluster */
  readonly subPlanIds: UUID[];

  /** Files assigned to each sub-plan */
  readonly assignments: ReadonlyArray<{
    readonly subPlanId: UUID;
    readonly agentId: UUID;
    readonly agentName: string;
    readonly files: string[];
    readonly stepNumbers: number[];
  }>;

  /** Files with conflicts (claimed by multiple clusters) */
  readonly conflicts: string[];
}
