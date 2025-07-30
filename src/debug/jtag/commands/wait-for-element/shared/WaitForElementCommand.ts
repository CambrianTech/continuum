/**
 * WaitForElement Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows screenshot/navigate/click/type examples exactly.
 */

import { CommandBase } from '@commandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { UUID } from '@shared/CrossPlatformUUID';
import { type WaitForElementParams, type WaitForElementResult, createWaitForElementParams } from '@commandsWaitForElement/shared/WaitForElementTypes';

export abstract class WaitForElementCommand extends CommandBase<WaitForElementParams, WaitForElementResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('wait-for-element', context, subpath, commander);
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