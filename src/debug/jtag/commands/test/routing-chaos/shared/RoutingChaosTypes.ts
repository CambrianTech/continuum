/**
 * Routing Chaos Test Types - Complex Multi-Hop Routing Validation
 * 
 * Tests the router's ability to handle complex routing scenarios:
 * browser->server->server->browser->server and back with random success/failure
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { createPayload } from '../../../../system/core/types/JTAGTypes';

// Command path constants - prevent typos in routing
export const ROUTING_CHAOS_COMMAND_PATH = 'test/routing-chaos' as const;
export type RoutingChaosCommandPath = typeof ROUTING_CHAOS_COMMAND_PATH;

export interface RoutingChaosParams extends JTAGPayload {
  testId: string;
  hopCount: number;
  maxHops: number;
  routingPath: string[];
  currentEnvironment: 'browser' | 'server';
  targetEnvironment?: 'browser' | 'server';
  failureRate: number; // 0.0 to 1.0 - probability of random failure
  delayRange: [number, number]; // [min, max] milliseconds
  payloadSize: 'small' | 'medium' | 'large';
  testStartTime: string;
  correlationTrace: string[];
}

export interface RoutingChaosResult extends JTAGPayload {
  testId: string;
  success: boolean;
  totalHops: number;
  actualPath: string[];
  totalDurationMs: number;
  errorEncountered?: string;
  performanceMetrics: {
    hopDurations: number[];
    averageHopTime: number;
    slowestHop: number;
    fastestHop: number;
    totalCorrelations: number;
    failedHops: number;
  };
  routingTrace: Array<{
    hop: number;
    from: string;
    to: string;
    durationMs: number;
    success: boolean;
    error?: string;
    timestamp: string;
  }>;
}

export const createRoutingChaosParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    testId?: string;
    hopCount?: number;
    maxHops?: number;
    routingPath?: string[];
    currentEnvironment?: 'browser' | 'server';
    targetEnvironment?: 'browser' | 'server';
    failureRate?: number;
    delayRange?: [number, number];
    payloadSize?: 'small' | 'medium' | 'large';
    correlationTrace?: string[];
  }
): RoutingChaosParams => createPayload(context, sessionId, {
  testId: data.testId ?? `chaos-${Date.now()}-${Math.random().toString(36).substr(2, 6)}`,
  hopCount: data.hopCount ?? 0,
  maxHops: data.maxHops ?? 10,
  routingPath: data.routingPath ?? [],
  currentEnvironment: data.currentEnvironment ?? 'browser',
  targetEnvironment: data.targetEnvironment,
  failureRate: data.failureRate ?? 0.1,
  delayRange: data.delayRange ?? [10, 100],
  payloadSize: data.payloadSize ?? 'small',
  testStartTime: new Date().toISOString(),
  correlationTrace: data.correlationTrace ?? [],
  ...data
});

export interface RoutingChainTestParams extends JTAGPayload {
  chainId: string;
  chainDepth: number;
  branchFactor: number; // Number of parallel chains to spawn
  errorInjection: {
    enabled: boolean;
    errorRate: number;
    errorTypes: ('timeout' | 'rejection' | 'corruption' | 'loss')[];
  };
  performanceTracking: {
    enabled: boolean;
    trackMemoryUsage: boolean;
    trackLatency: boolean;
  };
}

export interface RoutingChainTestResult extends JTAGPayload {
  chainId: string;
  totalChains: number;
  successfulChains: number;
  failedChains: number;
  averageChainDurationMs: number;
  totalTestDurationMs: number;
  errorBreakdown: Record<string, number>;
  performanceMetrics: {
    peakMemoryUsageMB?: number;
    averageLatencyMs: number;
    throughputChainsPerSecond: number;
    correlationEfficiency: number;
  };
  chainResults: Array<{
    chainIndex: number;
    success: boolean;
    durationMs: number;
    hopCount: number;
    errorType?: string;
    finalResult?: any;
  }>;
}

// Payload size generators for testing with different message sizes
export const generateTestPayload = (size: 'small' | 'medium' | 'large') => {
  switch (size) {
    case 'small':
      return { data: 'x'.repeat(100) }; // ~100 bytes
    case 'medium':  
      return { data: 'x'.repeat(10000) }; // ~10KB
    case 'large':
      return { data: 'x'.repeat(100000) }; // ~100KB
    default:
      return { data: 'test' };
  }
};

// Random routing path generators for chaos testing
export const generateRandomRoutingPath = (maxHops: number): string[] => {
  const environments = ['browser', 'server'];
  const path: string[] = [];
  
  for (let i = 0; i < maxHops; i++) {
    const env = environments[Math.floor(Math.random() * environments.length)];
    const hopType = Math.random() > 0.5 ? 'command' : 'event';
    path.push(`${env}/${hopType}/routing-chaos`);
  }
  
  return path;
};

// Error injection utilities
export const shouldInjectError = (errorRate: number): boolean => {
  return Math.random() < errorRate;
};

export const generateRandomError = (errorTypes: string[]): Error => {
  const errorType = errorTypes[Math.floor(Math.random() * errorTypes.length)];
  
  switch (errorType) {
    case 'timeout':
      return new Error('Simulated timeout error');
    case 'rejection':
      return new Error('Simulated rejection error');
    case 'corruption':
      return new Error('Simulated data corruption error');
    case 'loss':
      return new Error('Simulated message loss error');
    default:
      return new Error('Unknown simulated error');
  }
};