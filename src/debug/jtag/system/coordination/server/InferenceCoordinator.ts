/**
 * InferenceCoordinator - Hardware capacity guard for AI inference
 *
 * SINGLE RESPONSIBILITY: Prevent more concurrent requests to a provider
 * than its hardware/API can handle. Nothing else.
 *
 * What this does NOT do (handled elsewhere):
 * - Decide who should respond → AI cognition (should-respond LLM call)
 * - Limit responders per message → ChatCoordinationStream (maxResponders)
 * - Rate limit per persona → Not needed (AI cognition is the throttle)
 * - Stagger delays → Not needed (API clients handle rate limits with backoff)
 *
 * History: Previous design had 6 rules (card dealing, per-message responder
 * caps, reserved slots, cooldowns, stagger delays, auto-thinning queues).
 * This created a mechanical kill switch that overrode AI cognition.
 * Critically, a gating call (evaluateGating) consumed the persona's
 * "card" for a message via messageResponders tracking, so when the actual
 * response generation tried to acquire a slot with the same messageId,
 * every persona was denied — "already responded to message."
 *
 * The fix: strip to hardware capacity only. Provider concurrency limits
 * protect the infrastructure. Everything else is the AI's decision.
 */

export interface InferenceSlot {
  personaId: string;
  messageId: string;
  provider: string;
  acquiredAt: number;
}

/**
 * Provider groups that share the same backend.
 * All providers in a group share the same slot pool.
 *
 * CRITICAL: 'ollama', 'sentinel', 'candle', 'local' all route to the same
 * gRPC/Candle server which processes requests serially. They MUST share slots.
 */
const PROVIDER_GROUPS: Record<string, string> = {
  'ollama': 'local-inference',
  'sentinel': 'local-inference',
  'candle': 'local-inference',
  'local': 'local-inference',
};

/**
 * Per-provider hardware/API concurrency limits.
 * These represent REAL constraints — not policy throttles.
 */
const PROVIDER_CAPACITY: Record<string, number> = {
  'local-inference': 3,   // Worker pool with multiple model instances
  'anthropic': 15,        // Generous API limits
  'openai': 15,
  'groq': 5,             // Aggressive rate limits but decent concurrency
  'deepseek': 8,
  'xai': 8,
  'together': 10,
  'google': 10,
  'fireworks': 10,        // REST API, decent concurrency
  'alibaba': 8,           // Qwen/DashScope REST API
};

class InferenceCoordinatorImpl {
  private activeSlots: Map<string, InferenceSlot[]> = new Map();

  constructor() {
    for (const provider of Object.keys(PROVIDER_CAPACITY)) {
      this.activeSlots.set(provider, []);
    }
  }

  /**
   * Resolve provider to its slot group key.
   * Providers in the same group share the same slot pool.
   */
  private getSlotKey(provider: string): string {
    return PROVIDER_GROUPS[provider] || provider;
  }

  /**
   * Get hardware capacity for a provider slot group.
   */
  private capacity(slotKey: string): number {
    return PROVIDER_CAPACITY[slotKey] ?? 3;
  }

  /**
   * Request permission to perform inference.
   *
   * Only checks hardware capacity — can the provider handle another concurrent request?
   * All cognitive decisions (who responds, how many) are made upstream by
   * the coordination stream and should-respond LLM calls.
   *
   * @param personaId - The persona requesting the slot
   * @param messageId - The message being processed (for tracking/debugging)
   * @param provider - The inference provider (e.g., 'groq', 'ollama', 'anthropic')
   * @param options - Reserved for future use (isMentioned no longer affects scheduling)
   * @returns true if slot acquired, false if provider at hardware capacity
   */
  async requestSlot(
    personaId: string,
    messageId: string,
    provider: string,
    options?: { isMentioned?: boolean }
  ): Promise<boolean> {
    const slotKey = this.getSlotKey(provider);
    const maxConcurrent = this.capacity(slotKey);
    const slots = this.activeSlots.get(slotKey) || [];

    // The one rule: hardware capacity
    if (slots.length >= maxConcurrent) {
      console.log(`🎰 InferenceCoordinator: ${personaId.slice(0, 8)} denied — ${slotKey} at hardware capacity (${slots.length}/${maxConcurrent})`);
      return false;
    }

    // Acquire slot
    const slot: InferenceSlot = {
      personaId,
      messageId,
      provider,
      acquiredAt: Date.now()
    };
    slots.push(slot);
    this.activeSlots.set(slotKey, slots);

    console.log(`🎰 InferenceCoordinator: ${personaId.slice(0, 8)} GRANTED ${slotKey} slot (${slots.length}/${maxConcurrent})`);
    return true;
  }

  /**
   * Release slot after inference completes (success or failure).
   * MUST be called in both success and error paths.
   */
  releaseSlot(personaId: string, provider: string): void {
    const slotKey = this.getSlotKey(provider);
    const slots = this.activeSlots.get(slotKey);
    if (!slots) return;

    const index = slots.findIndex(s => s.personaId === personaId);
    if (index !== -1) {
      const slot = slots[index];
      const duration = Date.now() - slot.acquiredAt;
      slots.splice(index, 1);
      this.activeSlots.set(slotKey, slots);

      console.log(`🎰 InferenceCoordinator: ${personaId.slice(0, 8)} RELEASED ${slotKey} slot after ${duration}ms (${slots.length} remaining)`);
    }
  }

  /**
   * Get current coordinator stats for monitoring.
   */
  getStats(): {
    providers: Record<string, { active: number; max: number }>;
  } {
    const providers: Record<string, { active: number; max: number }> = {};
    for (const [provider, slots] of this.activeSlots) {
      providers[provider] = {
        active: slots.length,
        max: this.capacity(provider)
      };
    }
    return { providers };
  }

  /**
   * Clean up stale slots (safety valve if releaseSlot is missed due to crash).
   * Called periodically to prevent slot leaks.
   */
  cleanupStaleSlots(maxAgeMs: number = 180000): number {
    let cleaned = 0;
    const now = Date.now();

    for (const [provider, slots] of this.activeSlots) {
      const validSlots = slots.filter(slot => {
        if (now - slot.acquiredAt > maxAgeMs) {
          console.log(`🎰 InferenceCoordinator: Cleaning stale slot for ${slot.personaId.slice(0, 8)} (${provider}, age ${Math.round((now - slot.acquiredAt) / 1000)}s)`);
          cleaned++;
          return false;
        }
        return true;
      });
      this.activeSlots.set(provider, validSlots);
    }

    return cleaned;
  }
}

// Global singleton
export const InferenceCoordinator = new InferenceCoordinatorImpl();

// Safety valve: clean stale slots every 60 seconds
setInterval(() => {
  InferenceCoordinator.cleanupStaleSlots();
}, 60000);
