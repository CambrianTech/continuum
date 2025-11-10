/**
 * Cognitive Scheduler Interface - Adapter Pattern
 *
 * Different models have different cognitive capabilities and need
 * different attention management strategies:
 *
 * - Simple models: Heuristic scheduler (fast, rule-based)
 * - Advanced models: Neural scheduler (learned, adaptive)
 * - Visual models: Prioritize vision domains
 * - Audio models: Prioritize speech domains
 * - Fast models: Can handle realtime domains
 * - Slow models: Async domains only
 */

import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';

/**
 * Activity domains that personas can engage in
 *
 * Domains are divided into:
 * - EXTERNAL: Interacting with the world (chat, games, etc.)
 * - INTERNAL: Private cognitive processes (dreaming, simulating, reflecting)
 */
export enum ActivityDomain {
  // EXTERNAL DOMAINS (interacting with world)
  REALTIME_GAME = 'realtime_game',    // 16ms target, requires fast inference
  CHAT = 'chat',                       // 5s target, text-based
  CODE_REVIEW = 'code_review',         // 60s target, deep analysis
  VISION = 'vision',                   // Image processing (requires vision model)
  AUDIO = 'audio',                     // Speech/sound (requires audio model)

  // INTERNAL COGNITIVE DOMAINS (private mental processes)
  DREAMING = 'dreaming',               // Memory consolidation, latent space exploration
  TRAINING = 'training',               // Fine-tune LoRA adapters, continuous learning
  SIMULATING = 'simulating',           // Internal "what-if" scenarios, planning
  REFLECTING = 'reflecting',           // Metacognition, self-analysis, self-improvement
  PLANNING = 'planning',               // Long-term goal setting, strategic thinking
  BACKGROUND = 'background'            // Housekeeping, maintenance
}

/**
 * Queue item in a domain
 */
export interface QueueItem {
  id: UUID;
  type: 'message' | 'task' | 'event';
  domain: ActivityDomain;
  priority: number;        // 0.0-1.0
  timestamp: number;
  payload: any;
}

/**
 * Domain-specific queue configuration
 */
export interface DomainQueueConfig {
  domain: ActivityDomain;
  targetCadence: number;      // Target service interval (ms)
  minCadence: number;         // Minimum safe interval (ms)
  maxCadence: number;         // Maximum acceptable interval (ms)
  attentionRequired: number;  // Focus needed (0.0-1.0)
  canDefer: boolean;          // Can be delayed under load
  requiresCapability?: string; // Optional capability requirement
}

/**
 * Service result for a domain
 */
export interface ServiceResult {
  serviced: number;       // Items processed
  skipped?: boolean;      // Domain skipped
  deferred?: boolean;     // Domain deferred
  reason?: string;        // Why skipped/deferred
  timeUsed: number;       // Milliseconds spent
  energyUsed: number;     // Energy consumed
}

/**
 * Attention allocation across domains
 */
export interface AttentionAllocation {
  allocations: Map<ActivityDomain, number>; // Domain -> attention weight (0.0-1.0)
  totalBudget: number;                       // Total available attention
}

/**
 * Context for attention decision
 */
export interface CognitiveContext {
  // State
  energy: number;               // 0.0-1.0
  mood: string;                 // 'focused' | 'tired' | 'stressed' | 'idle'

  // Activity levels
  activeGames: number;
  unreadMessages: number;
  pendingReviews: number;
  backgroundTasksPending: number;

  // Performance
  avgResponseTime: number;      // Recent average (ms)
  queueBacklog: number;         // Total items across all queues

  // System
  cpuPressure: number;          // 0.0-1.0
  memoryPressure: number;       // 0.0-1.0

  // Model capabilities
  modelCapabilities: Set<string>; // e.g., ['text', 'vision', 'fast-inference']
}

/**
 * Universal Cognitive Scheduler Interface
 *
 * Adapters implement different attention management strategies
 * based on model capabilities and personality.
 */
export interface ICognitiveScheduler {
  /**
   * Scheduler name for debugging
   */
  readonly name: string;

  /**
   * Model capabilities this scheduler requires
   */
  readonly requiredCapabilities: Set<string>;

  /**
   * Initialize scheduler with persona identity
   */
  initialize(personaId: UUID, personaName: string): Promise<void>;

  /**
   * Determine which domains this scheduler can handle
   * based on model capabilities
   */
  getSupportedDomains(capabilities: Set<string>): ActivityDomain[];

  /**
   * Allocate attention budget across domains
   *
   * This is where different schedulers differ:
   * - HeuristicScheduler: Fixed rules
   * - NeuralScheduler: Learned weights
   * - VisualScheduler: Prioritizes vision
   */
  allocateAttention(
    budget: number,
    context: CognitiveContext
  ): Promise<AttentionAllocation>;

  /**
   * Determine next service interval (adaptive)
   *
   * Returns milliseconds until next service cycle
   */
  getNextServiceInterval(context: CognitiveContext): number;

  /**
   * Should a specific domain be serviced now?
   *
   * Considers: energy, queue state, system pressure, timing constraints
   */
  shouldServiceDomain(
    domain: ActivityDomain,
    context: CognitiveContext
  ): Promise<boolean>;

  /**
   * Get priority order for domains (which to service first)
   */
  getDomainPriority(context: CognitiveContext): ActivityDomain[];

  /**
   * Update scheduler policy based on results (learning)
   *
   * For HeuristicScheduler: No-op
   * For NeuralScheduler: Gradient descent on attention weights
   */
  updatePolicy(results: Map<ActivityDomain, ServiceResult>): Promise<void>;

  /**
   * System override - defer domains under pressure
   */
  deferDomains(domains: ActivityDomain[]): void;

  /**
   * System override - only allow specific domains
   */
  allowDomainsOnly(domains: ActivityDomain[]): void;

  /**
   * Clear all overrides
   */
  clearOverrides(): void;
}

/**
 * Base cognitive scheduler with common functionality
 */
export abstract class BaseCognitiveScheduler implements ICognitiveScheduler {
  abstract readonly name: string;
  abstract readonly requiredCapabilities: Set<string>;

  protected personaId!: UUID;
  protected personaName!: string;

  // System overrides
  protected deferredDomains: Set<ActivityDomain> = new Set();
  protected allowedDomains: Set<ActivityDomain> | null = null;

  async initialize(personaId: UUID, personaName: string): Promise<void> {
    this.personaId = personaId;
    this.personaName = personaName;
  }

  /**
   * Default supported domains (text-based only)
   */
  getSupportedDomains(capabilities: Set<string>): ActivityDomain[] {
    const supported: ActivityDomain[] = [
      ActivityDomain.CHAT,
      ActivityDomain.CODE_REVIEW,
      ActivityDomain.BACKGROUND,
      ActivityDomain.TRAINING
    ];

    // Add realtime if model supports fast inference
    if (capabilities.has('fast-inference')) {
      supported.push(ActivityDomain.REALTIME_GAME);
    }

    // Add vision if model supports it
    if (capabilities.has('vision')) {
      supported.push(ActivityDomain.VISION);
    }

    // Add audio if model supports it
    if (capabilities.has('audio')) {
      supported.push(ActivityDomain.AUDIO);
    }

    return supported;
  }

  /**
   * Check if domain is currently allowed (after system overrides)
   */
  protected isDomainAllowed(domain: ActivityDomain): boolean {
    // Check deferred list
    if (this.deferredDomains.has(domain)) {
      return false;
    }

    // Check allow list (if set, only these domains allowed)
    if (this.allowedDomains !== null && !this.allowedDomains.has(domain)) {
      return false;
    }

    return true;
  }

  deferDomains(domains: ActivityDomain[]): void {
    for (const domain of domains) {
      this.deferredDomains.add(domain);
    }
    console.log(`🚫 ${this.personaName}: Deferred domains: ${Array.from(this.deferredDomains).join(', ')}`);
  }

  allowDomainsOnly(domains: ActivityDomain[]): void {
    this.allowedDomains = new Set(domains);
    console.log(`✅ ${this.personaName}: Only allowed domains: ${Array.from(this.allowedDomains).join(', ')}`);
  }

  clearOverrides(): void {
    this.deferredDomains.clear();
    this.allowedDomains = null;
    console.log(`🔓 ${this.personaName}: Cleared all overrides`);
  }

  // Abstract methods that subclasses must implement
  abstract allocateAttention(
    budget: number,
    context: CognitiveContext
  ): Promise<AttentionAllocation>;

  abstract getNextServiceInterval(context: CognitiveContext): number;

  abstract shouldServiceDomain(
    domain: ActivityDomain,
    context: CognitiveContext
  ): Promise<boolean>;

  abstract getDomainPriority(context: CognitiveContext): ActivityDomain[];

  abstract updatePolicy(results: Map<ActivityDomain, ServiceResult>): Promise<void>;
}
