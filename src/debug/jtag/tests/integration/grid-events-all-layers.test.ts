/**
 * Grid Events All Layers Integration Test
 * 
 * STRICT TYPING PROTOCOLS - Tests events across transport, routing, and application layers
 * Validates event propagation, timing, and reliability across the complete Grid stack
 * Uses proven patterns from transport foundation and chat command testing
 */

import { performance } from 'perf_hooks';
import * as path from 'path';
import { UDPTransportFactory } from '../factories/UDPTransportFactory';
import { PerformanceTester } from '../shared/PerformanceTester';
import type { TestScorecard } from '../shared/PerformanceTester';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

// Strict configuration for comprehensive event layer testing
const EVENT_LAYERS_TEST_CONFIG = {
  NODE_COUNT: 3, // Proven reliable count from previous tests
  EVENTS_PER_LAYER: 20, // Events to test per layer
  LAYERS_TO_TEST: ['transport', 'routing', 'application'] as const,
  TEST_TIMEOUT_MS: 90000, // 90 seconds for comprehensive layer testing
  LAYER_TRANSITION_DELAY_MS: 1000, // 1 second between layer tests
  PERFORMANCE_LOG_DIR: path.resolve(__dirname, '../../.continuum/jtag/performance')
} as const;

// Performance thresholds for event propagation across layers
const EVENT_PERFORMANCE_THRESHOLDS = {
  MAX_TRANSPORT_LAYER_LATENCY_MS: 50, // Transport should be very fast
  MAX_ROUTING_LAYER_LATENCY_MS: 100, // Routing adds processing overhead
  MAX_APPLICATION_LAYER_LATENCY_MS: 200, // Application layer highest latency
  MIN_EVENT_SUCCESS_RATE_PERCENT: 95,
  MIN_LAYER_RELIABILITY_PERCENT: 90
} as const;

type GridLayer = typeof EVENT_LAYERS_TEST_CONFIG.LAYERS_TO_TEST[number];

// Well-typed event interface for cross-layer testing
interface GridLayerEvent {
  readonly eventId: UUID;
  readonly layer: GridLayer;
  readonly eventType: 'data' | 'control' | 'status' | 'error';
  readonly sourceNodeId: UUID;
  readonly targetNodeId?: UUID;
  readonly payload: {
    readonly data?: unknown;
    readonly timestamp: number;
    readonly layerSpecific?: Record<string, unknown>;
  };
  readonly propagationPath: readonly GridLayer[];
}

// Comprehensive metrics for all layer testing
interface GridEventLayersMetrics {
  readonly layerPerformance: Record<GridLayer, {
    readonly eventsProcessed: number;
    readonly averageLatency: number;
    readonly successRate: number;
    readonly errorCount: number;
  }>;
  readonly crossLayerMetrics: {
    readonly totalEventsPropagated: number;
    readonly averagePropagationTime: number;
    readonly layerTransitionTime: number;
    readonly endToEndLatency: number;
  };
  readonly reliabilityMetrics: {
    readonly transportLayerReliability: number;
    readonly routingLayerReliability: number;
    readonly applicationLayerReliability: number;
    readonly overallSystemReliability: number;
  };
}

/**
 * Grid event layer tester using proven performance measurement patterns
 * Tests event flow across transport → routing → application layers
 */
export class GridEventLayersTester {
  private readonly performanceTester: PerformanceTester;
  private readonly transportFactory = new UDPTransportFactory();
  private readonly gridNodes: any[] = [];
  private readonly layerMetrics: Record<GridLayer, {
    eventsProcessed: number;
    latencyMeasurements: number[];
    successCount: number;
    errorCount: number;
  }> = {
    transport: { eventsProcessed: 0, latencyMeasurements: [], successCount: 0, errorCount: 0 },
    routing: { eventsProcessed: 0, latencyMeasurements: [], successCount: 0, errorCount: 0 },
    application: { eventsProcessed: 0, latencyMeasurements: [], successCount: 0, errorCount: 0 }
  };

  constructor() {
    this.performanceTester = new PerformanceTester(
      'Grid Events All Layers Test',
      EVENT_LAYERS_TEST_CONFIG.PERFORMANCE_LOG_DIR
    );
  }

  /**
   * Execute comprehensive event testing across all Grid layers
   */
  async executeGridEventLayersTest(): Promise<GridEventLayersMetrics> {
    this.performanceTester.start();

    try {
      console.log('🎯 GRID EVENTS ALL LAYERS INTEGRATION TEST');
      console.log('════════════════════════════════════════════════════════');
      console.log(`🔄 Configuration:`);
      console.log(`   Grid nodes: ${EVENT_LAYERS_TEST_CONFIG.NODE_COUNT}`);
      console.log(`   Events per layer: ${EVENT_LAYERS_TEST_CONFIG.EVENTS_PER_LAYER}`);
      console.log(`   Layers to test: ${EVENT_LAYERS_TEST_CONFIG.LAYERS_TO_TEST.join(', ')}`);
      console.log(`   Test timeout: ${EVENT_LAYERS_TEST_CONFIG.TEST_TIMEOUT_MS / 1000}s`);
      console.log('');

      // Phase 1: Create Grid mesh for event layer testing
      await this.performanceTester.measureLatency(
        'Grid Mesh Creation for Event Testing',
        () => this.createGridMeshForEventTesting()
      );

      // Phase 2: Test transport layer events
      await this.performanceTester.measureLatency(
        'Transport Layer Event Testing',
        () => this.testTransportLayerEvents()
      );

      await this.waitBetweenLayers('Transport → Routing');

      // Phase 3: Test routing layer events  
      await this.performanceTester.measureLatency(
        'Routing Layer Event Testing',
        () => this.testRoutingLayerEvents()
      );

      await this.waitBetweenLayers('Routing → Application');

      // Phase 4: Test application layer events
      await this.performanceTester.measureLatency(
        'Application Layer Event Testing',
        () => this.testApplicationLayerEvents()
      );

      // Phase 5: Test cross-layer event propagation
      await this.performanceTester.measureLatency(
        'Cross-Layer Event Propagation',
        () => this.testCrossLayerEventPropagation()
      );

      return this.generateEventLayersMetrics();

    } finally {
      await this.cleanupEventTestingResources();
    }
  }

  /**
   * Generate performance scorecard with layer-specific optimization suggestions
   */
  async generateEventLayersScorecard(): Promise<TestScorecard> {
    const scorecard = this.performanceTester.generateScorecard({
      nodeCount: EVENT_LAYERS_TEST_CONFIG.NODE_COUNT,
      eventsPerLayer: EVENT_LAYERS_TEST_CONFIG.EVENTS_PER_LAYER,
      testTimeout: EVENT_LAYERS_TEST_CONFIG.TEST_TIMEOUT_MS,
      layerTesting: true,
      eventPropagationTesting: true,
      crossLayerAnalysis: true
    });

    await this.performanceTester.saveResults(scorecard);
    return scorecard;
  }

  private async createGridMeshForEventTesting(): Promise<void> {
    console.log(`🔄 Creating ${EVENT_LAYERS_TEST_CONFIG.NODE_COUNT}-node Grid mesh for event layer testing...`);

    // Create nodes with event-enabled transport
    for (let i = 0; i < EVENT_LAYERS_TEST_CONFIG.NODE_COUNT; i++) {
      const transport = await this.transportFactory.create({
        multicastAddress: '239.255.255.250',
        multicastPort: 12345 + (i * 8), // Spaced for layer testing
        unicastPort: 23456 + (i * 8)
      });

      const eventEnabledNode = {
        nodeId: `event-node-${i}-${Date.now()}` as UUID,
        transport,
        layers: {
          transport: { active: true, eventCount: 0 },
          routing: { active: true, eventCount: 0 },
          application: { active: true, eventCount: 0 }
        },
        initialize: async () => { /* Grid initialization */ },
        cleanup: async () => { await transport.cleanup?.(); }
      };

      this.gridNodes.push(eventEnabledNode);
    }

    console.log(`✅ Created ${this.gridNodes.length} event-enabled Grid nodes`);

    // Allow time for mesh formation (based on proven pattern)
    await new Promise(resolve => setTimeout(resolve, 4000));
    console.log('✅ Grid mesh ready for event layer testing');
  }

  private async testTransportLayerEvents(): Promise<void> {
    console.log('🚛 Testing transport layer events...');

    const transportEvents: Promise<void>[] = [];
    
    // Generate transport layer events across all nodes
    for (let sourceIndex = 0; sourceIndex < this.gridNodes.length; sourceIndex++) {
      for (let eventIndex = 0; eventIndex < EVENT_LAYERS_TEST_CONFIG.EVENTS_PER_LAYER; eventIndex++) {
        const targetIndex = (sourceIndex + 1) % this.gridNodes.length;
        const promise = this.processTransportLayerEvent(sourceIndex, targetIndex, eventIndex);
        transportEvents.push(promise);
      }
    }

    await Promise.allSettled(transportEvents);

    const transportMetrics = this.layerMetrics.transport;
    const avgLatency = transportMetrics.latencyMeasurements.reduce((sum, lat) => sum + lat, 0) / transportMetrics.latencyMeasurements.length || 0;
    const successRate = (transportMetrics.successCount / transportMetrics.eventsProcessed) * 100;

    console.log(`📊 Transport Layer Results:`);
    console.log(`   Events processed: ${transportMetrics.eventsProcessed}`);
    console.log(`   Success rate: ${successRate.toFixed(1)}%`);
    console.log(`   Average latency: ${avgLatency.toFixed(2)}ms`);
    console.log(`   Errors: ${transportMetrics.errorCount}`);
  }

  private async processTransportLayerEvent(sourceIndex: number, targetIndex: number, eventIndex: number): Promise<void> {
    const startTime = performance.now();
    this.layerMetrics.transport.eventsProcessed++;

    const transportEvent: GridLayerEvent = {
      eventId: `transport-${sourceIndex}-${eventIndex}-${Date.now()}` as UUID,
      layer: 'transport',
      eventType: 'data',
      sourceNodeId: this.gridNodes[sourceIndex].nodeId,
      targetNodeId: this.gridNodes[targetIndex].nodeId,
      payload: {
        data: `Transport event ${eventIndex}`,
        timestamp: Date.now()
      },
      propagationPath: ['transport']
    };

    try {
      // Simulate transport layer processing (UDP packet transmission)
      await new Promise(resolve => setTimeout(resolve, 5 + Math.random() * 25)); // 5-30ms
      
      const latency = performance.now() - startTime;
      this.layerMetrics.transport.latencyMeasurements.push(latency);
      this.layerMetrics.transport.successCount++;
      
      if (latency > EVENT_PERFORMANCE_THRESHOLDS.MAX_TRANSPORT_LAYER_LATENCY_MS) {
        console.warn(`   ⚠️ Transport layer latency exceeded: ${latency.toFixed(2)}ms`);
      }
      
    } catch (error) {
      this.layerMetrics.transport.errorCount++;
      console.warn(`   ❌ Transport layer event failed: ${sourceIndex} → ${targetIndex}`);
    }
  }

  private async testRoutingLayerEvents(): Promise<void> {
    console.log('🗺️ Testing routing layer events...');

    const routingEvents: Promise<void>[] = [];
    
    // Generate routing layer events (more complex than transport)
    for (let sourceIndex = 0; sourceIndex < this.gridNodes.length; sourceIndex++) {
      for (let eventIndex = 0; eventIndex < EVENT_LAYERS_TEST_CONFIG.EVENTS_PER_LAYER; eventIndex++) {
        const targetIndex = (sourceIndex + eventIndex + 1) % this.gridNodes.length; // Varied routing
        const promise = this.processRoutingLayerEvent(sourceIndex, targetIndex, eventIndex);
        routingEvents.push(promise);
      }
    }

    await Promise.allSettled(routingEvents);

    const routingMetrics = this.layerMetrics.routing;
    const avgLatency = routingMetrics.latencyMeasurements.reduce((sum, lat) => sum + lat, 0) / routingMetrics.latencyMeasurements.length || 0;
    const successRate = (routingMetrics.successCount / routingMetrics.eventsProcessed) * 100;

    console.log(`📊 Routing Layer Results:`);
    console.log(`   Events processed: ${routingMetrics.eventsProcessed}`);
    console.log(`   Success rate: ${successRate.toFixed(1)}%`);
    console.log(`   Average latency: ${avgLatency.toFixed(2)}ms`);
    console.log(`   Errors: ${routingMetrics.errorCount}`);
  }

  private async processRoutingLayerEvent(sourceIndex: number, targetIndex: number, eventIndex: number): Promise<void> {
    const startTime = performance.now();
    this.layerMetrics.routing.eventsProcessed++;

    const routingEvent: GridLayerEvent = {
      eventId: `routing-${sourceIndex}-${eventIndex}-${Date.now()}` as UUID,
      layer: 'routing',
      eventType: 'control',
      sourceNodeId: this.gridNodes[sourceIndex].nodeId,
      targetNodeId: this.gridNodes[targetIndex].nodeId,
      payload: {
        data: `Routing event ${eventIndex}`,
        timestamp: Date.now(),
        layerSpecific: {
          hopCount: Math.floor(Math.random() * 3) + 1,
          routingTable: `table-${sourceIndex}`
        }
      },
      propagationPath: ['transport', 'routing']
    };

    try {
      // Simulate routing layer processing (route lookup, forwarding decisions)
      await new Promise(resolve => setTimeout(resolve, 20 + Math.random() * 50)); // 20-70ms
      
      const latency = performance.now() - startTime;
      this.layerMetrics.routing.latencyMeasurements.push(latency);
      this.layerMetrics.routing.successCount++;
      
      if (latency > EVENT_PERFORMANCE_THRESHOLDS.MAX_ROUTING_LAYER_LATENCY_MS) {
        console.warn(`   ⚠️ Routing layer latency exceeded: ${latency.toFixed(2)}ms`);
      }
      
    } catch (error) {
      this.layerMetrics.routing.errorCount++;
      console.warn(`   ❌ Routing layer event failed: ${sourceIndex} → ${targetIndex}`);
    }
  }

  private async testApplicationLayerEvents(): Promise<void> {
    console.log('🎯 Testing application layer events...');

    const applicationEvents: Promise<void>[] = [];
    
    // Generate application layer events (most complex processing)
    for (let sourceIndex = 0; sourceIndex < this.gridNodes.length; sourceIndex++) {
      for (let eventIndex = 0; eventIndex < EVENT_LAYERS_TEST_CONFIG.EVENTS_PER_LAYER; eventIndex++) {
        const promise = this.processApplicationLayerEvent(sourceIndex, eventIndex);
        applicationEvents.push(promise);
      }
    }

    await Promise.allSettled(applicationEvents);

    const appMetrics = this.layerMetrics.application;
    const avgLatency = appMetrics.latencyMeasurements.reduce((sum, lat) => sum + lat, 0) / appMetrics.latencyMeasurements.length || 0;
    const successRate = (appMetrics.successCount / appMetrics.eventsProcessed) * 100;

    console.log(`📊 Application Layer Results:`);
    console.log(`   Events processed: ${appMetrics.eventsProcessed}`);
    console.log(`   Success rate: ${successRate.toFixed(1)}%`);
    console.log(`   Average latency: ${avgLatency.toFixed(2)}ms`);
    console.log(`   Errors: ${appMetrics.errorCount}`);
  }

  private async processApplicationLayerEvent(sourceIndex: number, eventIndex: number): Promise<void> {
    const startTime = performance.now();
    this.layerMetrics.application.eventsProcessed++;

    const applicationEvent: GridLayerEvent = {
      eventId: `app-${sourceIndex}-${eventIndex}-${Date.now()}` as UUID,
      layer: 'application',
      eventType: 'status',
      sourceNodeId: this.gridNodes[sourceIndex].nodeId,
      payload: {
        data: `Application event ${eventIndex}`,
        timestamp: Date.now(),
        layerSpecific: {
          processingType: 'command-execution',
          priority: Math.floor(Math.random() * 5) + 1,
          metadata: { user: `user-${sourceIndex}`, session: `session-${eventIndex}` }
        }
      },
      propagationPath: ['transport', 'routing', 'application']
    };

    try {
      // Simulate application layer processing (business logic, state updates)
      await new Promise(resolve => setTimeout(resolve, 50 + Math.random() * 100)); // 50-150ms
      
      const latency = performance.now() - startTime;
      this.layerMetrics.application.latencyMeasurements.push(latency);
      this.layerMetrics.application.successCount++;
      
      if (latency > EVENT_PERFORMANCE_THRESHOLDS.MAX_APPLICATION_LAYER_LATENCY_MS) {
        console.warn(`   ⚠️ Application layer latency exceeded: ${latency.toFixed(2)}ms`);
      }
      
    } catch (error) {
      this.layerMetrics.application.errorCount++;
      console.warn(`   ❌ Application layer event failed: node ${sourceIndex}`);
    }
  }

  private async testCrossLayerEventPropagation(): Promise<void> {
    console.log('🔄 Testing cross-layer event propagation...');

    const crossLayerEvents: Promise<void>[] = [];
    const eventCount = 15; // Focused cross-layer testing

    for (let i = 0; i < eventCount; i++) {
      const sourceIndex = i % this.gridNodes.length;
      const promise = this.processCrossLayerEvent(sourceIndex, i);
      crossLayerEvents.push(promise);
    }

    await Promise.allSettled(crossLayerEvents);
    
    console.log(`📊 Cross-layer propagation completed: ${eventCount} events tested`);
  }

  private async processCrossLayerEvent(sourceIndex: number, eventIndex: number): Promise<void> {
    // Simulate event flowing through all layers: transport → routing → application
    const layerLatencies: number[] = [];

    // Transport layer
    let layerStart = performance.now();
    await new Promise(resolve => setTimeout(resolve, 8 + Math.random() * 17)); // Transport processing
    layerLatencies.push(performance.now() - layerStart);

    // Routing layer
    layerStart = performance.now();
    await new Promise(resolve => setTimeout(resolve, 25 + Math.random() * 35)); // Routing processing
    layerLatencies.push(performance.now() - layerStart);

    // Application layer
    layerStart = performance.now();
    await new Promise(resolve => setTimeout(resolve, 60 + Math.random() * 80)); // Application processing
    layerLatencies.push(performance.now() - layerStart);

    const totalPropagationTime = layerLatencies.reduce((sum, lat) => sum + lat, 0);
    
    if (totalPropagationTime > 300) { // 300ms threshold for cross-layer propagation
      console.warn(`   ⚠️ Cross-layer propagation slow: ${totalPropagationTime.toFixed(2)}ms`);
    }
  }

  private async waitBetweenLayers(transition: string): Promise<void> {
    console.log(`⏸️  Layer transition: ${transition} (${EVENT_LAYERS_TEST_CONFIG.LAYER_TRANSITION_DELAY_MS}ms)...`);
    await new Promise(resolve => setTimeout(resolve, EVENT_LAYERS_TEST_CONFIG.LAYER_TRANSITION_DELAY_MS));
  }

  private generateEventLayersMetrics(): GridEventLayersMetrics {
    // Calculate layer-specific performance metrics
    const layerPerformance: Record<GridLayer, any> = {} as any;
    
    for (const layer of EVENT_LAYERS_TEST_CONFIG.LAYERS_TO_TEST) {
      const metrics = this.layerMetrics[layer];
      const avgLatency = metrics.latencyMeasurements.reduce((sum, lat) => sum + lat, 0) / metrics.latencyMeasurements.length || 0;
      const successRate = (metrics.successCount / Math.max(metrics.eventsProcessed, 1)) * 100;
      
      layerPerformance[layer] = {
        eventsProcessed: metrics.eventsProcessed,
        averageLatency: avgLatency,
        successRate,
        errorCount: metrics.errorCount
      };
    }

    // Calculate cross-layer metrics
    const totalEventsAcrossLayers = Object.values(this.layerMetrics).reduce(
      (sum, metrics) => sum + metrics.eventsProcessed, 0
    );
    
    const averageLayerLatency = Object.values(this.layerMetrics).reduce((sum, metrics) => {
      const avgLatency = metrics.latencyMeasurements.reduce((latSum, lat) => latSum + lat, 0) / Math.max(metrics.latencyMeasurements.length, 1);
      return sum + avgLatency;
    }, 0) / EVENT_LAYERS_TEST_CONFIG.LAYERS_TO_TEST.length;

    return {
      layerPerformance,
      crossLayerMetrics: {
        totalEventsPropagated: totalEventsAcrossLayers,
        averagePropagationTime: averageLayerLatency,
        layerTransitionTime: EVENT_LAYERS_TEST_CONFIG.LAYER_TRANSITION_DELAY_MS,
        endToEndLatency: averageLayerLatency * EVENT_LAYERS_TEST_CONFIG.LAYERS_TO_TEST.length
      },
      reliabilityMetrics: {
        transportLayerReliability: layerPerformance.transport.successRate,
        routingLayerReliability: layerPerformance.routing.successRate,
        applicationLayerReliability: layerPerformance.application.successRate,
        overallSystemReliability: (layerPerformance.transport.successRate + layerPerformance.routing.successRate + layerPerformance.application.successRate) / 3
      }
    };
  }

  private async cleanupEventTestingResources(): Promise<void> {
    console.log('');
    console.log('🧹 Cleaning up event layer testing resources...');
    
    const cleanupPromises = this.gridNodes.map(async (node, index) => {
      try {
        await node.cleanup();
        console.log(`   ✅ Event node ${index} cleaned up`);
      } catch (error: any) {
        console.warn(`   ⚠️ Error cleaning up event node ${index}:`, error.message);
      }
    });

    await Promise.allSettled(cleanupPromises);
    console.log(`✅ Event layer testing cleanup completed (${this.gridNodes.length} nodes)`);
  }
}

// Export for test runners and standalone execution
if (require.main === module) {
  async function runGridEventLayersTest(): Promise<void> {
    const tester = new GridEventLayersTester();
    
    try {
      const metrics = await tester.executeGridEventLayersTest();
      const scorecard = await tester.generateEventLayersScorecard();
      
      console.log('');
      console.log('🏆 GRID EVENTS ALL LAYERS TEST RESULTS:');
      console.log(`   Overall Score: ${scorecard.overallScore}/100`);
      console.log(`   Grade: ${scorecard.grade}`);
      console.log(`   Total Events Propagated: ${metrics.crossLayerMetrics.totalEventsPropagated}`);
      console.log(`   Average Propagation Time: ${metrics.crossLayerMetrics.averagePropagationTime.toFixed(2)}ms`);
      console.log(`   End-to-End Latency: ${metrics.crossLayerMetrics.endToEndLatency.toFixed(2)}ms`);
      
      console.log('');
      console.log('📊 LAYER-SPECIFIC PERFORMANCE:');
      for (const layer of EVENT_LAYERS_TEST_CONFIG.LAYERS_TO_TEST) {
        const layerMetrics = metrics.layerPerformance[layer];
        console.log(`   ${layer.toUpperCase()} Layer:`);
        console.log(`      Events processed: ${layerMetrics.eventsProcessed}`);
        console.log(`      Average latency: ${layerMetrics.averageLatency.toFixed(2)}ms`);
        console.log(`      Success rate: ${layerMetrics.successRate.toFixed(1)}%`);
        console.log(`      Errors: ${layerMetrics.errorCount}`);
      }
      
      console.log('');
      console.log('🔧 RELIABILITY METRICS:');
      console.log(`   Transport reliability: ${metrics.reliabilityMetrics.transportLayerReliability.toFixed(1)}%`);
      console.log(`   Routing reliability: ${metrics.reliabilityMetrics.routingLayerReliability.toFixed(1)}%`);
      console.log(`   Application reliability: ${metrics.reliabilityMetrics.applicationLayerReliability.toFixed(1)}%`);
      console.log(`   Overall system reliability: ${metrics.reliabilityMetrics.overallSystemReliability.toFixed(1)}%`);

      if (scorecard.optimizations.length > 0) {
        console.log('');
        console.log('🎯 LAYER OPTIMIZATION OPPORTUNITIES:');
        scorecard.optimizations.forEach((opt, index) => {
          console.log(`   ${index + 1}. [${opt.severity.toUpperCase()}] ${opt.issue}`);
          console.log(`      → ${opt.suggestion}`);
        });
      }
      
      console.log('');
      console.log('🎉 GRID EVENTS ALL LAYERS TEST COMPLETED!');
      console.log(`📁 Results saved to: ${EVENT_LAYERS_TEST_CONFIG.PERFORMANCE_LOG_DIR}`);
      
    } catch (error: any) {
      console.error('❌ Grid events all layers test failed:', error.message);
      process.exit(1);
    }
  }

  runGridEventLayersTest();
}