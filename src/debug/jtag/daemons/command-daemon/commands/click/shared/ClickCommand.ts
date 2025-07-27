// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Click Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows screenshot/navigate examples exactly.
 * 
 * DESIGN ANALYSIS:
 * ✅ Clean CommandBase inheritance with proper generics
 * ✅ Sensible default parameters (body element, left click)
 * ✅ Single responsibility - just click abstraction
 * ✅ No unnecessary interfaces or complexity
 * ✅ Proper constructor delegation pattern
 */

import { CommandBase } from '@commandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';
import { type ClickParams, createClickParams } from './ClickTypes';
import type { ClickResult } from './ClickTypes';

export abstract class ClickCommand extends CommandBase<ClickParams, ClickResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('click', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): ClickParams {
    return createClickParams(this.context, sessionId, {
      selector: 'body',
      button: 'left'
    });
  }

  abstract execute(params: ClickParams): Promise<ClickResult>;
}