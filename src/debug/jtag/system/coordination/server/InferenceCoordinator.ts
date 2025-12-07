/**
 * InferenceCoordinator - Global singleton to prevent thundering herd on AI adapters
 *
 * Problem: When a message arrives, ALL personas wake up simultaneously and try to
 * generate responses. Ollama is single-threaded - 4 concurrent requests causes cascade
 * timeouts as requests queue up and exceed 15s/30s limits.
 *
 * Solution: Simple slot-based coordination:
 * 1. Before inference, persona requests a slot for (messageId, provider)
 * 2. If slots available, grant immediately. If not, reject.
 * 3. After inference completes (success or fail), release slot.
 *
 * This is a SIMPLE solution that doesn't require architectural changes.
 * Each adapter has its own concurrency limit (ollama=2, anthropic=10, etc.)
 */

export interface InferenceSlot {
  personaId: string;
  messageId: string;
  provider: string;
  acquiredAt: number;
}

export interface ProviderLimits {
  maxConcurrent: number;
  staggerDelayMs: number;  // Random delay 0 to this value before each request
  cooldownMs: number;      // Minimum time between requests from same persona
}

const DEFAULT_PROVIDER_LIMITS: Record<string, ProviderLimits> = {
  'ollama': {
    maxConcurrent: 3,      // Allow 3 concurrent for better throughput
    staggerDelayMs: 500,   // Minimal stagger - just prevent exact collision
    cooldownMs: 500        // Minimal cooldown
  },
  'sentinel': {
    maxConcurrent: 3,      // Match Ollama settings
    staggerDelayMs: 500,
    cooldownMs: 500
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

// Maximum responders per message (across all providers)
const MAX_RESPONDERS_PER_MESSAGE = 5;

class InferenceCoordinatorImpl {
  private activeSlots: Map<string, InferenceSlot[]> = new Map();  // provider -> slots
  private messageResponders: Map<string, Set<string>> = new Map(); // messageId -> persona IDs
  private lastRequestTime: Map<string, number> = new Map(); // personaId -> timestamp
  private providerLimits: Map<string, ProviderLimits> = new Map();

  constructor() {
    // Initialize provider limits
    for (const [provider, limits] of Object.entries(DEFAULT_PROVIDER_LIMITS)) {
      this.providerLimits.set(provider, limits);
      this.activeSlots.set(provider, []);
    }
  }

  /**
   * Request permission to perform inference
   *
   * @returns true if slot acquired, false if should skip
   */
  async requestSlot(
    personaId: string,
    messageId: string,
    provider: string,
    options?: { isMentioned?: boolean }
  ): Promise<boolean> {
    const limits = this.providerLimits.get(provider) || DEFAULT_PROVIDER_LIMITS['ollama'];
    const slots = this.activeSlots.get(provider) || [];

    // Check 1: Per-message responder limit (unless @mentioned)
    if (!options?.isMentioned) {
      const responders = this.messageResponders.get(messageId) || new Set();
      if (responders.size >= MAX_RESPONDERS_PER_MESSAGE) {
        console.log(`🎰 InferenceCoordinator: ${personaId} denied - message ${messageId.slice(0, 8)} already has ${responders.size} responders`);
        return false;
      }
    }

    // Check 2: Per-provider concurrency limit
    if (slots.length >= limits.maxConcurrent) {
      console.log(`🎰 InferenceCoordinator: ${personaId} denied - ${provider} at capacity (${slots.length}/${limits.maxConcurrent})`);
      return false;
    }

    // Check 3: Per-persona cooldown
    const lastRequest = this.lastRequestTime.get(personaId) || 0;
    const timeSinceLastRequest = Date.now() - lastRequest;
    if (timeSinceLastRequest < limits.cooldownMs) {
      console.log(`🎰 InferenceCoordinator: ${personaId} denied - cooldown (${timeSinceLastRequest}ms < ${limits.cooldownMs}ms)`);
      return false;
    }

    // Apply stagger delay (random to spread out requests)
    const staggerDelay = Math.random() * limits.staggerDelayMs;
    if (staggerDelay > 100) {
      console.log(`🎰 InferenceCoordinator: ${personaId} waiting ${Math.round(staggerDelay)}ms stagger delay`);
      await this.delay(staggerDelay);
    }

    // Re-check slots after stagger (another persona might have taken one)
    const currentSlots = this.activeSlots.get(provider) || [];
    if (currentSlots.length >= limits.maxConcurrent) {
      console.log(`🎰 InferenceCoordinator: ${personaId} denied after stagger - ${provider} now at capacity`);
      return false;
    }

    // Acquire slot
    const slot: InferenceSlot = {
      personaId,
      messageId,
      provider,
      acquiredAt: Date.now()
    };
    currentSlots.push(slot);
    this.activeSlots.set(provider, currentSlots);

    // Track responders per message
    const responders = this.messageResponders.get(messageId) || new Set();
    responders.add(personaId);
    this.messageResponders.set(messageId, responders);

    // Update last request time
    this.lastRequestTime.set(personaId, Date.now());

    console.log(`🎰 InferenceCoordinator: ${personaId} GRANTED slot for ${provider} (${currentSlots.length}/${limits.maxConcurrent})`);
    return true;
  }

  /**
   * Release slot after inference completes (success or failure)
   */
  releaseSlot(personaId: string, provider: string): void {
    const slots = this.activeSlots.get(provider);
    if (!slots) return;

    const index = slots.findIndex(s => s.personaId === personaId);
    if (index !== -1) {
      const slot = slots[index];
      slots.splice(index, 1);
      this.activeSlots.set(provider, slots);

      const duration = Date.now() - slot.acquiredAt;
      console.log(`🎰 InferenceCoordinator: ${personaId} RELEASED ${provider} slot after ${duration}ms (${slots.length} remaining)`);
    }
  }

  /**
   * Get current coordinator stats for monitoring
   */
  getStats(): Record<string, { active: number; max: number }> {
    const stats: Record<string, { active: number; max: number }> = {};
    for (const [provider, slots] of this.activeSlots) {
      const limits = this.providerLimits.get(provider);
      stats[provider] = {
        active: slots.length,
        max: limits?.maxConcurrent || 0
      };
    }
    return stats;
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

    // Also clean up old message responder tracking (older than 5 minutes)
    const messageIds = Array.from(this.messageResponders.keys());
    // We don't have timestamps for messages, so just limit map size
    if (messageIds.length > 100) {
      // Keep newest 50
      const toRemove = messageIds.slice(0, messageIds.length - 50);
      for (const id of toRemove) {
        this.messageResponders.delete(id);
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
