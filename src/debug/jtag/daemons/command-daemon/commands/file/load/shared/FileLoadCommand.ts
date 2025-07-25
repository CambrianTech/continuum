/**
 * FileLoad Command - Abstract Base Class
 * 
 * HIERARCHICAL PATTERN: Part of file/ command group  
 * Inherits path from parent group: commands/file/load
 */

import { CommandBase } from '../../../../shared/CommandBase';
import type { ICommandDaemon } from '../../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../../shared/JTAGTypes';
import { FileLoadParams } from './FileLoadTypes';
import type { FileLoadResult } from './FileLoadTypes';

export abstract class FileLoadCommand extends CommandBase<FileLoadParams, FileLoadResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    // Hierarchical path: parent.subpath + "/load"
    super('file-load', context, subpath, commander);
  }

  public override getDefaultParams(): FileLoadParams {
    return new FileLoadParams({
      filepath: '',
      encoding: 'utf8'
    });
  }

  abstract execute(params: FileLoadParams): Promise<FileLoadResult>;
}