/**
 * Port Manager - Focused module for port allocation and conflict resolution
 * Handles DevTools port assignment, availability checking, and port cleanup
 */

export interface PortAllocation {
  port: number;
  allocated: Date;
  purpose: string;
  browserId?: string;
}

export interface PortConflictResolution {
  killedPorts: number[];
  freedPorts: number[];
  errors: string[];
}

export class PortManager {
  private allocatedPorts = new Map<number, PortAllocation>();
  private readonly devToolsPortRange = { min: 9222, max: 9299 };

  /**
   * Allocate available port for browser
   */
  async allocatePort(purpose: string = 'browser', browserId?: string): Promise<number> {
    try {
      // Import ProcessCommand for port checking
      const ProcessCommandModule = await import('../../../commands/kernel/system/ProcessCommand');
      const ProcessCommand = ProcessCommandModule.default;
      
      // Check ports in DevTools range
      const candidatePorts = this.getCandidatePorts();
      
      const result = await ProcessCommand.execute({
        subcommand: 'ports',
        ports: candidatePorts
      });
      
      if (result.success) {
        // Find first available port
        for (const port of candidatePorts) {
          const portData = result.data.portProcesses[port];
          if (!portData || portData.length === 0) {
            this.markPortAllocated(port, purpose, browserId);
            console.log(`🔌 Allocated port ${port} for ${purpose}`);
            return port;
          }
        }
      }
      
      // Fallback: simple allocation if ProcessCommand fails
      const availablePort = this.findAvailablePortFallback();
      this.markPortAllocated(availablePort, purpose, browserId);
      console.log(`🔌 Fallback allocation: port ${availablePort} for ${purpose}`);
      return availablePort;
      
    } catch (error) {
      console.log(`Port allocation error: ${error}`);
      
      // Emergency fallback
      const emergencyPort = this.devToolsPortRange.min + Math.floor(Math.random() * 78);
      this.markPortAllocated(emergencyPort, purpose, browserId);
      console.log(`🚨 Emergency allocation: port ${emergencyPort} for ${purpose}`);
      return emergencyPort;
    }
  }

  /**
   * Check if port is available
   */
  async isPortAvailable(port: number): Promise<boolean> {
    try {
      const ProcessCommandModule = await import('../../../commands/kernel/system/ProcessCommand');
      const ProcessCommand = ProcessCommandModule.default;
      
      const result = await ProcessCommand.execute({
        subcommand: 'ports',
        ports: [port]
      });
      
      if (result.success) {
        const portData = result.data.portProcesses[port];
        return !portData || portData.length === 0;
      }
      
      return false;
    } catch (error) {
      console.log(`Failed to check port ${port}: ${error}`);
      return false;
    }
  }

  /**
   * Free allocated port
   */
  freePort(port: number): void {
    if (this.allocatedPorts.has(port)) {
      const allocation = this.allocatedPorts.get(port)!;
      this.allocatedPorts.delete(port);
      console.log(`🔓 Freed port ${port} (was allocated for ${allocation.purpose})`);
    }
  }

  /**
   * Handle port conflicts intelligently
   */
  async handlePortConflicts(config: any): Promise<PortConflictResolution> {
    const result: PortConflictResolution = {
      killedPorts: [],
      freedPorts: [],
      errors: []
    };

    // Kill specific ports if requested
    if (config?.killPorts && Array.isArray(config.killPorts)) {
      for (const port of config.killPorts) {
        try {
          const killed = await this.killPortProcesses(port);
          if (killed) {
            result.killedPorts.push(port);
            this.freePort(port);
            result.freedPorts.push(port);
            console.log(`🔧 Killed processes on port ${port}`);
          }
        } catch (error) {
          const errorMsg = `Failed to kill port ${port}: ${error}`;
          result.errors.push(errorMsg);
          console.log(`❌ ${errorMsg}`);
        }
      }
    }

    return result;
  }

  /**
   * Kill processes on specific port
   */
  async killPortProcesses(port: number): Promise<boolean> {
    try {
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);
      
      // Use lsof to find and kill processes on port
      const { stdout } = await execAsync(`lsof -ti:${port}`);
      const pids = stdout.trim().split('\n').filter(pid => pid);
      
      if (pids.length > 0) {
        await execAsync(`kill -9 ${pids.join(' ')}`);
        console.log(`💀 Killed PIDs [${pids.join(', ')}] on port ${port}`);
        return true;
      }
      
      return false;
    } catch (error) {
      // Port might not have any processes, which is fine
      return false;
    }
  }

  /**
   * Get all allocated ports
   */
  getAllocatedPorts(): PortAllocation[] {
    return Array.from(this.allocatedPorts.values());
  }

  /**
   * Get port allocation info
   */
  getPortAllocation(port: number): PortAllocation | null {
    return this.allocatedPorts.get(port) || null;
  }

  /**
   * Clean up old allocations
   */
  cleanupOldAllocations(maxAgeMs: number = 24 * 60 * 60 * 1000): number {
    const now = Date.now();
    let cleaned = 0;

    for (const [port, allocation] of this.allocatedPorts.entries()) {
      if (now - allocation.allocated.getTime() > maxAgeMs) {
        this.allocatedPorts.delete(port);
        cleaned++;
      }
    }

    if (cleaned > 0) {
      console.log(`🧹 Cleaned up ${cleaned} old port allocations`);
    }

    return cleaned;
  }

  /**
   * Get port usage statistics
   */
  getPortStats() {
    const allocated = this.allocatedPorts.size;
    const available = this.devToolsPortRange.max - this.devToolsPortRange.min + 1 - allocated;
    
    return {
      allocated,
      available,
      totalRange: this.devToolsPortRange.max - this.devToolsPortRange.min + 1,
      utilizationPercent: (allocated / (this.devToolsPortRange.max - this.devToolsPortRange.min + 1)) * 100,
      allocatedPorts: Array.from(this.allocatedPorts.keys()).sort(),
      allocationsByPurpose: this.getAllocationsByPurpose()
    };
  }

  /**
   * Get candidate ports for allocation
   */
  private getCandidatePorts(): number[] {
    return Array.from(
      { length: this.devToolsPortRange.max - this.devToolsPortRange.min + 1 }, 
      (_, i) => this.devToolsPortRange.min + i
    );
  }

  /**
   * Mark port as allocated
   */
  private markPortAllocated(port: number, purpose: string, browserId?: string): void {
    this.allocatedPorts.set(port, {
      port,
      allocated: new Date(),
      purpose,
      browserId
    });
  }

  /**
   * Find available port using simple fallback method
   */
  private findAvailablePortFallback(): number {
    const candidatePorts = this.getCandidatePorts();
    
    for (const port of candidatePorts) {
      if (!this.allocatedPorts.has(port)) {
        return port;
      }
    }
    
    // If all ports in range are allocated, return random port (emergency)
    return this.devToolsPortRange.min + Math.floor(Math.random() * 78);
  }

  /**
   * Get allocations grouped by purpose
   */
  private getAllocationsByPurpose(): Record<string, number> {
    const byPurpose: Record<string, number> = {};
    
    for (const allocation of this.allocatedPorts.values()) {
      byPurpose[allocation.purpose] = (byPurpose[allocation.purpose] || 0) + 1;
    }
    
    return byPurpose;
  }

  /**
   * Reserve port range for specific purpose
   */
  reservePortRange(startPort: number, endPort: number, purpose: string): boolean {
    for (let port = startPort; port <= endPort; port++) {
      if (this.allocatedPorts.has(port)) {
        return false; // Range not available
      }
    }
    
    // Reserve the range
    for (let port = startPort; port <= endPort; port++) {
      this.markPortAllocated(port, purpose);
    }
    
    console.log(`🎯 Reserved port range ${startPort}-${endPort} for ${purpose}`);
    return true;
  }
}