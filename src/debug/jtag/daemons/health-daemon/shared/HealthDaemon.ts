/**
 * Health Daemon - System Health Management
 * 
 * Handles health checks, ping responses, and system monitoring across contexts.
 */

import { DaemonBase } from '../../../shared/DaemonBase';
import type { JTAGContext, JTAGMessage } from '../../../shared/JTAGTypes';
import { JTAGPayload } from '../../../shared/JTAGTypes';
import { getHighResolutionTime, getProcessInfo } from '../../../shared/CrossPlatformTypes';
import { JTAGRouter } from '../../../shared/JTAGRouter';
import { JTAG_ENDPOINTS } from '../../../shared/JTAGEndpoints';
import { createHealthPingResponse, createHealthErrorResponse, type HealthResponse } from '../../../shared/ResponseTypes';

// Health-specific payload - system-level, no session required
export class HealthPayload extends JTAGPayload {
  type: 'ping' | 'status' | 'metrics';
  timestamp: string;
  data?: {
    latency?: number;
    uptime?: number;
    connectionCount?: number;
    [key: string]: any;
  };

  constructor(data: Partial<HealthPayload>, context: JTAGContext) {
    super(context, 'system'); // Health daemon uses system-level session
    this.type = data.type || 'ping';
    this.timestamp = data.timestamp || new Date().toISOString();
    this.data = data.data || {};
  }
}

/**
 * Universal Health Handler - Symmetric daemon following router pattern
 */
export abstract class HealthDaemon extends DaemonBase {
  public readonly subpath: string = 'health';
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super('health-daemon', context, router);
  }

  /**
   * Initialize health daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`💓 ${this.toString()}: Health daemon initialized`);
  }

  /**
   * Handle incoming health messages
   */
  async handleMessage(message: JTAGMessage): Promise<HealthResponse> {
    const healthPayload = message.payload as HealthPayload;
    
    try {
      switch (healthPayload.type) {
        case 'ping':
          return await this.handlePing(healthPayload);
        case 'status':
          return await this.handleStatus(healthPayload);
        case 'metrics':
          return await this.handleMetrics(healthPayload);
        default:
          console.warn(`⚠️ ${this.toString()}: Unknown health message type: ${healthPayload.type}`);
          return new HealthErrorResponse('Unknown health message type', healthPayload.context);
      }
    } catch (error: any) {
      console.error(`❌ ${this.toString()}: Error processing health message:`, error.message);
      return new HealthErrorResponse(error.message, healthPayload.context);
    }
  }

  /**
   * Handle ping requests
   */
  private async handlePing(payload: HealthPayload): Promise<HealthPingResponse> {
    const pongId = `pong_${Date.now()}`;
    const uptime = this.getUptime();
    
    return new HealthPingResponse(pongId, uptime, healthPayload.context);
  }

  /**
   * Handle status requests
   */
  private async handleStatus(payload: HealthPayload): Promise<HealthPingResponse> {
    const statusId = `status_${Date.now()}`;
    const uptime = this.getUptime();
    const memory = this.getMemoryUsage();
    
    return new HealthPingResponse(statusId, uptime, healthPayload.context, memory);
  }

  /**
   * Handle metrics requests
   */
  private async handleMetrics(payload: HealthPayload): Promise<HealthPingResponse> {
    const metricsId = `metrics_${Date.now()}`;
    const uptime = this.getUptime();
    const memory = this.getMemoryUsage();
    
    return new HealthPingResponse(metricsId, uptime, healthPayload.context, memory);
  }

  /**
   * Get uptime - cross-platform compatible
   */
  /**
   * Get system uptime using cross-platform detection
   */
  private getUptime(): number {
    const processInfo = getProcessInfo();
    if (processInfo.uptime) {
      return processInfo.uptime();
    }
    // Browser fallback - return time since page load in seconds
    return getHighResolutionTime() / 1000;
  }

  /**
   * Get memory usage - cross-platform compatible  
   */
  private getMemoryUsage(): any {
    if (typeof process !== 'undefined' && process.memoryUsage) {
      return process.memoryUsage();
    }
    // Browser fallback - return performance memory if available
    if (typeof performance !== 'undefined' && (performance as any).memory) {
      return (performance as any).memory;
    }
    return {};
  }
}