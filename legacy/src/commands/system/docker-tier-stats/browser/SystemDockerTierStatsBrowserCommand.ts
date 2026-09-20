/**
 * System Docker Tier Stats Command - Browser Implementation
 *
 * Snapshot of the Docker storage tier (capacity, used bytes, pressure ratio, detection state). Phase 1 of #1239 — exposes the data the existing `DockerTierPool` (`modules/docker_tier_pool.rs`) already computes, without depending on the not-yet-instantiated `PressureBroker` singleton. Wired so `bin/continuum status` can surface a `Docker disk: ...` row + warn at >90%, and so future scheduler hot paths can refuse before ENOSPC. Returns `detected: false` + zeros on hosts where Docker isn't installed.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SystemDockerTierStatsParams, SystemDockerTierStatsResult } from '../shared/SystemDockerTierStatsTypes';

export class SystemDockerTierStatsBrowserCommand extends CommandBase<SystemDockerTierStatsParams, SystemDockerTierStatsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('system/docker-tier-stats', context, subpath, commander);
  }

  async execute(params: SystemDockerTierStatsParams): Promise<SystemDockerTierStatsResult> {
    console.log('🌐 BROWSER: Delegating System Docker Tier Stats to server');
    return await this.remoteExecute(params);
  }
}
