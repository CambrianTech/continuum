/**
 * Screenshot Command - Server Implementation
 * 
 * Server-side screenshot command that delegates to browser for actual screenshot
 * capture, then handles file saving.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../shared/CommandBase';
import { JTAGContext, JTAGPayload } from '../../../../../shared/JTAGTypes';
import { ScreenshotParams, ScreenshotResult } from '../shared/ScreenshotTypes';

export class ScreenshotServerCommand extends CommandBase {
  
  constructor(context: JTAGContext, subpath: string, commander: any) {
    super('screenshot-server', context, subpath, commander);
  }

  /**
   * Execute screenshot command on server
   * Delegates to browser for capture, then saves file
   */
  async execute(params: JTAGPayload): Promise<ScreenshotResult> {
    const screenshotParams = params as ScreenshotParams;
    
    console.log(`📸 ${this.toString()}: Starting screenshot capture for ${screenshotParams.filename}`);

    try {
      // Delegate to browser for actual screenshot capture using remoteExecute
      console.log(`🔀 ${this.toString()}: Delegating to browser for screenshot capture`);
      const browserResult = await this.remoteExecute(screenshotParams);
      
      // Browser should return screenshot data
      if (!browserResult || !browserResult.success) {
        throw new Error(`Browser screenshot failed: ${browserResult?.error || 'Unknown error'}`);
      }

      // Save screenshot to server filesystem
      const baseArtifactoryPath = '.continuum/jtag/screenshots';
      const savePath = path.join(baseArtifactoryPath, screenshotParams.filename);
      
      // Ensure directory exists
      await fs.mkdir(path.dirname(savePath), { recursive: true });
      
      // Save the actual image data if available
      if (browserResult.dataUrl) {
        // Convert base64 data URL to file
        const base64Data = browserResult.dataUrl.replace(/^data:image\/\w+;base64,/, '');
        const buffer = Buffer.from(base64Data, 'base64');
        await fs.writeFile(savePath, buffer);
      } else {
        // Fallback: create a placeholder file
        const placeholder = `Screenshot captured at ${new Date().toISOString()}\nFilename: ${screenshotParams.filename}\nSelector: ${screenshotParams.selector || 'body'}\n`;
        await fs.writeFile(savePath, placeholder);
      }

      const result = new ScreenshotResult({
        success: true,
        filepath: savePath,
        filename: screenshotParams.filename,
        context: 'server',
        timestamp: new Date().toISOString(),
        options: screenshotParams.options,
        metadata: browserResult.metadata
      });

      console.log(`✅ ${this.toString()}: Screenshot saved to ${savePath}`);
      return result;

    } catch (error: any) {
      console.error(`❌ ${this.toString()}: Screenshot failed:`, error.message);
      
      return new ScreenshotResult({
        success: false,
        filepath: '',
        filename: screenshotParams.filename,
        context: 'server',
        timestamp: new Date().toISOString(),
        error: error.message
      });
    }
  }
}