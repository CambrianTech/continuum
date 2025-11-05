/**
 * Recipe System Types
 *
 * Recipes are composable command pipelines that define how humans and AIs collaborate.
 * They're templates for conversation patterns, stored as JSON and executed as command chains.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Conversation pattern strategies
 */
export type ConversationPattern =
  | 'human-focused'    // Wait for humans, avoid AI loops (General chat)
  | 'collaborative'    // AIs engage deeply with each other (Academy)
  | 'competitive'      // AIs compete for best result (Image competition)
  | 'teaching'         // Teacher/student adaptive learning
  | 'exploring'        // Human + AIs discover together (Web browsing)
  | 'cooperative';     // Team coordination (Gaming)

/**
 * RAG template configuration - what gets included in context
 */
export interface RAGTemplate {
  messageHistory: {
    maxMessages: number;
    orderBy: 'chronological' | 'relevance' | 'importance';
    includeTimestamps: boolean;
  };
  artifacts?: {
    types: string[];      // ['image', 'code', 'document', 'screenshot']
    maxItems: number;
    includeMetadata: boolean;
  };
  participants?: {
    includeRoles: boolean;
    includeExpertise: boolean;
    includeHistory: boolean;
  };
  roomMetadata?: boolean;
  gameState?: boolean;
  browserContext?: boolean;
  examResults?: boolean;
  custom?: Record<string, unknown>;
}

/**
 * Strategy configuration - how AIs behave
 */
export interface RecipeStrategy {
  conversationPattern: ConversationPattern;
  responseRules: string[];      // Human-readable rules
  decisionCriteria: string[];   // What LLM should consider
}

/**
 * Single step in recipe command pipeline
 */
export interface RecipeStep {
  command: string;                    // 'rag/build', 'ai/should-respond', 'ai/generate'
  params: Record<string, unknown>;    // Command parameters
  outputTo?: string;                  // Variable name for next step
  condition?: string;                 // 'decision.shouldRespond === true'
  onError?: 'fail' | 'skip' | 'retry';
}

/**
 * Recipe Entity - stored in database
 * Loaded from JSON files in system/recipes/*.json
 */
export interface RecipeEntity {
  // Identity
  uniqueId: string;
  name: string;
  displayName: string;
  description: string;
  version?: number;

  // Command pipeline
  pipeline: RecipeStep[];

  // Context building
  ragTemplate: RAGTemplate;

  // AI behavior
  strategy: RecipeStrategy;

  // Sharing
  isPublic: boolean;
  createdBy: UUID;
  tags: string[];

  // Forking
  parentRecipeId?: UUID;

  // Usage tracking
  usageCount: number;
  lastUsedAt: Date;
}

/**
 * Recipe execution context - runtime state during pipeline execution
 */
export interface RecipeExecutionContext {
  recipeId: UUID;
  personaId: UUID;
  roomId: UUID;
  sessionId: UUID;
  startedAt: Date;

  // Variable storage for pipeline steps
  variables: Record<string, unknown>;

  // Execution trace for debugging
  trace: RecipeExecutionStep[];
}

/**
 * Single step execution trace
 */
export interface RecipeExecutionStep {
  stepIndex: number;
  command: string;
  params: Record<string, unknown>;
  result?: unknown;
  error?: string;
  executedAt: Date;
  durationMs: number;
}

/**
 * Recipe JSON file format (matches system/recipes/*.json)
 */
export interface RecipeDefinition {
  uniqueId: string;
  name: string;
  displayName: string;
  description: string;
  version: number;

  pipeline: RecipeStep[];
  ragTemplate: RAGTemplate;
  strategy: RecipeStrategy;

  isPublic: boolean;
  tags: string[];
}
