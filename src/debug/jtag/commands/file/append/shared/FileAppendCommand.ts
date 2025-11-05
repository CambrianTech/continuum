/**
 * FileAppend Command - Abstract Base Class
 * 
 * HIERARCHICAL PATTERN: Part of file/ command group  
 * Inherits path from parent group: commands/file/append
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { type FileAppendParams, createFileAppendParams } from './FileAppendTypes';
import type { FileAppendResult } from './FileAppendTypes';

export abstract class FileAppendCommand extends CommandBase<FileAppendParams, FileAppendResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    // Hierarchical path: parent.subpath + "/append"
    super('file-append', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): FileAppendParams {
    return createFileAppendParams(this.context, sessionId, {
      filepath: '',
      content: '',
      encoding: 'utf8',
      createIfMissing: true
    });
  }

  abstract execute(params: FileAppendParams): Promise<FileAppendResult>;
}