/**
 * FileAppend Command - Abstract Base Class
 * 
 * HIERARCHICAL PATTERN: Part of file/ command group  
 * Inherits path from parent group: commands/file/append
 */

import { CommandBase } from '@commandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { FileAppendParams } from './FileAppendTypes';
import type { FileAppendResult } from './FileAppendTypes';

export abstract class FileAppendCommand extends CommandBase<FileAppendParams, FileAppendResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    // Hierarchical path: parent.subpath + "/append"
    super('file-append', context, subpath, commander);
  }

  public override getDefaultParams(): FileAppendParams {
    return new FileAppendParams({
      filepath: '',
      content: '',
      encoding: 'utf8',
      createIfMissing: true
    });
  }

  abstract execute(params: FileAppendParams): Promise<FileAppendResult>;
}