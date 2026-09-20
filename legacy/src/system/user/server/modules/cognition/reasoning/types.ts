/**
 * Reasoning System Types
 *
 * Core types for agent reasoning: tasks, plans, evaluations, learnings
 */

import type { UUID } from '../../../../../core/types/CrossPlatformUUID';

/**
 * Task: High-level goal that needs reasoning
 */
export interface Task {
  id: UUID;
  domain: string;
  taskType?: string;  // Specific type within domain (e.g., 'fix-error', 'shell-complete')
  contextId: UUID;
  description: string;
  priority: number;
  triggeredBy: UUID;
  createdAt: number;
}

/**
 * PlanStep: One step in a multi-step plan
 */
export interface PlanStep {
  stepNumber: number;
  action: string;
  expectedOutcome: string;
  completed: boolean;
  completedAt?: number;
  result?: any;
  tools?: string[];  // Suggested tools for this step (e.g., ['code/read', 'code/edit'])
}

/**
 * Plan: Structured approach to accomplish a task
 */
export interface Plan {
  id: UUID;
  taskId: UUID;
  goal: string;
  learnings: string[];
  risks: string[];
  steps: PlanStep[];
  currentStep: number;
  contingencies: Record<string, string[]>;
  successCriteria: string[];
  createdAt: number;
  lastAdjustedAt: number;
  previousAttempts: number;
  domain: string;
}

/**
 * ExecutionResult: Outcome of executing a plan step
 */
export interface ExecutionResult {
  success: boolean;
  output?: any;
  error?: Error;
  duration: number;
  metadata?: any;
}

/**
 * PlanAdjustment: Decision about how to proceed after feedback
 */
export interface PlanAdjustment {
  action: 'CONTINUE' | 'CONTINGENCY' | 'REPLAN' | 'ABORT';
  updatedPlan: Plan;
  reasoning: string;
}

/**
 * Evaluation: Self-assessment of task outcome
 */
export interface Evaluation {
  taskId: UUID;
  planId: UUID;
  meetsSuccessCriteria: boolean;
  criteriaBreakdown: Record<string, boolean>;
  whatWorked: string[];
  mistakes: string[];
  improvements: string[];
  extractedPattern: string;
  evaluatedAt: number;
  duration: number;
  stepsExecuted: number;
  replansRequired: number;
}

/**
 * LearningEntry: Extracted knowledge from past experiences
 */
export interface LearningEntry {
  id: UUID;
  personaId: UUID;
  domain: string;
  pattern: string;
  context: string;
  successCount: number;
  failureCount: number;
  confidence: number;
  learnedFrom: UUID[];
  firstSeenAt: number;
  lastUsedAt: number;
  useCount: number;
}
