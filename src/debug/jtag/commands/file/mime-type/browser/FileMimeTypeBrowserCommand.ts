/**
 * File MIME Type Command - Browser Implementation
 * Delegates to server for MIME type detection
 */

import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '../../../../system/core/types/SystemScopes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { FileMimeTypeCommand } from '../shared/FileMimeTypeCommand';
import type { FileMimeTypeResult } from '../shared/FileMimeTypeTypes';

export class FileMimeTypeBrowserCommand extends FileMimeTypeCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<FileMimeTypeResult> {
    // Browser cannot access filesystem - delegate to server
    return await this.remoteExecute({ ...params, userId: SYSTEM_SCOPES.SYSTEM });
  }
}
