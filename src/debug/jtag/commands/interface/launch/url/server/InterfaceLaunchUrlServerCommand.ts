/**
 * Interface Launch Url Command - Server Implementation
 *
 * Opens a URL in the default browser. Enables personas to view what they build.
 * Uses `open` on macOS, `xdg-open` on Linux, `start` on Windows.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { InterfaceLaunchUrlParams, InterfaceLaunchUrlResult } from '../shared/InterfaceLaunchUrlTypes';
import { createInterfaceLaunchUrlResultFromParams } from '../shared/InterfaceLaunchUrlTypes';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export class InterfaceLaunchUrlServerCommand extends CommandBase<InterfaceLaunchUrlParams, InterfaceLaunchUrlResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/launch/url', context, subpath, commander);
  }

  async execute(params: InterfaceLaunchUrlParams): Promise<InterfaceLaunchUrlResult> {
    console.log('🚀 SERVER: Opening URL in browser:', params.url);

    // Validate required URL parameter
    if (!params.url || params.url.trim() === '') {
      throw new ValidationError(
        'url',
        `Missing required parameter 'url'. ` +
        `Use the help tool with 'Interface Launch Url' or see the interface/launch/url README for usage.`
      );
    }

    try {
      // Validate URL format
      const urlObj = new URL(params.url);
      const validatedUrl = urlObj.href;

      // Get platform-specific open command
      const openCommand = this.getOpenCommand(validatedUrl);

      // Execute open command
      await execAsync(openCommand);

      console.log(`✅ SERVER: Launched browser for ${validatedUrl}`);

      // Optional: take screenshot after delay (if screenshot command is available)
      let screenshotPath: string | undefined;
      if (params.screenshot) {
        // Wait for page to load
        await new Promise(resolve => setTimeout(resolve, 2000));
        console.log(`📸 SERVER: Screenshot requested for external URL (requires separate screenshot tool)`);
        // Note: For now, screenshots of external URLs need to use a headless browser
        // This is noted as a future enhancement
      }

      return createInterfaceLaunchUrlResultFromParams(params, {
        success: true,
        url: validatedUrl,
        launched: true,
        screenshotPath: screenshotPath ?? '',
      });

    } catch (error) {
      console.error(`❌ SERVER: Launch failed:`, error);

      // Re-throw validation errors
      if (error instanceof ValidationError) {
        throw error;
      }

      // For URL parsing errors
      if (error instanceof TypeError && error.message.includes('Invalid URL')) {
        throw new ValidationError('url', `Invalid URL: ${params.url}. Please provide a valid URL like "http://localhost:3000".`);
      }

      // For other errors, return failure result
      return createInterfaceLaunchUrlResultFromParams(params, {
        success: false,
        url: params.url,
        launched: false,
        screenshotPath: '',
      });
    }
  }

  /**
   * Get platform-specific command to open a URL in the default browser
   */
  private getOpenCommand(url: string): string {
    // Escape URL for shell (handle special characters)
    const escapedUrl = url.replace(/"/g, '\\"');

    switch (process.platform) {
      case 'darwin':
        return `open "${escapedUrl}"`;
      case 'win32':
        return `start "" "${escapedUrl}"`;
      case 'linux':
      default:
        return `xdg-open "${escapedUrl}"`;
    }
  }
}
