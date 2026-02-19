/**
 * Recipe Load Command (Shared)
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { RecipeLoadParams, RecipeLoadResult } from './RecipeLoadTypes';

export abstract class RecipeLoadCommand extends CommandBase<RecipeLoadParams, RecipeLoadResult> {
  constructor(name: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(name, context, subpath, commander);
  }

  abstract execute(params: RecipeLoadParams): Promise<RecipeLoadResult>;
}
