/**
 * code/read server command - Read source code files
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { CodeDaemon } from '../../../../daemons/code-daemon/shared/CodeDaemon';
import type { CodeReadParams, CodeReadResult } from '../shared/CodeReadTypes';
import { createCodeReadResultFromParams } from '../shared/CodeReadTypes';
import { CodeReadCommand } from '../shared/CodeReadCommand';

export class CodeReadServerCommand extends CodeReadCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code-read', context, subpath, commander);
  }

  /**
   * Execute code/read command
   *
   * Delegates to CodeDaemon.readFile() static method
   */
  protected async executeCommand(params: CodeReadParams): Promise<CodeReadResult> {
    // Validate params
    if (!params.path) {
      return createCodeReadResultFromParams(params, {
        success: false,
        error: 'Missing required parameter: path'
      });
    }

    console.log(`📂 CODE SERVER: Reading file ${params.path} via CodeDaemon`);

    try {
      // Call CodeDaemon static method (auto-context injection, auto-event emission)
      const result = await CodeDaemon.readFile(params.path, {
        startLine: params.startLine,
        endLine: params.endLine,
        includeMetadata: params.includeMetadata,
        forceRefresh: params.forceRefresh
      });

      if (result.success) {
        console.log(`✅ CODE SERVER: Read ${params.path} (${result.metadata.linesReturned} lines)`);
      } else {
        console.log(`❌ CODE SERVER: Failed to read ${params.path}: ${result.error}`);
      }

      return createCodeReadResultFromParams(params, result);
    } catch (error) {
      console.error(`❌ CODE SERVER: Exception reading ${params.path}:`, error);

      return createCodeReadResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}
