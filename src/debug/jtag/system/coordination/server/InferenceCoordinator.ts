/**
 * InferenceCoordinator - RTOS-style fair scheduling for AI inference
 *
 * Problem: When a message arrives, ALL personas wake up simultaneously and try to
 * generate responses. Cloud AIs (15 slots) respond faster than local-inference (1 slot),
 * filling the MAX_RESPONDERS_PER_MESSAGE limit before local personas get through.
 *
 * Solution: RTOS-inspired fair scheduling with auto-thinning queues:
 *
 * 1. NEWEST-FIRST: Newer messages get priority (older ones are stale)
 * 2. CARD DEALING: Deal 1 slot per persona first (round-robin fairness)
 * 3. AUTO-THINNING: When overloaded, drop oldest requests
 * 4. RESERVED SLOTS: 1 of 5 slots reserved for local-inference
 * 5. PER-PERSONA CAP: Max N responses per persona per message window
 *
 * The card-dealing analogy:
 * - Deal one card (slot) to each persona first
 * - Then deal additional cards if capacity allows
 * - When deck is full, remove oldest cards (stale requests)
 * - Always deal to local-inference persona (reserved seat at table)
 *
 * This ensures local-inference personas (Helper AI with LoRA) get fair access
 * even though they're slower than cloud APIs.
 */

export interface InferenceSlot {
  personaId: string;
  messageId: string;
  provider: string;
  acquiredAt: number;
}

/**
 * Wait queue entry for fair scheduling
 */
export interface WaitQueueEntry {
  personaId: string;
  messageId: string;
  provider: string;
  requestedAt: number;
  isMentioned: boolean;
}

export interface ProviderLimits {
  maxConcurrent: number;
  staggerDelayMs: number;  // Random delay 0 to this value before each request
  cooldownMs: number;      // Minimum time between requests from same persona
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

const DEFAULT_PROVIDER_LIMITS: Record<string, ProviderLimits> = {
  // LOCAL INFERENCE GROUP: Worker pool with multiple model instances
  // Default 3 concurrent to match auto-detected workers (can be configured via INFERENCE_WORKERS)
  'local-inference': {
    maxConcurrent: 3,      // Worker pool handles concurrent requests
    staggerDelayMs: 50,    // Minimal stagger with pool
    cooldownMs: 200        // Reduced cooldown with concurrent capacity
  },
  'anthropic': {
    maxConcurrent: 15,     // API rate limits are generous
    staggerDelayMs: 100,
    cooldownMs: 100
  },
  'openai': {
    maxConcurrent: 15,
    staggerDelayMs: 100,
    cooldownMs: 100
  },
  'groq': {
    maxConcurrent: 5,      // Groq has aggressive rate limits but still decent
    staggerDelayMs: 500,
    cooldownMs: 1000
  },
  'deepseek': {
    maxConcurrent: 8,
    staggerDelayMs: 200,
    cooldownMs: 300
  },
  'xai': {
    maxConcurrent: 8,
    staggerDelayMs: 200,
    cooldownMs: 300
  },
  'together': {
    maxConcurrent: 10,
    staggerDelayMs: 200,
    cooldownMs: 300
  }
};

// ========== RTOS SCHEDULING CONSTANTS ==========

// Maximum responders per message (across all providers)
const MAX_RESPONDERS_PER_MESSAGE = 5;

// Reserved slots for local-inference (guaranteed seats at table)
// With worker pool, local-inference can handle multiple concurrent requests
const RESERVED_LOCAL_INFERENCE_SLOTS = 2;  // 2 of 5 slots reserved for local-inference
const MAX_CLOUD_RESPONDERS = MAX_RESPONDERS_PER_MESSAGE - RESERVED_LOCAL_INFERENCE_SLOTS;

// Stale request timeout - kick requests waiting too long (RTOS preemption)
const STALE_WAIT_TIMEOUT_MS = 20000;  // 20 seconds max wait (faster than before)

// Auto-thinning: Max pending requests per provider before dropping oldest
// When queue exceeds this, oldest entries are evicted (newest-first priority)
const MAX_PENDING_PER_PROVIDER = 3;

// Message age cutoff - messages older than this are deprioritized
const MESSAGE_FRESHNESS_MS = 30000;  // 30 seconds - newer messages get priority

// Card dealing: Max slots per persona per message window
// Ensures no single persona hogs all slots
const MAX_SLOTS_PER_PERSONA_PER_MESSAGE = 1;

class InferenceCoordinatorImpl {
  private activeSlots: Map<string, InferenceSlot[]> = new Map();  // slotKey -> slots
  private messageResponders: Map<string, Set<string>> = new Map(); // messageId -> persona IDs
  private messageProviders: Map<string, Set<string>> = new Map();  // messageId -> provider slot keys (for diversity)
  private lastRequestTime: Map<string, number> = new Map(); // personaId -> timestamp
  private providerLimits: Map<string, ProviderLimits> = new Map();
  private waitQueue: Map<string, WaitQueueEntry[]> = new Map(); // messageId -> waiting personas

  constructor() {
    // Initialize provider limits
    for (const [provider, limits] of Object.entries(DEFAULT_PROVIDER_LIMITS)) {
      this.providerLimits.set(provider, limits);
      this.activeSlots.set(provider, []);
    }
  }

  /**
   * Check if provider is local-inference group
   */
  private isLocalInference(provider: string): boolean {
    const slotKey = this.getSlotKey(provider);
    return slotKey === 'local-inference';
  }

  /**
   * Auto-thin queue when overloaded (RTOS preemption)
   *
   * Strategy: Newest-first priority
   * - When queue exceeds MAX_PENDING_PER_PROVIDER, drop oldest entries
   * - Stale messages (older than MESSAGE_FRESHNESS_MS) get deprioritized
   * - This ensures the system stays responsive even under load
   */
  private autoThinQueue(slotKey: string): number {
    const slots = this.activeSlots.get(slotKey) || [];
    const now = Date.now();
    let evicted = 0;

    // If under limit, no thinning needed
    if (slots.length <= MAX_PENDING_PER_PROVIDER) {
      return 0;
    }

    // Sort by age (oldest first) so we can evict oldest
    const sortedSlots = [...slots].sort((a, b) => a.acquiredAt - b.acquiredAt);

    // Evict oldest entries until under limit
    while (sortedSlots.length > MAX_PENDING_PER_PROVIDER) {
      const oldest = sortedSlots.shift()!;
      const age = now - oldest.acquiredAt;

      // Check if this is stale (older than freshness cutoff)
      if (age > MESSAGE_FRESHNESS_MS) {
        console.log(`🎰 AUTO-THIN: Evicting stale ${oldest.personaId} (age ${Math.round(age / 1000)}s > ${MESSAGE_FRESHNESS_MS / 1000}s freshness cutoff)`);
        evicted++;
      } else {
        // Even fresh entries get evicted if queue is too long
        console.log(`🎰 AUTO-THIN: Evicting ${oldest.personaId} to make room (queue ${slots.length} > max ${MAX_PENDING_PER_PROVIDER})`);
        evicted++;
      }
    }

    // Update slots with thinned list
    if (evicted > 0) {
      this.activeSlots.set(slotKey, sortedSlots);
    }

    return evicted;
  }

  /**
   * Check if persona has already responded to this message
   * (Card dealing: max 1 slot per persona per message)
   */
  private hasPersonaRespondedToMessage(personaId: string, messageId: string): boolean {
    const responders = this.messageResponders.get(messageId);
    return responders?.has(personaId) ?? false;
  }

  /**
   * Resolve provider to its slot group key.
   * Providers in the same group share the same slot pool.
   */
  private getSlotKey(provider: string): string {
    return PROVIDER_GROUPS[provider] || provider;
  }

  /**
   * Request permission to perform inference
   *
   * RTOS-style fair scheduling:
   * 1. @mentioned personas always get through (explicit user request)
   * 2. Local-inference has 1 reserved slot out of 5 responders
   * 3. Cloud providers share the remaining 4 slots
   * 4. Wait queue tracks who's been waiting longest for priority
   *
   * @returns true if slot acquired, false if should skip
   */
  async requestSlot(
    personaId: string,
    messageId: string,
    provider: string,
    options?: { isMentioned?: boolean }
  ): Promise<boolean> {
    // Resolve provider to slot group (e.g., 'ollama' → 'local-inference')
    const slotKey = this.getSlotKey(provider);
    const limits = this.providerLimits.get(slotKey) || DEFAULT_PROVIDER_LIMITS['local-inference'];
    const slots = this.activeSlots.get(slotKey) || [];
    const isLocal = this.isLocalInference(provider);

    // Get current message state
    const responders = this.messageResponders.get(messageId) || new Set();
    const providersResponded = this.messageProviders.get(messageId) || new Set();

    // Count local vs cloud responders for this message
    const localRespondersForMessage = Array.from(responders).filter(pid => {
      // Check if this persona responded via local-inference
      // (We track this in messageProviders)
      return providersResponded.has('local-inference');
    }).length;
    const cloudRespondersForMessage = responders.size - localRespondersForMessage;

    // ========== RTOS FAIR SCHEDULING LOGIC ==========

    // AUTO-THIN: Keep queue lean by evicting oldest entries
    const evicted = this.autoThinQueue(slotKey);
    if (evicted > 0) {
      console.log(`🎰 InferenceCoordinator: Auto-thinned ${evicted} stale entries from ${slotKey}`);
    }

    // Rule 0: @mentioned PRIORITY - but still respect hardware limits
    // CRITICAL FIX: @mentioned must STILL respect local-inference maxConcurrent
    // because the Rust gRPC backend can only process 1 request at a time (write lock)
    // Allowing multiple @mentioned to bypass causes 90s timeout cascade
    let skipOtherChecks = false;
    if (options?.isMentioned) {
      // For local-inference: respect maxConcurrent even for @mentioned
      if (isLocal && slots.length >= limits.maxConcurrent) {
        console.log(`🎰 InferenceCoordinator: ${personaId} @mentioned but local-inference at capacity (${slots.length}/${limits.maxConcurrent}) - DENIED`);
        return false;  // Cannot bypass hardware limits
      } else {
        console.log(`🎰 InferenceCoordinator: ${personaId} PRIORITY (@mentioned) for ${slotKey}`);
        skipOtherChecks = true;  // Skip other checks for mentioned personas
      }
    }

    // Non-mentioned personas (and @mentioned local that was denied above) go through full checks
    if (!skipOtherChecks) {
      // Rule 1: CARD DEALING - Max 1 response per persona per message
      if (this.hasPersonaRespondedToMessage(personaId, messageId)) {
        console.log(`🎰 InferenceCoordinator: ${personaId} denied - already responded to ${messageId.slice(0, 8)} (card dealing: 1 per persona)`);
        return false;
      }

      // Rule 2: Check absolute max responders
      if (responders.size >= MAX_RESPONDERS_PER_MESSAGE) {
        console.log(`🎰 InferenceCoordinator: ${personaId} denied - message ${messageId.slice(0, 8)} at max responders (${responders.size}/${MAX_RESPONDERS_PER_MESSAGE})`);
        return false;
      }

      // Rule 3: RESERVED SLOT - Local-inference gets guaranteed 1 slot
      if (isLocal) {
        // Local persona: check if reserved slot is available
        // Reserved slot means: even if 4 cloud responders, local still gets in
        const localAlreadyResponded = providersResponded.has('local-inference');
        if (localAlreadyResponded) {
          // Another local persona already responded - apply normal limit
          if (responders.size >= MAX_RESPONDERS_PER_MESSAGE) {
            console.log(`🎰 InferenceCoordinator: ${personaId} denied - local reserved slot already used`);
            return false;
          }
        }
        // Local persona gets through if under max (reserved slot guarantees access)
        console.log(`🎰 InferenceCoordinator: ${personaId} 🏠 using reserved local-inference slot`);
      } else {
        // Cloud persona: check against cloud-specific limit
        // Cloud can only use (MAX - reserved) slots = 4 slots
        if (cloudRespondersForMessage >= MAX_CLOUD_RESPONDERS) {
          console.log(`🎰 InferenceCoordinator: ${personaId} denied - cloud slots full (${cloudRespondersForMessage}/${MAX_CLOUD_RESPONDERS}), 1 reserved for local`);
          return false;
        }
      }

      // Rule 4: Per-provider concurrency limit
      if (slots.length >= limits.maxConcurrent) {
        console.log(`🎰 InferenceCoordinator: ${personaId} denied - ${slotKey} at capacity (${slots.length}/${limits.maxConcurrent})`);
        return false;
      }

      // Rule 5: Per-persona cooldown
      const lastRequest = this.lastRequestTime.get(personaId) || 0;
      const timeSinceLastRequest = Date.now() - lastRequest;
      if (timeSinceLastRequest < limits.cooldownMs) {
        console.log(`🎰 InferenceCoordinator: ${personaId} denied - cooldown (${timeSinceLastRequest}ms < ${limits.cooldownMs}ms)`);
        return false;
      }

      // Rule 6: Stagger delay (spread out requests)
      const staggerDelay = Math.random() * limits.staggerDelayMs;
      if (staggerDelay > 50) {
        console.log(`🎰 InferenceCoordinator: ${personaId} waiting ${Math.round(staggerDelay)}ms stagger`);
        await this.delay(staggerDelay);

        // Re-check after stagger
        const slotsAfterStagger = this.activeSlots.get(slotKey) || [];
        if (slotsAfterStagger.length >= limits.maxConcurrent) {
          console.log(`🎰 InferenceCoordinator: ${personaId} denied after stagger - ${slotKey} now full`);
          return false;
        }
      }
    }

    // ========== ACQUIRE SLOT ==========

    // Get current slots (re-fetch for freshness)
    const currentSlots = this.activeSlots.get(slotKey) || [];

    // Create slot
    const slot: InferenceSlot = {
      personaId,
      messageId,
      provider,
      acquiredAt: Date.now()
    };
    currentSlots.push(slot);
    this.activeSlots.set(slotKey, currentSlots);

    // Track responders and which providers responded
    responders.add(personaId);
    this.messageResponders.set(messageId, responders);
    providersResponded.add(slotKey);
    this.messageProviders.set(messageId, providersResponded);

    // Update last request time
    this.lastRequestTime.set(personaId, Date.now());

    const slotType = isLocal ? '🏠 LOCAL' : '☁️ CLOUD';
    console.log(`🎰 InferenceCoordinator: ${personaId} GRANTED ${slotType} slot (${currentSlots.length}/${limits.maxConcurrent}) [responders: ${responders.size}/${MAX_RESPONDERS_PER_MESSAGE}]`);
    return true;
  }

  /**
   * Release slot after inference completes (success or failure)
   */
  releaseSlot(personaId: string, provider: string): void {
    // Resolve provider to slot group
    const slotKey = this.getSlotKey(provider);
    const slots = this.activeSlots.get(slotKey);
    if (!slots) return;

    const index = slots.findIndex(s => s.personaId === personaId);
    if (index !== -1) {
      const slot = slots[index];
      slots.splice(index, 1);
      this.activeSlots.set(slotKey, slots);

      const duration = Date.now() - slot.acquiredAt;
      console.log(`🎰 InferenceCoordinator: ${personaId} RELEASED ${slotKey} slot after ${duration}ms (${slots.length} remaining)`);
    }
  }

  /**
   * Get current coordinator stats for monitoring
   */
  getStats(): {
    providers: Record<string, { active: number; max: number }>;
    scheduling: {
      maxResponders: number;
      reservedLocalSlots: number;
      maxCloudSlots: number;
      maxPendingPerProvider: number;
      messageFreshnessMs: number;
      maxSlotsPerPersona: number;
      activeMessages: number;
    };
  } {
    const providers: Record<string, { active: number; max: number }> = {};
    for (const [provider, slots] of this.activeSlots) {
      const limits = this.providerLimits.get(provider);
      providers[provider] = {
        active: slots.length,
        max: limits?.maxConcurrent || 0
      };
    }
    return {
      providers,
      scheduling: {
        maxResponders: MAX_RESPONDERS_PER_MESSAGE,
        reservedLocalSlots: RESERVED_LOCAL_INFERENCE_SLOTS,
        maxCloudSlots: MAX_CLOUD_RESPONDERS,
        maxPendingPerProvider: MAX_PENDING_PER_PROVIDER,
        messageFreshnessMs: MESSAGE_FRESHNESS_MS,
        maxSlotsPerPersona: MAX_SLOTS_PER_PERSONA_PER_MESSAGE,
        activeMessages: this.messageResponders.size
      }
    };
  }

  /**
   * Clean up stale slots (safety valve if releases are missed)
   * Call periodically to prevent slot leaks
   */
  cleanupStaleSlots(maxAgeMs: number = 180000): number {
    let cleaned = 0;
    const now = Date.now();

    for (const [provider, slots] of this.activeSlots) {
      const validSlots = slots.filter(slot => {
        if (now - slot.acquiredAt > maxAgeMs) {
          console.log(`🎰 InferenceCoordinator: Cleaning stale slot for ${slot.personaId} (${provider}, age ${now - slot.acquiredAt}ms)`);
          cleaned++;
          return false;
        }
        return true;
      });
      this.activeSlots.set(provider, validSlots);
    }

    // Also clean up old message responder/provider tracking
    const messageIds = Array.from(this.messageResponders.keys());
    // We don't have timestamps for messages, so just limit map size
    if (messageIds.length > 100) {
      // Keep newest 50
      const toRemove = messageIds.slice(0, messageIds.length - 50);
      for (const id of toRemove) {
        this.messageResponders.delete(id);
        this.messageProviders.delete(id);
      }
    }

    // Clean up wait queue (stale entries)
    for (const [messageId, queue] of this.waitQueue) {
      const validEntries = queue.filter(entry => {
        if (now - entry.requestedAt > STALE_WAIT_TIMEOUT_MS) {
          console.log(`🎰 InferenceCoordinator: Kicking stale wait entry for ${entry.personaId} (waited ${now - entry.requestedAt}ms)`);
          cleaned++;
          return false;
        }
        return true;
      });
      if (validEntries.length === 0) {
        this.waitQueue.delete(messageId);
      } else {
        this.waitQueue.set(messageId, validEntries);
      }
    }

    return cleaned;
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// Global singleton
export const InferenceCoordinator = new InferenceCoordinatorImpl();

// Start cleanup interval (every 60 seconds)
setInterval(() => {
  InferenceCoordinator.cleanupStaleSlots();
}, 60000);
