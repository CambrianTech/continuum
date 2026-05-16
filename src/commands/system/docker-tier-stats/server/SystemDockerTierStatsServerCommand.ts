/**
 * System Docker Tier Stats Command — Server Implementation
 *
 * Phase 1 of #1239 — pass-through to the Rust `system/docker-tier-stats`
 * IPC handler. The Rust side calls `DockerTierPool::snapshot_stats()` to
 * probe Docker.raw + return capacity / used / pressure / detected.
 *
 * Pattern matches `SystemResourcesServerCommand` (also routes to
 * `SystemResourceModule` via the same RustCoreIPC client).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type {
  SystemDockerTierStatsParams,
  SystemDockerTierStatsResult,
} from '../shared/SystemDockerTierStatsTypes';
import { createSystemDockerTierStatsResultFromParams } from '../shared/SystemDockerTierStatsTypes';
import {
  RustCoreIPCClient,
  getContinuumCoreSocketPath,
} from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SystemDockerTierStatsServerCommand extends CommandBase<
  SystemDockerTierStatsParams,
  SystemDockerTierStatsResult
> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('system/docker-tier-stats', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(params: SystemDockerTierStatsParams): Promise<SystemDockerTierStatsResult> {
    await this.rustClient.connect();
    try {
      const stats = await this.rustClient.dockerTierStats();
      return createSystemDockerTierStatsResultFromParams(params, {
        success: true,
        stats,
      });
    } finally {
      this.rustClient.disconnect();
    }
  }
}
