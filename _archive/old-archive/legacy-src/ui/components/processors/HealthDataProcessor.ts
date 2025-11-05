/**
 * Health Data Processor - Dynamic Module Example
 * =============================================
 * Example processor module that can be discovered and loaded dynamically
 */

import { DataSourceType } from '../../../types/shared/WidgetServerTypes';

export interface HealthData {
  readonly status: 'healthy' | 'warning' | 'error';
  readonly uptime: number;
  readonly memoryUsage: number;
  readonly cpuUsage: number;
  readonly timestamp: string;
}

export class HealthDataProcessor {
  /**
   * Process health data into standardized format
   */
  process(dataSource: DataSourceType, data: unknown): HealthData | null {
    if (dataSource !== 'health' || !this.isValidHealthData(data)) {
      return null;
    }

    return this.normalizeHealthData(data);
  }

  /**
   * Check if this processor supports the data source
   */
  supports(dataSource: DataSourceType): boolean {
    return dataSource === 'health';
  }

  /**
   * Validate incoming health data
   */
  private isValidHealthData(data: unknown): data is any {
    return data !== null && typeof data === 'object';
  }

  /**
   * Normalize health data to standard format
   */
  private normalizeHealthData(data: any): HealthData {
    const uptime = this.extractUptime(data);
    const memoryUsage = this.extractMemoryUsage(data);
    const cpuUsage = this.extractCpuUsage(data);

    return {
      status: this.calculateOverallStatus(uptime, memoryUsage, cpuUsage),
      uptime,
      memoryUsage,
      cpuUsage,
      timestamp: new Date().toISOString()
    };
  }

  private extractUptime(data: any): number {
    return data.uptime || data.uptimeSeconds || 0;
  }

  private extractMemoryUsage(data: any): number {
    return data.memory?.usage || data.memoryUsage || data.mem || 0;
  }

  private extractCpuUsage(data: any): number {
    return data.cpu?.usage || data.cpuUsage || data.cpu || 0;
  }

  private calculateOverallStatus(
    uptime: number, 
    memory: number, 
    cpu: number
  ): HealthData['status'] {
    if (memory > 90 || cpu > 90) return 'error';
    if (memory > 70 || cpu > 70 || uptime < 60) return 'warning';
    return 'healthy';
  }
}

// Export for dynamic loading
export default HealthDataProcessor;