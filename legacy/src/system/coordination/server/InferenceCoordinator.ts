/**
 * InferenceCoordinator - Hardware capacity guard for AI inference
 *
 * SINGLE RESPONSIBILITY: Prevent more concurrent requests to a provider
 * than its hardware/API can handle. Nothing else.
 *
 * Behavior: requests at-or-below capacity grant immediately. Requests above
 * capacity QUEUE FIFO and resolve when a slot frees, with a 60s ceiling. The
 * old "deny-immediately-on-full" semantics caused the silence-after-first-wave
 * bug on local-inference (capacity=1 on M5): exactly one persona's request was
 * granted, the other 13 were denied, the caller exited as wasRedundant, the
 * user saw "alive once, then dead." Queueing preserves plurality — every
 * persona that wanted to speak gets a turn, just slower under load.
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

import { RustCoreIPCClient } from '../../../../core/continuum-core/bindings/RustCoreIPC';

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
 * CRITICAL: legacy 'candle', 'sentinel', and 'local' all consume the same
 * local-inference capacity. Runtime persona chat should request 'local';
 * 'candle' remains a compatibility key for training/legacy callers.
 */
const PROVIDER_GROUPS: Record<string, string> = {
  'sentinel': 'local-inference',
  'candle': 'local-inference',
  'local': 'local-inference',
};

/**
 * Per-provider hardware/API concurrency limits.
 * These represent REAL constraints — not policy throttles.
 *
 * `local-inference` is resolved asynchronously from Rust's InferenceModule
 * IPC (`inference/capacity`) at startup — see `InferenceCoordinatorImpl.
 * initLocalCapacity`. Prior to that resolution, we default conservatively
 * to 1 so we never over-admit before we know what the hardware is. Once
 * Rust answers, `localInferenceCapacity` is updated in place and all
 * subsequent `requestSlot` calls see the real value.
 *
 * This removes the TS/Rust dual-formula drift that issue #887 fixes — the
 * formula now lives once in Rust's `system_resources::local_inference_
 * capacity()` and TS reads from it.
 */
const PROVIDER_CAPACITY: Record<string, number> = {
  'local-inference': 1,   // bootstrap default; overwritten via initLocalCapacity()
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

/** A persona waiting for a slot. Resolved when admitted (or rejected on timeout). */
interface PendingRequest {
  personaId: string;
  messageId: string;
  provider: string;
  enqueuedAt: number;
  resolve: (granted: boolean) => void;
  timer: ReturnType<typeof setTimeout>;
}

/**
 * Maximum time a persona will wait for a slot before timing out.
 *
 * Why 60s: covers worst-case warm M5 generation (~30s for 200 tokens cold +
 * ~20s for slot ahead) plus headroom. If a persona has waited 60s without
 * getting a turn, the queue is genuinely overloaded and dropping the request
 * is the right answer (caller treats as wasRedundant — another persona will
 * speak instead). Below 60s and we'd silently drop personas in normal load.
 *
 * This is the single explicit timeout in the admission path. It exists so
 * waiters never hang indefinitely if releaseSlot is missed (the cleanup
 * safety valve picks them up at 180s but that's the floor, not the ceiling).
 */
const SLOT_WAIT_TIMEOUT_MS = 60_000;

class InferenceCoordinatorImpl {
  private activeSlots: Map<string, InferenceSlot[]> = new Map();
  private waitQueues: Map<string, PendingRequest[]> = new Map();
  private localCapacityResolved = false;

  constructor() {
    for (const provider of Object.keys(PROVIDER_CAPACITY)) {
      this.activeSlots.set(provider, []);
    }
    // Fire-and-forget: ask Rust for the real local-inference capacity.
    // We don't await — constructor runs at module-load time and the Rust
    // IPC server may not be ready yet. We keep the conservative default
    // of 1 until the answer arrives. Failures are logged and retried.
    this.initLocalCapacity().catch(err => {
      console.warn('[InferenceCoordinator] initial capacity fetch failed, staying at default 1:', err?.message ?? err);
    });
  }

  /**
   * Pull `inference/capacity` from Rust (the single source of truth) and
   * update `PROVIDER_CAPACITY['local-inference']` in place. Retries on
   * transient IPC failure (the Rust socket may not be up yet at server
   * boot). Safe to call multiple times — idempotent once resolved.
   */
  private async initLocalCapacity(attempt = 1): Promise<void> {
    if (this.localCapacityResolved) return;
    const maxAttempts = 5;
    const delayMs = Math.min(2000 * attempt, 10_000);
    try {
      const capacity = await RustCoreIPCClient.getInstance().inferenceCapacity();
      PROVIDER_CAPACITY['local-inference'] = capacity;
      this.localCapacityResolved = true;
      console.log(`[InferenceCoordinator] local-inference capacity resolved via Rust IPC: ${capacity}`);
    } catch (err) {
      if (attempt >= maxAttempts) {
        console.warn(`[InferenceCoordinator] giving up on IPC capacity resolution after ${maxAttempts} attempts; staying at default 1`);
        return;
      }
      setTimeout(() => {
        this.initLocalCapacity(attempt + 1).catch(() => { /* logged in outer */ });
      }, delayMs);
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
   * @param provider - The inference provider (e.g., 'groq', 'candle', 'anthropic')
   * @param options - Reserved for future use (isMentioned no longer affects scheduling)
   * @returns true if slot acquired, false if provider at hardware capacity
   */
  async requestSlot(
    personaId: string,
    messageId: string,
    provider: string,
    _options?: { isMentioned?: boolean }
  ): Promise<boolean> {
    const slotKey = this.getSlotKey(provider);
    const slots = this.activeSlots.get(slotKey) || [];
    const maxConcurrent = this.capacity(slotKey);

    // Fast path: capacity available right now → grant immediately.
    if (slots.length < maxConcurrent) {
      this.grantSlot(slotKey, personaId, messageId, provider);
      return true;
    }

    // Slow path: at capacity → enqueue and wait.
    //
    // The previous behavior (return false immediately when full) caused the
    // silence-after-first-wave bug: with local-inference capacity = 1 on M5,
    // exactly one persona's slot was granted per message; the other 13 hit
    // this path, returned false, and the caller (PersonaResponseGenerator)
    // exited as wasRedundant. The user saw "alive once, then dead."
    //
    // Now: queue FIFO. When releaseSlot drains, the next waiter is granted
    // and resolves. Plurality preserved — slow under load, never silent.
    return new Promise<boolean>((resolve) => {
      const queue = this.waitQueues.get(slotKey) || [];
      const timer = setTimeout(() => {
        // Timeout: remove from queue if still pending, resolve false.
        const cur = this.waitQueues.get(slotKey) || [];
        const idx = cur.findIndex(r => r === pending);
        if (idx !== -1) {
          cur.splice(idx, 1);
          this.waitQueues.set(slotKey, cur);
        }
        resolve(false);
      }, SLOT_WAIT_TIMEOUT_MS);

      const pending: PendingRequest = {
        personaId,
        messageId,
        provider,
        enqueuedAt: Date.now(),
        resolve,
        timer,
      };
      queue.push(pending);
      this.waitQueues.set(slotKey, queue);
    });
  }

  /** Internal: actually claim a slot. Caller must already hold capacity. */
  private grantSlot(slotKey: string, personaId: string, messageId: string, provider: string): void {
    const slots = this.activeSlots.get(slotKey) || [];
    slots.push({ personaId, messageId, provider, acquiredAt: Date.now() });
    this.activeSlots.set(slotKey, slots);
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
      slots.splice(index, 1);
      this.activeSlots.set(slotKey, slots);
    }

    // Drain the queue: now that a slot is free, grant it to the next waiter.
    // FIFO — earliest enqueue gets the slot first. This is the plurality
    // mechanism: when 14 personas all decide to respond and capacity is 1,
    // they all eventually speak in arrival order rather than 13 being silenced.
    const queue = this.waitQueues.get(slotKey);
    if (queue && queue.length > 0) {
      const maxConcurrent = this.capacity(slotKey);
      const currentSlots = this.activeSlots.get(slotKey) || [];
      while (currentSlots.length < maxConcurrent && queue.length > 0) {
        const next = queue.shift()!;
        clearTimeout(next.timer);
        this.grantSlot(slotKey, next.personaId, next.messageId, next.provider);
        next.resolve(true);
        // Re-read because grantSlot mutated the map
        const refreshed = this.activeSlots.get(slotKey) || [];
        if (refreshed.length >= maxConcurrent) break;
      }
      this.waitQueues.set(slotKey, queue);
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
          cleaned++;
          return false;
        }
        return true;
      });
      this.activeSlots.set(provider, validSlots);
      // After cleanup, slots opened up — drain the queue for this provider
      // so waiters don't sit forever just because a release was missed.
      if (cleaned > 0) {
        const queue = this.waitQueues.get(provider);
        if (queue && queue.length > 0) {
          const maxConcurrent = this.capacity(provider);
          while (validSlots.length < maxConcurrent && queue.length > 0) {
            const next = queue.shift()!;
            clearTimeout(next.timer);
            this.grantSlot(provider, next.personaId, next.messageId, next.provider);
            next.resolve(true);
            const refreshed = this.activeSlots.get(provider) || [];
            if (refreshed.length >= maxConcurrent) break;
          }
          this.waitQueues.set(provider, queue);
        }
      }
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
