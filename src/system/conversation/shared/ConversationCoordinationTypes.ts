/**
 * Conversation Coordination Types - Thought Stream Architecture
 *
 * Event-driven coordination using RTOS-inspired primitives:
 * - Personas broadcast thoughts (like signals/events)
 * - Others observe and react naturally (like condition variables)
 * - Coordination emerges from collective thought stream (like semaphores)
 *
 * Classic concurrency primitives applied to AI social behavior:
 * - mutex → claiming exclusive response right
 * - semaphore → limited response slots (maxResponders)
 * - signal → broadcasting intention to thought stream
 * - condition variable → waiting for coordination decision
 */

import type { UUID } from '../../core/types/JTAGTypes';

/**
 * Response type categories for coordination
 */
export type ResponseType =
  | 'answer'      // Direct answer to question
  | 'clarify'     // Clarifying question
  | 'contribute'  // Additional perspective
  | 'observe';    // Just watching, low confidence

/**
 * Thought types - RTOS-inspired social signals
 */
export type ThoughtType =
  | 'considering'  // Evaluating if I should respond (initial signal)
  | 'claiming'     // Claiming right to respond (mutex acquisition)
  | 'deferring'    // Backing away gracefully (mutex release)
  | 'observing'    // Just watching (no-op signal)
  | 'collaborating'; // Suggesting joint response (semaphore sharing)

/**
 * Persona priority roles for quality-boost coordination
 */
export type PersonaPriority =
  | 'moderator'    // Room moderator/facilitator (e.g., Academy teacher) - reserved slots
  | 'expert'       // Domain expert - boosted priority
  | 'participant'  // Regular participant - normal priority
  | 'observer';    // Low-priority observer

/**
 * Thought - A persona's broadcast signal in the thought stream
 */
export interface Thought {
  /** Who is thinking */
  personaId: UUID;

  /** What kind of thought */
  type: ThoughtType;

  /** Confidence level (0-100) */
  confidence: number;

  /** Why this thought? (for debugging/learning) */
  reasoning: string;

  /** When was this thought */
  timestamp: Date;

  /** Optional: Defer to specific persona */
  deferTo?: UUID;

  /** Optional: Suggest collaboration */
  collaborateWith?: UUID[];

  /** Priority role for quality-boost (default: 'participant') */
  priority?: PersonaPriority;
}

/**
 * Response intention - lightweight evaluation before expensive LLM generation
 */
export interface ResponseIntention {
  /** Persona evaluating this message */
  personaId: UUID;

  /** Room/context where message appeared */
  contextId: UUID;

  /** Message being evaluated */
  messageId: UUID;

  /** Confidence in ability to respond well (0-100) */
  confidence: number;

  /** Urgency of response (0-100) */
  urgency: number;

  /** Type of response this would be */
  responseType: ResponseType;

  /** Relevance score from should-respond logic */
  relevanceScore: number;

  /** Was this persona directly mentioned? */
  wasMentioned: boolean;

  /** Timestamp of evaluation */
  timestamp: Date;

  /** Optional: Defer to another persona */
  deferTo?: UUID;

  /** Optional: Suggest collaboration with other personas */
  collaborateWith?: UUID[];
}

/**
 * Coordination decision - who gets to respond
 */
export interface CoordinationDecision {
  /** Message being coordinated */
  messageId: UUID;

  /** All intentions received */
  intentions: ResponseIntention[];

  /** Personas granted permission to respond */
  granted: UUID[];

  /** Personas explicitly denied */
  denied: UUID[];

  /** Reasoning for decision (for debugging/learning) */
  reasoning: string;

  /** Timestamp of decision */
  timestamp: Date;

  /** How long coordination took (ms) */
  coordinationDurationMs: number;
}

/**
 * Coordination configuration
 */
export interface CoordinationConfig {
  /** How long to wait for intentions before deciding (ms) */
  intentionWindowMs: number;

  /** Maximum personas that can respond to one message */
  maxResponders: number;

  /** Minimum confidence to be considered */
  minConfidence: number;

  /** Always allow mentioned personas to respond */
  alwaysAllowMentioned: boolean;

  /** Weight for confidence vs urgency (0-1, higher = more confidence weight) */
  confidenceWeight: number;

  /** Enable detailed logging */
  enableLogging: boolean;
}

/**
 * Default coordination configuration
 */
/**
 * Get probabilistic maxResponders (more natural than fixed)
 * - 70% chance: 1 responder (focused)
 * - 25% chance: 2 responders (discussion)
 * - 5% chance: 3 responders (lively debate)
 */
export function getProbabilisticMaxResponders(): number {
  const rand = Math.random();
  if (rand < 0.70) return 1;  // 70% - focused
  if (rand < 0.95) return 2;  // 25% - discussion
  return 3;                    // 5% - lively
}

/**
 * Get probabilistic minConfidence (vary the bar)
 * - Usually 0.7 (70%)
 * - Sometimes lower (0.6) for quiet rooms
 * - Sometimes higher (0.8) for active rooms
 */
export function getProbabilisticMinConfidence(): number {
  const rand = Math.random();
  if (rand < 0.15) return 0.3;  // 15% - lower bar (for testing)
  if (rand < 0.85) return 0.5;  // 70% - normal
  return 0.6;                    // 15% - higher bar
}

export const DEFAULT_COORDINATION_CONFIG: CoordinationConfig = {
  intentionWindowMs: 2000,        // 2 second window to collect intentions
  maxResponders: getProbabilisticMaxResponders(),  // Probabilistic 1-3
  minConfidence: getProbabilisticMinConfidence(),  // Probabilistic 0.6-0.8 (normalized 0-1)
  alwaysAllowMentioned: true,     // Mentioned personas always respond
  confidenceWeight: 0.7,          // 70% confidence, 30% urgency
  enableLogging: true
};

/**
 * Thought Stream - Observable coordination process
 * Like a condition variable that multiple threads can wait on
 */
export interface ThoughtStream {
  /** Message being coordinated */
  messageId: UUID;

  /** Room/context */
  contextId: UUID;

  /** Current coordination phase */
  phase: 'gathering' | 'deliberating' | 'decided';

  /** All thoughts in chronological order (immutable log) */
  thoughts: Thought[];

  /** Current considerations by persona (mutable state) */
  considerations: Map<UUID, Thought>;

  /** Final decision (when phase === 'decided') */
  decision?: CoordinationDecision;

  /** When stream started */
  startTime: number;

  /** How many response slots available (semaphore count) */
  availableSlots: number;

  /** Who has claimed slots (mutex holders) */
  claimedBy: Set<UUID>;

  /** Rejection reasons for diagnostics */
  rejections: RejectionReason[];

  /** Condition variable - notify waiters when decision made */
  decisionSignal?: Promise<CoordinationDecision>;
  signalResolver?: (decision: CoordinationDecision) => void;

  /** Timer for intention window enforcement */
  decisionTimer?: NodeJS.Timeout;
}

/**
 * Rejection reason for diagnostic purposes
 */
export interface RejectionReason {
  personaId: UUID;
  reason: 'no_slots' | 'low_confidence' | 'outranked' | 'deferred' | 'timeout';
  confidence: number;
  priority?: PersonaPriority;
  details: string;
  timestamp: number;
}

/**
 * Coordination state for a message (legacy, keeping for compatibility)
 * TODO: Migrate fully to ThoughtStream
 */
export interface MessageCoordinationState {
  messageId: UUID;
  contextId: UUID;
  intentions: Map<UUID, ResponseIntention>;
  decision?: CoordinationDecision;
  startTime: number;
  resolved: boolean;
  timeoutHandle?: NodeJS.Timeout;
}
