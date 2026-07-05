/**
 * Skill Validate Command - Browser Implementation
 *
 * Validate a generated skill by running TypeScript compilation and tests in an ExecutionSandbox. Updates SkillEntity with validation results.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SkillValidateParams, SkillValidateResult } from '../shared/SkillValidateTypes';

export class SkillValidateBrowserCommand extends CommandBase<SkillValidateParams, SkillValidateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/validate', context, subpath, commander);
  }

  async execute(params: SkillValidateParams): Promise<SkillValidateResult> {
    console.log('🌐 BROWSER: Delegating Skill Validate to server');
    return await this.remoteExecute(params);
  }
}
