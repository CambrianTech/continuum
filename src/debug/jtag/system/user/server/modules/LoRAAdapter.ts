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
import type { AIProviderAdapter } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

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
  private aiProvider?: AIProviderAdapter;

  constructor(config: {
    id: UUID;
    name: string;
    domain: string;
    path: string;
    sizeMB: number;
    priority?: number;
    aiProvider?: AIProviderAdapter;
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
    this.aiProvider = config.aiProvider;
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
   * Phase 6: Provider-agnostic via AIProviderAdapter.applySkill()
   * - If aiProvider supports applySkill, delegate to it
   * - Otherwise, just track state (stub for providers without skill support)
   */
  async load(): Promise<void> {
    if (this.state.loaded) {
      console.log(`🧬 LoRAAdapter: ${this.state.name} already loaded`);
      return;
    }

    console.log(`📥 LoRAAdapter: Loading ${this.state.name} from ${this.state.path}...`);

    // Delegate to AI provider if available and supports skill management
    if (this.aiProvider?.applySkill) {
      await this.aiProvider.applySkill({
        skillId: this.state.id,
        skillName: this.state.name,
        skillPath: this.state.path,
        domain: this.state.domain,
      });
    }
    // Otherwise, just track state (stub mode for providers without skill support)

    this.state.loaded = true;
    this.state.lastUsed = Date.now();

    console.log(`✅ LoRAAdapter: ${this.state.name} loaded successfully`);
  }

  /**
   * Unload adapter weights from GPU memory
   *
   * Phase 6: Provider-agnostic via AIProviderAdapter.removeSkill()
   * - If aiProvider supports removeSkill, delegate to it
   * - Otherwise, just track state (stub for providers without skill support)
   */
  async unload(): Promise<void> {
    if (!this.state.loaded) {
      console.log(`🧬 LoRAAdapter: ${this.state.name} already unloaded`);
      return;
    }

    console.log(`📤 LoRAAdapter: Unloading ${this.state.name}...`);

    // Delegate to AI provider if available and supports skill management
    if (this.aiProvider?.removeSkill) {
      await this.aiProvider.removeSkill(this.state.id);
    }
    // Otherwise, just track state (stub mode for providers without skill support)

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
   * Phase 6: Provider-agnostic via AIProviderAdapter.enableSkillTraining()
   * - If aiProvider supports training, delegate to it
   * - Otherwise, just track state (stub for providers without training support)
   */
  async enableTraining(): Promise<void> {
    if (!this.state.loaded) {
      throw new Error(`Cannot enable training - adapter ${this.state.name} not loaded`);
    }

    console.log(`🧬 LoRAAdapter: Enabling training mode for ${this.state.name}...`);

    // Delegate to AI provider if available and supports skill training
    if (this.aiProvider?.enableSkillTraining) {
      await this.aiProvider.enableSkillTraining(this.state.id);
    }
    // Otherwise, just track state (stub mode for providers without training support)

    this.state.trainingActive = true;

    console.log(`✅ LoRAAdapter: Training mode enabled for ${this.state.name}`);
  }

  /**
   * Disable fine-tuning mode for this adapter
   *
   * Phase 6: Provider-agnostic via AIProviderAdapter.disableSkillTraining()
   * - If aiProvider supports training, delegate to it
   * - Otherwise, just track state (stub for providers without training support)
   */
  async disableTraining(): Promise<void> {
    if (!this.state.trainingActive) {
      return;
    }

    console.log(`🧬 LoRAAdapter: Disabling training mode for ${this.state.name}...`);

    // Delegate to AI provider if available and supports skill training
    if (this.aiProvider?.disableSkillTraining) {
      await this.aiProvider.disableSkillTraining(this.state.id);
    }
    // Otherwise, just track state (stub mode for providers without training support)

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
