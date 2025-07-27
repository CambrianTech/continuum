/**
 * Screenshot Command - Server Implementation (Simplified)
 * 
 * MINIMAL WORK PER COMMAND: Just implements what server does
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import { JTAGMessageFactory } from '@shared/JTAGTypes';
import { type ScreenshotParams, type ScreenshotResult, createScreenshotResult } from '@screenshotShared/ScreenshotTypes';
import { PersistenceError } from '@shared/ErrorTypes';

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
    
    console.log(`📸 SERVER: Starting screenshot`);

    try {
      // Simple check: do we need browser capture?
      if (!screenshotParams.dataUrl) {
        console.log(`🔀 SERVER: Need image capture → delegating to browser`);
        return await this.remoteExecute(screenshotParams);
      }
      
      // We have image data → delegate to file save command
      console.log(`💾 SERVER: Saving image data (${screenshotParams.dataUrl.length} bytes)`);
      
      const filename = screenshotParams.filename || 'screenshot.png';
      const filepath = `screenshots/${filename}`;
      
      // Convert image data to buffer for file save command
      let content: Buffer | string;
      if (screenshotParams.dataUrl) {
        const base64Data = screenshotParams.dataUrl.replace(/^data:image\/\w+;base64,/, '');
        content = Buffer.from(base64Data, 'base64');
        console.log(`📁 SERVER: Delegating binary image save to file command`);
      } else {
        content = `Screenshot captured at ${new Date().toISOString()}\nFilename: ${filename}\n`;
        console.log(`📝 SERVER: Delegating placeholder text save to file command`);
      }
      
      // Delegate to file save command via router
      const saveMessage = JTAGMessageFactory.createRequest(
        screenshotParams.context,
        `server/${this.commander.subpath}/screenshot`,
        `server/${this.commander.subpath}/file/save`,
        {
          ...screenshotParams,
          filepath: filepath,
          content: content
        },
        JTAGMessageFactory.generateCorrelationId()
      );
      
      const saveResult = await this.commander.router.postMessage(saveMessage);
      
      console.log(`🔍 SERVER: File save router result:`, JSON.stringify(saveResult, null, 2));
      
      if (!saveResult.success) {
        throw new PersistenceError(filename, 'write', `File save command failed: ${JSON.stringify(saveResult)}`);
      }
      
      return createScreenshotResult(screenshotParams.context, screenshotParams.sessionId, {
        success: true,
        filepath: filepath,
        filename: filename,
        options: screenshotParams.options
      });

    } catch (error: any) {
      console.error(`❌ SERVER: Failed:`, error.message);
      const screenshotError = error instanceof Error ? new PersistenceError(screenshotParams.filename || 'screenshot', 'write', error.message, { cause: error }) : new PersistenceError(screenshotParams.filename || 'screenshot', 'write', String(error));
      return createScreenshotResult(screenshotParams.context, screenshotParams.sessionId, {
        success: false,
        filepath: '',
        filename: screenshotParams.filename,
        error: screenshotError
      });
    }
  }
}