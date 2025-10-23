/**
 * Resource Manager - Central resource allocation using ResourceModerator
 *
 * Single source of truth for system resources
 * Pluggable moderator strategy (mechanical → AI-driven)
 */

import type { UUID } from '../../core/types/JTAGTypes';
import {
  ResourceModerator,
  getDefaultResourceModerator,
  type SystemResources,
  type AdapterResources,
  type ResourceRequest,
  type ResourceDecision,
  type ResourceContext
} from './ResourceModerator';

/**
 * Global Resource Manager (Singleton)
 */
export class ResourceManager {
  private moderator: ResourceModerator;
  private adapters: Map<UUID, AdapterResources> = new Map();
  private systemResources: SystemResources;

  constructor(moderator?: ResourceModerator) {
    this.moderator = moderator || getDefaultResourceModerator();

    // Initialize system resources (will be updated by monitoring)
    this.systemResources = {
      totalGpuMemory: 8192,      // 8GB default (TODO: detect actual GPU)
      availableGpuMemory: 8192,
      totalWorkers: 10,          // Max 10 concurrent workers
      activeWorkers: 0,
      cpuUsage: 0,
      systemMemory: 16384,       // 16GB default
      availableMemory: 16384
    };

    console.log('🔧 ResourceManager: Initialized with moderator:', this.moderator.constructor.name);
  }

  /**
   * Register adapter with resource manager
   */
  registerAdapter(adapterId: UUID, displayName: string): void {
    const quota = this.moderator.calculateGpuQuota(adapterId, this.buildContext({
      adapterId,
      requestType: 'evaluation',
      priority: 'normal',
      timestamp: Date.now()
    }));

    const maxWorkers = this.moderator.calculateMaxWorkers(adapterId, this.buildContext({
      adapterId,
      requestType: 'evaluation',
      priority: 'normal',
      timestamp: Date.now()
    }));

    this.adapters.set(adapterId, {
      adapterId,
      displayName,
      gpuMemoryUsed: 0,
      gpuMemoryQuota: quota,
      activeWorkers: 0,
      maxWorkers,
      pendingEvaluations: 0,
      failureRate: 0,
      avgResponseTime: 5000, // Default 5 seconds
      lastActivityTime: Date.now()
    });

    console.log(`🔧 ResourceManager: Registered adapter ${displayName} (GPU: ${quota}MB, Workers: ${maxWorkers})`);
  }

  /**
   * Unregister adapter
   */
  unregisterAdapter(adapterId: UUID): void {
    const adapter = this.adapters.get(adapterId);
    if (adapter) {
      // Release resources
      this.systemResources.availableGpuMemory += adapter.gpuMemoryUsed;
      this.systemResources.activeWorkers -= adapter.activeWorkers;
      this.adapters.delete(adapterId);
      console.log(`🔧 ResourceManager: Unregistered adapter ${adapter.displayName}`);
    }
  }

  /**
   * Request resource allocation
   */
  requestResources(request: ResourceRequest): ResourceDecision {
    const context = this.buildContext(request);
    const decision = this.moderator.shouldGrant(context);

    if (decision.granted) {
      // Allocate resources
      this.allocateResources(request, decision);
    }

    return decision;
  }

  /**
   * Release resources after use
   */
  releaseResources(
    adapterId: UUID,
    resourceType: 'worker' | 'gpu_memory',
    amount?: number
  ): void {
    const adapter = this.adapters.get(adapterId);
    if (!adapter) return;

    if (resourceType === 'worker') {
      adapter.activeWorkers = Math.max(0, adapter.activeWorkers - 1);
      this.systemResources.activeWorkers = Math.max(0, this.systemResources.activeWorkers - 1);
    } else if (resourceType === 'gpu_memory' && amount) {
      adapter.gpuMemoryUsed = Math.max(0, adapter.gpuMemoryUsed - amount);
      this.systemResources.availableGpuMemory += amount;
    }

    adapter.lastActivityTime = Date.now();
  }

  /**
   * Report evaluation result (for failure rate tracking)
   */
  reportEvaluation(
    adapterId: UUID,
    success: boolean,
    durationMs: number
  ): void {
    const adapter = this.adapters.get(adapterId);
    if (!adapter) return;

    adapter.pendingEvaluations = Math.max(0, adapter.pendingEvaluations - 1);

    // Update failure rate (exponential moving average)
    const alpha = 0.1; // Smoothing factor
    const currentFailure = success ? 0 : 1;
    adapter.failureRate = alpha * currentFailure + (1 - alpha) * adapter.failureRate;

    // Update avg response time (exponential moving average)
    adapter.avgResponseTime = alpha * durationMs + (1 - alpha) * adapter.avgResponseTime;

    adapter.lastActivityTime = Date.now();
  }

  /**
   * Check if adapter is available for evaluation
   */
  isAvailable(adapterId: UUID): boolean {
    const adapter = this.adapters.get(adapterId);
    if (!adapter) return false;

    // Check throttle status
    const context = this.buildContext({
      adapterId,
      requestType: 'evaluation',
      priority: 'normal',
      timestamp: Date.now()
    });

    const throttle = this.moderator.shouldThrottle(adapterId, context);
    if (throttle.throttle) {
      return false;
    }

    // Check worker availability
    if (adapter.activeWorkers >= adapter.maxWorkers) {
      return false;
    }

    // Check GPU memory
    if (adapter.gpuMemoryUsed >= adapter.gpuMemoryQuota) {
      return false;
    }

    return true;
  }

  /**
   * Get adapter resources
   */
  getAdapterResources(adapterId: UUID): AdapterResources | undefined {
    return this.adapters.get(adapterId);
  }

  /**
   * Get system resources
   */
  getSystemResources(): SystemResources {
    return { ...this.systemResources };
  }

  /**
   * Get all adapter stats
   */
  getAllAdapterStats(): AdapterResources[] {
    return Array.from(this.adapters.values());
  }

  /**
   * Perform resource cleanup (called periodically)
   */
  performCleanup(): void {
    const context = this.buildContext({
      adapterId: '00000000-0000-0000-0000-000000000000' as UUID,
      requestType: 'evaluation',
      priority: 'normal',
      timestamp: Date.now()
    });

    const suggestions = this.moderator.suggestReclamation(context);

    for (const suggestion of suggestions) {
      console.log(`🧹 ResourceManager: ${suggestion.action} for ${suggestion.adapterId.slice(0, 8)} - ${suggestion.reason}`);
      // TODO: Actually perform reclamation (emit events to adapters)
    }
  }

  /**
   * Build resource context for moderator
   */
  private buildContext(request: ResourceRequest): ResourceContext {
    return {
      system: this.systemResources,
      adapters: new Map(this.adapters),
      request,
      now: Date.now()
    };
  }

  /**
   * Allocate resources after grant
   */
  private allocateResources(request: ResourceRequest, decision: ResourceDecision): void {
    const adapter = this.adapters.get(request.adapterId);
    if (!adapter) return;

    if (decision.grantedWorker) {
      adapter.activeWorkers++;
      adapter.pendingEvaluations++;
      this.systemResources.activeWorkers++;
    }

    if (decision.grantedGpuMemory) {
      adapter.gpuMemoryUsed += decision.grantedGpuMemory;
      this.systemResources.availableGpuMemory -= decision.grantedGpuMemory;
    }

    adapter.lastActivityTime = Date.now();
  }
}

/** Singleton instance */
let resourceManagerInstance: ResourceManager | null = null;

/**
 * Get global resource manager instance
 */
export function getResourceManager(): ResourceManager {
  if (!resourceManagerInstance) {
    resourceManagerInstance = new ResourceManager();
  }
  return resourceManagerInstance;
}

/**
 * Reset resource manager (for testing)
 */
export function resetResourceManager(): void {
  resourceManagerInstance = null;
}
