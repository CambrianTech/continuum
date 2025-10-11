/**
 * Genome Stats Command Types
 * Performance monitoring for genome inference system
 *
 * Usage:
 * ./jtag genome/stats                    # Overall pool stats
 * ./jtag genome/stats --genomeId=<id>    # Specific genome stats
 * ./jtag genome/stats --format=json      # Machine-readable output
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Genome Stats Request Parameters
 */
export interface GenomeStatsParams extends CommandParams {
  // Optional filters
  genomeId?: UUID;           // Stats for specific genome
  personaId?: UUID;          // Stats for specific persona's genome
  includeHistory?: boolean;  // Include historical metrics (last hour)
  format?: 'table' | 'json'; // Output format (default: table)
}

/**
 * Performance Metrics
 */
export interface PerformanceMetrics {
  // Timing metrics (milliseconds)
  layerLoadTime: TimingStats;
  layerStackTime: TimingStats;
  processSpawnTime: TimingStats;
  inferenceTime: TimingStats;
  processTeardownTime: TimingStats;
  totalRequestTime: TimingStats;

  // Throughput metrics
  requestsPerMinute: number;
  successRate: number;
  errorRate: number;

  // Resource metrics
  avgMemoryUsageMB: number;
  peakMemoryUsageMB: number;
  avgCPUPercent: number;
}

export interface TimingStats {
  min: number;
  max: number;
  avg: number;
  p50: number;  // Median
  p95: number;  // 95th percentile
  p99: number;  // 99th percentile
  count: number; // Sample count
}

/**
 * Pool Statistics
 */
export interface PoolStats {
  // Pool configuration
  poolType: 'hot' | 'warm' | 'cold';
  maxSize: number;
  currentSize: number;
  idleCount: number;
  activeCount: number;

  // Hit rates
  hitRate: number;          // 0-1
  evictionCount: number;    // Total evictions
  evictionRate: number;     // Evictions per minute

  // Health
  healthyProcesses: number;
  unhealthyProcesses: number;
  crashCount: number;       // Total crashes
  crashRate: number;        // Crashes per minute
}

/**
 * Cache Statistics
 */
export interface CacheStats {
  // Layer cache metrics
  cacheSize: number;        // Current entries
  maxCacheSize: number;
  hitRate: number;          // 0-1
  missRate: number;         // 0-1

  // Memory metrics
  totalMemoryMB: number;    // Total cache memory
  avgLayerSizeMB: number;   // Average layer size

  // Efficiency
  loadTime: TimingStats;    // Time to load from cache
  diskLoadTime: TimingStats; // Time to load from disk
}

/**
 * Thrashing Detection
 */
export interface ThrashingMetrics {
  isThrashing: boolean;
  assemblyToInferenceRatio: number;  // Should be < 0.5
  cacheChurnRate: number;            // Evictions per request
  recommendations: string[];         // Suggested optimizations
}

/**
 * Per-Genome Statistics
 */
export interface GenomeStats {
  genomeId: UUID;
  genomeName: string;

  // Usage metrics
  totalRequests: number;
  successfulRequests: number;
  failedRequests: number;
  avgResponseTimeMs: number;

  // Resource usage
  avgMemoryMB: number;
  peakMemoryMB: number;
  layerCount: number;

  // Current state
  currentPool: 'hot' | 'warm' | 'cold' | 'not-loaded';
  lastUsed: string;          // ISO timestamp
  timesEvicted: number;

  // Performance
  performance: PerformanceMetrics;
}

/**
 * Overall System Statistics
 */
export interface SystemStats {
  // Timestamp
  timestamp: string;         // ISO timestamp
  uptimeSeconds: number;

  // Pool statistics
  hotPool: PoolStats;
  warmPool: PoolStats;
  coldStarts: number;

  // Cache statistics
  layerCache: CacheStats;

  // System-wide performance
  systemPerformance: PerformanceMetrics;

  // Thrashing detection
  thrashing: ThrashingMetrics;

  // Active genomes
  activeGenomes: number;
  loadedGenomes: UUID[];
}

/**
 * Genome Stats Result
 */
export interface GenomeStatsResult extends CommandResult {
  // System-wide stats (always included)
  system: SystemStats;

  // Per-genome stats (if genomeId specified)
  genome?: GenomeStats;

  // Historical data (if includeHistory=true)
  history?: HistoricalMetrics;
}

/**
 * Historical Metrics (last hour)
 */
export interface HistoricalMetrics {
  timeRange: {
    start: string;           // ISO timestamp
    end: string;
  };

  // Time-series data (1-minute buckets)
  timeSeries: TimeSeriesData[];

  // Aggregated stats
  hourlyStats: {
    totalRequests: number;
    avgResponseTime: number;
    peakMemoryMB: number;
    crashCount: number;
  };
}

export interface TimeSeriesData {
  timestamp: string;         // ISO timestamp
  requestCount: number;
  avgResponseTime: number;
  memoryUsageMB: number;
  cpuPercent: number;
  errorCount: number;
}

/**
 * Genome Stats Constants
 */
export const GENOME_STATS_CONSTANTS = {
  COMMAND: 'genome/stats',

  // Performance targets (from GENOME-PERFORMANCE-STRATEGY.md)
  TARGETS: {
    LAYER_LOAD_MS: 100,
    LAYER_STACK_MS: 50,
    PROCESS_SPAWN_MS: 500,
    PROCESS_TEARDOWN_MS: 100,
    TOTAL_REQUEST_MS: 3000,
    THRASHING_RATIO: 0.5,      // Assembly < 50% of inference
  },

  // Pool defaults
  POOL: {
    HOT_SIZE: 3,
    WARM_SIZE: 10,
    MIN_PROCESSES: 1,
    MAX_PROCESSES: 10,
  },

  // Cache defaults
  CACHE: {
    MAX_LAYERS: 20,
    MAX_MEMORY_MB: 2048,
  },
} as const;
