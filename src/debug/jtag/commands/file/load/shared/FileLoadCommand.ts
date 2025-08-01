/**
 * FileLoad Command - Abstract Base Class
 * 
 * HIERARCHICAL PATTERN: Part of file/ command group  
 * Inherits path from parent group: commands/file/load
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { type FileLoadParams, createFileLoadParams } from './FileLoadTypes';
import type { FileLoadResult } from './FileLoadTypes';

export abstract class FileLoadCommand extends CommandBase<FileLoadParams, FileLoadResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    // Hierarchical path: parent.subpath + "/load"
    super('file-load', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): FileLoadParams {
    return createFileLoadParams(this.context, sessionId, {
      filepath: '',
      encoding: 'utf8'
    });
  }

  abstract execute(params: FileLoadParams): Promise<FileLoadResult>;
}