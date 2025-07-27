/**
 * FileLoad Command - Abstract Base Class
 * 
 * HIERARCHICAL PATTERN: Part of file/ command group  
 * Inherits path from parent group: commands/file/load
 */

import { CommandBase } from '@commandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';
import { FileLoadParams } from './FileLoadTypes';
import type { FileLoadResult } from './FileLoadTypes';

export abstract class FileLoadCommand extends CommandBase<FileLoadParams, FileLoadResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    // Hierarchical path: parent.subpath + "/load"
    super('file-load', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): FileLoadParams {
    return new FileLoadParams({
      filepath: '',
      encoding: 'utf8'
    }, this.context, sessionId);
  }

  abstract execute(params: FileLoadParams): Promise<FileLoadResult>;
}