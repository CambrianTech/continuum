/**
 * WaitForElement Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows screenshot/navigate/click/type examples exactly.
 */

import { CommandBase } from '../../../shared/CommandBase';
import type { ICommandDaemon } from '../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';
import { WaitForElementParams } from './WaitForElementTypes';
import type { WaitForElementResult } from './WaitForElementTypes';

export abstract class WaitForElementCommand extends CommandBase<WaitForElementParams, WaitForElementResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('wait-for-element', context, subpath, commander);
  }

  public override getDefaultParams(): WaitForElementParams {
    return new WaitForElementParams({
      selector: 'body',
      timeout: 30000,
      visible: true,
      interval: 100
    });
  }

  abstract execute(params: WaitForElementParams): Promise<WaitForElementResult>;
}