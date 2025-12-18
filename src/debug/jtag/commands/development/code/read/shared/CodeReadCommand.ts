/**
 * Code Read Command - Shared Base Class
 *
 * Base class for code read operations with environment routing
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { CodeReadParams, CodeReadResult } from './CodeReadTypes';

/**
 * Base class for code read commands
 * Provides environment routing via CommandBase
 */
export abstract class CodeReadCommand extends CommandBase<CodeReadParams, CodeReadResult> {
  constructor(commandName: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(commandName, context, subpath, commander);
  }

  /**
   * Execute with environment routing
   */
  async execute(params: CodeReadParams): Promise<CodeReadResult> {
    // Ensure backend is set
    const effectiveParams = {
      ...params,
      backend: params.backend ?? 'server'
    } as CodeReadParams;

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
  protected abstract executeCommand(params: CodeReadParams): Promise<CodeReadResult>;
}
