/**
 * Scroll Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows established pattern exactly.
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from '@shared/CrossPlatformUUID';
import { type ScrollParams, type ScrollResult, createScrollParams } from '@commandsScroll/shared/ScrollTypes';

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