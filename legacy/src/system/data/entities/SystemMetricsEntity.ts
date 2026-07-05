/**
 * SystemMetricsEntity - Time-series snapshots of system health
 *
 * Stores periodic GPU, CPU, and memory samples for trend graphs.
 * Written fire-and-forget by MetricsCollector into a dedicated metrics database.
 * Queried by ContinuumMetricsWidget for sparkline/graph rendering.
 */

import { BaseEntity } from './BaseEntity';
import { NumberField, TextField } from '../decorators/FieldDecorators';

export class SystemMetricsEntity extends BaseEntity {
  static readonly collection = 'system_metrics';

  /** Unix timestamp in ms */
  @NumberField()
  timestamp!: number;

  // ── CPU ──────────────────────────────────────────────────────────
  /** CPU usage 0.0-1.0 (global across all cores) */
  @NumberField()
  cpuUsage!: number;

  /** Number of physical cores */
  @NumberField()
  cpuCores!: number;

  // ── Memory ──────────────────────────────────────────────────────
  /** System memory pressure 0.0-1.0 */
  @NumberField()
  memoryPressure!: number;

  /** Total system memory in MB */
  @NumberField()
  memoryTotalMb!: number;

  /** Used system memory in MB */
  @NumberField()
  memoryUsedMb!: number;

  // ── GPU ──────────────────────────────────────────────────────────
  /** GPU memory pressure 0.0-1.0 */
  @NumberField()
  gpuPressure!: number;

  /** Total GPU VRAM in MB */
  @NumberField()
  gpuTotalMb!: number;

  /** Used GPU VRAM in MB */
  @NumberField()
  gpuUsedMb!: number;

  /** GPU hardware name */
  @TextField({ nullable: true })
  gpuName?: string;

  get collection(): string {
    return SystemMetricsEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (this.cpuUsage < 0 || this.cpuUsage > 1) {
      return { success: false, error: 'cpuUsage must be 0.0-1.0' };
    }
    if (this.memoryPressure < 0 || this.memoryPressure > 1) {
      return { success: false, error: 'memoryPressure must be 0.0-1.0' };
    }
    return { success: true };
  }
}
