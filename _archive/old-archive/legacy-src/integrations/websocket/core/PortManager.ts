/**
 * Port Manager - Handles port allocation and conflict resolution
 * Single responsibility: Smart port management for WebSocket daemon
 */

import { CONTINUUM_PORTS } from '../../../system/configuration/PortConfiguration';

export class PortManager {
  private port: number;
  private host: string;

  constructor(port: number = CONTINUUM_PORTS.main.http, host: string = 'localhost') {
    this.port = port;
    this.host = host;
  }

  /**
   * Check if port is available and healthy
   */
  async isPortHealthy(): Promise<boolean> {
    try {
      const response = await fetch(`http://${this.host}:${this.port}/`);
      return response.status === 200 || response.status === 404;
    } catch {
      return false;
    }
  }

  /**
   * Attempt to resolve port conflict
   */
  async resolvePortConflict(): Promise<boolean> {
    try {
      // Check if existing service is healthy first
      if (await this.isPortHealthy()) {
        console.log(`✅ Healthy service already running on port ${this.port}`);
        return false; // Don't start another server
      }

      // Try to kill unhealthy processes
      const { spawn } = await import('child_process');
      return new Promise((resolve) => {
        const lsof = spawn('lsof', ['-ti', `:${this.port}`]);
        let pids = '';
        
        lsof.stdout.on('data', (data) => {
          pids += data.toString();
        });
        
        lsof.on('close', () => {
          if (pids.trim()) {
            const pidList = pids.trim().split('\n');
            for (const pid of pidList) {
              try {
                process.kill(parseInt(pid), 'SIGTERM');
                console.log(`🔄 Killed unhealthy process ${pid} on port ${this.port}`);
              } catch {
                // Process might already be dead
              }
            }
            // Give processes time to die
            setTimeout(() => resolve(true), 2000);
          } else {
            resolve(false);
          }
        });
      });
    } catch {
      return false;
    }
  }

  getPort(): number {
    return this.port;
  }

  getHost(): string {
    return this.host;
  }
}