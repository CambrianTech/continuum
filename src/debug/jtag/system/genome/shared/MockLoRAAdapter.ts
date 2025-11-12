/**
 * MockLoRAAdapter - In-memory LoRA adapter for testing (no GPU required)
 *
 * This is a simple mock that simulates adapter loading/unloading
 * without actually touching GPU memory or real .safetensors files.
 *
 * Used for:
 * - Unit testing genome daemon logic
 * - Integration testing multi-persona scenarios
 * - Development without GPU hardware
 *
 * Phase 7: Use mocks for all testing
 * Phase 8: Replace with real Ollama/HuggingFace adapters
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Adapter configuration
 */
export interface MockLoRAConfig {
  id: UUID;
  name: string;          // e.g., 'wine-expertise', 'typescript-expertise'
  domain: string;        // e.g., 'knowledge', 'personality', 'code'
  sizeMB: number;        // Simulated memory size
  priority?: number;     // 0.0-1.0 (higher = keep in memory longer)
}

/**
 * Adapter state (for serialization/inspection)
 */
export interface MockLoRAState {
  id: UUID;
  name: string;
  domain: string;
  sizeMB: number;
  priority: number;
  loaded: boolean;
  loadedAt: number | null;
  lastUsedAt: number | null;
  usageCount: number;
}

/**
 * MockLoRAAdapter - Simulates LoRA adapter lifecycle
 *
 * Key behaviors:
 * - load(): Simulates disk I/O with delay
 * - unload(): Simulates GPU memory release
 * - markUsed(): Updates LRU tracking
 * - calculateEvictionScore(): For LRU algorithm
 */
export class MockLoRAAdapter {
  private config: MockLoRAConfig;

  // State
  private loaded: boolean = false;
  private loadedAt: number | null = null;
  private lastUsedAt: number | null = null;
  private usageCount: number = 0;

  constructor(config: MockLoRAConfig) {
    this.config = config;
  }

  /**
   * Get adapter ID
   */
  getId(): UUID {
    return this.config.id;
  }

  /**
   * Get adapter name
   */
  getName(): string {
    return this.config.name;
  }

  /**
   * Get domain
   */
  getDomain(): string {
    return this.config.domain;
  }

  /**
   * Get memory size in MB
   */
  getSize(): number {
    return this.config.sizeMB;
  }

  /**
   * Get priority (0.0-1.0)
   */
  getPriority(): number {
    return this.config.priority ?? 0.5;
  }

  /**
   * Check if adapter is currently loaded
   */
  isLoaded(): boolean {
    return this.loaded;
  }

  /**
   * Get timestamp when adapter was loaded
   */
  getLoadedAt(): number | null {
    return this.loadedAt;
  }

  /**
   * Get timestamp of last use
   */
  getLastUsedAt(): number | null {
    return this.lastUsedAt;
  }

  /**
   * Get total usage count
   */
  getUsageCount(): number {
    return this.usageCount;
  }

  /**
   * Load adapter into "memory" (simulated)
   *
   * Simulates disk I/O delay based on adapter size:
   * - Small adapters (<256MB): 50ms
   * - Medium adapters (256-512MB): 100ms
   * - Large adapters (>512MB): 200ms
   *
   * @throws Error if already loaded
   */
  async load(): Promise<void> {
    if (this.loaded) {
      throw new Error(`Adapter ${this.config.name} is already loaded`);
    }

    // Simulate disk I/O delay
    const delayMs = this.config.sizeMB < 256 ? 50 : this.config.sizeMB < 512 ? 100 : 200;
    await new Promise(resolve => setTimeout(resolve, delayMs));

    this.loaded = true;
    this.loadedAt = Date.now();
    this.lastUsedAt = Date.now();
    this.usageCount++;

    console.log(`🧬 MockLoRAAdapter: Loaded ${this.config.name} (${this.config.sizeMB}MB, delay: ${delayMs}ms)`);
  }

  /**
   * Unload adapter from "memory" (simulated)
   *
   * @throws Error if not loaded
   */
  async unload(): Promise<void> {
    if (!this.loaded) {
      throw new Error(`Adapter ${this.config.name} is not loaded`);
    }

    // Simulate GPU memory release (instant)
    this.loaded = false;
    this.loadedAt = null;

    console.log(`🧬 MockLoRAAdapter: Unloaded ${this.config.name}`);
  }

  /**
   * Mark adapter as used (for LRU tracking)
   *
   * Updates lastUsedAt timestamp and increments usage count.
   * Called when adapter is activated or used for inference.
   */
  markUsed(): void {
    this.lastUsedAt = Date.now();
    this.usageCount++;
  }

  /**
   * Calculate eviction score (for LRU algorithm)
   *
   * Higher score = more likely to evict
   *
   * Algorithm:
   * - age_seconds = time since last use
   * - priority_weight = adapter priority (0.0-1.0)
   * - score = age_seconds / (priority_weight * 10)
   *
   * Special cases:
   * - Priority > 0.9: Never evict (score = -Infinity)
   * - Not loaded: Cannot evict (score = -Infinity)
   * - Never used: Score = age since load / priority
   *
   * @returns Eviction score (higher = evict first)
   */
  calculateEvictionScore(): number {
    // Cannot evict if not loaded
    if (!this.loaded) {
      return -Infinity;
    }

    // Never evict high-priority adapters
    const priority = this.getPriority();
    if (priority > 0.9) {
      return -Infinity;
    }

    // Calculate age in seconds
    const now = Date.now();
    const lastUsed = this.lastUsedAt || this.loadedAt || now;
    const ageSeconds = (now - lastUsed) / 1000;

    // Weight by priority (higher priority = lower score)
    const priorityWeight = Math.max(priority, 0.1);  // Avoid division by zero
    const score = ageSeconds / (priorityWeight * 10);

    return score;
  }

  /**
   * Get adapter state (for serialization/debugging)
   */
  getState(): MockLoRAState {
    return {
      id: this.config.id,
      name: this.config.name,
      domain: this.config.domain,
      sizeMB: this.config.sizeMB,
      priority: this.getPriority(),
      loaded: this.loaded,
      loadedAt: this.loadedAt,
      lastUsedAt: this.lastUsedAt,
      usageCount: this.usageCount
    };
  }

  /**
   * Get human-readable description
   */
  toString(): string {
    const status = this.loaded ? 'loaded' : 'unloaded';
    return `MockLoRAAdapter(${this.config.name}, ${this.config.sizeMB}MB, ${status})`;
  }
}
