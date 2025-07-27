/**
 * GetText Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows established pattern exactly.
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';
import { GetTextParams, type GetTextResult } from './GetTextTypes';

export abstract class GetTextCommand extends CommandBase<GetTextParams, GetTextResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('get-text', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): GetTextParams {
    return new GetTextParams({
      selector: 'body',
      trim: true,
      innerText: true
    }, this.context, sessionId);
  }

  abstract execute(params: GetTextParams): Promise<GetTextResult>;
}