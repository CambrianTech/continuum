/**
 * PersonaGenome - LoRA adapter paging system (virtual memory for skills)
 *
 * Philosophy: Slingshot thinking - don't carry all rocks at once
 * - Limited GPU memory (the slingshot pouch)
 * - Many specialized skills (the pile of rocks)
 * - Page adapters in/out based on current task (pick the right rock for THIS shot)
 * - LRU eviction when memory full (reload as needed)
 *
 * Architecture inspired by OS virtual memory:
 * - activeAdapters = pages in RAM (GPU memory)
 * - availableAdapters = pages on disk (safetensors files)
 * - memoryBudget = RAM limit
 * - LRU eviction = page replacement algorithm
 *
 * This is Phase 6 - adapter paging WITHOUT actual Ollama training
 * Phase 7 will add real fine-tuning integration
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { LoRAAdapter, type LoRAAdapterState } from './LoRAAdapter';
import { randomUUID } from 'crypto';

/**
 * Genome configuration
 */
export interface PersonaGenomeConfig {
  /** Base model name (e.g., 'deepseek-coder-v2', 'llama-3') */
  baseModel: string;

  /** Maximum GPU memory for adapters in MB */
  memoryBudgetMB: number;

  /** Path to LoRA adapters directory on disk */
  adaptersPath: string;

  /** Initial adapters to register (but not load) */
  initialAdapters?: Array<{
    name: string;
    domain: string;
    path: string;
    sizeMB: number;
    priority?: number;
  }>;
}

/**
 * Genome state (for serialization/debugging)
 */
export interface PersonaGenomeState {
  baseModel: string;
  memoryBudgetMB: number;
  memoryUsedMB: number;
  memoryPressure: number; // 0.0-1.0 (percentage of budget used)
  activeAdapters: LoRAAdapterState[];
  availableAdapters: LoRAAdapterState[];
  currentAdapter: string | null;
  learningMode: boolean;
}

/**
 * PersonaGenome - LoRA adapter paging system
 *
 * Responsibilities:
 * - Register available adapters (on disk)
 * - Load/unload adapters based on task domain
 * - Track memory usage and enforce budget
 * - LRU eviction when memory full
 * - Enable fine-tuning mode for training tasks
 */
export class PersonaGenome {
  private config: PersonaGenomeConfig;

  /** Adapters currently loaded in GPU memory */
  private activeAdapters: Map<string, LoRAAdapter> = new Map();

  /** Adapters registered but not loaded (on disk) */
  private availableAdapters: Map<string, LoRAAdapter> = new Map();

  /** Current memory usage in MB */
  private memoryUsedMB: number = 0;

  /** Currently active adapter (last activated) */
  private currentAdapter: LoRAAdapter | null = null;

  /** Whether fine-tuning is currently active */
  private learningMode: boolean = false;

  /** Logger function */
  private log: (message: string) => void;

  constructor(config: PersonaGenomeConfig, logger?: (message: string) => void) {
    this.log = logger || console.log.bind(console);
    this.config = config;

    // Register initial adapters (but don't load them yet)
    if (config.initialAdapters) {
      for (const adapterConfig of config.initialAdapters) {
        this.registerAdapter(adapterConfig);
      }
    }

    this.log(`🧬 PersonaGenome: Initialized with base model ${config.baseModel}, memory budget ${config.memoryBudgetMB}MB`);
  }

  /**
   * Register a new adapter (adds to available pool, doesn't load)
   */
  registerAdapter(config: {
    name: string;
    domain: string;
    path: string;
    sizeMB: number;
    priority?: number;
  }): void {
    const adapter = new LoRAAdapter({
      id: randomUUID() as UUID,
      name: config.name,
      domain: config.domain,
      path: config.path,
      sizeMB: config.sizeMB,
      priority: config.priority,
      logger: this.log
    });

    this.availableAdapters.set(config.name, adapter);

    this.log(`🧬 PersonaGenome: Registered adapter ${config.name} (${config.domain} domain, ${config.sizeMB}MB)`);
  }

  /**
   * Activate a skill adapter for the current task
   *
   * If already loaded: Just switch to it and update lastUsed
   * If not loaded: Load from disk (evicting LRU adapters if needed)
   *
   * This is the KEY method that implements virtual memory paging
   */
  async activateSkill(skillName: string): Promise<void> {
    // Already active? Just mark as used
    if (this.activeAdapters.has(skillName)) {
      const adapter = this.activeAdapters.get(skillName)!;
      adapter.markUsed();
      this.currentAdapter = adapter;
      // this.log(`🧬 PersonaGenome: Skill ${skillName} already active (cache hit)`);
      return;
    }

    // Check if adapter is registered
    const adapter = this.availableAdapters.get(skillName);
    if (!adapter) {
      // this.log(`⚠️ PersonaGenome: Skill ${skillName} not registered - cannot activate`);
      return;
    }

    // this.log(`🧬 PersonaGenome: Activating skill ${skillName} (cache miss - paging in)...`);

    // Check if we need to evict adapters to make space
    const adapterSize = adapter.getSize();
    while (this.memoryUsedMB + adapterSize > this.config.memoryBudgetMB) {
      await this.evictLRU();
    }

    // Load adapter from disk
    await adapter.load();

    // Move from available to active
    this.activeAdapters.set(skillName, adapter);
    this.memoryUsedMB += adapterSize;
    this.currentAdapter = adapter;

    // this.log(`✅ PersonaGenome: Skill ${skillName} activated (memory: ${this.memoryUsedMB}/${this.config.memoryBudgetMB}MB)`);
  }

  /**
   * Evict least-recently-used adapter from memory
   *
   * Uses weighted LRU algorithm:
   * - Never evict adapters with priority > 0.9
   * - Score = age_seconds / (priority * 10)
   * - Higher score = more likely to evict
   */
  async evictLRU(): Promise<void> {
    if (this.activeAdapters.size === 0) {
      this.log(`⚠️ PersonaGenome: No adapters to evict`);
      return;
    }

    // Find adapter with highest eviction score
    let maxScore = -Infinity;
    let victimName: string | null = null;
    let victim: LoRAAdapter | null = null;

    for (const [name, adapter] of this.activeAdapters.entries()) {
      const score = adapter.calculateEvictionScore();
      if (score > maxScore) {
        maxScore = score;
        victimName = name;
        victim = adapter;
      }
    }

    if (!victim || !victimName) {
      this.log(`⚠️ PersonaGenome: No evictable adapters found`);
      return;
    }

    this.log(`📤 PersonaGenome: Evicting ${victimName} (score=${maxScore.toFixed(2)}) to free ${victim.getSize()}MB...`);

    // Unload adapter
    await victim.unload();

    // Move from active back to available
    this.activeAdapters.delete(victimName);
    this.memoryUsedMB -= victim.getSize();

    this.log(`✅ PersonaGenome: Evicted ${victimName} (memory: ${this.memoryUsedMB}/${this.config.memoryBudgetMB}MB)`);
  }

  /**
   * Enable fine-tuning mode for the current adapter
   *
   * Phase 6: Stubbed - no actual training yet
   * Phase 7: Will enable gradient accumulation in Ollama
   */
  async enableLearningMode(skillName: string): Promise<void> {
    if (!this.activeAdapters.has(skillName)) {
      throw new Error(`Cannot enable learning mode - adapter ${skillName} not loaded`);
    }

    const adapter = this.activeAdapters.get(skillName)!;
    await adapter.enableTraining();

    this.learningMode = true;

    this.log(`🧬 PersonaGenome: Learning mode enabled for ${skillName}`);
  }

  /**
   * Disable fine-tuning mode for the current adapter
   *
   * Phase 6: Stubbed - no actual training yet
   * Phase 7: Will save updated weights to disk
   */
  async disableLearningMode(skillName: string): Promise<void> {
    if (!this.activeAdapters.has(skillName)) {
      return;
    }

    const adapter = this.activeAdapters.get(skillName)!;
    await adapter.disableTraining();

    this.learningMode = false;

    this.log(`🧬 PersonaGenome: Learning mode disabled for ${skillName}`);
  }

  /**
   * Get memory pressure (0.0-1.0)
   */
  getMemoryPressure(): number {
    return this.memoryUsedMB / this.config.memoryBudgetMB;
  }

  /**
   * Get current memory usage in MB
   */
  getMemoryUsed(): number {
    return this.memoryUsedMB;
  }

  /**
   * Get memory budget in MB
   */
  getMemoryBudget(): number {
    return this.config.memoryBudgetMB;
  }

  /**
   * Check if learning mode is active
   */
  isLearningMode(): boolean {
    return this.learningMode;
  }

  /**
   * Get currently active adapter
   */
  getCurrentAdapter(): LoRAAdapter | null {
    return this.currentAdapter;
  }

  /**
   * Get list of all active adapters
   */
  getActiveAdapters(): LoRAAdapter[] {
    return Array.from(this.activeAdapters.values());
  }

  /**
   * Get list of all available adapters (active + on disk)
   */
  getAllAdapters(): LoRAAdapter[] {
    const all = new Map<string, LoRAAdapter>();

    // Add all available adapters
    for (const [name, adapter] of this.availableAdapters.entries()) {
      all.set(name, adapter);
    }

    // Override with active adapters (which have updated state)
    for (const [name, adapter] of this.activeAdapters.entries()) {
      all.set(name, adapter);
    }

    return Array.from(all.values());
  }

  /**
   * Get full genome state (for serialization/debugging)
   */
  getState(): PersonaGenomeState {
    return {
      baseModel: this.config.baseModel,
      memoryBudgetMB: this.config.memoryBudgetMB,
      memoryUsedMB: this.memoryUsedMB,
      memoryPressure: this.getMemoryPressure(),
      activeAdapters: Array.from(this.activeAdapters.values()).map(a => a.getState()),
      availableAdapters: Array.from(this.availableAdapters.values()).map(a => a.getState()),
      currentAdapter: this.currentAdapter?.getName() ?? null,
      learningMode: this.learningMode
    };
  }

  /**
   * Unload all adapters (cleanup on shutdown)
   */
  async shutdown(): Promise<void> {
    this.log(`🧬 PersonaGenome: Shutting down - unloading ${this.activeAdapters.size} adapters...`);

    for (const adapter of this.activeAdapters.values()) {
      await adapter.unload();
    }

    this.activeAdapters.clear();
    this.memoryUsedMB = 0;
    this.currentAdapter = null;

    this.log(`✅ PersonaGenome: Shutdown complete`);
  }
}
