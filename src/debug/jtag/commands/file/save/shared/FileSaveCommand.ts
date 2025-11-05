/**
 * FileSave Command - Generic Base Class
 * 
 * GENERIC HIERARCHY: Extends FileCommand<FileSaveParams, FileSaveResult>
 * Inherits path from parent group: commands/file/save
 */

import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { FileCommand } from '../../shared/FileTypes';
import { type FileSaveParams, type FileSaveResult, createFileSaveParams } from './FileSaveTypes';

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