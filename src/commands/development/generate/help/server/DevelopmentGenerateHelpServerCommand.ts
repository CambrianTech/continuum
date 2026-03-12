/**
 * Development Generate Help Command - Server Implementation
 *
 * Display comprehensive generator documentation. This is the primary
 * documentation entry point for AI agents learning to use the generator.
 * AIs call ./jtag development/generate/help to learn spec format, see examples,
 * and understand the workflow.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentGenerateHelpParams, DevelopmentGenerateHelpResult } from '../shared/DevelopmentGenerateHelpTypes';
import { createDevelopmentGenerateHelpResultFromParams } from '../shared/DevelopmentGenerateHelpTypes';
import { HelpFormatter } from '@generator/HelpFormatter';

export class DevelopmentGenerateHelpServerCommand extends CommandBase<DevelopmentGenerateHelpParams, DevelopmentGenerateHelpResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/generate/help', context, subpath, commander);
  }

  async execute(params: DevelopmentGenerateHelpParams): Promise<DevelopmentGenerateHelpResult> {
    const topic = params.topic || 'full';

    let content: string;
    if (topic === 'full') {
      content = HelpFormatter.fullHelp();
    } else {
      content = HelpFormatter.topicHelp(topic);
    }

    return createDevelopmentGenerateHelpResultFromParams(params, {
      success: true,
      content,
      topic
    });
  }
}
