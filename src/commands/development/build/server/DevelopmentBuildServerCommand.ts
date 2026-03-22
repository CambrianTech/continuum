/**
 * Development Build Command - Server Implementation
 *
 * Zero-friction TypeScript build check. Returns success or structured errors.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentBuildParams, DevelopmentBuildResult, TypeScriptError } from '../shared/DevelopmentBuildTypes';
import { createDevelopmentBuildResultFromParams } from '../shared/DevelopmentBuildTypes';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export class DevelopmentBuildServerCommand extends CommandBase<DevelopmentBuildParams, DevelopmentBuildResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/build', context, subpath, commander);
  }

  async execute(params: DevelopmentBuildParams): Promise<DevelopmentBuildResult> {
    const startTime = Date.now();

    try {
      // Run TypeScript compiler in check-only mode
      const { stdout, stderr } = await execAsync('npx tsc --noEmit', {
        cwd: process.cwd(),
        timeout: 120000 // 2 minute timeout
      });

      const duration = Date.now() - startTime;
      const output = stdout + stderr;

      return createDevelopmentBuildResultFromParams(params, {
        success: true,
        errorCount: 0,
        errors: [],
        duration,
        output: params.quiet ? '' : output
      });

    } catch (error: any) {
      const duration = Date.now() - startTime;

      // tsc returns non-zero on errors, output is in stdout/stderr
      const output = (error.stdout || '') + (error.stderr || '');
      const errors = this.parseTypeScriptErrors(output);

      return createDevelopmentBuildResultFromParams(params, {
        success: false,
        errorCount: errors.length,
        errors,
        duration,
        output: params.quiet ? '' : output
      });
    }
  }

  /**
   * Parse TypeScript compiler output into structured errors
   * Format: file(line,col): error TSxxxx: message
   */
  private parseTypeScriptErrors(output: string): TypeScriptError[] {
    const errors: TypeScriptError[] = [];
    // Match: path/file.ts(10,5): error TS2345: Argument of type...
    const errorRegex = /^(.+?)\((\d+),(\d+)\):\s*error\s+(TS\d+):\s*(.+)$/gm;

    let match;
    while ((match = errorRegex.exec(output)) !== null) {
      errors.push({
        file: match[1],
        line: parseInt(match[2], 10),
        column: parseInt(match[3], 10),
        code: match[4],
        message: match[5]
      });
    }

    return errors;
  }
}
