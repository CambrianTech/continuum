/**
 * FileSave Command - Generic Base Class
 * 
 * GENERIC HIERARCHY: Extends FileCommand<FileSaveParams, FileSaveResult>
 * Inherits path from parent group: commands/file/save
 */

import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { FileCommand } from '@fileShared/FileTypes';
import { FileSaveParams } from './FileSaveTypes';
import type { FileSaveResult } from './FileSaveTypes';

export abstract class FileSaveCommand extends FileCommand<FileSaveParams, FileSaveResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    // Hierarchical path: parent.subpath + "/save"
    super('file-save', context, subpath, commander);
  }

  public override getDefaultParams(): FileSaveParams {
    return new FileSaveParams({
      filepath: '',
      content: '',
      encoding: 'utf8',
      createDirs: true
    });
  }

  abstract execute(params: FileSaveParams): Promise<FileSaveResult>;
}