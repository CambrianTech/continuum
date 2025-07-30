// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Navigate Command - Abstract Base Class
 * 
 * Clean abstraction following CommandBase pattern. Minimal interface with
 * proper generics usage - extends CommandBase<NavigateParams, NavigateResult>
 * for type safety without complexity.
 * 
 * DESIGN ANALYSIS:
 * ✅ Proper inheritance from CommandBase
 * ✅ Clean constructor delegation 
 * ✅ Sensible default parameters
 * ✅ Single abstract method to implement
 * ✅ No unnecessary complexity or interfaces
 */

import { CommandBase } from '@commandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from '@shared/CrossPlatformUUID';
import { type NavigateParams, type NavigateResult, createNavigateParams } from '@commandsNavigate/shared/NavigateTypes';

export abstract class NavigateCommand extends CommandBase<NavigateParams, NavigateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('navigate', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): NavigateParams {
    return createNavigateParams(this.context, sessionId, {
      url: 'about:blank'
    });
  }

  abstract execute(params: NavigateParams): Promise<NavigateResult>;
}