/**
 * LoRAAdapter - Represents a single LoRA skill adapter
 *
 * Philosophy: LoRA adapters are "just attributes" within PersonaUser
 * - Not separate processes
 * - Just weight files on disk
 * - Load/unload based on task domain
 *
 * Slingshot thinking: Don't carry all rocks at once - pick the right one for THIS shot
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * LoRA adapter state
 */
export interface LoRAAdapterState {
  /** Unique identifier for this adapter */
  id: UUID;

  /** Human-readable name (e.g., 'typescript-expertise', 'conversational') */
  name: string;

  /** Domain this adapter specializes in */
  domain: string;

  /** Path to adapter weights on disk (safetensors format) */
  path: string;

  /** Whether adapter is currently loaded in GPU memory */
  loaded: boolean;

  /** Last time adapter was used (for LRU eviction) */
  lastUsed: number;

  /** Memory footprint in MB (for budget tracking) */
  sizeMB: number;

  /** Whether fine-tuning is currently active on this adapter */
  trainingActive: boolean;

  /** Priority score (0.0-1.0) - higher = less likely to evict */
  priority: number;
}

/**
 * LoRAAdapter - Wrapper for a single LoRA skill adapter
 *
 * Responsibilities:
 * - Track adapter state (loaded, lastUsed, etc.)
 * - Provide interface for loading/unloading
 * - Support LRU eviction via lastUsed timestamp
 */
export class LoRAAdapter {
  private state: LoRAAdapterState;

  constructor(config: {
    id: UUID;
    name: string;
    domain: string;
    path: string;
    sizeMB: number;
    priority?: number;
  }) {
    this.state = {
      id: config.id,
      name: config.name,
      domain: config.domain,
      path: config.path,
      loaded: false,
      lastUsed: 0,
      sizeMB: config.sizeMB,
      trainingActive: false,
      priority: config.priority ?? 0.5
    };
  }

  /**
   * Get adapter ID
   */
  getId(): UUID {
    return this.state.id;
  }

  /**
   * Get adapter name
   */
  getName(): string {
    return this.state.name;
  }

  /**
   * Get adapter domain
   */
  getDomain(): string {
    return this.state.domain;
  }

  /**
   * Get adapter path on disk
   */
  getPath(): string {
    return this.state.path;
  }

  /**
   * Check if adapter is loaded in memory
   */
  isLoaded(): boolean {
    return this.state.loaded;
  }

  /**
   * Get last used timestamp
   */
  getLastUsed(): number {
    return this.state.lastUsed;
  }

  /**
   * Get adapter size in MB
   */
  getSize(): number {
    return this.state.sizeMB;
  }

  /**
   * Get adapter priority
   */
  getPriority(): number {
    return this.state.priority;
  }

  /**
   * Check if training is active
   */
  isTrainingActive(): boolean {
    return this.state.trainingActive;
  }

  /**
   * Get full adapter state (for serialization/debugging)
   */
  getState(): LoRAAdapterState {
    return { ...this.state };
  }

  /**
   * Load adapter weights from disk into GPU memory
   *
   * Phase 6: Stubbed - no actual Ollama integration yet
   * Phase 7: Will call Ollama API to load adapter weights
   */
  async load(): Promise<void> {
    if (this.state.loaded) {
      console.log(`🧬 LoRAAdapter: ${this.state.name} already loaded`);
      return;
    }

    console.log(`📥 LoRAAdapter: Loading ${this.state.name} from ${this.state.path}...`);

    // TODO (Phase 7): Actual Ollama integration
    // await OllamaClient.loadAdapter(this.state.path);

    this.state.loaded = true;
    this.state.lastUsed = Date.now();

    console.log(`✅ LoRAAdapter: ${this.state.name} loaded successfully`);
  }

  /**
   * Unload adapter weights from GPU memory
   *
   * Phase 6: Stubbed - no actual Ollama integration yet
   * Phase 7: Will call Ollama API to unload adapter weights
   */
  async unload(): Promise<void> {
    if (!this.state.loaded) {
      console.log(`🧬 LoRAAdapter: ${this.state.name} already unloaded`);
      return;
    }

    console.log(`📤 LoRAAdapter: Unloading ${this.state.name}...`);

    // TODO (Phase 7): Actual Ollama integration
    // await OllamaClient.unloadAdapter(this.state.id);

    this.state.loaded = false;

    console.log(`✅ LoRAAdapter: ${this.state.name} unloaded successfully`);
  }

  /**
   * Mark adapter as recently used (updates LRU timestamp)
   */
  markUsed(): void {
    this.state.lastUsed = Date.now();
  }

  /**
   * Enable fine-tuning mode for this adapter
   *
   * Phase 6: Stubbed - no actual training yet
   * Phase 7: Will enable gradient accumulation for fine-tuning
   */
  async enableTraining(): Promise<void> {
    if (!this.state.loaded) {
      throw new Error(`Cannot enable training - adapter ${this.state.name} not loaded`);
    }

    console.log(`🧬 LoRAAdapter: Enabling training mode for ${this.state.name}...`);

    // TODO (Phase 7): Actual fine-tuning integration
    // await OllamaClient.enableFineTuning(this.state.id);

    this.state.trainingActive = true;

    console.log(`✅ LoRAAdapter: Training mode enabled for ${this.state.name}`);
  }

  /**
   * Disable fine-tuning mode for this adapter
   *
   * Phase 6: Stubbed - no actual training yet
   * Phase 7: Will disable gradient accumulation and save weights
   */
  async disableTraining(): Promise<void> {
    if (!this.state.trainingActive) {
      return;
    }

    console.log(`🧬 LoRAAdapter: Disabling training mode for ${this.state.name}...`);

    // TODO (Phase 7): Actual fine-tuning integration
    // await OllamaClient.disableFineTuning(this.state.id);
    // await OllamaClient.saveAdapterWeights(this.state.path);

    this.state.trainingActive = false;

    console.log(`✅ LoRAAdapter: Training mode disabled for ${this.state.name}`);
  }

  /**
   * Calculate eviction score for LRU+Priority algorithm
   *
   * Lower score = more likely to evict
   * - Never evict adapters with priority > 0.9
   * - Balance recency (lastUsed) with importance (priority)
   *
   * Formula: lastUsed / (priority * 10)
   */
  calculateEvictionScore(): number {
    if (this.state.priority > 0.9) {
      return Infinity; // Never evict critical adapters
    }

    const now = Date.now();
    const ageSeconds = (now - this.state.lastUsed) / 1000;

    // Score = age / (priority * 10)
    // Higher score = more likely to evict
    return ageSeconds / (this.state.priority * 10);
  }
}
