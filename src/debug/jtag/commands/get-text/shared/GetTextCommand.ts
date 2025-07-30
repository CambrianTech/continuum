/**
 * GetText Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows established pattern exactly.
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from '@shared/CrossPlatformUUID';
import { type GetTextParams, createGetTextParams, type GetTextResult } from '@commandsGetText/shared/GetTextTypes';

export abstract class GetTextCommand extends CommandBase<GetTextParams, GetTextResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('get-text', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): GetTextParams {
    return createGetTextParams(this.context, sessionId, {
      selector: 'body',
      trim: true,
      innerText: true
    });
  }

  abstract execute(params: GetTextParams): Promise<GetTextResult>;
}