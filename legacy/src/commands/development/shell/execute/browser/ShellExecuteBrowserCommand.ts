/**
 * Shell Execute Command - Browser Implementation
 *
 * Delegates to server since shell commands can only run server-side.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { ShellExecuteCommand } from '../shared/ShellExecuteCommand';
import type { ShellExecuteParams, ShellExecuteResult } from '../shared/ShellExecuteTypes';

export class ShellExecuteBrowserCommand extends ShellExecuteCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Browser implementation: Delegate to server
   * Shell commands can only run on the server side
   */
  async execute(params: ShellExecuteParams): Promise<ShellExecuteResult> {
    // Validate params before delegating (fast client-side validation)
    const validation = this.validateParams(params);
    if (!validation.valid) {
      return this.formatErrorResult(params, validation.error ?? 'Invalid parameters');
    }

    // Delegate to server for actual execution
    return await this.remoteExecute(params);
  }
}
