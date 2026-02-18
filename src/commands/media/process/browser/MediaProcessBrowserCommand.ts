/**
 * Media Process Command - Browser Implementation
 *
 * Browser always delegates to server for ffmpeg execution.
 * Media processing cannot be done in browser context.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { MediaProcessParams, MediaProcessResult } from '../shared/MediaProcessTypes';

export class MediaProcessBrowserCommand extends CommandBase<MediaProcessParams, MediaProcessResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('media/process', context, subpath, commander);
  }

  /**
   * Browser always delegates to server - all media processing happens server-side
   */
  async execute(params: JTAGPayload): Promise<MediaProcessResult> {
    const mediaParams = params as MediaProcessParams;

    console.debug(`🎬 MEDIA BROWSER: Delegating to server for processing`);

    // Delegate to server for ffmpeg execution
    return await this.remoteExecute(mediaParams);
  }
}
