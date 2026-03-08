/**
 * PersonaTimingConfig - Single source of truth for all persona timing constants
 *
 * Centralizes magic numbers from PersonaState, PersonaInbox, PersonaMessageGate,
 * PersonaResponseGenerator, ContentDeduplicator, and BaseAIProviderAdapter.
 */

export const PersonaTimingConfig = {
  /** Adaptive cadence: max wait time before timeout (signal-based wakeup) */
  cadence: {
    idleMs: 1000,
    activeMs: 500,
    tiredMs: 2000,
    overwhelmedMs: 3000,
  },

  /** Energy thermodynamics (Tier 4: re-enabled with sane rates) */
  energy: {
    depletionRatePerMs: 0.000005,   // ~0.5% per 1s of processing
    recoveryRatePerMs: 0.000002,    // ~0.2% per 1s of rest (slower than depletion)
    attentionFatigueRate: 0.000001, // Gentle attention decay
    floor: 0.1,                     // Never fully dead — graceful degradation
  },

  /** Echo chamber detection */
  echoChamber: {
    windowMs: 2 * 60 * 1000,       // 2 minutes lookback
    aiMessageThreshold: 5,         // AI messages without human before gating
  },

  /** Content deduplication */
  dedup: {
    contentWindowMs: 60_000,       // Don't post same content within 60s
    maxEntriesPerPersona: 50,      // Max tracked entries per persona
    inboxWindowMs: 3000,           // Look back 3s for inbox duplicates
  },

  /** Message cache (in-memory, for post-inference validation) */
  messageCache: {
    maxPerRoom: 50,
  },

  /** Inbox priority aging (RTOS-style starvation prevention) */
  inbox: {
    agingRateMs: 30_000,           // Time for aging boost to reach maximum
    maxAgingBoost: 0.5,            // Maximum priority boost from aging
    maxSize: 1000,                 // Default max inbox size
    popTimeoutMs: 5000,            // Default pop timeout
    waitForWorkTimeoutMs: 30_000,  // Default waitForWork timeout
  },

  /** AI generation */
  generation: {
    timeoutMs: 180_000,            // 180s generous limit for local generation
    voiceMaxTokens: 800,           // Cap tokens for voice responses
  },

  /** Circuit breaker (Tier 4: re-enabled with sane threshold) */
  circuitBreaker: {
    maxConsecutiveFailures: 10,    // After 10 failures, open circuit
    cooldownMs: 60_000,            // 60s cooldown then reset
  },
} as const;
