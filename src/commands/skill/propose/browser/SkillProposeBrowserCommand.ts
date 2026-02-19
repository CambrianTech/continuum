/**
 * Skill Propose Command - Browser Implementation
 *
 * Propose a new skill (command) specification. Creates a SkillEntity with status 'proposed'. For team-scoped skills, creates a DecisionProposal for governance approval.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SkillProposeParams, SkillProposeResult } from '../shared/SkillProposeTypes';

export class SkillProposeBrowserCommand extends CommandBase<SkillProposeParams, SkillProposeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/propose', context, subpath, commander);
  }

  async execute(params: SkillProposeParams): Promise<SkillProposeResult> {
    console.log('🌐 BROWSER: Delegating Skill Propose to server');
    return await this.remoteExecute(params);
  }
}
