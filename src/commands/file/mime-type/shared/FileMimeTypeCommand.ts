/**
 * File MIME Type Command - Abstract Base
 * Detects MIME type from file path
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { FileMimeTypeParams, FileMimeTypeResult } from './FileMimeTypeTypes';
import { MIME_TYPES, getMediaTypeFromMime } from './FileMimeTypeTypes';

export abstract class FileMimeTypeCommand extends CommandBase<FileMimeTypeParams, FileMimeTypeResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('file/mime-type', context, subpath, commander);
  }

  /**
   * Detect MIME type from file extension
   * Override in server implementation to add content inspection
   */
  protected detectMimeType(filepath: string): {
    mimeType: string;
    extension: string;
    detectionMethod: 'extension' | 'content' | 'default';
  } {
    const ext = filepath.toLowerCase().split('.').pop() || '';
    const mimeType = MIME_TYPES[ext] || 'application/octet-stream';

    return {
      mimeType,
      extension: ext,
      detectionMethod: mimeType === 'application/octet-stream' ? 'default' : 'extension'
    };
  }

  /**
   * Create result object
   */
  protected createResult(
    params: FileMimeTypeParams,
    detection: ReturnType<typeof this.detectMimeType>
  ): FileMimeTypeResult {
    return {
      success: true,
      filepath: params.filepath || '',
      exists: true,
      mimeType: detection.mimeType,
      mediaType: getMediaTypeFromMime(detection.mimeType),
      extension: detection.extension,
      detectionMethod: detection.detectionMethod,
      timestamp: new Date().toISOString(),
      context: params.context,
      sessionId: params.sessionId
    };
  }
}
