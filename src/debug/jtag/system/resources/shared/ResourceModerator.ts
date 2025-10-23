/**
 * Resource Moderator - Holistic resource allocation with pluggable strategies
 *
 * Like BaseModerator for ThoughtStream, but for GPU/memory/worker allocation
 * Allows AI decision-making to intervene in resource management
 *
 * Architecture:
 * - Abstract base class with overridable decision points
 * - Mechanical default implementation (simple limits)
 * - Future: ML-based predictive allocation
 * - Future: AI-driven priority adjustment
 */

import type { UUID } from '../../core/types/JTAGTypes';

/**
 * System-wide resource availability
 */
export interface SystemResources {
  totalGpuMemory: number;      // MB total
  availableGpuMemory: number;  // MB available
  totalWorkers: number;        // Max concurrent workers
  activeWorkers: number;       // Currently running
  cpuUsage: number;            // Percentage (0-100)
  systemMemory: number;        // MB total
  availableMemory: number;     // MB available
}

/**
 * Per-adapter resource usage
 */
export interface AdapterResources {
  adapterId: UUID;
  displayName: string;
  gpuMemoryUsed: number;       // MB
  gpuMemoryQuota: number;      // MB allocated
  activeWorkers: number;
  maxWorkers: number;
  pendingEvaluations: number;
  failureRate: number;         // 0.0-1.0
  avgResponseTime: number;     // Milliseconds
  lastActivityTime: number;    // Timestamp
}

/**
 * Resource allocation request
 */
export interface ResourceRequest {
  adapterId: UUID;
  requestType: 'evaluation' | 'model_load' | 'worker_spawn';
  gpuMemoryNeeded?: number;    // MB (for model loading)
  workerNeeded?: boolean;      // Need worker thread
  priority: 'low' | 'normal' | 'high' | 'critical';
  estimatedDuration?: number;  // Milliseconds
  timestamp: number;
}

/**
 * Resource allocation decision
 */
export interface ResourceDecision {
  granted: boolean;
  reason: string;
  grantedGpuMemory?: number;   // MB granted (may be less than requested)
  grantedWorker?: boolean;
  waitTimeMs?: number;         // If denied, how long to wait
  alternatives?: string[];     // Suggested alternatives (e.g., "unload idle model")
}

/**
 * Resource moderation context
 */
export interface ResourceContext {
  system: SystemResources;
  adapters: Map<UUID, AdapterResources>;
  request: ResourceRequest;
  now: number;
}

/**
 * Abstract Resource Moderator - Base class for allocation strategies
 */
export abstract class ResourceModerator {
  /**
   * Should this request be granted? (Core decision point)
   */
  abstract shouldGrant(context: ResourceContext): ResourceDecision;

  /**
   * Calculate GPU memory quota for adapter (dynamic allocation)
   */
  abstract calculateGpuQuota(adapterId: UUID, context: ResourceContext): number;

  /**
   * Calculate max workers for adapter (dynamic concurrency)
   */
  abstract calculateMaxWorkers(adapterId: UUID, context: ResourceContext): number;

  /**
   * Should adapter be throttled? (thrashing/failure detection)
   */
  abstract shouldThrottle(adapterId: UUID, context: ResourceContext): {
    throttle: boolean;
    reason?: string;
    durationMs?: number;
  };

  /**
   * Prioritize pending requests (when resources are scarce)
   */
  abstract prioritizeRequests(
    requests: ResourceRequest[],
    context: ResourceContext
  ): ResourceRequest[];

  /**
   * Suggest resource reclamation (when system is full)
   */
  abstract suggestReclamation(context: ResourceContext): {
    adapterId: UUID;
    action: 'unload_model' | 'kill_worker' | 'pause_evaluations';
    reason: string;
  }[];
}

/**
 * Mechanical Resource Moderator - Simple fixed limits (default)
 *
 * Like PolynomialDecayModerator for ThoughtStream:
 * - Fixed quotas per adapter type
 * - Simple failure rate threshold
 * - FIFO request queue
 */
export class MechanicalResourceModerator extends ResourceModerator {
  private readonly GPU_QUOTA_OLLAMA = 2048;     // 2GB per Ollama adapter
  private readonly GPU_QUOTA_API = 0;           // API adapters use no GPU
  private readonly MAX_WORKERS_OLLAMA = 1;      // 1 worker per Ollama (synchronous)
  private readonly MAX_WORKERS_API = 5;         // 5 concurrent API calls
  private readonly FAILURE_THRESHOLD = 0.5;     // 50% failure rate = throttle
  private readonly THROTTLE_DURATION = 60000;   // 1 minute throttle
  private readonly IDLE_TIMEOUT = 300000;       // 5 minutes idle = reclaim

  shouldGrant(context: ResourceContext): ResourceDecision {
    const { system, adapters, request } = context;
    const adapter = adapters.get(request.adapterId);

    if (!adapter) {
      return {
        granted: false,
        reason: 'Adapter not found'
      };
    }

    // Check if adapter is throttled
    const throttleCheck = this.shouldThrottle(request.adapterId, context);
    if (throttleCheck.throttle) {
      return {
        granted: false,
        reason: throttleCheck.reason!,
        waitTimeMs: throttleCheck.durationMs
      };
    }

    // Handle different request types
    switch (request.requestType) {
      case 'evaluation':
        return this.grantEvaluation(adapter, system, request);

      case 'model_load':
        return this.grantModelLoad(adapter, system, request);

      case 'worker_spawn':
        return this.grantWorkerSpawn(adapter, system, request);

      default:
        return {
          granted: false,
          reason: `Unknown request type: ${request.requestType}`
        };
    }
  }

  private grantEvaluation(
    adapter: AdapterResources,
    system: SystemResources,
    request: ResourceRequest
  ): ResourceDecision {
    // Check worker availability
    if (adapter.activeWorkers >= adapter.maxWorkers) {
      return {
        granted: false,
        reason: `Worker pool full (${adapter.activeWorkers}/${adapter.maxWorkers})`,
        waitTimeMs: adapter.avgResponseTime, // Wait for avg evaluation time
        alternatives: ['Wait for current evaluation to complete']
      };
    }

    // Check GPU memory (if needed)
    if (adapter.gpuMemoryUsed >= adapter.gpuMemoryQuota) {
      return {
        granted: false,
        reason: `GPU memory quota exceeded (${adapter.gpuMemoryUsed}MB/${adapter.gpuMemoryQuota}MB)`,
        alternatives: ['Unload idle models', 'Wait for evaluation to complete']
      };
    }

    // Check system resources
    if (system.availableGpuMemory < 512) { // Need at least 512MB free
      return {
        granted: false,
        reason: `System GPU memory low (${system.availableGpuMemory}MB available)`,
        alternatives: this.suggestReclamation({ system, adapters: new Map(), request, now: Date.now() })
          .map(r => r.action)
      };
    }

    // Grant evaluation
    return {
      granted: true,
      reason: 'Resources available',
      grantedWorker: true
    };
  }

  private grantModelLoad(
    adapter: AdapterResources,
    system: SystemResources,
    request: ResourceRequest
  ): ResourceDecision {
    const memoryNeeded = request.gpuMemoryNeeded || 1024; // Default 1GB

    // Check adapter quota
    if (adapter.gpuMemoryUsed + memoryNeeded > adapter.gpuMemoryQuota) {
      return {
        granted: false,
        reason: `Would exceed GPU quota (${adapter.gpuMemoryUsed + memoryNeeded}MB > ${adapter.gpuMemoryQuota}MB)`,
        alternatives: ['Unload existing model first']
      };
    }

    // Check system availability
    if (system.availableGpuMemory < memoryNeeded) {
      return {
        granted: false,
        reason: `Insufficient system GPU memory (need ${memoryNeeded}MB, ${system.availableGpuMemory}MB available)`,
        alternatives: this.suggestReclamation({ system, adapters: new Map(), request, now: Date.now() })
          .map(r => `${r.adapterId.slice(0, 8)}: ${r.action}`)
      };
    }

    // Grant model load
    return {
      granted: true,
      reason: 'Sufficient GPU memory available',
      grantedGpuMemory: memoryNeeded
    };
  }

  private grantWorkerSpawn(
    adapter: AdapterResources,
    system: SystemResources,
    request: ResourceRequest
  ): ResourceDecision {
    // Check adapter worker limit
    if (adapter.activeWorkers >= adapter.maxWorkers) {
      return {
        granted: false,
        reason: `Max workers reached (${adapter.activeWorkers}/${adapter.maxWorkers})`
      };
    }

    // Check system worker limit
    if (system.activeWorkers >= system.totalWorkers) {
      return {
        granted: false,
        reason: `System worker pool full (${system.activeWorkers}/${system.totalWorkers})`,
        alternatives: ['Wait for worker to free up', 'Kill idle worker']
      };
    }

    // Grant worker spawn
    return {
      granted: true,
      reason: 'Worker pool has capacity',
      grantedWorker: true
    };
  }

  calculateGpuQuota(adapterId: UUID, context: ResourceContext): number {
    const adapter = context.adapters.get(adapterId);
    if (!adapter) return 0;

    // Simple heuristic: Ollama adapters get GPU, API adapters don't
    const isOllama = adapter.displayName.toLowerCase().includes('ollama') ||
                     adapter.displayName.toLowerCase().includes('local');

    return isOllama ? this.GPU_QUOTA_OLLAMA : this.GPU_QUOTA_API;
  }

  calculateMaxWorkers(adapterId: UUID, context: ResourceContext): number {
    const adapter = context.adapters.get(adapterId);
    if (!adapter) return 0;

    // Simple heuristic: Ollama = 1 worker, API = 5 workers
    const isOllama = adapter.displayName.toLowerCase().includes('ollama') ||
                     adapter.displayName.toLowerCase().includes('local');

    return isOllama ? this.MAX_WORKERS_OLLAMA : this.MAX_WORKERS_API;
  }

  shouldThrottle(adapterId: UUID, context: ResourceContext): {
    throttle: boolean;
    reason?: string;
    durationMs?: number;
  } {
    const adapter = context.adapters.get(adapterId);
    if (!adapter) {
      return { throttle: false };
    }

    // Throttle if failure rate too high
    if (adapter.failureRate > this.FAILURE_THRESHOLD) {
      return {
        throttle: true,
        reason: `High failure rate (${(adapter.failureRate * 100).toFixed(0)}% > ${this.FAILURE_THRESHOLD * 100}%)`,
        durationMs: this.THROTTLE_DURATION
      };
    }

    return { throttle: false };
  }

  prioritizeRequests(
    requests: ResourceRequest[],
    context: ResourceContext
  ): ResourceRequest[] {
    // Simple FIFO with priority boost
    return [...requests].sort((a, b) => {
      // Priority order: critical > high > normal > low
      const priorityWeight: Record<string, number> = {
        critical: 1000,
        high: 100,
        normal: 10,
        low: 1
      };

      const weightA = priorityWeight[a.priority] || 10;
      const weightB = priorityWeight[b.priority] || 10;

      if (weightA !== weightB) {
        return weightB - weightA; // Higher priority first
      }

      // Within same priority, FIFO (oldest first)
      return a.timestamp - b.timestamp;
    });
  }

  suggestReclamation(context: ResourceContext): {
    adapterId: UUID;
    action: 'unload_model' | 'kill_worker' | 'pause_evaluations';
    reason: string;
  }[] {
    const suggestions: {
      adapterId: UUID;
      action: 'unload_model' | 'kill_worker' | 'pause_evaluations';
      reason: string;
    }[] = [];

    const now = context.now;

    for (const [adapterId, adapter] of context.adapters) {
      const idleTime = now - adapter.lastActivityTime;

      // Suggest unloading idle models (5+ minutes idle with GPU memory)
      if (idleTime > this.IDLE_TIMEOUT && adapter.gpuMemoryUsed > 0) {
        suggestions.push({
          adapterId,
          action: 'unload_model',
          reason: `Idle for ${(idleTime / 60000).toFixed(1)} minutes with ${adapter.gpuMemoryUsed}MB GPU memory`
        });
      }

      // Suggest killing idle workers (5+ minutes idle with active workers)
      if (idleTime > this.IDLE_TIMEOUT && adapter.activeWorkers > 0) {
        suggestions.push({
          adapterId,
          action: 'kill_worker',
          reason: `Idle for ${(idleTime / 60000).toFixed(1)} minutes with ${adapter.activeWorkers} active workers`
        });
      }

      // Suggest pausing high-failure adapters
      if (adapter.failureRate > this.FAILURE_THRESHOLD) {
        suggestions.push({
          adapterId,
          action: 'pause_evaluations',
          reason: `High failure rate (${(adapter.failureRate * 100).toFixed(0)}%)`
        });
      }
    }

    // Sort by priority: unload_model > kill_worker > pause_evaluations
    return suggestions.sort((a, b) => {
      const order = { unload_model: 3, kill_worker: 2, pause_evaluations: 1 };
      return order[b.action] - order[a.action];
    });
  }
}

/**
 * Get default resource moderator instance
 */
export function getDefaultResourceModerator(): ResourceModerator {
  return new MechanicalResourceModerator();
}

/**
 * Future: AI-Driven Resource Moderator
 *
 * Uses ML model to predict resource needs and optimize allocation:
 * - Learns adapter usage patterns
 * - Predicts evaluation duration based on message content
 * - Dynamically adjusts quotas based on demand
 * - Preemptively reclaims resources before exhaustion
 */
export class AIResourceModerator extends ResourceModerator {
  // TODO: Implement ML-based resource allocation
  // - Train on historical adapter usage
  // - Predict GPU memory needs per message type
  // - Optimize for throughput vs latency
  // - Learn from thrashing patterns

  shouldGrant(context: ResourceContext): ResourceDecision {
    // Placeholder: Delegate to mechanical for now
    return new MechanicalResourceModerator().shouldGrant(context);
  }

  calculateGpuQuota(adapterId: UUID, context: ResourceContext): number {
    // TODO: ML-based quota calculation
    return new MechanicalResourceModerator().calculateGpuQuota(adapterId, context);
  }

  calculateMaxWorkers(adapterId: UUID, context: ResourceContext): number {
    // TODO: ML-based worker allocation
    return new MechanicalResourceModerator().calculateMaxWorkers(adapterId, context);
  }

  shouldThrottle(adapterId: UUID, context: ResourceContext): {
    throttle: boolean;
    reason?: string;
    durationMs?: number;
  } {
    // TODO: ML-based throttling (predict recovery time)
    return new MechanicalResourceModerator().shouldThrottle(adapterId, context);
  }

  prioritizeRequests(
    requests: ResourceRequest[],
    context: ResourceContext
  ): ResourceRequest[] {
    // TODO: ML-based prioritization (predict value per request)
    return new MechanicalResourceModerator().prioritizeRequests(requests, context);
  }

  suggestReclamation(context: ResourceContext): {
    adapterId: UUID;
    action: 'unload_model' | 'kill_worker' | 'pause_evaluations';
    reason: string;
  }[] {
    // TODO: ML-based reclamation (predict least-likely-needed resources)
    return new MechanicalResourceModerator().suggestReclamation(context);
  }
}
