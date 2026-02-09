/**
 * Daemon Orchestrator - Wave-based Parallel Startup
 *
 * Starts daemons in dependency order:
 * 1. CRITICAL: data, command, events (no dependencies, start immediately)
 * 2. INTEGRATION: user, ai-provider, etc. (wait for their deps)
 * 3. LIGHTWEIGHT: health, widget, etc. (can start anytime)
 *
 * This ensures UI-serving daemons are ready ASAP while heavy
 * initialization happens in the background.
 */

import type { DaemonBase, DaemonEntry } from '../../../../daemons/command-daemon/shared/DaemonBase';
import type { JTAGContext } from '../../types/JTAGTypes';
import type { JTAGRouter } from '../../router/shared/JTAGRouter';

// ============================================================================
// Types
// ============================================================================

export type DaemonPhase = 'critical' | 'integration' | 'lightweight';

export interface DaemonDependency {
  name: string;
  waitFor: string[];  // Daemon names that must be READY first
  phase: DaemonPhase;
}

export interface DaemonInitMetrics {
  daemon: string;
  phase: DaemonPhase;
  startTime: number;
  endTime: number;
  durationMs: number;
  waitedFor: string[];
  queuedMessages: number;
}

// ============================================================================
// Dependency Graph
// ============================================================================

/**
 * Daemon dependency graph
 *
 * Names match DaemonEntry.name from generated.ts (PascalCase).
 * Order matters within each phase - listed in priority order.
 * Daemons with empty waitFor[] can start immediately in parallel.
 */
export const DAEMON_DEPENDENCIES: DaemonDependency[] = [
  // ═══════════════════════════════════════════════════════════════════════════
  // PHASE 1: CRITICAL PATH (Start immediately, no dependencies)
  // These are required for basic system operation and UI rendering
  // ═══════════════════════════════════════════════════════════════════════════
  { name: 'DataDaemon', waitFor: [], phase: 'critical' },
  { name: 'CommandDaemon', waitFor: [], phase: 'critical' },
  { name: 'EventsDaemon', waitFor: [], phase: 'critical' },
  { name: 'SessionDaemon', waitFor: [], phase: 'critical' },

  // ═══════════════════════════════════════════════════════════════════════════
  // PHASE 2: INTEGRATION (Wait for DataDaemon, then start in parallel)
  // These integrate with the data layer and other services
  // ═══════════════════════════════════════════════════════════════════════════
  { name: 'UserDaemon', waitFor: ['DataDaemon'], phase: 'integration' },
  { name: 'RoomMembershipDaemon', waitFor: ['DataDaemon'], phase: 'integration' },
  { name: 'ArchiveDaemon', waitFor: ['DataDaemon'], phase: 'integration' },
  { name: 'TrainingDaemon', waitFor: ['DataDaemon'], phase: 'integration' },
  { name: 'GovernanceDaemon', waitFor: ['DataDaemon'], phase: 'integration' },
  { name: 'LeaseDaemon', waitFor: ['DataDaemon'], phase: 'integration' },

  // AI provider has no data dependency - can start in critical phase
  { name: 'AIProviderDaemon', waitFor: [], phase: 'integration' },

  // ═══════════════════════════════════════════════════════════════════════════
  // PHASE 3: LIGHTWEIGHT (No dependencies, can start anytime)
  // These provide auxiliary services that aren't on the critical path
  // ═══════════════════════════════════════════════════════════════════════════
  { name: 'HealthDaemon', waitFor: [], phase: 'lightweight' },
  { name: 'WidgetDaemon', waitFor: [], phase: 'lightweight' },
  { name: 'ProxyDaemon', waitFor: [], phase: 'lightweight' },
  { name: 'LoggerDaemon', waitFor: [], phase: 'lightweight' },
  { name: 'ConsoleDaemon', waitFor: ['LoggerDaemon'], phase: 'lightweight' },
  { name: 'ArtifactsDaemon', waitFor: [], phase: 'lightweight' },
];

// ============================================================================
// Orchestrator
// ============================================================================

export class DaemonOrchestrator {
  private readonly daemonEntries: DaemonEntry[];
  private readonly context: JTAGContext;
  private readonly router: JTAGRouter;
  private readonly createDaemon: (entry: DaemonEntry, context: JTAGContext, router: JTAGRouter) => DaemonBase | null;

  private readyDaemons = new Set<string>();
  private daemonInstances = new Map<string, DaemonBase>();
  private initMetrics: DaemonInitMetrics[] = [];
  private resolvers = new Map<string, () => void>();

  constructor(
    daemonEntries: DaemonEntry[],
    context: JTAGContext,
    router: JTAGRouter,
    createDaemon: (entry: DaemonEntry, context: JTAGContext, router: JTAGRouter) => DaemonBase | null
  ) {
    this.daemonEntries = daemonEntries;
    this.context = context;
    this.router = router;
    this.createDaemon = createDaemon;
  }

  /**
   * Start all daemons in dependency order
   * Returns when all daemons are ready (or failed)
   */
  async startAll(): Promise<{ daemons: DaemonBase[]; metrics: DaemonInitMetrics[] }> {
    const startTime = Date.now();
    console.log(`🎭 DaemonOrchestrator: Starting ${this.daemonEntries.length} daemons in dependency order...`);

    // Build lookup from dependency graph
    const depGraph = new Map<string, DaemonDependency>();
    for (const dep of DAEMON_DEPENDENCIES) {
      depGraph.set(dep.name, dep);
    }

    // Sort entries by phase priority
    const sortedEntries = this.sortByPhase(this.daemonEntries, depGraph);

    // Start daemons respecting dependencies
    const startPromises: Promise<void>[] = [];

    for (const entry of sortedEntries) {
      const dep = depGraph.get(entry.name);
      const waitFor = dep?.waitFor ?? [];
      const phase = dep?.phase ?? 'lightweight';

      startPromises.push(this.startDaemon(entry, waitFor, phase));
    }

    // Wait for all daemons
    await Promise.all(startPromises);

    const totalMs = Date.now() - startTime;
    console.log(`🎭 DaemonOrchestrator: All ${this.daemonInstances.size} daemons ready in ${totalMs}ms`);

    // Log phase breakdown
    this.logPhaseBreakdown();

    return {
      daemons: Array.from(this.daemonInstances.values()),
      metrics: this.initMetrics
    };
  }

  /**
   * Start a single daemon, waiting for its dependencies first
   */
  private async startDaemon(entry: DaemonEntry, waitFor: string[], phase: DaemonPhase): Promise<void> {
    const startTime = Date.now();

    // Wait for dependencies to be ready
    if (waitFor.length > 0) {
      await this.waitForDaemons(waitFor);
    }

    const waitedMs = Date.now() - startTime;
    const initStartTime = Date.now();

    try {
      // Create daemon instance
      const daemon = this.createDaemon(entry, this.context, this.router);

      if (daemon) {
        // Initialize daemon
        await daemon.initializeDaemon();

        this.daemonInstances.set(entry.name, daemon);
        this.markReady(entry.name);

        // Record metrics
        const endTime = Date.now();
        this.initMetrics.push({
          daemon: entry.name,
          phase,
          startTime: initStartTime,
          endTime,
          durationMs: endTime - initStartTime,
          waitedFor: waitFor,
          queuedMessages: daemon.startupQueueSize
        });

        const totalMs = endTime - startTime;
        const initMs = endTime - initStartTime;
        if (waitedMs > 10) {
          console.log(`  ✅ ${entry.name} (${phase}) ready in ${initMs}ms (waited ${waitedMs}ms for deps)`);
        } else {
          console.log(`  ✅ ${entry.name} (${phase}) ready in ${initMs}ms`);
        }
      }
    } catch (error) {
      console.error(`  ❌ ${entry.name} failed:`, error);
      // Still mark as "ready" so dependent daemons don't hang forever
      // They'll get errors when trying to use this daemon
      this.markReady(entry.name);
    }
  }

  /**
   * Wait for specific daemons to be ready
   */
  private waitForDaemons(names: string[]): Promise<void> {
    const unready = names.filter(n => !this.readyDaemons.has(n));
    if (unready.length === 0) {
      return Promise.resolve();
    }

    return new Promise<void>(resolve => {
      let remaining = unready.length;

      const checkDone = () => {
        remaining--;
        if (remaining === 0) {
          resolve();
        }
      };

      for (const name of unready) {
        if (this.readyDaemons.has(name)) {
          checkDone();
        } else {
          // Store resolver to be called when daemon is ready
          const existingResolver = this.resolvers.get(name);
          this.resolvers.set(name, () => {
            existingResolver?.();
            checkDone();
          });
        }
      }
    });
  }

  /**
   * Mark a daemon as ready and notify waiters
   */
  private markReady(name: string): void {
    this.readyDaemons.add(name);
    const resolver = this.resolvers.get(name);
    if (resolver) {
      resolver();
      this.resolvers.delete(name);
    }
  }

  /**
   * Sort daemon entries by phase priority
   */
  private sortByPhase(entries: DaemonEntry[], depGraph: Map<string, DaemonDependency>): DaemonEntry[] {
    const phaseOrder: Record<DaemonPhase, number> = {
      critical: 0,
      integration: 1,
      lightweight: 2
    };

    return [...entries].sort((a, b) => {
      const aPhase = depGraph.get(a.name)?.phase ?? 'lightweight';
      const bPhase = depGraph.get(b.name)?.phase ?? 'lightweight';
      return phaseOrder[aPhase] - phaseOrder[bPhase];
    });
  }

  /**
   * Log breakdown of init times by phase
   */
  private logPhaseBreakdown(): void {
    const byPhase = new Map<DaemonPhase, DaemonInitMetrics[]>();

    for (const metric of this.initMetrics) {
      const list = byPhase.get(metric.phase) || [];
      list.push(metric);
      byPhase.set(metric.phase, list);
    }

    console.log(`\n📊 Daemon Init Breakdown:`);
    for (const phase of ['critical', 'integration', 'lightweight'] as DaemonPhase[]) {
      const metrics = byPhase.get(phase) || [];
      const totalMs = metrics.reduce((sum, m) => sum + m.durationMs, 0);
      const maxMs = Math.max(...metrics.map(m => m.durationMs), 0);
      console.log(`  ${phase}: ${metrics.length} daemons, max=${maxMs}ms, total=${totalMs}ms`);
    }
  }

  /**
   * Get initialization metrics
   */
  getMetrics(): DaemonInitMetrics[] {
    return this.initMetrics;
  }

  /**
   * Get daemon by name
   */
  getDaemon(name: string): DaemonBase | undefined {
    return this.daemonInstances.get(name);
  }
}
