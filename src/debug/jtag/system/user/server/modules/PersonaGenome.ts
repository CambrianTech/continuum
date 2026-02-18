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
 * This is Phase 6 - adapter paging with PEFT/Candle training integration
 * Phase 7 will add continuous learning
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { LoRAAdapter, type LoRAAdapterState } from './LoRAAdapter';
import { generateUUID } from '../../../core/types/CrossPlatformUUID';
import type { AIProviderAdapter } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import type { RustCognitionBridge } from './RustCognitionBridge';
import type { GenomeAdapterInfo } from '../../../../shared/generated';
import { AdapterStore } from '../../../genome/server/AdapterStore';

/**
 * Genome configuration
 *
 * NOTE: GPU memory management is handled by the Rust inference-worker.
 * PersonaGenome just tracks logical state - the worker owns actual GPU allocation.
 */
export interface PersonaGenomeConfig {
  /** Base model name (e.g., 'deepseek-coder-v2', 'llama-3') */
  baseModel: string;

  /** Soft memory budget hint in MB (worker may override based on actual GPU) */
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
    trainedModelName?: string;
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

  /**
   * AI Provider adapter for actual skill loading (CandleAdapter, etc.)
   * When set, LoRAAdapter.load() will call aiProvider.applySkill() for real adapter loading.
   * Without this, adapters run in stub mode (just tracking state, no real GPU loading).
   */
  private aiProvider: AIProviderAdapter | null = null;

  /**
   * Rust cognition bridge for LRU eviction decisions.
   * When set, activateSkill() delegates decisions to Rust (sub-microsecond).
   * Without this, falls back to local TS logic (for tests/init before bridge is ready).
   */
  private rustBridge: RustCognitionBridge | null = null;

  constructor(config: PersonaGenomeConfig, logger: (message: string) => void) {
    this.log = logger;
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
   * Set the AI provider for real adapter loading
   *
   * This enables the genome vision - when set to CandleAdapter:
   * - LoRAAdapter.load() → CandleAdapter.applySkill() → InferenceWorker.loadAdapter()
   * - Multi-adapter composition at inference time
   * - Real GPU memory management
   *
   * Call this after construction when AIProviderDaemon is ready.
   * Updates all registered adapters to use this provider.
   */
  setAIProvider(provider: AIProviderAdapter): void {
    this.aiProvider = provider;
    this.log(`🧬 PersonaGenome: AI provider set to ${provider.providerId} (skill loading enabled)`);

    // Update all existing adapters to use this provider
    // This enables real loading for adapters registered before provider was set
    for (const [name, adapter] of this.availableAdapters.entries()) {
      adapter.setAIProvider(provider);
    }
    for (const [name, adapter] of this.activeAdapters.entries()) {
      adapter.setAIProvider(provider);
    }

    const totalAdapters = this.availableAdapters.size + this.activeAdapters.size;
    if (totalAdapters > 0) {
      this.log(`🧬 PersonaGenome: Updated ${totalAdapters} existing adapters with ${provider.providerId}`);
    }
  }

  /**
   * Get the current AI provider (for testing/debugging)
   */
  getAIProvider(): AIProviderAdapter | null {
    return this.aiProvider;
  }

  /**
   * Set Rust cognition bridge for sub-microsecond LRU decisions.
   * Call after PersonaUser creates the bridge.
   */
  setRustBridge(bridge: RustCognitionBridge): void {
    this.rustBridge = bridge;
    this.log(`🧬 PersonaGenome: Rust bridge set (LRU decisions delegated to Rust)`);
  }

  /**
   * Build GenomeAdapterInfo array for syncing to Rust.
   */
  private buildAdapterInfoForRust(): GenomeAdapterInfo[] {
    const result: GenomeAdapterInfo[] = [];

    for (const [, adapter] of this.activeAdapters) {
      const state = adapter.getState();
      result.push({
        name: state.name,
        domain: state.domain,
        size_mb: state.sizeMB,
        priority: state.priority,
        is_loaded: true,
        last_used_ms: state.lastUsed,
        ollama_model_name: state.trainedModelName ?? undefined,
      });
    }

    for (const [, adapter] of this.availableAdapters) {
      const state = adapter.getState();
      result.push({
        name: state.name,
        domain: state.domain,
        size_mb: state.sizeMB,
        priority: state.priority,
        is_loaded: false,
        last_used_ms: state.lastUsed,
        ollama_model_name: state.trainedModelName ?? undefined,
      });
    }

    return result;
  }

  /**
   * Sync current adapter state to Rust.
   * Call after adapter registration, load, or unload.
   */
  async syncToRust(): Promise<void> {
    if (!this.rustBridge) return;

    try {
      const adapters = this.buildAdapterInfoForRust();
      await this.rustBridge.genomeSync(adapters, this.config.memoryBudgetMB);
    } catch (error) {
      this.log(`⚠️ PersonaGenome: Rust sync failed: ${error}`);
    }
  }

  /**
   * Register a new adapter (adds to available pool, doesn't load)
   *
   * If aiProvider is set, the adapter will be able to do real GPU loading.
   * Otherwise, it operates in stub mode (tracking state only).
   */
  registerAdapter(config: {
    name: string;
    domain: string;
    path: string;
    sizeMB: number;
    priority?: number;
    trainedModelName?: string;
  }): void {
    const adapter = new LoRAAdapter({
      id: generateUUID() as UUID,
      name: config.name,
      domain: config.domain,
      path: config.path,
      sizeMB: config.sizeMB,
      priority: config.priority,
      trainedModelName: config.trainedModelName,
      aiProvider: this.aiProvider ?? undefined, // Pass provider for real loading
      logger: this.log
    });

    this.availableAdapters.set(config.name, adapter);

    const modelInfo = config.trainedModelName ? `, trained=${config.trainedModelName}` : '';
    const providerInfo = this.aiProvider ? ` [${this.aiProvider.providerId}]` : ' [stub mode]';
    this.log(`🧬 PersonaGenome: Registered adapter ${config.name} (${config.domain} domain, ${config.sizeMB}MB${modelInfo})${providerInfo}`);
  }

  /**
   * Activate a skill adapter for the current task.
   *
   * When Rust bridge is available: Rust decides what to evict/load (sub-μs),
   * TypeScript executes the GPU operations.
   *
   * When no Rust bridge (tests/init): Uses local TS logic.
   */
  async activateSkill(skillName: string): Promise<void> {
    if (this.rustBridge) {
      return this.activateSkillViaRust(skillName);
    }
    return this.activateSkillLocal(skillName);
  }

  /**
   * Activate adapter by domain name (not adapter name).
   * Searches registered adapters for one matching the given domain.
   * Falls back to activateSkill if an exact match is found.
   */
  async activateForDomain(domain: string): Promise<void> {
    // Search available and active adapters for one matching this domain
    for (const [name, adapter] of this.availableAdapters) {
      if (adapter.getDomain() === domain) {
        return this.activateSkill(name);
      }
    }
    for (const [name, adapter] of this.activeAdapters) {
      if (adapter.getDomain() === domain) {
        return this.activateSkill(name);
      }
    }
    // No adapter for this domain — that's OK, it's a gap
  }

  /**
   * Rust-backed skill activation: ONE IPC call for the decision,
   * then execute GPU ops based on Rust's instructions.
   */
  private async activateSkillViaRust(skillName: string): Promise<void> {
    const decision = await this.rustBridge!.genomeActivateSkill(
      skillName, this.config.memoryBudgetMB
    );

    if (!decision.activated) {
      return; // Unknown skill — Rust said no
    }

    // Cache hit — just update local state
    if (!decision.to_load) {
      const adapter = this.activeAdapters.get(skillName);
      if (adapter) {
        adapter.markUsed();
        this.currentAdapter = adapter;
      }
      return;
    }

    // Execute evictions (GPU unload)
    for (const evictedName of decision.evicted) {
      const victim = this.activeAdapters.get(evictedName);
      if (victim) {
        const freedMB = victim.getSize();
        this.log(`📤 PersonaGenome: Evicting ${evictedName} (Rust decision) to free ${freedMB}MB...`);
        await victim.unload();
        this.activeAdapters.delete(evictedName);
        this.memoryUsedMB -= freedMB;
      }
    }

    // Load the new adapter (GPU load)
    const adapter = this.availableAdapters.get(skillName);
    if (!adapter) return;

    await adapter.load(this.config.baseModel);
    this.activeAdapters.set(skillName, adapter);
    this.availableAdapters.delete(skillName);
    this.memoryUsedMB += adapter.getSize();
    this.currentAdapter = adapter;
  }

  /**
   * Local TS skill activation (for tests/init before Rust bridge is ready).
   */
  private async activateSkillLocal(skillName: string): Promise<void> {
    // Already active? Just mark as used
    if (this.activeAdapters.has(skillName)) {
      const adapter = this.activeAdapters.get(skillName)!;
      adapter.markUsed();
      this.currentAdapter = adapter;
      return;
    }

    // Check if adapter is registered
    const adapter = this.availableAdapters.get(skillName);
    if (!adapter) {
      return;
    }

    const adapterSize = adapter.getSize();

    // Evict if we're over our soft budget hint
    while (this.memoryUsedMB + adapterSize > this.config.memoryBudgetMB) {
      await this.evictLRU();
    }

    // Load adapter
    await adapter.load(this.config.baseModel);

    // Track logical state
    this.activeAdapters.set(skillName, adapter);
    this.availableAdapters.delete(skillName);
    this.memoryUsedMB += adapterSize;
    this.currentAdapter = adapter;
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

    const freedMB = victim.getSize();
    this.log(`📤 PersonaGenome: Evicting ${victimName} (score=${maxScore.toFixed(2)}) to free ${freedMB}MB...`);

    // Unload adapter
    await victim.unload();

    // Move from active back to available
    this.activeAdapters.delete(victimName);
    this.memoryUsedMB -= freedMB;

    // NOTE: GPU memory is managed by the Rust gpu_allocator module
    // PersonaGenome just tracks logical state here

    this.log(`✅ PersonaGenome: Evicted ${victimName} (memory: ${this.memoryUsedMB}/${this.config.memoryBudgetMB}MB)`);
  }

  /**
   * Enable fine-tuning mode for the current adapter
   *
   * Phase 6: Stubbed - no actual training yet
   * Phase 7: Will enable continuous learning with PEFT
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
   * Phase 7: Will save updated adapter weights to disk
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
   * Get adapter by trait name (domain)
   *
   * Checks both active and available adapters.
   * Prefers active adapters since they're already loaded.
   */
  getAdapterByTrait(traitName: string): LoRAAdapter | null {
    // Check active adapters first (already loaded, faster)
    if (this.activeAdapters.has(traitName)) {
      return this.activeAdapters.get(traitName)!;
    }

    // Check available adapters (registered but not loaded)
    if (this.availableAdapters.has(traitName)) {
      return this.availableAdapters.get(traitName)!;
    }

    return null;
  }

  /**
   * Check if an adapter exists (active or registered)
   */
  hasAdapter(adapterName: string): boolean {
    return this.activeAdapters.has(adapterName) || this.availableAdapters.has(adapterName);
  }

  /**
   * Get list of all active adapters
   */
  getActiveAdapters(): LoRAAdapter[] {
    return Array.from(this.activeAdapters.values());
  }

  /**
   * Get active adapters in format suitable for TextGenerationRequest
   *
   * This is the bridge between PersonaGenome and the AI provider system.
   * Returns adapter info that CandleAdapter can use to load/apply LoRA weights.
   */
  getActiveAdaptersForRequest(): Array<{ name: string; path: string; domain: string; scale: number }> {
    const result: Array<{ name: string; path: string; domain: string; scale: number }> = [];

    for (const adapter of this.activeAdapters.values()) {
      if (!adapter.isLoaded()) continue;

      const state = adapter.getState();

      // Validate path exists on disk — reject stale/missing adapters at the boundary
      if (!AdapterStore.isValidAdapterPath(state.path)) {
        this.log(`⚠️ PersonaGenome: Skipping adapter ${state.name} — path does not exist: ${state.path}`);
        continue;
      }

      result.push({
        name: state.name,
        path: state.path,
        domain: state.domain,
        scale: 1.0,
      });
    }

    return result;
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
