/**
 * Gpu Stats Command - Server Implementation
 *
 * Routes to Rust GpuModule via continuum-core IPC:
 * - gpu/stats: Full GPU memory manager snapshot
 * - gpu/pressure: Quick pressure query (0.0-1.0)
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GpuStatsParams, GpuStatsResult } from '../shared/GpuStatsTypes';
import { createGpuStatsResultFromParams } from '../shared/GpuStatsTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class GpuStatsServerCommand extends CommandBase<GpuStatsParams, GpuStatsResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('gpu/stats', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(params: GpuStatsParams): Promise<GpuStatsResult> {
    await this.rustClient.connect();

    try {
      const stats = await this.rustClient.gpuStats();

      return createGpuStatsResultFromParams(params, {
        success: true,
        gpuName: stats.gpuName,
        totalVramMb: stats.totalVramMb,
        totalUsedMb: stats.totalUsedMb,
        pressure: stats.pressure,
        reserveMb: stats.reserveMb,
        rendering: stats.rendering,
        inference: stats.inference,
        tts: stats.tts,
      });
    } finally {
      this.rustClient.disconnect();
    }
  }
}
