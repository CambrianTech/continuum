/**
 * GenomeManager - Holistic LoRA Orchestration & GPU Resource Management
 *
 * Philosophy: "One system that helps you coordinate without overloading"
 *
 * Responsibilities:
 * - GPU resource tracking (total availability across all GPUs)
 * - LoRA adapter paging (load/unload genomic layers dynamically)
 * - Training job queue (prevent GPU oversubscription)
 * - Adapter registry (which PersonaUsers have which LoRA layers loaded)
 * - Provider coordination (PEFT, OpenAI, DeepSeek adapters)
 *
 * Architecture:
 * - Singleton pattern (one manager for entire system)
 * - Uses adapter pattern (PEFTLoRAAdapter, OpenAILoRAAdapter, etc.)
 * - RTOS-inspired resource management (never oversubscribe GPU)
 * - Graceful degradation (fall back to base models when GPU full)
 *
 * SERVER-ONLY: Uses Node.js for GPU detection and process management
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { TraitType } from '../../../genome/entities/GenomeLayerEntity';
import type {
  LoRATrainingRequest,
  LoRATrainingResult,
  TrainingJob,
  TrainingJobStatus,
  FineTuningStrategy
} from '../shared/FineTuningTypes';
import type { LoRATrainer } from '../shared/BaseLoRATrainer';

/**
 * GPU Resource Status
 */
export interface GPUResource {
  id: string;                    // GPU identifier (cuda:0, metal:0, etc.)
  name: string;                  // Human-readable name (NVIDIA RTX 4090, Apple M3 Max, etc.)
  totalMemoryMB: number;         // Total GPU memory in MB
  availableMemoryMB: number;     // Currently available memory in MB
  utilization: number;           // 0.0 - 1.0 (percentage)
  temperature?: number;          // Celsius (if available)
  powerDraw?: number;            // Watts (if available)
}

/**
 * Loaded LoRA Adapter in GPU Memory
 */
export interface LoadedAdapter {
  personaId: UUID;
  traitType: TraitType;
  modelPath: string;             // Path to .safetensors or .gguf file
  memoryUsageMB: number;         // How much GPU memory this adapter uses
  loadedAt: number;              // Timestamp when loaded
  lastUsedAt: number;            // Timestamp of last inference
  useCount: number;              // How many times used since loaded
}

/**
 * Training Queue Entry
 */
interface TrainingQueueEntry {
  job: TrainingJob;
  request: LoRATrainingRequest;
  trainer: LoRATrainer;
  priority: number;              // Higher = train sooner (0-100)
}

/**
 * GenomeManager Configuration
 */
export interface GenomeManagerConfig {
  // GPU resource limits
  maxGPUMemoryUsagePercent?: number;    // Max % of GPU memory to use (default: 80%)
  maxConcurrentTraining?: number;        // Max training jobs at once (default: 1)

  // LoRA paging strategy
  maxLoadedAdapters?: number;            // Max adapters in GPU memory (default: 5)
  adapterEvictionStrategy?: 'lru' | 'lfu' | 'hybrid';  // default: 'hybrid'

  // Training queue
  enableTrainingQueue?: boolean;         // Enable queued training (default: true)
  maxQueuedJobs?: number;                // Max jobs in queue (default: 10)
}

/**
 * GenomeManager - Singleton orchestrator for all LoRA operations
 *
 * MVP Status: Interface structure only (Phase 7.0)
 * Full Implementation: Phase 7.1+
 */
export class GenomeManager {
  private static instance: GenomeManager | null = null;

  private config: Required<GenomeManagerConfig>;
  private gpuResources: GPUResource[] = [];
  private loadedAdapters: Map<string, LoadedAdapter> = new Map();
  private trainingQueue: TrainingQueueEntry[] = [];
  private activeTrainingJobs: Map<UUID, TrainingJob> = new Map();
  private adapters: Map<string, LoRATrainer> = new Map();

  private constructor(config: Partial<GenomeManagerConfig> = {}) {
    this.config = {
      maxGPUMemoryUsagePercent: config.maxGPUMemoryUsagePercent ?? 80,
      maxConcurrentTraining: config.maxConcurrentTraining ?? 1,
      maxLoadedAdapters: config.maxLoadedAdapters ?? 5,
      adapterEvictionStrategy: config.adapterEvictionStrategy ?? 'hybrid',
      enableTrainingQueue: config.enableTrainingQueue ?? true,
      maxQueuedJobs: config.maxQueuedJobs ?? 10
    };
  }

  /**
   * Get singleton instance
   */
  static shared(config?: Partial<GenomeManagerConfig>): GenomeManager {
    if (!GenomeManager.instance) {
      GenomeManager.instance = new GenomeManager(config);
    }
    return GenomeManager.instance;
  }

  /**
   * Register LoRA trainer adapter
   *
   * Example:
   *   GenomeManager.shared().registerAdapter('peft', new PEFTLoRAAdapter());
   *   GenomeManager.shared().registerAdapter('openai', new OpenAILoRAAdapter());
   */
  registerAdapter(providerId: string, trainer: LoRATrainer): void {
    this.adapters.set(providerId, trainer);
    console.log(`✅ GenomeManager: Registered ${providerId} adapter`);
  }

  /**
   * Get trainer adapter for provider
   */
  getAdapter(providerId: string): LoRATrainer | undefined {
    return this.adapters.get(providerId);
  }

  /**
   * Initialize GPU detection and resource tracking
   *
   * MVP: Returns stub data (no actual GPU detection)
   * Phase 7.1+: Detect CUDA/Metal/ROCm GPUs, query memory/utilization
   */
  async initializeGPUResources(): Promise<void> {
    console.log('🔧 GenomeManager: Initializing GPU resources...');

    // MVP: Stub implementation
    // TODO Phase 7.1: Actual GPU detection
    this.gpuResources = [
      {
        id: 'stub-gpu-0',
        name: 'Stub GPU (No Detection)',
        totalMemoryMB: 8192,
        availableMemoryMB: 8192,
        utilization: 0.0
      }
    ];

    console.log(`✅ GenomeManager: Detected ${this.gpuResources.length} GPU(s)`);
  }

  /**
   * Get current GPU resource status
   */
  getGPUResources(): GPUResource[] {
    return [...this.gpuResources];
  }

  /**
   * Get total available GPU memory across all GPUs
   */
  getTotalAvailableMemoryMB(): number {
    return this.gpuResources.reduce((sum, gpu) => sum + gpu.availableMemoryMB, 0);
  }

  /**
   * Check if sufficient GPU memory available for training
   */
  canStartTraining(estimatedMemoryMB: number): boolean {
    const available = this.getTotalAvailableMemoryMB();
    const maxUsable = this.gpuResources.reduce(
      (sum, gpu) => sum + (gpu.totalMemoryMB * (this.config.maxGPUMemoryUsagePercent / 100)),
      0
    );

    return (available >= estimatedMemoryMB) &&
           (this.activeTrainingJobs.size < this.config.maxConcurrentTraining);
  }

  /**
   * Submit training job
   *
   * Queues job if GPU resources unavailable, otherwise starts immediately.
   *
   * MVP: Returns stub job (no actual training)
   * Phase 7.1+: Coordinates with adapters for actual training
   */
  async submitTrainingJob(
    request: LoRATrainingRequest,
    providerId: string,
    priority: number = 50
  ): Promise<TrainingJob> {
    const trainer = this.adapters.get(providerId);
    if (!trainer) {
      throw new Error(`No trainer adapter registered for provider: ${providerId}`);
    }

    if (!trainer.supportsFineTuning()) {
      throw new Error(`Provider ${providerId} does not support fine-tuning (MVP stub)`);
    }

    // Create training job
    const job: TrainingJob = {
      id: this.generateJobId(),
      personaId: request.personaId,
      personaName: request.personaName,
      traitType: request.traitType,
      baseModel: request.baseModel,
      status: 'pending',
      progress: 0.0
    };

    // Check if we can start immediately
    const estimatedMemoryMB = this.estimateTrainingMemory(request);
    if (this.canStartTraining(estimatedMemoryMB)) {
      // Start training immediately
      await this.startTraining(job, request, trainer);
    } else {
      // Queue for later
      if (!this.config.enableTrainingQueue) {
        throw new Error('GPU resources unavailable and training queue disabled');
      }

      if (this.trainingQueue.length >= this.config.maxQueuedJobs) {
        throw new Error(`Training queue full (${this.config.maxQueuedJobs} jobs)`);
      }

      this.trainingQueue.push({ job, request, trainer, priority });
      this.trainingQueue.sort((a, b) => b.priority - a.priority);

      console.log(`📋 GenomeManager: Queued training job ${job.id} (queue: ${this.trainingQueue.length})`);
    }

    return job;
  }

  /**
   * Start training job
   *
   * MVP: Throws "not implemented" (stub)
   * Phase 7.1+: Coordinates GPU allocation, calls trainer.trainLoRA()
   */
  private async startTraining(
    job: TrainingJob,
    request: LoRATrainingRequest,
    trainer: LoRATrainer
  ): Promise<void> {
    job.status = 'preparing';
    job.startedAt = Date.now();
    this.activeTrainingJobs.set(job.id, job);

    console.log(`🚀 GenomeManager: Starting training job ${job.id}`);

    try {
      // MVP: Not implemented yet
      throw new Error(
        'Training execution not implemented yet (Phase 7.0 MVP). ' +
        'Full implementation in Phase 7.1+ will coordinate GPU resources and call trainer adapters.'
      );

      // TODO Phase 7.1: Actual training coordination
      // 1. Allocate GPU resources
      // 2. Update job status to 'training'
      // 3. Call trainer.trainLoRA(request)
      // 4. Monitor progress
      // 5. Handle completion/failure
      // 6. Release GPU resources
      // 7. Process training queue
    } catch (error) {
      job.status = 'failed';
      job.completedAt = Date.now();
      job.result = {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };

      this.activeTrainingJobs.delete(job.id);
      this.processTrainingQueue();
    }
  }

  /**
   * Process training queue (start next job if GPU available)
   */
  private async processTrainingQueue(): Promise<void> {
    if (this.trainingQueue.length === 0) return;
    if (this.activeTrainingJobs.size >= this.config.maxConcurrentTraining) return;

    const entry = this.trainingQueue.shift();
    if (!entry) return;

    const estimatedMemoryMB = this.estimateTrainingMemory(entry.request);
    if (this.canStartTraining(estimatedMemoryMB)) {
      await this.startTraining(entry.job, entry.request, entry.trainer);
    } else {
      // Re-queue if still can't start
      this.trainingQueue.unshift(entry);
    }
  }

  /**
   * Estimate GPU memory required for training
   *
   * Heuristic: Base on model size, batch size, LoRA rank
   */
  private estimateTrainingMemory(request: LoRATrainingRequest): number {
    // Very rough estimate (Phase 7.0 MVP)
    // TODO Phase 7.1: More accurate estimation based on model architecture
    const rank = request.rank ?? 32;
    const batchSize = request.batchSize ?? 4;

    // Assume ~100MB per rank per batch (very rough)
    return rank * batchSize * 100;
  }

  /**
   * Load LoRA adapter into GPU memory for inference
   *
   * Implements LRU/LFU eviction when memory full.
   *
   * MVP: Returns stub (no actual loading)
   * Phase 7.1+: Load .safetensors/.gguf into GPU, evict if needed
   */
  async loadAdapter(
    personaId: UUID,
    traitType: TraitType,
    modelPath: string
  ): Promise<void> {
    const key = `${personaId}-${traitType}`;

    // Already loaded?
    if (this.loadedAdapters.has(key)) {
      const adapter = this.loadedAdapters.get(key)!;
      adapter.lastUsedAt = Date.now();
      adapter.useCount++;
      return;
    }

    // Check if need to evict
    if (this.loadedAdapters.size >= this.config.maxLoadedAdapters) {
      await this.evictAdapter();
    }

    // MVP: Stub loading
    console.log(`📥 GenomeManager: Loading adapter ${key} (stub)`);

    // TODO Phase 7.1: Actual adapter loading
    // 1. Load .safetensors or .gguf file
    // 2. Upload to GPU memory
    // 3. Track memory usage

    const adapter: LoadedAdapter = {
      personaId,
      traitType,
      modelPath,
      memoryUsageMB: 0, // Stub
      loadedAt: Date.now(),
      lastUsedAt: Date.now(),
      useCount: 1
    };

    this.loadedAdapters.set(key, adapter);
  }

  /**
   * Unload LoRA adapter from GPU memory
   */
  async unloadAdapter(personaId: UUID, traitType: TraitType): Promise<void> {
    const key = `${personaId}-${traitType}`;

    if (!this.loadedAdapters.has(key)) {
      return;
    }

    console.log(`📤 GenomeManager: Unloading adapter ${key}`);

    // TODO Phase 7.1: Actual GPU memory release
    this.loadedAdapters.delete(key);
  }

  /**
   * Evict least valuable adapter from GPU memory
   *
   * Strategy: Hybrid LRU + LFU
   * - Score = (useCount / age) * recencyBoost
   * - Evict lowest score
   */
  private async evictAdapter(): Promise<void> {
    if (this.loadedAdapters.size === 0) return;

    let lowestScore = Infinity;
    let evictKey: string | null = null;

    const now = Date.now();

    for (const [key, adapter] of this.loadedAdapters) {
      const ageMinutes = (now - adapter.loadedAt) / (1000 * 60);
      const recencyMinutes = (now - adapter.lastUsedAt) / (1000 * 60);
      const recencyBoost = Math.max(0.1, 1 / (recencyMinutes + 1));

      const score = (adapter.useCount / (ageMinutes + 1)) * recencyBoost;

      if (score < lowestScore) {
        lowestScore = score;
        evictKey = key;
      }
    }

    if (evictKey) {
      const adapter = this.loadedAdapters.get(evictKey)!;
      console.log(`🗑️ GenomeManager: Evicting adapter ${evictKey} (score: ${lowestScore.toFixed(2)})`);
      await this.unloadAdapter(adapter.personaId, adapter.traitType);
    }
  }

  /**
   * Get loaded adapters status
   */
  getLoadedAdapters(): LoadedAdapter[] {
    return Array.from(this.loadedAdapters.values());
  }

  /**
   * Get training queue status
   */
  getTrainingQueue(): TrainingJob[] {
    return this.trainingQueue.map(entry => entry.job);
  }

  /**
   * Get active training jobs
   */
  getActiveTrainingJobs(): TrainingJob[] {
    return Array.from(this.activeTrainingJobs.values());
  }

  /**
   * Get training job by ID
   */
  getTrainingJob(jobId: UUID): TrainingJob | undefined {
    return this.activeTrainingJobs.get(jobId) ??
           this.trainingQueue.find(entry => entry.job.id === jobId)?.job;
  }

  /**
   * Cancel training job
   */
  async cancelTrainingJob(jobId: UUID): Promise<boolean> {
    // Remove from queue
    const queueIndex = this.trainingQueue.findIndex(entry => entry.job.id === jobId);
    if (queueIndex >= 0) {
      this.trainingQueue.splice(queueIndex, 1);
      return true;
    }

    // Cancel active job
    const job = this.activeTrainingJobs.get(jobId);
    if (job) {
      job.status = 'cancelled';
      job.completedAt = Date.now();
      this.activeTrainingJobs.delete(jobId);

      // TODO Phase 7.1: Actually stop training process

      this.processTrainingQueue();
      return true;
    }

    return false;
  }

  /**
   * Generate unique job ID
   */
  private generateJobId(): UUID {
    // Simple UUID v4 generation
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
      const r = Math.random() * 16 | 0;
      const v = c === 'x' ? r : (r & 0x3 | 0x8);
      return v.toString(16);
    }) as UUID;
  }

  /**
   * Shutdown genome manager (cleanup resources)
   */
  async shutdown(): Promise<void> {
    console.log('🛑 GenomeManager: Shutting down...');

    // Cancel all active training
    for (const job of this.activeTrainingJobs.values()) {
      await this.cancelTrainingJob(job.id);
    }

    // Unload all adapters
    for (const adapter of this.loadedAdapters.values()) {
      await this.unloadAdapter(adapter.personaId, adapter.traitType);
    }

    // Clear queue
    this.trainingQueue = [];

    console.log('✅ GenomeManager: Shutdown complete');
  }
}
