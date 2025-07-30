/**
 * FileSave Command - Generic Base Class
 * 
 * GENERIC HIERARCHY: Extends FileCommand<FileSaveParams, FileSaveResult>
 * Inherits path from parent group: commands/file/save
 */

import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from '@shared/CrossPlatformUUID';
import { FileCommand } from '@commandsFile/shared/FileTypes';
import { type FileSaveParams, type FileSaveResult, createFileSaveParams } from '@fileSave/shared/FileSaveTypes';

export abstract class FileSaveCommand extends FileCommand<FileSaveParams, FileSaveResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    // Hierarchical path: parent.subpath + "/save"
    super('file-save', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): FileSaveParams {
    return createFileSaveParams(this.context, sessionId, {
      filepath: '',
      content: '',
      encoding: 'utf8',
      createDirs: true
    });
  }

  abstract execute(params: FileSaveParams): Promise<FileSaveResult>;
}