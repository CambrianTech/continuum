/**
 * Skill List Command - Browser Implementation
 *
 * List skills with optional filters by status, scope, and creator. Returns SkillEntity records from the database.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SkillListParams, SkillListResult } from '../shared/SkillListTypes';

export class SkillListBrowserCommand extends CommandBase<SkillListParams, SkillListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/list', context, subpath, commander);
  }

  async execute(params: SkillListParams): Promise<SkillListResult> {
    console.log('🌐 BROWSER: Delegating Skill List to server');
    return await this.remoteExecute(params);
  }
}
