/**
 * WaitForElement Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows screenshot/navigate/click/type examples exactly.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { type WaitForElementParams, type WaitForElementResult, createWaitForElementParams } from './WaitForElementTypes';

export abstract class WaitForElementCommand extends CommandBase<WaitForElementParams, WaitForElementResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/wait-for-element', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): WaitForElementParams {
    return createWaitForElementParams(this.context, sessionId, {
      selector: 'body',
      timeout: 30000,
      visible: true,
      interval: 100
    });
  }

  abstract execute(params: WaitForElementParams): Promise<WaitForElementResult>;
}