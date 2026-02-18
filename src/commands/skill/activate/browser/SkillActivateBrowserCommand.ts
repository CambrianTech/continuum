/**
 * Skill Activate Command - Browser Implementation
 *
 * Activate a validated skill by registering it as a live command. The skill becomes available for use by the creator (personal) or all personas (team).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SkillActivateParams, SkillActivateResult } from '../shared/SkillActivateTypes';

export class SkillActivateBrowserCommand extends CommandBase<SkillActivateParams, SkillActivateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/activate', context, subpath, commander);
  }

  async execute(params: SkillActivateParams): Promise<SkillActivateResult> {
    console.log('🌐 BROWSER: Delegating Skill Activate to server');
    return await this.remoteExecute(params);
  }
}
