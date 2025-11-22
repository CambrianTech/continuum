/**
 * code/find shared command - Find files by name pattern
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { CodeFindParams, CodeFindResult } from './CodeFindTypes';

/**
 * Shared base for code/find command
 */
export abstract class CodeFindCommand extends CommandBase<CodeFindParams, CodeFindResult> {
  constructor(
    name: string,
    context: JTAGContext,
    subpath: string,
    commander: ICommandDaemon
  ) {
    super(name, context, subpath, commander);
  }

  /**
   * Execute with environment routing
   */
  async execute(params: CodeFindParams): Promise<CodeFindResult> {
    // Ensure backend is set
    const effectiveParams = {
      ...params,
      backend: params.backend ?? 'server'
    } as CodeFindParams;

    // If we're not in the requested environment, delegate
    if (this.context.environment !== effectiveParams.backend) {
      return await this.remoteExecute(effectiveParams);
    }

    // We're in the correct environment, execute locally
    return await this.executeCommand(effectiveParams);
  }

  /**
   * Subclasses implement this for their specific environment
   */
  protected abstract executeCommand(params: CodeFindParams): Promise<CodeFindResult>;
}
