/**
 * Media Resize Command - Browser Implementation
 *
 * Browser stub that delegates to server for actual image resizing.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { MediaResizeParams, MediaResizeResult } from '../shared/MediaResizeTypes';

export class MediaResizeBrowserCommand extends CommandBase<MediaResizeParams, MediaResizeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('media/resize', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<MediaResizeResult> {
    // Browser cannot resize images - delegate to server
    const resizeParams = params as MediaResizeParams;
    return (await this.remoteExecute(resizeParams)) as unknown as MediaResizeResult;
  }
}
