/**
 * P2P Multi-Node Test Scenarios
 * 
 * Tests that properly handle the P2P nature of UDP multicast transport.
 * These tests create multiple transport instances that can communicate with each other.
 */

import { TransportTestScenario, TestCategory, type TransportMetrics } from '../framework/TransportTestFramework';
import type { JTAGTransport } from '../../system/transports/shared/TransportTypes';
import type { JTAGMessage } from '../../system/core/types/JTAGTypes';
import { UDPTransportFactory } from '../factories/UDPTransportFactory';
import { NodeType, NodeCapability } from '../../system/transports/udp-multicast-transport/shared/UDPMulticastTypes';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

/**
 * P2P Message Metrics
 */
export interface P2PMessageMetrics extends TransportMetrics {
  readonly peerCount: number;
  readonly messagesExchanged: number;
  readonly crossPeerSuccess: boolean;
}

/**
 * P2P Discovery Metrics  
 */
export interface P2PDiscoveryMetrics extends TransportMetrics {
  readonly nodesCreated: number;
  readonly peersDiscovered: number;
  readonly discoverySuccess: boolean;
}

/**
 * Test: P2P Peer Discovery
 * Creates multiple transport nodes and verifies they discover each other
 */
export class P2PPeerDiscoveryScenario extends TransportTestScenario<{ nodeCount: number }, P2PDiscoveryMetrics> {
  constructor(nodeCount = 2) {
    super('P2P Peer Discovery', TestCategory.DISCOVERY, { nodeCount }, 20000);
  }

  async execute(): Promise<any> {
    const startTime = Date.now();
    const factory = new UDPTransportFactory();
    const transports = [];
    
    try {
      // Create multiple transport nodes
      console.log(`🌐 Creating ${this.config.nodeCount} P2P transport nodes...`);
      
      for (let i = 0; i < this.config.nodeCount; i++) {
        const transport = await factory.create({
          nodeType: NodeType.SERVER,
          capabilities: [NodeCapability.FILE_OPERATIONS],
          basePort: 47000 + (i * 10)
        });
        
        // Initialize each transport
        if (typeof (transport as any).initialize === 'function') {
          await (transport as any).initialize();
        }
        
        transports.push(transport);
      }
      
      // Wait for peer discovery
      console.log('⏳ Waiting for peer discovery...');
      await new Promise(resolve => setTimeout(resolve, 8000));
      
      // Check discovery results
      let totalPeersDiscovered = 0;
      for (const transport of transports) {
        if (typeof (transport as any).getStats === 'function') {
          const stats = (transport as any).getStats();
          console.log(`📡 Node discovered ${stats.nodesDiscovered || 0} peers`);
          totalPeersDiscovered += stats.nodesDiscovered || 0;
        }
      }
      
      // Expected: Each node should discover (nodeCount - 1) peers
      const expectedDiscoveries = this.config.nodeCount * (this.config.nodeCount - 1);
      const discoverySuccess = totalPeersDiscovered >= expectedDiscoveries * 0.5; // 50% success threshold
      
      const metrics: P2PDiscoveryMetrics = {
        nodesCreated: transports.length,
        peersDiscovered: totalPeersDiscovered,
        discoverySuccess,
        connectionsEstablished: transports.filter(t => t.isConnected()).length
      };
      
      const duration = Date.now() - startTime;
      return this.createResult(discoverySuccess, duration, metrics);
      
    } catch (error: any) {
      const duration = Date.now() - startTime;
      const metrics: P2PDiscoveryMetrics = {
        nodesCreated: transports.length,
        peersDiscovered: 0,
        discoverySuccess: false,
        errorsEncountered: 1
      };
      
      return this.createResult(false, duration, metrics, error.message);
    } finally {
      // Cleanup
      for (const transport of transports) {
        try {
          await factory.cleanup(transport);
        } catch (cleanupError) {
          // Ignore cleanup errors
        }
      }
    }
  }
}

/**
 * Test: P2P Message Exchange
 * Creates two transport nodes and tests message passing between them
 */
export class P2PMessageExchangeScenario extends TransportTestScenario<{ messageCount: number }, P2PMessageMetrics> {
  constructor(messageCount = 2) {
    super('P2P Message Exchange', TestCategory.MESSAGE_PASSING, { messageCount }, 25000);
  }

  async execute(): Promise<any> {
    const startTime = Date.now();
    const factory = new UDPTransportFactory();
    
    try {
      // Create two transport nodes
      console.log('🌐 Creating P2P transport pair...');
      const nodeA = await factory.create({
        nodeType: NodeType.SERVER,
        capabilities: [NodeCapability.FILE_OPERATIONS],
        basePort: 48000
      });
      
      const nodeB = await factory.create({
        nodeType: NodeType.SERVER,
        capabilities: [NodeCapability.SCREENSHOT],
        basePort: 48010
      });
      
      // Initialize both nodes
      if (typeof (nodeA as any).initialize === 'function') {
        await (nodeA as any).initialize();
      }
      if (typeof (nodeB as any).initialize === 'function') {
        await (nodeB as any).initialize();
      }
      
      // Set up message handlers
      const receivedOnA: JTAGMessage[] = [];
      const receivedOnB: JTAGMessage[] = [];
      
      if (nodeA.setMessageHandler) {
        nodeA.setMessageHandler((message) => {
          console.log('📥 Node A received:', message.command);
          receivedOnA.push(message);
        });
      }
      
      if (nodeB.setMessageHandler) {
        nodeB.setMessageHandler((message) => {
          console.log('📥 Node B received:', message.command);
          receivedOnB.push(message);
        });
      }
      
      // Wait for peer discovery
      console.log('⏳ Waiting for peer discovery...');
      await new Promise(resolve => setTimeout(resolve, 6000));
      
      // Send messages from A to B and B to A
      console.log('📤 Exchanging messages...');
      const sendPromises = [];
      
      for (let i = 0; i < this.config.messageCount; i++) {
        // A → B
        const messageAtoB: JTAGMessage = {
          id: generateUUID(),
          command: `test-a-to-b-${i}`,
          payload: { from: 'nodeA', sequence: i },
          timestamp: new Date().toISOString()
        };
        
        // B → A  
        const messageBtoA: JTAGMessage = {
          id: generateUUID(),
          command: `test-b-to-a-${i}`,
          payload: { from: 'nodeB', sequence: i },
          timestamp: new Date().toISOString()
        };
        
        sendPromises.push(nodeA.send(messageAtoB));
        sendPromises.push(nodeB.send(messageBtoA));
      }
      
      const sendResults = await Promise.all(sendPromises);
      const successfulSends = sendResults.filter(r => r.success).length;
      
      // Wait for message delivery
      await new Promise(resolve => setTimeout(resolve, 3000));
      
      const messagesExchanged = receivedOnA.length + receivedOnB.length;
      const expectedMessages = this.config.messageCount * 2; // A→B + B→A
      const crossPeerSuccess = messagesExchanged >= expectedMessages * 0.5; // 50% success threshold
      
      const metrics: P2PMessageMetrics = {
        peerCount: 2,
        messagesExchanged,
        crossPeerSuccess,
        messagesSent: successfulSends,
        messagesReceived: messagesExchanged,
        bytesTransferred: this.estimateBytes(expectedMessages)
      };
      
      const duration = Date.now() - startTime;
      
      console.log(`📊 P2P Results: sent ${successfulSends}/${sendPromises.length}, received ${messagesExchanged}/${expectedMessages}`);
      
      return this.createResult(crossPeerSuccess, duration, metrics);
      
    } catch (error: any) {
      const duration = Date.now() - startTime;
      const metrics: P2PMessageMetrics = {
        peerCount: 0,
        messagesExchanged: 0,
        crossPeerSuccess: false,
        errorsEncountered: 1
      };
      
      return this.createResult(false, duration, metrics, error.message);
    }
  }
  
  private estimateBytes(messageCount: number): number {
    return messageCount * 250; // Estimated bytes per P2P message
  }
}

/**
 * Export P2P test scenarios
 */
export const P2P_MULTI_NODE_SCENARIOS = [
  new P2PPeerDiscoveryScenario(2),
  new P2PMessageExchangeScenario(2)
];