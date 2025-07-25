/**
 * GetText Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows established pattern exactly.
 */

import { CommandBase } from '../../../shared/CommandBase';
import type { ICommandDaemon } from '../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';
import { GetTextParams } from './GetTextTypes';
import type { GetTextResult } from './GetTextTypes';

export abstract class GetTextCommand extends CommandBase<GetTextParams, GetTextResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('get-text', context, subpath, commander);
  }

  public override getDefaultParams(): GetTextParams {
    return new GetTextParams({
      selector: 'body',
      trim: true,
      innerText: true
    });
  }

  abstract execute(params: GetTextParams): Promise<GetTextResult>;
}