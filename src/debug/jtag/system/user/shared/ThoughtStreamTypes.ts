/**
 * ThoughtStream - RTOS-Inspired AI Coordination
 *
 * Exposes persona reasoning as observable events for distributed coordination
 * Inspired by real-time operating system task coordination primitives
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Thought stream states (analogous to RTOS task states)
 *
 * considering -> claiming -> responding -> completed
 *            \-> deferring (when another persona claims)
 */
export type ThoughtStreamState =
  | 'considering'    // Evaluating if should respond (like task ready)
  | 'claiming'       // Decided to respond, signaling intent (like acquiring mutex)
  | 'responding'     // Generating response (like task running)
  | 'deferring'      // Backing off, someone else claimed (like task blocked)
  | 'completed'      // Response sent (like task done)
  | 'cancelled';     // Aborted consideration (like task killed)

/**
 * AI-generated decision about whether to respond
 */
export interface ThoughtDecision {
  shouldRespond: boolean;
  confidence: number;          // 0-100: How relevant is this to my expertise?
  reasoning: string;           // Natural language explanation
  uniqueValue?: string;        // What unique perspective can I offer?
  deferTo?: string[];          // Other personas better suited (if deferring)
}

/**
 * ThoughtStream event - broadcasted during persona deliberation
 *
 * These events allow personas to coordinate through observation
 * without centralized control (distributed coordination)
 */
export interface ThoughtStreamEvent {
  personaId: UUID;
  personaName: string;
  roomId: UUID;
  messageId: UUID;
  state: ThoughtStreamState;

  // AI reasoning (from LLM evaluation)
  decision?: ThoughtDecision;

  // Timing for coordination
  timestamp: number;

  // Context
  triggerMessage: string;      // Message that triggered consideration
  otherPersonasPresent: string[];
}

/**
 * Event names for thought stream coordination
 */
export const THOUGHT_STREAM_EVENTS = {
  CONSIDERING: 'persona:thoughtstream:considering',
  CLAIMING: 'persona:thoughtstream:claiming',
  RESPONDING: 'persona:thoughtstream:responding',
  DEFERRING: 'persona:thoughtstream:deferring',
  COMPLETED: 'persona:thoughtstream:completed',
  CANCELLED: 'persona:thoughtstream:cancelled'
} as const;

/**
 * Helper to get event name for a state
 */
export function getThoughtStreamEventName(state: ThoughtStreamState): string {
  switch (state) {
    case 'considering': return THOUGHT_STREAM_EVENTS.CONSIDERING;
    case 'claiming': return THOUGHT_STREAM_EVENTS.CLAIMING;
    case 'responding': return THOUGHT_STREAM_EVENTS.RESPONDING;
    case 'deferring': return THOUGHT_STREAM_EVENTS.DEFERRING;
    case 'completed': return THOUGHT_STREAM_EVENTS.COMPLETED;
    case 'cancelled': return THOUGHT_STREAM_EVENTS.CANCELLED;
  }
}
