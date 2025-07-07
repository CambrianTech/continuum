/**
 * macOS Browser Adapter - Handles AppleScript-based browser automation
 * 
 * This abstract class provides AppleScript execution capabilities
 * and script loading for macOS browser adapters.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { readFile } from 'fs/promises';
import path from 'path';
import { BaseBrowserAdapter } from '../base/BaseBrowserAdapter.js';

const execAsync = promisify(exec);

export abstract class MacOSBrowserAdapter extends BaseBrowserAdapter {
  protected scriptsPath: string;
  
  constructor(browserName: string) {
    super(browserName, 'darwin');
    this.scriptsPath = path.join(__dirname, '../../scripts/macos');
  }
  
  // Abstract methods that concrete adapters must implement
  protected abstract getAppName(): string; // e.g., "Opera GX", "Google Chrome"
  
  // AppleScript execution utilities
  protected async executeAppleScript(script: string): Promise<string> {
    try {
      const { stdout } = await execAsync(`osascript -e '${script.replace(/'/g, "'\"'\"'")}'`);
      return stdout.trim();
    } catch (error) {
      throw new Error(`AppleScript execution failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  
  protected async loadScript(scriptName: string): Promise<string> {
    try {
      const scriptPath = path.join(this.scriptsPath, `${scriptName}.applescript`);
      const script = await readFile(scriptPath, 'utf8');
      return script;
    } catch (error) {
      throw new Error(`Failed to load script ${scriptName}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  
  protected async executeScriptTemplate(templateName: string, variables: Record<string, string>): Promise<string> {
    let script = await this.loadScript(templateName);
    
    // Replace template variables
    Object.entries(variables).forEach(([key, value]) => {
      script = script.replace(new RegExp(`\\{\\{${key}\\}\\}`, 'g'), value);
    });
    
    return this.executeAppleScript(script);
  }
  
  // Default implementations using AppleScript templates
  async countTabs(urlPattern: string): Promise<number> {
    try {
      const result = await this.executeScriptTemplate('count-tabs', {
        APP_NAME: this.getAppName(),
        URL_PATTERN: urlPattern
      });
      return parseInt(result) || 0;
    } catch {
      return 0;
    }
  }
  
  async closeTabs(urlPattern: string): Promise<number> {
    try {
      const result = await this.executeScriptTemplate('close-tabs', {
        APP_NAME: this.getAppName(),
        URL_PATTERN: urlPattern
      });
      return parseInt(result) || 0;
    } catch {
      return 0;
    }
  }
  
  async focusTab(urlPattern: string): Promise<boolean> {
    try {
      const result = await this.executeScriptTemplate('focus-tab', {
        APP_NAME: this.getAppName(),
        URL_PATTERN: urlPattern
      });
      return result.startsWith('found-');
    } catch {
      return false;
    }
  }
  
  async isAvailable(): Promise<boolean> {
    try {
      const result = await this.executeScriptTemplate('check-browser-available', {
        APP_NAME: this.getAppName()
      });
      return result === 'true';
    } catch {
      return false;
    }
  }
  
  async getBrowserVersion(): Promise<string | null> {
    try {
      return await this.executeScriptTemplate('get-browser-version', {
        APP_NAME: this.getAppName()
      });
    } catch {
      return null;
    }
  }
}