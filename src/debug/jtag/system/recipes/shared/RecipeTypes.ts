/**
 * Recipe System Types
 *
 * Recipes are composable command pipelines that define how humans and AIs collaborate.
 * They're templates for conversation patterns, stored as JSON and executed as command chains.
 *
 * ## Recipe vs Activity Architecture
 *
 * **Recipe = Template (Class)**
 * - Static definition of behavior
 * - Defines pipeline, RAG template, strategy, layout
 * - Stored in system/recipes/*.json and loaded to database
 * - Immutable during runtime
 *
 * **Activity = Instance (Object)**
 * - Runtime instance of a Recipe
 * - Has mutable state (phase, progress, variables)
 * - Tracks participants (users + AIs) with roles
 * - Can override recipe config
 * - Stored in ActivityEntity
 *
 * Examples:
 * - Recipe "general-chat" → Activity for #general room
 * - Recipe "settings" → Activity for user's settings session
 * - Recipe "academy-lesson" → Activity for a lesson with teacher + student
 *
 * See: system/data/entities/ActivityEntity.ts
 * See: system/activities/shared/ActivityTypes.ts
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
 * Recipe input parameter definition
 * Recipe = function definition, Activity = function call with arguments
 */
export interface RecipeInput {
  /** Parameter type for validation */
  type: 'string' | 'number' | 'boolean' | 'userId' | 'roomId' | 'entityId';
  /** Is this input required? */
  required?: boolean;
  /** Default value if not provided */
  default?: unknown;
  /** Human-readable description */
  description?: string;
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

  // UI composition - defines what widgets compose the experience
  layout?: ActivityUILayout;

  /**
   * Input parameters for dynamic recipes.
   * Recipe = function definition, Activity = function call with arguments.
   *
   * Examples:
   * - persona-details: { personaId: { type: 'userId', required: true } }
   * - log-viewer: { personaId: { type: 'userId' }, logPath: { type: 'string', required: true } }
   * - room-chat: { roomId: { type: 'string', required: true } }
   */
  inputs?: Record<string, RecipeInput>;

  /**
   * Fields that activities CANNOT override.
   * Recipe author controls what's immutable.
   *
   * Examples:
   * - ["layout.mainWidget"] - main experience locked
   * - ["layout.mainWidget", "pipeline"] - core behavior locked
   * - [] or undefined - everything can be overridden
   *
   * Activities can always reset to recipe defaults.
   */
  locked?: string[];

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
 * Panel arrangement options
 */
export type PanelArrangement = 'single' | 'split-h' | 'split-v' | 'tabs' | 'stack';

/**
 * Panel configuration - widgets + how they're arranged
 */
export interface PanelConfig {
  /** Widget tag names */
  widgets: string[];
  /** How to arrange multiple widgets (default: 'single' for 1, 'tabs' for many) */
  arrangement?: PanelArrangement;
  /** Panel-specific config passed to widgets */
  config?: Record<string, unknown>;
}

/**
 * Activity UI Layout - defines what widgets compose the collaborative experience
 *
 * Recipes define not just AI behavior, but the entire collaborative environment.
 * All panels are arrays for composability.
 *
 * Examples:
 * - Chat: main=["chat-widget"], right=null
 * - Settings: main=["settings-widget"], right=["chat-widget"] (help room)
 * - IDE: main=["editor", "terminal"], left=["file-tree"], right=["ai-assistant"]
 * - Academy: main=["lesson-widget"], right=["teacher-chat", "progress-widget"]
 */
export interface ActivityUILayout {
  /** Main content area - primary experience */
  main: string[] | PanelConfig;

  /** Left panel (sidebar) - navigation, lists, trees */
  left?: string[] | PanelConfig | null;

  /** Right panel - AI assistants, tools, helpers */
  right?: string[] | PanelConfig | null;

  /** Bottom panel - terminals, logs, output */
  bottom?: string[] | PanelConfig | null;

  // Legacy support (deprecated - use main/right instead)
  /** @deprecated Use main instead */
  mainWidget?: string;
  /** @deprecated Use right instead */
  rightPanel?: RightPanelConfig | null;
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
