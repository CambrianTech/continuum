/**
 * Shared Cognition Types — contracts between the modules of the
 * shared-cognition pipeline.
 *
 * See `docs/architecture/SHARED-COGNITION.md` for the full architectural
 * picture. The thesis: persona response is two distinct cognitive
 * operations that today are fused into one expensive call per persona —
 * (1) objective analysis of what the message means, and (2) specialty-
 * rendered response through the persona's LoRA-adapted genome. Splitting
 * them lets the objective layer run once + cheap, while each persona's
 * specialty layer runs short + LoRA-rendered.
 *
 * Pipeline:
 *
 *   Message arrives in room
 *      ↓
 *   SharedAnalysisService.analyze(message, roomId) ─────────┐
 *      ↓                                                     │
 *   SharedAnalysis  ◄────────────────────────────────────────┘
 *      ↓
 *   ResponseOrchestrator.pickResponders(analysis, room)
 *      ↓
 *   ResponderDecision[]   (one per persona, may include skips)
 *      ↓
 *   For each {shouldRespond:true} in priority order:
 *      ↓
 *   PRG.respondFromSharedAnalysis(analysis, persona, lead?)
 *      ↓
 *   ChatMessageEntity (posted to room)
 *
 * Levers can be invoked by personas at any point to override the default
 * orchestration policy (escalate to own think pass, cede the floor,
 * request more analysis depth, etc.). See `LeverCall` and the lever
 * surface section of the architecture doc.
 *
 * NO LOGIC IN THIS FILE. Only the contracts that A.1-A.5 implement
 * against. Memento implements A.2 against this surface; this file is
 * the agreed boundary.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// =============================================================================
// SHARED ANALYSIS — output of A.1 (SharedAnalysisService)
// =============================================================================

/**
 * The objective layer of cognition. ONE shared analysis per message,
 * computed by `SharedAnalysisService` on the base model (no LoRA), used
 * by every persona that responds. The point is to do the heavy
 * "what does this message mean, what RAG matters, what's the situation"
 * thinking exactly once instead of N times.
 *
 * Cached by content hash + roomId so repeated analysis of the same
 * message (e.g. retry after a failed render) hits the cache.
 */
export interface SharedAnalysis {
  // ─── Identity / cache key ─────────────────────────────────────────
  /** The chat message this analysis is FOR. */
  messageId: UUID;
  roomId: UUID;
  /**
   * Content-addressable cache key. Stable hash of (message body +
   * recent history snapshot + RAG context fingerprint). Identical
   * inputs return the cached SharedAnalysis without re-running inference.
   */
  cacheKey: string;
  generatedAt: Date;

  // ─── Objective reading ────────────────────────────────────────────
  /** Concise summary of what the message is saying / asking. */
  summary: string;
  /** Concepts the message touches — for downstream specialty matching. */
  keyConcepts: string[];
  /**
   * What KIND of message this is. Influences orchestration: a 'social'
   * greeting may not need 4 specialists weighing in; a 'task' or
   * 'question' may.
   */
  intent: SharedAnalysisIntent;
  /**
   * Optional ambient signal — e.g. 'frustrated', 'curious', 'urgent'.
   * Personas can use this to color their voice; the architecture is
   * agnostic to what's filled in here.
   */
  emotionalTone?: string;

  // ─── Orchestration hints (read by ResponseOrchestrator) ───────────
  /**
   * For each known specialty, why (if at all) this specialty's
   * perspective would matter on this message. A specialty key with
   * an empty value = "this specialty has no signal here, stay silent
   * by default."
   *
   * Keys are stable specialty identifiers (e.g. 'code-review',
   * 'education', 'general'). Values are short prose explaining the
   * relevance — enough that the persona's render prompt can ground
   * its contribution in a specific angle, not just generic flavor.
   */
  suggestedAngles: Record<string, string>;

  /**
   * Compact distillation of the conversation context the analysis
   * relied on. Per-persona renders consume this without re-loading
   * RAG. Optional — analysis may produce empty if RAG was minimal.
   */
  relevantContext?: string;

  // ─── Diagnostic / observability ───────────────────────────────────
  durationMs: number;
  modelUsed: string;
  /**
   * Whether this analysis came from cache vs fresh inference. For
   * latency tracking + cache-effectiveness measurement.
   */
  fromCache: boolean;
}

/**
 * What kind of message this is. Used by the orchestrator to decide
 * whether broad participation is warranted (a question often is) vs
 * minimal contribution (a 'social' message often only needs 1 voice).
 *
 * Keep this enum small and stable — adding a new kind is an
 * orchestration policy change, not a routine extension.
 */
export type SharedAnalysisIntent =
  | 'question'   // user is seeking information / answer
  | 'request'    // user is asking for action / task execution
  | 'statement'  // user is sharing or asserting; reactions may vary
  | 'task'       // explicit work item that the system should do
  | 'social'     // greeting / acknowledgment / chit-chat
  | 'other';     // doesn't fit; orchestrator falls back to defaults

// =============================================================================
// RESPONDER DECISION — output of A.2 (ResponseOrchestrator)
// =============================================================================

/**
 * Per-persona orchestration decision. One of these for each persona in
 * the room. The orchestrator reads `SharedAnalysis.suggestedAngles` +
 * persona specialty + recent contribution history, produces a decision
 * for each, returns the array.
 *
 * `shouldRespond=false` is a first-class outcome — silence with reason
 * is the architecture's preferred answer when the persona has nothing
 * additive to add. The reason string is stored for tunability and for
 * the persona's own meta-cognitive trace (it can see why it was filtered).
 */
export interface ResponderDecision {
  personaId: UUID;
  shouldRespond: boolean;

  /**
   * 0.0..1.0 — how relevant this persona's specialty is to the
   * message + analysis. Above the orchestrator's threshold = respond;
   * below = silent. Threshold is a tunable, exposed via lever
   * `requestThinkBudget`-style overrides.
   */
  relevanceScore: number;

  /**
   * Which keys from `SharedAnalysis.suggestedAngles` matched this
   * persona's specialty. Empty when relevanceScore is low.
   * Becomes part of the persona's render prompt so contribution
   * is grounded in a specific angle.
   */
  matchedAngles: string[];

  /**
   * Human-readable explanation of the decision. Always populated —
   * for both selection and skip cases. This is observable in the
   * coordination stream + diagnostics.
   */
  explanation: string;

  /**
   * Phase B: which persona leads the streaming chain-of-thought
   * (others see the lead's render in flight and build on it).
   * In Phase A, exactly one decision has isLead=true and others
   * are parallel; in Phase B, isLead=true identifies the streaming
   * lead and others wait for partial output.
   */
  isLead?: boolean;
}

// =============================================================================
// LEVER CALLS — A.5 (cognition/* tools personas can call)
// =============================================================================

/**
 * Lever names — the public surface personas use to override default
 * orchestration policy. See "Levers personas pull" section of
 * SHARED-COGNITION.md for semantics.
 *
 * Stable string literal type so command tooling + telemetry have a
 * canonical enum to dispatch on.
 */
export type LeverName =
  | 'requestDeeperAnalysis'
  | 'escalateToOwnThinkPass'
  | 'cedeFloorTo'
  | 'claimLead'
  | 'requestThinkBudget'
  | 'inviteSpecialist'
  | 'seekDisagreement'
  | 'withholdContribution'
  | 'requestCrossDomainAdapter';

/**
 * A persona's lever invocation. Recorded in the chat coordination
 * stream as an observable event. Args are lever-specific (typed as
 * unknown here; per-lever helpers below cast to the right shape).
 */
export interface LeverCall {
  /** Persona invoking the lever. */
  personaId: UUID;
  /** Which lever. */
  lever: LeverName;
  /**
   * Lever-specific arguments. Per-lever shapes are documented in
   * the architecture doc and enforced by the helper functions A.5
   * exposes. Storing as unknown keeps this contract narrow — the
   * lever module owns the per-lever schema.
   */
  args?: Record<string, unknown>;
  /**
   * Why the persona invoked the lever. Optional but strongly
   * encouraged — this trace is what makes the lever surface
   * trainable. ("I should cedeFloorTo CodeReview because this
   * is a security question I'm not strong on.")
   */
  reason?: string;
  /** When invoked. */
  timestamp: Date;
}

/**
 * The result of evaluating a batch of lever calls against the current
 * orchestration. The orchestrator may consult lever calls when
 * computing decisions — e.g. a persona that called `cedeFloorTo(X)`
 * is silenced; X gets a relevance bump.
 */
export interface LeverApplicationOutcome {
  /** Levers that were applied successfully. */
  applied: LeverCall[];
  /**
   * Levers that were rejected (e.g. requested specialist not in room,
   * invalid args). Reason carried for trainability.
   */
  rejected: Array<{ call: LeverCall; reason: string }>;
}

// =============================================================================
// RENDER REQUEST — input to A.3 (PRG.respondFromSharedAnalysis)
// =============================================================================

/**
 * What `PRG.respondFromSharedAnalysis` receives. The persona's render
 * pass uses the shared analysis as the FOUNDATION — it doesn't rederive
 * the objective picture. Its job is to render this persona's specialty
 * perspective on what's already been objectively analyzed.
 */
export interface PersonaRenderRequest {
  /** The objective layer this render builds on. */
  analysis: SharedAnalysis;

  /** The orchestrator's per-persona decision (carries angles + reason). */
  decision: ResponderDecision;

  /**
   * Phase B: prior contributions in this turn that the persona has
   * seen. Lets non-lead personas build on the lead's reasoning rather
   * than re-derive it. Empty array in Phase A; populated in Phase B
   * by the streaming coordinator.
   */
  priorContributions?: PriorContribution[];
}

/**
 * A contribution that's already been made by another persona this turn.
 * In Phase B's streaming model, each persona's render output is
 * broadcast as it arrives; later personas see the running thread.
 */
export interface PriorContribution {
  personaId: UUID;
  /**
   * The contribution text. May be partial (still streaming) or
   * complete; consumers check `isComplete`.
   */
  text: string;
  isComplete: boolean;
  postedAt: Date;
}
