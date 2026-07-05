/**
 * Media Prewarm Command - Browser Implementation
 *
 * Pre-warm vision description cache for image media. Fires VisionDescriptionService.describeBase64() so that by the time personas build RAG context, descriptions are cached. Called fire-and-forget by chat/send when images are attached.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MediaPrewarmParams, MediaPrewarmResult } from '../shared/MediaPrewarmTypes';

export class MediaPrewarmBrowserCommand extends CommandBase<MediaPrewarmParams, MediaPrewarmResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('media/prewarm', context, subpath, commander);
  }

  async execute(params: MediaPrewarmParams): Promise<MediaPrewarmResult> {
    console.log('🌐 BROWSER: Delegating Media Prewarm to server');
    return await this.remoteExecute(params);
  }
}
