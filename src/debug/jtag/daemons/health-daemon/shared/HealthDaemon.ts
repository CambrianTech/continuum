/**
 * Health Daemon - System Health Management
 * 
 * Handles health checks, ping responses, and system monitoring across contexts.
 */

import { DaemonBase } from '@shared/DaemonBase';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '@shared/JTAGTypes';
import { createPayload } from '@shared/JTAGTypes';
import { getHighResolutionTime, getProcessInfo } from '@shared/CrossPlatformTypes';
import { JTAGRouter } from '@shared/JTAGRouter';
import { JTAG_ENDPOINTS } from '@shared/JTAGEndpoints';
import { createHealthPingResponse, createHealthErrorResponse, type HealthResponse } from '@shared/ResponseTypes';
import { type UUID } from '@shared/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '@shared/SystemScopes';

// Health-specific payload - system-level, no session required
export interface HealthPayload extends JTAGPayload {
  readonly type: 'ping' | 'status' | 'metrics';
  readonly timestamp: string;
  readonly data?: {
    latency?: number;
    uptime?: number;
    connectionCount?: number;
    [key: string]: any;
  };
}

export const createHealthPayload = (
  context: JTAGContext,
  data: Omit<Partial<HealthPayload>, 'context' | 'sessionId'>
): HealthPayload => createPayload(context, SYSTEM_SCOPES.SYSTEM, {
  type: data.type ?? 'ping',
  timestamp: data.timestamp ?? new Date().toISOString(),
  data: data.data ?? {},
  ...data
});

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
          return createHealthErrorResponse('Unknown health message type', healthPayload.context, healthPayload.sessionId);
      }
    } catch (error: any) {
      console.error(`❌ ${this.toString()}: Error processing health message:`, error.message);
      return createHealthErrorResponse(error.message, healthPayload.context, healthPayload.sessionId);
    }
  }

  /**
   * Handle ping requests
   */
  private async handlePing(payload: HealthPayload): Promise<HealthResponse> {
    const pongId = `pong_${Date.now()}`;
    const uptime = this.getUptime();
    
    return createHealthPingResponse(pongId, uptime, payload.context, undefined, payload.sessionId);
  }

  /**
   * Handle status requests
   */
  private async handleStatus(payload: HealthPayload): Promise<HealthResponse> {
    const statusId = `status_${Date.now()}`;
    const uptime = this.getUptime();
    const memory = this.getMemoryUsage();
    
    return createHealthPingResponse(statusId, uptime, payload.context, memory, payload.sessionId);
  }

  /**
   * Handle metrics requests
   */
  private async handleMetrics(payload: HealthPayload): Promise<HealthResponse> {
    const metricsId = `metrics_${Date.now()}`;
    const uptime = this.getUptime();
    const memory = this.getMemoryUsage();
    
    return createHealthPingResponse(metricsId, uptime, payload.context, memory, payload.sessionId);
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