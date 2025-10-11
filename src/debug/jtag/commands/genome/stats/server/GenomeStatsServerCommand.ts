/**
 * Genome Stats Server Command
 *
 * Provides comprehensive performance monitoring for genome inference system.
 * Returns real-time metrics, pool stats, and actionable recommendations.
 *
 * Usage:
 * ./jtag genome/stats                    # Overall system stats
 * ./jtag genome/stats --genomeId=<id>    # Specific genome
 * ./jtag genome/stats --format=json      # Machine-readable
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { GenomeStatsParams, GenomeStatsResult } from '../shared/GenomeStatsTypes';
import { GENOME_STATS_CONSTANTS } from '../shared/GenomeStatsTypes';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { ProcessPool } from '../../../../system/genome/server/ProcessPool';

export class GenomeStatsServerCommand extends CommandBase<GenomeStatsParams, GenomeStatsResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/stats', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeStatsResult> {
    const statsParams = params as GenomeStatsParams;

    try {
      // Get ProcessPool from AIProviderDaemon
      let pool: ProcessPool;
      let poolStats: ReturnType<ProcessPool['getStats']>;

      try {
        pool = AIProviderDaemon.getProcessPool() as ProcessPool;
        poolStats = pool.getStats();
      } catch (error) {
        // ProcessPool not available (might be on browser side or not initialized)
        console.warn('⚠️  GenomeStatsServerCommand: ProcessPool not available, returning placeholder data');

        const systemStats: GenomeStatsResult = {
          ...statsParams,
          success: true,
          timestamp: Date.now(),
          systemOverview: {
            totalProcesses: 0,
            activeInferences: 0,
            queuedRequests: 0,
            totalGenomes: 0,
            loadedGenomes: 0,
            totalLayers: 0,
            cachedLayers: 0,
            cacheHitRate: 0,
            systemHealthy: true,
            uptime: process.uptime() * 1000,
          },
          poolStats: this.generatePlaceholderPoolStats(),
          performanceMetrics: this.generatePlaceholderPerformanceMetrics(),
          thrashingMetrics: {
            isThrashing: false,
            assemblyToInferenceRatio: 0,
            cacheChurnRate: 0,
            recommendations: [
              'ProcessPool not available - check server initialization',
            ],
          },
          warnings: [
            error instanceof Error ? error.message : 'ProcessPool not available',
          ],
        };

        return systemStats;
      }

      // Map ProcessPool stats to GenomeStatsResult format
      const systemStats: GenomeStatsResult = {
        ...statsParams,
        success: true,
        timestamp: Date.now(),
        systemOverview: {
          totalProcesses: poolStats.total,
          activeInferences: poolStats.byState.busy || 0,
          queuedRequests: 0, // TODO: Add queue tracking in Phase 2.3
          totalGenomes: 0, // TODO: Query from database
          loadedGenomes: 0, // TODO: Track loaded genomes
          totalLayers: 0, // TODO: Query from database
          cachedLayers: 0, // TODO: Add layer cache tracking in Phase 2.2
          cacheHitRate: 0, // TODO: Add cache hit rate tracking
          systemHealthy: poolStats.byState.unhealthy === 0,
          uptime: process.uptime() * 1000,
        },
        poolStats: {
          hot: this.mapPoolTierStats('hot', poolStats),
          warm: this.mapPoolTierStats('warm', poolStats),
          cold: this.mapPoolTierStats('cold', poolStats),
        },
        performanceMetrics: this.generatePlaceholderPerformanceMetrics(), // TODO: Add real metrics in Phase 2.3
        thrashingMetrics: {
          isThrashing: false,
          assemblyToInferenceRatio: 0, // TODO: Calculate in Phase 2.2
          cacheChurnRate: 0, // TODO: Track in Phase 2.2
          recommendations: this.generateRecommendations(poolStats),
        },
        warnings: this.generateWarnings(poolStats),
      };

      // If specific genome requested, add genome-specific stats
      if (statsParams.genomeId) {
        systemStats.genomeSpecific = {
          genomeId: statsParams.genomeId,
          genomeName: 'Unknown (Phase 2.1 - not yet implemented)',
          layerCount: 0,
          totalSizeMB: 0,
          loadedInPool: 'none' as const,
          lastUsed: 0,
          useCount: 0,
          avgInferenceTime: 0,
          successRate: 0,
        };
      }

      // If specific persona requested, add persona-specific stats
      if (statsParams.personaId) {
        systemStats.personaSpecific = {
          personaId: statsParams.personaId,
          personaName: 'Unknown (Phase 2.1 - not yet implemented)',
          genomeId: undefined,
          requestsTotal: 0,
          requestsPending: 0,
          requestsProcessed: 0,
          requestsFailed: 0,
          avgWaitTime: 0,
          avgProcessingTime: 0,
          queueHealth: 'healthy' as const,
        };
      }

      return systemStats;
    } catch (error) {
      console.error('❌ GenomeStatsServerCommand failed:', error);
      return {
        ...statsParams,
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        timestamp: Date.now(),
        systemOverview: this.generateEmptySystemOverview(),
        poolStats: this.generatePlaceholderPoolStats(),
        performanceMetrics: this.generatePlaceholderPerformanceMetrics(),
        thrashingMetrics: {
          isThrashing: false,
          assemblyToInferenceRatio: 0,
          cacheChurnRate: 0,
          recommendations: ['Error occurred - check logs'],
        },
        warnings: [error instanceof Error ? error.message : 'Unknown error'],
      };
    }
  }

  private generatePlaceholderPoolStats() {
    return {
      hot: {
        poolType: 'hot' as const,
        maxSize: GENOME_STATS_CONSTANTS.POOL.HOT_SIZE,
        currentSize: 0,
        idleCount: 0,
        activeCount: 0,
        hitRate: 0,
        evictionCount: 0,
        evictionRate: 0,
        healthyProcesses: 0,
        unhealthyProcesses: 0,
        crashCount: 0,
        crashRate: 0,
      },
      warm: {
        poolType: 'warm' as const,
        maxSize: GENOME_STATS_CONSTANTS.POOL.WARM_SIZE,
        currentSize: 0,
        idleCount: 0,
        activeCount: 0,
        hitRate: 0,
        evictionCount: 0,
        evictionRate: 0,
        healthyProcesses: 0,
        unhealthyProcesses: 0,
        crashCount: 0,
        crashRate: 0,
      },
      cold: {
        poolType: 'cold' as const,
        maxSize: 999, // Unlimited (disk-based)
        currentSize: 0,
        idleCount: 0,
        activeCount: 0,
        hitRate: 0,
        evictionCount: 0,
        evictionRate: 0,
        healthyProcesses: 0,
        unhealthyProcesses: 0,
        crashCount: 0,
        crashRate: 0,
      },
    };
  }

  private generatePlaceholderPerformanceMetrics() {
    return {
      layerLoadTime: this.generateEmptyTimingStats(),
      layerStackTime: this.generateEmptyTimingStats(),
      processSpawnTime: this.generateEmptyTimingStats(),
      inferenceTime: this.generateEmptyTimingStats(),
      processTeardownTime: this.generateEmptyTimingStats(),
      totalRequestTime: this.generateEmptyTimingStats(),
      requestsPerMinute: 0,
      successRate: 0,
      errorRate: 0,
      avgMemoryUsageMB: 0,
      peakMemoryUsageMB: 0,
      avgCPUPercent: 0,
    };
  }

  private generateEmptyTimingStats() {
    return {
      min: 0,
      max: 0,
      avg: 0,
      p50: 0,
      p95: 0,
      p99: 0,
      count: 0,
    };
  }

  private generateEmptySystemOverview() {
    return {
      totalProcesses: 0,
      activeInferences: 0,
      queuedRequests: 0,
      totalGenomes: 0,
      loadedGenomes: 0,
      totalLayers: 0,
      cachedLayers: 0,
      cacheHitRate: 0,
      systemHealthy: false,
      uptime: 0,
    };
  }

  /**
   * Map ProcessPool stats by tier to GenomeStatsResult format
   */
  private mapPoolTierStats(tier: 'hot' | 'warm' | 'cold', poolStats: ReturnType<ProcessPool['getStats']>) {
    return {
      poolType: tier,
      maxSize: tier === 'hot' ? GENOME_STATS_CONSTANTS.POOL.HOT_SIZE :
               tier === 'warm' ? GENOME_STATS_CONSTANTS.POOL.WARM_SIZE : 999,
      currentSize: poolStats.byTier[tier] || 0,
      idleCount: tier === 'hot' ? poolStats.byState.idle : 0, // TODO: Track per-tier idle count
      activeCount: tier === 'hot' ? poolStats.byState.busy : 0, // TODO: Track per-tier active count
      hitRate: 0, // TODO: Track hit rate in Phase 2.2
      evictionCount: 0, // TODO: Track evictions
      evictionRate: 0, // TODO: Calculate eviction rate
      healthyProcesses: poolStats.total - (poolStats.byState.unhealthy || 0),
      unhealthyProcesses: poolStats.byState.unhealthy || 0,
      crashCount: 0, // TODO: Track crashes
      crashRate: 0, // TODO: Calculate crash rate
    };
  }

  /**
   * Generate recommendations based on pool stats
   */
  private generateRecommendations(poolStats: ReturnType<ProcessPool['getStats']>): string[] {
    const recommendations: string[] = [];

    if (poolStats.total === 0) {
      recommendations.push('No processes in pool - inference requests will trigger cold starts');
    }

    if (poolStats.byState.unhealthy > 0) {
      recommendations.push(`${poolStats.byState.unhealthy} unhealthy processes detected - check logs for errors`);
    }

    if (poolStats.total < GENOME_STATS_CONSTANTS.POOL.MIN_PROCESSES) {
      recommendations.push(`Pool below minimum size (${poolStats.total}/${GENOME_STATS_CONSTANTS.POOL.MIN_PROCESSES}) - consider increasing minProcesses`);
    }

    if (poolStats.total >= GENOME_STATS_CONSTANTS.POOL.MAX_PROCESSES) {
      recommendations.push('Pool at maximum capacity - consider increasing maxProcesses for better concurrency');
    }

    if (recommendations.length === 0) {
      recommendations.push('ProcessPool operating normally');
      recommendations.push('Phase 2.1 complete - ready for Phase 2.2 (genome loading)');
    }

    return recommendations;
  }

  /**
   * Generate warnings based on pool stats
   */
  private generateWarnings(poolStats: ReturnType<ProcessPool['getStats']>): string[] {
    const warnings: string[] = [];

    if (poolStats.byState.unhealthy > 0) {
      warnings.push(`⚠️  ${poolStats.byState.unhealthy} unhealthy processes`);
    }

    if (poolStats.totalErrors > 0) {
      warnings.push(`⚠️  ${poolStats.totalErrors} total errors recorded`);
    }

    // Phase 2 warnings
    warnings.push('ℹ️  Phase 2.2 features (genome loading) not yet implemented');
    warnings.push('ℹ️  Phase 2.3 features (actual inference) not yet implemented');

    return warnings;
  }
}
