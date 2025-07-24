/**
 * Screenshot Command - Server Implementation (Simplified)
 * 
 * MINIMAL WORK PER COMMAND: Just implements what server does
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../shared/JTAGTypes';
import type { ScreenshotParams } from '../shared/ScreenshotTypes';
import { ScreenshotResult } from '../shared/ScreenshotTypes';
import type { ICommandDaemon } from '../../../shared/CommandBase';

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
      
      // We have image data → save it
      console.log(`💾 SERVER: Saving image data (${screenshotParams.dataUrl.length} bytes)`);
      
      const globalPath = path.resolve(process.cwd(), '.continuum/jtag/screenshots', screenshotParams.filename || 'screenshot.png');
      
      // Ensure directory exists
      await fs.mkdir(path.dirname(globalPath), { recursive: true });
      
      if (screenshotParams.dataUrl) {
        // Convert and save actual image
        const base64Data = screenshotParams.dataUrl.replace(/^data:image\/\w+;base64,/, '');
        const buffer = Buffer.from(base64Data, 'base64');
        await fs.writeFile(globalPath, buffer);
        console.log(`📁 SERVER: Saved to: ${globalPath}`);
      } else {
        // Fallback placeholder
        const placeholder = `Screenshot captured at ${new Date().toISOString()}\nFilename: ${screenshotParams.filename}\n`;
        await fs.writeFile(globalPath, placeholder);
        console.log(`📝 SERVER: Created placeholder file`);
      }
      
      return new ScreenshotResult({
        success: true,
        filepath: globalPath,
        filename: screenshotParams.filename,
        environment: this.context.environment,
        timestamp: new Date().toISOString(),
        options: screenshotParams.options,
        metadata: {
          ...screenshotParams.metadata,
          globalPath: globalPath
        }
      });

    } catch (error: any) {
      console.error(`❌ SERVER: Failed:`, error.message);
      return new ScreenshotResult({
        success: false,
        filepath: '',
        filename: screenshotParams.filename,
        environment: this.context.environment,
        timestamp: new Date().toISOString(),
        error: error.message
      });
    }
  }
}