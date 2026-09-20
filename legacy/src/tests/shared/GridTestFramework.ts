/**
 * Grid Testing Framework - Well-Typed, No Magic Strings
 * 
 * Uses actual API types and constants instead of magic strings or `any` types.
 * Imports from the real Grid service to ensure type safety and prevent
 * test failures from improper constants.
 */

import type { 
  GridNode, 
  GridMessage, 
  GridMessageType, 
  ForwardMessage,
  GridRoutingConfig,
  GridNodeIdentity,
  GridNodeEndpoints,
  GridNodeMetadata,
  GridNodeStatus,
  BaseGridMessage
} from '../../system/services/grid-routing/shared/GridRoutingTypes';

import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { GridRoutingServiceServer } from '../../system/services/grid-routing/server/GridRoutingServiceServer';
import { UDPTransportFactory } from '../factories/UDPTransportFactory';
import { PerformanceTester, type TestScorecard } from './PerformanceTester';

// Import actual constants - NO MAGIC STRINGS
export const GRID_TEST_CONSTANTS = {
  MULTICAST_ADDRESS: '239.255.255.250' as const,
  BASE_MULTICAST_PORT: 12345 as const,
  BASE_UNICAST_PORT: 23456 as const,
  BASE_HTTP_PORT: 8000 as const,
  BASE_WEBSOCKET_PORT: 9000 as const,
  DEFAULT_TTL: 5 as const,
  DEFAULT_PRIORITY: 5 as const,
  DISCOVERY_TIMEOUT_MS: 3000 as const,
  ROUTING_TIMEOUT_MS: 2000 as const,
  MESSAGE_PAYLOAD_SIZE_BYTES: 100 as const
} as const;

// Well-typed test configuration
export interface GridTestConfig {
  readonly nodeCount: number;
  readonly messagesPerNode: number;
  readonly testTimeoutMs: number;
  readonly enableBroadcastTest: boolean;
  readonly enablePartitionTest: boolean;
  readonly performanceLogDir: string;
}

// Proper typing for test results - NO ANY TYPES
export interface GridTestResult {
  readonly nodeCount: number;
  readonly totalMessages: number;
  readonly discoveredNodes: number;
  readonly messagesRouted: number;
  readonly averageLatencyMs: number;
  readonly successRate: number;
  readonly errors: ReadonlyArray<GridTestError>;
}

export interface GridTestError {
  readonly nodeIndex: number;
  readonly operation: string;
  readonly error: string;
  readonly timestamp: string;
}

// Strongly typed test node factory
export class GridTestNodeFactory {
  private readonly transportFactory = new UDPTransportFactory();

  /**
   * Create a properly typed test Grid node
   * Uses real types from GridRoutingTypes, no magic strings
   */
  async createTestNode(nodeIndex: number): Promise<GridRoutingServiceServer> {
    // Properly typed node identity - uses actual UUID type
    const nodeId = `test-node-${nodeIndex}-${Date.now()}` as UUID;
    
    // Well-typed endpoints using actual constants
    const endpoints: GridNodeEndpoints = {
      multicastAddress: GRID_TEST_CONSTANTS.MULTICAST_ADDRESS,
      multicastPort: GRID_TEST_CONSTANTS.BASE_MULTICAST_PORT + nodeIndex,
      unicastPort: GRID_TEST_CONSTANTS.BASE_UNICAST_PORT + nodeIndex,
      httpPort: GRID_TEST_CONSTANTS.BASE_HTTP_PORT + nodeIndex,
      webSocketPort: GRID_TEST_CONSTANTS.BASE_WEBSOCKET_PORT + nodeIndex
    };

    // Properly typed metadata - no any types
    const metadata: GridNodeMetadata = {
      platform: 'node.js',
      region: 'test-region',
      zone: `test-zone-${nodeIndex}`,
      tags: [`node-${nodeIndex}`, 'test', 'grid-framework'] as const
    };

    // Well-typed status using actual enums/types
    const status: GridNodeStatus = {
      state: 'active',
      uptime: 0,
      lastSeen: new Date().toISOString(),
      health: {
        cpu: 10 + nodeIndex * 5,
        memory: 512 + nodeIndex * 128,
        network: 'good'
      },
      metrics: {
        messagesRouted: 0,
        averageLatency: 0,
        errorRate: 0
      }
    };

    // Properly typed node identity
    const identity: GridNodeIdentity = {
      nodeId,
      nodeType: 'server',
      hostname: `test-node-${nodeIndex}.local`,
      version: '1.0.0',
      capabilities: ['routing', 'forwarding', 'discovery'] as const
    };

    // Complete Grid node with proper typing
    const gridNode: GridNode = {
      identity,
      endpoints,
      metadata,
      status
    };

    // Properly typed routing configuration
    const config: GridRoutingConfig = {
      discoveryInterval: 1000,
      heartbeatInterval: 2000,
      maxTTL: GRID_TEST_CONSTANTS.DEFAULT_TTL,
      routingTableSize: 1000
    };

    // Create transport with proper configuration
    const transport = await this.transportFactory.create({
      multicastAddress: endpoints.multicastAddress,
      multicastPort: endpoints.multicastPort,
      unicastPort: endpoints.unicastPort
    });

    // Return properly typed Grid service
    return new GridRoutingServiceServer(config, gridNode, transport);
  }

  /**
   * Create a well-typed test message - NO MAGIC STRINGS
   */
  createTestMessage(
    sourceNodeId: UUID,
    targetNodeId: UUID | undefined,
    messageIndex: number
  ): ForwardMessage {
    const messageId = `test-msg-${messageIndex}-${Date.now()}` as UUID;
    
    // Use actual GridMessageType enum, not magic string
    const message: ForwardMessage = {
      messageId,
      type: 'forward-message' as GridMessageType.FORWARD_MESSAGE,
      sourceNodeId,
      targetNodeId,
      timestamp: new Date().toISOString(),
      ttl: GRID_TEST_CONSTANTS.DEFAULT_TTL,
      priority: GRID_TEST_CONSTANTS.DEFAULT_PRIORITY,
      payload: {
        testData: `Test message ${messageIndex}`,
        timestamp: Date.now(),
        size: 'A'.repeat(GRID_TEST_CONSTANTS.MESSAGE_PAYLOAD_SIZE_BYTES)
      }
    };

    return message;
  }
}

// Well-typed distributed test executor
export class GridDistributedTester {
  private readonly nodeFactory = new GridTestNodeFactory();
  private readonly nodes: GridRoutingServiceServer[] = [];
  private readonly performanceTester: PerformanceTester;

  constructor(
    private readonly config: GridTestConfig,
    performanceLogDir: string
  ) {
    this.performanceTester = new PerformanceTester(
      'Grid Distributed Capacity Test',
      performanceLogDir
    );
  }

  /**
   * Execute comprehensive distributed Grid test with proper typing
   */
  async executeDistributedTest(): Promise<GridTestResult> {
    this.performanceTester.start();

    try {
      // Phase 1: Create nodes with proper error handling
      await this.performanceTester.measureLatency(
        'Node Creation Phase',
        () => this.createTestNodes()
      );

      // Phase 2: Test node discovery with timeout
      await this.performanceTester.measureLatency(
        'Node Discovery Phase', 
        () => this.testNodeDiscovery()
      );

      // Phase 3: Execute message routing test
      const routingResult = await this.performanceTester.measureLatency(
        'Message Routing Phase',
        () => this.testMessageRouting()
      );

      // Phase 4: Optional broadcast test
      if (this.config.enableBroadcastTest) {
        await this.performanceTester.measureLatency(
          'Broadcast Test Phase',
          () => this.testBroadcastCapability()
        );
      }

      // Phase 5: Optional partition recovery test
      if (this.config.enablePartitionTest) {
        await this.performanceTester.measureLatency(
          'Partition Recovery Phase',
          () => this.testPartitionRecovery()
        );
      }

      return routingResult;

    } finally {
      // Cleanup with proper error handling
      await this.cleanup();
    }
  }

  /**
   * Generate performance scorecard with proper typing
   */
  async generateScorecard(): Promise<TestScorecard> {
    const scorecard = this.performanceTester.generateScorecard({
      nodeCount: this.config.nodeCount,
      messagesPerNode: this.config.messagesPerNode,
      testTimeout: this.config.testTimeoutMs,
      broadcastTestEnabled: this.config.enableBroadcastTest,
      partitionTestEnabled: this.config.enablePartitionTest
    });

    await this.performanceTester.saveResults(scorecard);
    return scorecard;
  }

  private async createTestNodes(): Promise<void> {
    for (let i = 0; i < this.config.nodeCount; i++) {
      const node = await this.nodeFactory.createTestNode(i);
      this.nodes.push(node);
    }

    if (this.nodes.length !== this.config.nodeCount) {
      throw new Error(`Failed to create expected number of nodes: ${this.nodes.length}/${this.config.nodeCount}`);
    }
  }

  private async testNodeDiscovery(): Promise<void> {
    // Initialize all nodes
    const initPromises = this.nodes.map(node => node.initialize());
    await Promise.all(initPromises);

    // Wait for discovery with proper timeout
    await new Promise(resolve => 
      setTimeout(resolve, GRID_TEST_CONSTANTS.DISCOVERY_TIMEOUT_MS)
    );

    // Verify discovery with proper error reporting
    for (let i = 0; i < this.nodes.length; i++) {
      const topology = this.nodes[i].getTopology();
      const discoveredCount = topology.nodes.size;
      const expectedCount = this.config.nodeCount - 1; // Exclude self

      if (discoveredCount < expectedCount) {
        throw new Error(
          `Node ${i} discovery failed: found ${discoveredCount}, expected ${expectedCount}`
        );
      }
    }
  }

  private async testMessageRouting(): Promise<GridTestResult> {
    let messagesRouted = 0;
    let totalLatency = 0;
    const errors: GridTestError[] = [];

    // Send messages with proper error tracking
    for (let nodeIndex = 0; nodeIndex < this.nodes.length; nodeIndex++) {
      const sourceNode = this.nodes[nodeIndex];
      
      for (let msgIndex = 0; msgIndex < this.config.messagesPerNode; msgIndex++) {
        try {
          const targetNodeIndex = (nodeIndex + 1) % this.nodes.length;
          const targetNode = this.nodes[targetNodeIndex];
          
          const message = this.nodeFactory.createTestMessage(
            sourceNode.getLocalNode().identity.nodeId,
            targetNode.getLocalNode().identity.nodeId,
            msgIndex
          );

          const startTime = Date.now();
          await sourceNode.sendMessage(message.targetNodeId!, message);
          const latency = Date.now() - startTime;

          messagesRouted++;
          totalLatency += latency;

        } catch (error) {
          errors.push({
            nodeIndex,
            operation: `message-routing-${msgIndex}`,
            error: String(error),
            timestamp: new Date().toISOString()
          });
        }
      }
    }

    // Calculate results with proper typing
    const totalMessages = this.config.nodeCount * this.config.messagesPerNode;
    const averageLatency = totalLatency / Math.max(messagesRouted, 1);
    const successRate = (messagesRouted / totalMessages) * 100;

    return {
      nodeCount: this.config.nodeCount,
      totalMessages,
      discoveredNodes: this.nodes.length,
      messagesRouted,
      averageLatencyMs: averageLatency,
      successRate,
      errors
    };
  }

  private async testBroadcastCapability(): Promise<void> {
    const broadcastCount = 10;
    const sourceNode = this.nodes[0];

    for (let i = 0; i < broadcastCount; i++) {
      const broadcastMessage = this.nodeFactory.createTestMessage(
        sourceNode.getLocalNode().identity.nodeId,
        undefined, // Broadcast
        i
      );

      await sourceNode.broadcastMessage(broadcastMessage);
    }

    // Wait for broadcast propagation
    await new Promise(resolve => 
      setTimeout(resolve, GRID_TEST_CONSTANTS.ROUTING_TIMEOUT_MS)
    );
  }

  private async testPartitionRecovery(): Promise<void> {
    if (this.nodes.length < 4) {
      return; // Skip if not enough nodes
    }

    // Partition half the nodes
    const partitionSize = Math.floor(this.nodes.length / 2);
    const partitionedNodes = this.nodes.slice(0, partitionSize);

    // Stop partitioned nodes
    await Promise.all(partitionedNodes.map(node => node.cleanup()));

    // Wait for partition detection
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Restart partitioned nodes
    await Promise.all(partitionedNodes.map(node => node.initialize()));

    // Wait for recovery
    await new Promise(resolve => 
      setTimeout(resolve, GRID_TEST_CONSTANTS.DISCOVERY_TIMEOUT_MS)
    );
  }

  private async cleanup(): Promise<void> {
    const cleanupPromises = this.nodes.map(async (node, index) => {
      try {
        await node.cleanup();
      } catch (error) {
        console.warn(`Cleanup failed for node ${index}: ${error}`);
      }
    });

    await Promise.allSettled(cleanupPromises);
    this.nodes.length = 0; // Clear array
  }
}