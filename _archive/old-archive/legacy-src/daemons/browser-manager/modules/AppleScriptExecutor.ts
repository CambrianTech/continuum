/**
 * AppleScript Executor - Centralized AppleScript execution
 * Allows for proper mocking in tests while keeping scripts in separate files
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import * as path from 'path';
import * as fs from 'fs';
import { fileURLToPath } from 'url';

const execAsync = promisify(exec);

export interface AppleScriptResult {
  stdout: string;
  stderr?: string;
}

export class AppleScriptExecutor {
  private scriptsPath: string;

  constructor() {
    // Get __dirname equivalent for ES modules
    const __filename = fileURLToPath(import.meta.url);
    const __dirname = path.dirname(__filename);
    this.scriptsPath = path.join(__dirname, 'applescript');
  }

  /**
   * Execute AppleScript file with function call
   */
  async executeScript(scriptFile: string, functionName: string, args: string[]): Promise<AppleScriptResult> {
    const scriptPath = path.join(this.scriptsPath, scriptFile);
    
    if (!fs.existsSync(scriptPath)) {
      throw new Error(`AppleScript file not found: ${scriptPath}`);
    }

    // Build the osascript command to call the function
    const quotedArgs = args.map(arg => `"${arg.replace(/"/g, '\\"')}"`).join(', ');
    const command = `osascript -l AppleScript -e "
      set scriptPath to "${scriptPath}"
      set scriptAlias to (scriptPath as POSIX file) as alias
      set scriptContent to (load script scriptAlias)
      tell scriptContent to ${functionName}(${quotedArgs})
    "`;

    try {
      const result = await execAsync(command);
      return {
        stdout: result.stdout,
        stderr: result.stderr
      };
    } catch (error) {
      if (error instanceof Error && 'stdout' in error) {
        return {
          stdout: (error as any).stdout || '',
          stderr: (error as any).stderr || error.message
        };
      }
      throw error;
    }
  }

  /**
   * Execute inline AppleScript (for backward compatibility)
   */
  async executeInline(script: string): Promise<AppleScriptResult> {
    try {
      const result = await execAsync(`osascript -e '${script.replace(/'/g, "'\"'\"'")}'`);
      return {
        stdout: result.stdout,
        stderr: result.stderr
      };
    } catch (error) {
      if (error instanceof Error && 'stdout' in error) {
        return {
          stdout: (error as any).stdout || '',
          stderr: (error as any).stderr || error.message
        };
      }
      throw error;
    }
  }
}

// Singleton instance for easy mocking
export const appleScriptExecutor = new AppleScriptExecutor();