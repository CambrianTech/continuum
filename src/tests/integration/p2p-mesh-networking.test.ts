/**
 * P2P Mesh Networking Integration Tests
 * 
 * Tests real P2P communication between multiple JTAG instances.
 * Spawns multiple processes with different ports to test mesh networking.
 */

import { spawn, exec } from 'child_process';
import { promisify } from 'util';
import { createTestId } from '../test-utils/TestIdGenerator';
import { DynamicPortConfigServer } from '../../system/core/config/server/DynamicPortConfigServer';
import type { JTAGInstanceConfig } from '../../system/core/config/shared/PortConfigTypes';
import { PORT_RANGES } from '../../system/core/config/shared/PortConfigTypes';

const execAsync = promisify(exec);

interface TestJTAGInstance {
  process: any;
  config: JTAGInstanceConfig;
}

/**
 * Multi-instance P2P test suite
 */
export class P2PIntegrationTestSuite {
  private instances: TestJTAGInstance[] = [];
  private testId: string;
  private portManager: DynamicPortConfigServer;

  constructor() {
    this.testId = createTestId('p2p-test');
    this.portManager = new DynamicPortConfigServer(PORT_RANGES.P2P_TESTING);
  }

  /**
   * Create dynamic port configuration for multiple instances
   */
  private async createInstanceConfigs(instanceCount: number): Promise<JTAGInstanceConfig[]> {
    return await this.portManager.createMultiInstanceConfigs(
      instanceCount, 
      'test', 
      ['screenshot', 'file-operations', 'p2p-testing']
    );
  }

  /**
   * Start JTAG instance with custom configuration
   */
  private async startJTAGInstance(config: JTAGInstanceConfig): Promise<TestJTAGInstance> {
    console.log(`🚀 Starting JTAG instance ${config.nodeId} on ports WS:${config.wsPort} HTTP:${config.httpPort}`);
    
    // Create environment with custom port configuration
    const env = {
      ...process.env,
      ...this.portManager.getEnvironmentVariables(config)
    };

    return new Promise((resolve, reject) => {
      const process = spawn('npm', ['run', 'system:start'], {
        cwd: process.cwd(),
        env,
        detached: true,
        stdio: ['ignore', 'pipe', 'pipe']
      });

      let startupData = '';
      const timeout = setTimeout(() => {
        reject(new Error(`Instance ${config.nodeId} failed to start within timeout`));
      }, 45000); // 45 second timeout

      process.stdout?.on('data', (data: Buffer) => {
        startupData += data.toString();
        
        // Look for successful startup indicators
        if (startupData.includes('System healthy') || 
            startupData.includes('Bootstrap complete')) {
          clearTimeout(timeout);
          
          const instance: TestJTAGInstance = {
            process,
            config
          };
          
          resolve(instance);
        }
      });

      process.stderr?.on('data', (data: Buffer) => {
        console.error(`❌ Instance ${config.nodeId} error:`, data.toString());
      });

      process.on('error', (error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });
  }

  /**
   * Test P2P node discovery
   */
  async testNodeDiscovery(instanceCount = 3): Promise<boolean> {
    console.log(`\n🧪 Testing P2P node discovery with ${instanceCount} instances`);
    
    try {
      // Create instance configurations
      const configs = await this.createInstanceConfigs(instanceCount);
      
      // Start all instances
      console.log('📡 Starting JTAG instances...');
      const startPromises = configs.map(config => this.startJTAGInstance(config));
      this.instances = await Promise.all(startPromises);
      
      console.log(`✅ Started ${this.instances.length} JTAG instances`);
      
      // Wait for discovery protocol to complete
      console.log('⏳ Waiting for P2P discovery...');
      await new Promise(resolve => setTimeout(resolve, 10000)); // 10 seconds for discovery
      
      // Test discovery by checking if nodes can see each other
      const discoveryResults = await this.verifyNodeDiscovery();
      
      console.log('📊 Discovery Results:');
      discoveryResults.forEach(result => {
        console.log(`   ${result.nodeId}: discovered ${result.discoveredNodes.length} peers`);
      });
      
      // Should discover instanceCount-1 peers (all others)
      const expectedPeers = instanceCount - 1;
      const allDiscovered = discoveryResults.every(r => r.discoveredNodes.length >= expectedPeers);
      
      if (allDiscovered) {
        console.log('✅ P2P Node Discovery: SUCCESS');
        return true;
      } else {
        console.log('❌ P2P Node Discovery: FAILED - not all nodes discovered each other');
        return false;
      }
      
    } catch (error: any) {
      console.error('❌ P2P Node Discovery test failed:', error.message);
      return false;
    } finally {
      await this.cleanup();
    }
  }

  /**
   * Verify node discovery by querying each instance
   */
  private async verifyNodeDiscovery(): Promise<any[]> {
    const results = [];
    
    for (const instance of this.instances) {
      try {
        // Query the instance's discovered nodes via HTTP API
        const response = await fetch(`http://localhost:${instance.config.httpPort}/api/p2p/nodes`);
        const data = await response.json();
        
        results.push({
          nodeId: instance.config.nodeId,
          discoveredNodes: data.nodes || [],
          error: null
        });
      } catch (error: any) {
        results.push({
          nodeId: instance.config.nodeId,
          discoveredNodes: [],
          error: error.message
        });
      }
    }
    
    return results;
  }

  /**
   * Test P2P message routing
   */
  async testMessageRouting(): Promise<boolean> {
    console.log('\n🧪 Testing P2P message routing');
    
    if (this.instances.length < 2) {
      console.log('❌ Need at least 2 instances for routing test');
      return false;
    }
    
    try {
      const sourceNode = this.instances[0];
      const targetNode = this.instances[1];
      
      // Send a message from source to target via P2P routing
      const message = {
        command: 'ping',
        params: { message: 'P2P routing test' },
        targetNodeId: targetNode.config.nodeId
      };
      
      console.log(`📤 Sending message from ${sourceNode.config.nodeId} to ${targetNode.config.nodeId}`);
      
      const response = await fetch(`http://localhost:${sourceNode.config.httpPort}/api/p2p/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(message)
      });
      
      const result = await response.json();
      
      if (result.success) {
        console.log('✅ P2P Message Routing: SUCCESS');
        return true;
      } else {
        console.log('❌ P2P Message Routing: FAILED -', result.error);
        return false;
      }
      
    } catch (error: any) {
      console.error('❌ P2P Message Routing test failed:', error.message);
      return false;
    }
  }

  /**
   * Test mesh resilience - node failure and recovery
   */
  async testMeshResilience(): Promise<boolean> {
    console.log('\n🧪 Testing P2P mesh resilience');
    
    if (this.instances.length < 3) {
      console.log('❌ Need at least 3 instances for resilience test');
      return false;
    }
    
    try {
      // Kill middle node
      const middleNode = this.instances[1];
      console.log(`💀 Terminating node ${middleNode.config.nodeId}`);
      middleNode.process.kill('SIGTERM');
      
      // Wait for failure detection
      await new Promise(resolve => setTimeout(resolve, 5000));
      
      // Test routing still works between remaining nodes
      const sourceNode = this.instances[0];
      const targetNode = this.instances[2];
      
      const message = {
        command: 'ping',
        params: { message: 'Resilience test' },
        targetNodeId: targetNode.config.nodeId
      };
      
      console.log(`📤 Testing routing after node failure: ${sourceNode.config.nodeId} → ${targetNode.config.nodeId}`);
      
      const response = await fetch(`http://localhost:${sourceNode.config.httpPort}/api/p2p/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(message)
      });
      
      const result = await response.json();
      
      if (result.success) {
        console.log('✅ P2P Mesh Resilience: SUCCESS');
        return true;
      } else {
        console.log('❌ P2P Mesh Resilience: FAILED -', result.error);
        return false;
      }
      
    } catch (error: any) {
      console.error('❌ P2P Mesh Resilience test failed:', error.message);
      return false;
    }
  }

  /**
   * Clean up all test instances
   */
  async cleanup(): Promise<void> {
    console.log('\n🧹 Cleaning up P2P test instances...');
    
    for (const instance of this.instances) {
      try {
        if (!instance.process.killed) {
          instance.process.kill('SIGTERM');
          
          // Give it time to shut down gracefully
          await new Promise(resolve => setTimeout(resolve, 2000));
          
          if (!instance.process.killed) {
            instance.process.kill('SIGKILL');
          }
        }
        
        // Clean up ports using port manager
        try {
          await this.portManager.killPortProcesses(instance.config);
        } catch (error) {
          // Ignore cleanup errors
        }
        
      } catch (error: any) {
        console.warn(`⚠️ Error cleaning up instance ${instance.config.nodeId}:`, error.message);
      }
    }
    
    this.instances = [];
    console.log('✅ P2P test cleanup complete');
  }
}

/**
 * Run P2P integration tests
 */
async function runP2PIntegrationTests(): Promise<void> {
  console.log('🚀 P2P Mesh Networking Integration Tests');
  console.log('=========================================');
  
  const testSuite = new P2PIntegrationTestSuite();
  const results: boolean[] = [];
  
  try {
    // Test 1: Node Discovery
    results.push(await testSuite.testNodeDiscovery(3));
    
    // Test 2: Message Routing  
    results.push(await testSuite.testMessageRouting());
    
    // Test 3: Mesh Resilience
    results.push(await testSuite.testMeshResilience());
    
  } finally {
    await testSuite.cleanup();
  }
  
  // Results summary
  console.log('\n📊 P2P Integration Test Results:');
  const passed = results.filter(r => r).length;
  const total = results.length;
  
  console.log(`✅ Passed: ${passed}/${total}`);
  console.log(`❌ Failed: ${total - passed}/${total}`);
  
  if (passed === total) {
    console.log('🎉 All P2P integration tests PASSED!');
    process.exit(0);
  } else {
    console.log('💥 Some P2P integration tests FAILED!');
    process.exit(1);
  }
}

// Run tests if executed directly
if (require.main === module) {
  runP2PIntegrationTests().catch(error => {
    console.error('❌ P2P integration test suite failed:', error);
    process.exit(1);
  });
}

export { P2PIntegrationTestSuite };