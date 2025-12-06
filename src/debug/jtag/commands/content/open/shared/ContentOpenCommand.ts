/**
 * Content Open Command - Shared Base Implementation
 *
 * Opens content and adds it to user's openItems array.
 * Emits content:opened event for widgets to respond to.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ContentOpenParams, ContentOpenResult } from './ContentOpenTypes';

export abstract class ContentOpenCommand extends CommandBase<ContentOpenParams, ContentOpenResult> {
  constructor(name: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(name, context, subpath, commander);
  }

  protected abstract executeContentCommand(params: ContentOpenParams): Promise<ContentOpenResult>;

  async execute(params: ContentOpenParams): Promise<ContentOpenResult> {
    return this.executeContentCommand(params);
  }
}
