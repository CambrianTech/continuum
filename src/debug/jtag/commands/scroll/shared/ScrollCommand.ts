/**
 * Scroll Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows established pattern exactly.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { type ScrollParams, type ScrollResult, createScrollParams } from './ScrollTypes';

export abstract class ScrollCommand extends CommandBase<ScrollParams, ScrollResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('scroll', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): ScrollParams {
    return createScrollParams(this.context, sessionId, {
      x: 0,
      y: 0,
      behavior: 'smooth'
    });
  }

  abstract execute(params: ScrollParams): Promise<ScrollResult>;
}