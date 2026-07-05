/**
 * Lease Request Command - Shared Base Implementation
 *
 * Requests a file lease for editing. Handles:
 * - Immediate grant (UNRESTRICTED files)
 * - Approval requests (PEER_REVIEW, SENIOR_REVIEW, HUMAN_REVIEW)
 * - Queue management (file already leased)
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { LeaseRequestParams, LeaseRequestResult } from './LeaseRequestTypes';

/**
 * Abstract base for LeaseRequest commands
 */
export abstract class LeaseRequestCommand extends CommandBase<LeaseRequestParams, LeaseRequestResult> {
  static readonly commandName = 'lease/request';

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('utilities/lease/request', context, subpath, commander);
  }

  /**
   * Natural environment: server (needs file system and LeaseDaemon)
   */
  protected static get naturalEnvironment(): 'browser' | 'server' | 'auto' {
    return 'server';
  }

  /**
   * Entry point - subclasses implement environment-specific logic
   */
  async execute(params: LeaseRequestParams): Promise<LeaseRequestResult> {
    return this.executeLeaseRequest(params);
  }

  /**
   * Abstract method for lease request operation
   * Server implementation handles the lease logic
   */
  protected abstract executeLeaseRequest(params: LeaseRequestParams): Promise<LeaseRequestResult>;
}
