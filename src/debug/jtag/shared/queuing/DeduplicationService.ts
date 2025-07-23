/**
 * Generic Deduplication Service - Works with any type that has hashCode()
 */

export interface Hashable {
  hashCode(): string;
}

export interface DeduplicationConfig {
  enabled: boolean;
  windowMs: number; // Time window for deduplication
}

export class DeduplicationService<T extends Hashable> {
  private config: DeduplicationConfig;
  private hashMap = new Map<string, number>(); // hash -> timestamp
  private stats = { prevented: 0, checked: 0 };

  constructor(config: Partial<DeduplicationConfig> = {}) {
    this.config = {
      enabled: true,
      windowMs: 60000, // 1 minute default
      ...config
    };
  }

  /**
   * Check if item is duplicate within window
   * Returns true if item should be processed, false if duplicate
   */
  shouldProcess(item: T): boolean {
    this.stats.checked++;

    if (!this.config.enabled) {
      return true;
    }

    const hash = item.hashCode();
    const now = Date.now();

    // Clean old entries
    this.cleanOldEntries(now);

    // Check for recent duplicate
    const lastSeen = this.hashMap.get(hash);
    if (lastSeen && (now - lastSeen) < this.config.windowMs) {
      this.stats.prevented++;
      return false; // Duplicate found
    }

    // Record this item
    this.hashMap.set(hash, now);
    return true; // Should process
  }

  /**
   * Get deduplication statistics
   */
  get statistics() {
    const efficiency = this.stats.checked > 0 
      ? (this.stats.prevented / this.stats.checked) * 100 
      : 0;

    return {
      enabled: this.config.enabled,
      prevented: this.stats.prevented,
      checked: this.stats.checked,
      efficiency: Math.round(efficiency * 100) / 100,
      hashMapSize: this.hashMap.size
    };
  }

  /**
   * Clear all tracked hashes
   */
  clear(): void {
    this.hashMap.clear();
    this.stats = { prevented: 0, checked: 0 };
  }

  /**
   * Clean entries older than the deduplication window
   */
  private cleanOldEntries(now: number): void {
    const cutoff = now - this.config.windowMs;
    let cleaned = 0;

    for (const [hash, timestamp] of this.hashMap.entries()) {
      if (timestamp < cutoff) {
        this.hashMap.delete(hash);
        cleaned++;
      }
    }

    if (cleaned > 0) {
      console.log(`🧹 DeduplicationService: Cleaned ${cleaned} old entries`);
    }
  }
}