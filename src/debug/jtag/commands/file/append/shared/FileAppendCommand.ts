/**
 * FileAppend Command - Abstract Base Class
 * 
 * HIERARCHICAL PATTERN: Part of file/ command group  
 * Inherits path from parent group: commands/file/append
 */

import { CommandBase } from '@commandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';
import { type FileAppendParams, createFileAppendParams } from '@fileAppend/shared/FileAppendTypes';
import type { FileAppendResult } from '@fileAppend/shared/FileAppendTypes';

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