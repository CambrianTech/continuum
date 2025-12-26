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

  // UI composition (optional - defaults handled by layout system)
  layout?: ActivityUILayout;

  isPublic: boolean;
  tags: string[];
}

/**
 * Activity UI Layout - defines what widgets compose the collaborative experience
 *
 * Recipes define not just AI behavior, but the entire collaborative environment:
 * - What the human sees (main content widget)
 * - Where AI companions appear (right panel)
 * - What tools are available
 *
 * Examples:
 * - Chat: main=chat-widget, rightPanel=null (no assistant needed, already chatting)
 * - Settings: main=settings-widget, rightPanel=chat-widget (help room)
 * - Theme: main=theme-widget, rightPanel=chat-widget (theme AI assists design)
 * - Logs: main=logs-widget, rightPanel=chat-widget (AI helps analyze logs)
 * - Browser: main=browser-widget, rightPanel=chat-widget (AI sees page, assists)
 * - Game: main=game-widget, rightPanel=chat-widget (AI teammates/opponents)
 */
export interface ActivityUILayout {
  /** Main content widget tag name (e.g., 'chat-widget', 'settings-widget') */
  mainWidget: string;

  /** Right panel configuration. null = hidden, string = chat room name */
  rightPanel?: RightPanelConfig | null;

  /** Future: left panel, bottom panel, floating panels, etc. */
}

/**
 * Right panel configuration
 *
 * The right panel typically shows a contextual AI assistant via ChatWidget.
 * Different activities connect to different "rooms" with different personas.
 */
export interface RightPanelConfig {
  /** Widget to display (default: 'chat-widget') */
  widget?: string;

  /** For chat-widget: which room to connect to */
  room?: string;

  /** Display in compact mode for sidebar use */
  compact?: boolean;

  /** Widget-specific configuration */
  config?: Record<string, unknown>;
}

/**
 * NOTE: Content type → recipe mapping should NOT be hardcoded here.
 *
 * The right architecture is:
 * 1. Each activity/content type has a recipe JSON file (e.g., settings-assistant.json)
 * 2. Recipe JSON includes the `layout` field defining UI composition
 * 3. ActivityLayoutService discovers recipes dynamically via RecipeLoader
 * 4. Content types are mapped through their associated rooms (room.recipeId)
 *
 * For activities that aren't "rooms" (settings, theme, logs), we can either:
 * - Create virtual rooms with recipes
 * - Or have content types themselves declare their recipe
 *
 * Current temporary implementation: ActivityLayoutService has minimal defaults
 * that will be replaced when proper recipe JSON files are created.
 */
