/**
 * Skill Generate Command - Browser Implementation
 *
 * Generate code files for a proposed skill using the CommandGenerator. Retrieves the SkillEntity and produces source files.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SkillGenerateParams, SkillGenerateResult } from '../shared/SkillGenerateTypes';

export class SkillGenerateBrowserCommand extends CommandBase<SkillGenerateParams, SkillGenerateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/generate', context, subpath, commander);
  }

  async execute(params: SkillGenerateParams): Promise<SkillGenerateResult> {
    console.log('🌐 BROWSER: Delegating Skill Generate to server');
    return await this.remoteExecute(params);
  }
}
