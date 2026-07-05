/**
 * CoordinationDecisionEntity - Universal decision point capture for training and replay
 *
 * Stores complete context for ANY decision (respond/silent, attack/retreat, approve/reject)
 * across ANY domain (chat, games, code review, etc.)
 *
 * Enables:
 * 1. Time-Travel Debugging - Replay decisions with different personas/models
 * 2. Autopilot Training - Learn user's decision patterns
 * 3. Meta-Learning - Companion AIs learn from human overrides
 * 4. Cross-Domain Transfer - Same pattern works everywhere
 *
 * See: docs/COORDINATION-DECISION-ARCHITECTURE.md
 */

import { BaseEntity } from './BaseEntity';
import { TextField, EnumField, JsonField } from '../decorators/FieldDecorators';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Actor type for decision maker
 */
export type ActorType = 'human' | 'ai-persona';

/**
 * Domain where decision occurred
 */
export type DecisionDomain = 'chat' | 'game' | 'code' | 'analysis';

/**
 * Visual context type (domain-specific UI state)
 */
export type VisualContextType = 'chat-ui' | 'game-screen' | 'code-diff' | 'dashboard';

/**
 * Decision action (domain-specific)
 */
export type DecisionAction =
  | 'POSTED'      // Chat: sent message
  | 'SILENT'      // Chat: stayed quiet
  | 'ERROR'       // Any: error occurred
  | 'TIMEOUT'     // Any: took too long
  | 'ATTACK'      // Game: aggressive action
  | 'RETREAT'     // Game: defensive action
  | 'EXPLORE'     // Game: exploration action
  | 'APPROVE'     // Code: approved PR
  | 'REJECT'      // Code: rejected PR
  | 'COMMENT';    // Code: left comment only

/**
 * Chat UI visual context
 */
export interface ChatUIVisualContext {
  type: 'chat-ui';
  visibleMessages?: Array<{
    id: UUID;
    senderId: UUID;
    content: string;
    timestamp: number;
  }>;
  scrollPosition?: number;
  activeTab?: string;
  notifications?: Array<{
    roomId: UUID;
    unreadCount: number;
  }>;
}

/**
 * Game screen visual context
 */
export interface GameScreenVisualContext {
  type: 'game-screen';
  screenshot?: string;  // Base64 encoded
  gameState?: {
    playerPosition: { x: number; y: number; z: number };
    enemiesVisible: Array<{
      id: string;
      type: string;
      distance: number;
    }>;
    healthBar: number;
    inventory: Array<{
      id: string;
      name: string;
      quantity: number;
    }>;
  };
  controlInputs?: {
    keyboard: string[];
    mouse: { x: number; y: number; buttons: number };
    gamepad?: { buttons: number[]; axes: number[] };
  };
}

/**
 * Code diff visual context
 */
export interface CodeDiffVisualContext {
  type: 'code-diff';
  files?: Array<{
    path: string;
    diff: string;
    linterWarnings: Array<{
      line: number;
      message: string;
      severity: 'error' | 'warning' | 'info';
    }>;
  }>;
  testResults?: {
    passed: number;
    failed: number;
    skipped: number;
    duration: number;
  };
  ciStatus?: 'passing' | 'failing' | 'pending';
}

/**
 * Dashboard visual context
 */
export interface DashboardVisualContext {
  type: 'dashboard';
  activeView?: string;
  metrics?: Record<string, number>;
  alerts?: Array<{
    severity: 'info' | 'warning' | 'error';
    message: string;
  }>;
}

/**
 * Union of all visual context types
 */
export type VisualContext =
  | ChatUIVisualContext
  | GameScreenVisualContext
  | CodeDiffVisualContext
  | DashboardVisualContext;

/**
 * RAG context structure (complete input to LLM)
 */
export interface RAGContext {
  // Identity
  identity: {
    systemPrompt: string;
    bio: string;
    role: string;
  };

  // Recipe strategy (conversation rules, decision criteria)
  recipeStrategy?: {
    name: string;
    rules: string[];
    parameters: Record<string, unknown>;
  };

  // Conversation history
  conversationHistory: Array<{
    role: 'system' | 'user' | 'assistant';
    content: string;
    timestamp: number;
  }>;

  // Artifacts (screenshots, files, images)
  artifacts?: Array<{
    type: 'image' | 'file' | 'code';
    name: string;
    content: string;  // Base64 or text
    mimeType?: string;
  }>;

  // Private memories (internal knowledge)
  privateMemories?: Array<{
    type: string;
    content: string;
    relevance: number;
  }>;

  // Metadata
  metadata: {
    timestamp: number;
    tokenCount: number;
    contextWindow: number;
  };
}

/**
 * Coordination snapshot (ThoughtStream state)
 */
export interface CoordinationSnapshot {
  /** ThoughtStream context (for multi-agent coordination) */
  thoughtStreamId?: string;
  phase?: 'gathering' | 'deciding' | 'closed';
  availableSlots?: number;

  /** This actor's thought in the stream */
  myThought?: {
    confidence: number;
    priority: number;
    timestamp: number;
  };

  /** Other actors' thoughts */
  competingThoughts?: Array<{
    actorId: UUID;
    actorName: string;
    confidence: number;
    priority: number;
  }>;

  /** Who else is considering responding */
  othersConsideringCount: number;
  othersConsideringNames: string[];
}

/**
 * Activity ambient state (Phase 3bis)
 */
export interface AmbientState {
  /** Temperature (0-1): Conversation "heat" level */
  temperature: number;

  /** Is the human currently present/watching? */
  userPresent: boolean;

  /** Time since last response (ms) */
  timeSinceLastResponse: number;

  /** Recent activity level */
  messagesInLastMinute: number;

  /** Was this actor explicitly mentioned? */
  mentionedByName: boolean;

  /** Queue pressure (future) */
  pressure?: number;
}

/**
 * Companion AI suggestion (for meta-learning)
 */
export interface CompanionSuggestion {
  suggestedAction: DecisionAction;
  confidence: number;
  reasoning: string;
  wasFollowed: boolean;  // Did human follow the suggestion?
}

/**
 * Decision data
 */
export interface DecisionData {
  /** The action taken */
  action: DecisionAction;

  /** Confidence in this decision (0-1) */
  confidence: number;

  /** LLM's reasoning (if available) */
  reasoning?: string;

  /** Response content (if action produced output) */
  responseContent?: string;

  /** Model that made the decision */
  modelUsed?: string;
  modelProvider?: string;

  /** Resource usage */
  tokensUsed?: number;
  responseTime: number;  // ms to decide + generate

  /** Companion AI suggestion (for meta-learning) */
  companionSuggestion?: CompanionSuggestion;
}

/**
 * Outcome data (post-hoc evaluation)
 */
export interface OutcomeData {
  /** Was this a good decision? */
  wasGoodDecision: boolean;

  /** Numeric rating (optional, 0-1 or 1-5 scale) */
  rating?: number;

  /** Why was it good/bad? */
  reasoning: string;

  /** Who rated it? */
  ratedBy: 'self' | 'user' | 'system' | 'community';

  /** When rated */
  ratedAt: number;

  /** Follow-up data (for long-term outcome tracking) */
  followUp?: {
    conversationContinued: boolean;
    userSatisfaction?: number;
    mistakeCorrected?: boolean;
  };
}

/**
 * Decision metadata
 */
export interface DecisionMetadata {
  /** Session this decision occurred in */
  sessionId: UUID;

  /** Context (room, game session, PR, etc.) */
  contextId: UUID;

  /** Decision sequence number (for ordering) */
  sequenceNumber: number;

  /** Tags for filtering */
  tags?: string[];  // ['greeting', 'technical-question', 'off-topic']

  /** Experiment/version tracking */
  experimentId?: string;
  modelVersion?: string;
  systemVersion?: string;
}

/**
 * CoordinationDecisionEntity - Complete decision point capture
 *
 * Every decision (human or AI) logs:
 * - Complete RAG context (EXACTLY what they saw)
 * - Coordination state (ThoughtStream snapshot)
 * - Ambient state (temperature, user presence, pressure)
 * - Visual context (domain-specific: chat UI, game screen, code diff)
 * - Decision + outcome (action, confidence, post-hoc rating)
 *
 * Enables:
 * - Time-travel debugging: Replay with different personas
 * - Autopilot training: Learn user's decision patterns
 * - Meta-learning: Companion suggestions → training data
 * - Domain transfer: Same pattern works everywhere
 */
export class CoordinationDecisionEntity extends BaseEntity {
  static readonly collection = 'coordination_decisions';

  // ============================================================================
  // IDENTITY - Who and what
  // ============================================================================

  @TextField({ index: true })
  actorId!: UUID;

  @EnumField({ index: true })
  actorType!: ActorType;

  @TextField()
  actorName!: string;

  @TextField({ index: true })
  triggerEventId!: UUID;  // MessageId, gameEventId, PRId, etc.

  @EnumField({ index: true })
  domain!: DecisionDomain;

  // ============================================================================
  // COMPLETE CONTEXT - What they saw (full reproducibility)
  // ============================================================================

  /**
   * Complete RAG context - EXACTLY what the LLM saw
   * Stored inline for small contexts (<4KB) or null if stored in blob storage
   *
   * Access pattern:
   * - If ragContext is set, use it directly
   * - If ragContextRef is set, retrieve from BlobStorage
   */
  @JsonField({ nullable: true })
  ragContext?: RAGContext;

  /**
   * Reference to RAG context in blob storage (sha256:hex hash)
   * Used for large contexts (>4KB) to avoid bloating SQLite
   *
   * When set, ragContext should be null and data is in BlobStorage
   */
  @TextField({ nullable: true })
  ragContextRef?: string;

  /**
   * Domain-specific visual context
   * What the actor actually saw on their screen
   */
  @JsonField({ nullable: true })
  visualContext?: VisualContext;

  // ============================================================================
  // COORDINATION STATE - Who else is involved
  // ============================================================================

  @JsonField()
  coordinationSnapshot!: CoordinationSnapshot;

  // ============================================================================
  // AMBIENT STATE - Activity metadata (Phase 3bis)
  // ============================================================================

  @JsonField()
  ambientState!: AmbientState;

  // ============================================================================
  // THE DECISION - What they chose
  // ============================================================================

  @JsonField()
  decision!: DecisionData;

  // ============================================================================
  // OUTCOME - Training label (post-hoc evaluation)
  // ============================================================================

  @JsonField({ nullable: true })
  outcome?: OutcomeData;

  // ============================================================================
  // METADATA - For querying and analysis
  // ============================================================================

  @JsonField()
  metadata!: DecisionMetadata;

  // ============================================================================
  // BaseEntity requirements
  // ============================================================================

  get collection(): string {
    return CoordinationDecisionEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    // Identity validation
    if (!this.actorId?.trim()) {
      return { success: false, error: 'actorId is required' };
    }
    if (!this.actorType) {
      return { success: false, error: 'actorType is required' };
    }
    if (!this.actorName?.trim()) {
      return { success: false, error: 'actorName is required' };
    }
    if (!this.triggerEventId?.trim()) {
      return { success: false, error: 'triggerEventId is required' };
    }
    if (!this.domain) {
      return { success: false, error: 'domain is required' };
    }

    // Context validation - need either inline ragContext OR blob reference
    if (!this.ragContext && !this.ragContextRef) {
      return { success: false, error: 'ragContext or ragContextRef is required' };
    }
    // If inline ragContext is present, validate its structure
    if (this.ragContext) {
      if (!this.ragContext.identity?.systemPrompt) {
        return { success: false, error: 'ragContext.identity.systemPrompt is required' };
      }
      if (!this.ragContext.conversationHistory) {
        return { success: false, error: 'ragContext.conversationHistory is required' };
      }
    }
    // If using blob reference, validate hash format
    if (this.ragContextRef && !this.ragContextRef.startsWith('sha256:')) {
      return { success: false, error: 'ragContextRef must be a valid sha256: hash' };
    }

    // Coordination validation
    if (!this.coordinationSnapshot) {
      return { success: false, error: 'coordinationSnapshot is required' };
    }
    if (typeof this.coordinationSnapshot.othersConsideringCount !== 'number') {
      return { success: false, error: 'coordinationSnapshot.othersConsideringCount is required' };
    }

    // Ambient state validation
    if (!this.ambientState) {
      return { success: false, error: 'ambientState is required' };
    }
    if (typeof this.ambientState.temperature !== 'number') {
      return { success: false, error: 'ambientState.temperature is required' };
    }
    if (typeof this.ambientState.userPresent !== 'boolean') {
      return { success: false, error: 'ambientState.userPresent is required' };
    }

    // Decision validation
    if (!this.decision) {
      return { success: false, error: 'decision is required' };
    }
    if (!this.decision.action) {
      return { success: false, error: 'decision.action is required' };
    }
    if (typeof this.decision.confidence !== 'number') {
      return { success: false, error: 'decision.confidence is required' };
    }
    if (typeof this.decision.responseTime !== 'number') {
      return { success: false, error: 'decision.responseTime is required' };
    }

    // Metadata validation
    if (!this.metadata) {
      return { success: false, error: 'metadata is required' };
    }
    if (!this.metadata.sessionId?.trim()) {
      return { success: false, error: 'metadata.sessionId is required' };
    }
    if (!this.metadata.contextId?.trim()) {
      return { success: false, error: 'metadata.contextId is required' };
    }
    if (typeof this.metadata.sequenceNumber !== 'number') {
      return { success: false, error: 'metadata.sequenceNumber is required' };
    }

    // Outcome validation (optional, but if present must be complete)
    if (this.outcome) {
      if (typeof this.outcome.wasGoodDecision !== 'boolean') {
        return { success: false, error: 'outcome.wasGoodDecision is required when outcome is present' };
      }
      if (!this.outcome.reasoning?.trim()) {
        return { success: false, error: 'outcome.reasoning is required when outcome is present' };
      }
      if (!this.outcome.ratedBy) {
        return { success: false, error: 'outcome.ratedBy is required when outcome is present' };
      }
      if (typeof this.outcome.ratedAt !== 'number') {
        return { success: false, error: 'outcome.ratedAt is required when outcome is present' };
      }
    }

    return { success: true };
  }

  static getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'timestamp',
      defaultSortDirection: 'desc',  // Newest decisions first
      defaultPageSize: 50,
      cursorField: 'timestamp'
    };
  }
}
