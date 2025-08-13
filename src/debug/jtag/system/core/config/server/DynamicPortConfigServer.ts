/**
 * Dynamic Port Configuration - Server Implementation
 * 
 * Server-specific port allocation using Node.js APIs.
 * Provides dynamic port allocation for multiple JTAG instances to enable
 * P2P testing and multi-instance deployments without port conflicts.
 */

import { promisify } from 'util';
import { exec } from 'child_process';
import type { 
  JTAGInstanceConfig, 
  PortRangeConfig, 
  PortAvailabilityResult,
  InstanceEnvironmentVars,
  PortConfigValidation,
  PortManagerStatus,
  PORT_RANGES
} from '../shared/PortConfigTypes';

const execAsync = promisify(exec);

/**
 * Server-side dynamic port configuration manager
 */
export class DynamicPortConfigServer {
  private allocatedPorts = new Set<number>();
  private instances = new Map<string, JTAGInstanceConfig>();

  constructor(private portRange: PortRangeConfig) {}

  /**
   * Check if a port is available (server-specific implementation)
   */
  async isPortAvailable(port: number): Promise<boolean> {
    try {
      const { stdout } = await execAsync(`lsof -i :${port}`, { timeout: 2000 });
      return false; // Port is in use
    } catch (error) {
      return true; // Port is available (lsof returned no results)
    }
  }

  /**
   * Find available ports in range
   */
  async findAvailablePorts(count: number, startPort?: number): Promise<number[]> {
    const searchStart = startPort || this.portRange.startPort;
    const searchEnd = this.portRange.endPort;
    const availablePorts: number[] = [];

    for (let port = searchStart; port <= searchEnd && availablePorts.length < count; port++) {
      // Skip reserved ports
      if (this.portRange.reservedPorts.includes(port)) {
        continue;
      }

      // Skip already allocated ports
      if (this.allocatedPorts.has(port)) {
        continue;
      }

      // Check if port is actually available
      if (await this.isPortAvailable(port)) {
        availablePorts.push(port);
        this.allocatedPorts.add(port);
      }
    }

    if (availablePorts.length < count) {
      throw new Error(
        `Could not find ${count} available ports in range ${searchStart}-${searchEnd}. ` +
        `Found only ${availablePorts.length} available ports.`
      );
    }

    return availablePorts;
  }

  /**
   * Create configuration for a single JTAG instance
   */
  async createInstanceConfig(
    nodeId: string, 
    nodeType: 'server' | 'browser' | 'test' = 'test',
    capabilities: string[] = []
  ): Promise<JTAGInstanceConfig> {
    // Need 3 ports: WebSocket, HTTP, and UDP unicast
    const ports = await this.findAvailablePorts(3);
    
    const config: JTAGInstanceConfig = {
      nodeId,
      nodeType,
      capabilities,
      wsPort: ports[0],
      httpPort: ports[1],
      multicastPort: 37471, // Fixed multicast port (shared across all instances)
      unicastPort: ports[2]
    };

    this.instances.set(nodeId, config);
    return config;
  }

  /**
   * Create configurations for multiple JTAG instances
   */
  async createMultiInstanceConfigs(
    instanceCount: number,
    nodeType: 'server' | 'browser' | 'test' = 'test',
    capabilities: string[] = []
  ): Promise<JTAGInstanceConfig[]> {
    const configs: JTAGInstanceConfig[] = [];

    for (let i = 0; i < instanceCount; i++) {
      const nodeId = `test-node-${i}`;
      const config = await this.createInstanceConfig(nodeId, nodeType, capabilities);
      configs.push(config);
    }

    return configs;
  }

  /**
   * Release allocated ports
   */
  releaseInstance(nodeId: string): void {
    const instance = this.instances.get(nodeId);
    if (instance) {
      this.allocatedPorts.delete(instance.wsPort);
      this.allocatedPorts.delete(instance.httpPort);
      this.allocatedPorts.delete(instance.unicastPort);
      this.instances.delete(nodeId);
    }
  }

  /**
   * Release all allocated ports
   */
  releaseAll(): void {
    this.allocatedPorts.clear();
    this.instances.clear();
  }

  /**
   * Get environment variables for instance configuration
   */
  getEnvironmentVariables(config: JTAGInstanceConfig): Record<string, string> {
    return {
      JTAG_NODE_ID: config.nodeId,
      JTAG_WS_PORT: config.wsPort.toString(),
      JTAG_HTTP_PORT: config.httpPort.toString(),
      JTAG_MULTICAST_PORT: config.multicastPort.toString(),
      JTAG_UNICAST_PORT: config.unicastPort.toString(),
      JTAG_NODE_TYPE: config.nodeType,
      JTAG_CAPABILITIES: config.capabilities.join(','),
      JTAG_TEST_MODE: 'true'
    };
  }

  /**
   * Kill processes using allocated ports (cleanup utility)
   */
  async killPortProcesses(config: JTAGInstanceConfig): Promise<void> {
    const ports = [config.wsPort, config.httpPort, config.unicastPort];
    
    for (const port of ports) {
      try {
        await execAsync(`lsof -ti:${port} | xargs kill -9 2>/dev/null || true`);
      } catch (error) {
        // Ignore cleanup errors
      }
    }
  }

  /**
   * Validate port configuration
   */
  validateConfig(config: JTAGInstanceConfig): PortConfigValidation {
    const errors: string[] = [];
    
    if (config.wsPort < 1 || config.wsPort > 65535) {
      errors.push(`Invalid WebSocket port: ${config.wsPort}`);
    }
    
    if (config.httpPort < 1 || config.httpPort > 65535) {
      errors.push(`Invalid HTTP port: ${config.httpPort}`);
    }
    
    if (config.unicastPort < 0 || config.unicastPort > 65535) {
      errors.push(`Invalid unicast port: ${config.unicastPort}`);
    }
    
    if (config.wsPort === config.httpPort) {
      errors.push('WebSocket and HTTP ports cannot be the same');
    }
    
    if (config.wsPort === config.unicastPort || config.httpPort === config.unicastPort) {
      errors.push('Unicast port conflicts with WebSocket or HTTP port');
    }
    
    return {
      valid: errors.length === 0,
      errors
    };
  }

  /**
   * Get status summary of all managed instances
   */
  getStatus(): PortManagerStatus {
    return {
      allocated: this.allocatedPorts.size,
      instances: this.instances.size,
      portRange: this.portRange
    };
  }
}