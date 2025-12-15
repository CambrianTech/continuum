/**
 * Recipe Load Browser Command
 *
 * Routes to server for file system access
 */

import { RecipeLoadCommand } from '../shared/RecipeLoadCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { RecipeLoadParams, RecipeLoadResult } from '../shared/RecipeLoadTypes';

export class RecipeLoadBrowserCommand extends RecipeLoadCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('workspace/recipe/load', context, subpath, commander);
  }

  async execute(params: RecipeLoadParams): Promise<RecipeLoadResult> {
    // Browser cannot access file system - route to server
    const { Commands } = await import('@system/core/shared/Commands');
    return Commands.execute<RecipeLoadParams, RecipeLoadResult>('recipe/load', params);
  }
}
