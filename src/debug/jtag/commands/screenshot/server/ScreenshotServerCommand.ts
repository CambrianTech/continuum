/**
 * Screenshot Command - Server Implementation (Simplified)
 * 
 * MINIMAL WORK PER COMMAND: Just implements what server does
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { type ScreenshotParams, type ScreenshotResult, createScreenshotResult } from '../shared/ScreenshotTypes';
import { PersistenceError } from '../../../system/core/types/ErrorTypes';
import type { FileSaveParams, FileSaveResult } from '../../file/save/shared/FileSaveTypes';
import type { MediaItem } from '../../../system/data/entities/ChatMessageEntity';

export class ScreenshotServerCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('screenshot', context, subpath, commander);
  }

  /**
   * Server does TWO things:
   * 1. If no image data → delegate to browser for capture
   * 2. If has image data → save to filesystem
   */
  async execute(params: JTAGPayload): Promise<ScreenshotResult> {
    const screenshotParams = params as ScreenshotParams;
    
    // console.debug(`📸 SERVER: Starting screenshot`);
    // console.debug(`🔍 DEBUG: ScreenshotServerCommand received sessionId: ${screenshotParams.sessionId}`);

    // Simple check: do we need browser capture?
    if (!screenshotParams.dataUrl) {
      // console.debug(`🔀 SERVER: Need image capture → delegating to browser`);
      return await this.remoteExecute(screenshotParams);
    }
    
    // We have image data  → delegate to file save command
    // we may have also called remoteExecute (above) to delegate to the browser and it called back to US via remoteExecute()
    // console.debug(`💾 SERVER: Saving image data (${screenshotParams.dataUrl.length} bytes)`);
    
    const filename = screenshotParams.filename ?? 'screenshot.png';
    const filepath = `screenshots/${filename}`;
    
    // Convert image data to buffer for file save command
    let content: Buffer | string;
    if (screenshotParams.dataUrl) {
      const base64Data = screenshotParams.dataUrl.replace(/^data:image\/\w+;base64,/, '');
      content = Buffer.from(base64Data, 'base64');
      // console.debug(`📁 SERVER: Delegating binary image save to file command`);
    } else {
      content = `Screenshot captured at ${new Date().toISOString()}\nFilename: ${filename}\n`;
      // console.debug(`📝 SERVER: Delegating placeholder text save to file command`);
    }

    const saveParams: FileSaveParams = {
      ...screenshotParams,
      filepath: filepath,
      content: content
    };

    const saveResult: FileSaveResult = await this.remoteExecute(saveParams, 'file/save', this.context.environment);

    // console.debug(`🔍 SERVER: File save result:`, JSON.stringify(saveResult, null, 2));

    if (!saveResult.success) {
      throw new PersistenceError(filename, 'write', `File save command failed: ${JSON.stringify(saveResult)}`);
    }

    // Create MediaItem for AI cognition
    const base64Data = screenshotParams.dataUrl ? screenshotParams.dataUrl.replace(/^data:image\/\w+;base64,/, '') : undefined;
    const mimeType = screenshotParams.dataUrl ? screenshotParams.dataUrl.match(/^data:(image\/\w+);base64,/)?.[1] : undefined;

    const media: MediaItem | undefined = base64Data ? {
      type: 'image',
      base64: base64Data,
      mimeType: mimeType ?? 'image/png',
      filename: filename,
      url: `file://${saveResult.filepath}`,
      width: screenshotParams.metadata?.width,
      height: screenshotParams.metadata?.height,
      size: content instanceof Buffer ? content.length : undefined,
      alt: `Screenshot captured at ${new Date().toISOString()}`,
      description: screenshotParams.querySelector ? `Screenshot of ${screenshotParams.querySelector}` : 'Screenshot',
      uploadedAt: Date.now(),
      uploadedBy: screenshotParams.sessionId
    } : undefined;

    return createScreenshotResult(screenshotParams.context, screenshotParams.sessionId, {
      success: true,
      filepath: saveResult.filepath,
      filename: filename,
      options: screenshotParams.options,
      media
    });
  }
}