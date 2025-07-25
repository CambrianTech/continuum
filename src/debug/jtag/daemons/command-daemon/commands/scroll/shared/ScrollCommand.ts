/**
 * Scroll Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows established pattern exactly.
 */

import { CommandBase } from '../../../shared/CommandBase';
import type { ICommandDaemon } from '../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';
import { ScrollParams } from './ScrollTypes';
import type { ScrollResult } from './ScrollTypes';

export abstract class ScrollCommand extends CommandBase<ScrollParams, ScrollResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('scroll', context, subpath, commander);
  }

  public override getDefaultParams(): ScrollParams {
    return new ScrollParams({
      x: 0,
      y: 0,
      behavior: 'smooth'
    });
  }

  abstract execute(params: ScrollParams): Promise<ScrollResult>;
}