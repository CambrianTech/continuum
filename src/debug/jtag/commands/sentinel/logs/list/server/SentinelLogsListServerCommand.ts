/**
 * Sentinel Logs List Command - Server Implementation
 *
 * List available log streams for a sentinel.
 * Uses async file operations - NEVER blocks.
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { SentinelLogsListParams, SentinelLogsListResult, LogStreamInfo } from '../shared/SentinelLogsListTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

const BASE_DIR = '.sentinel-workspaces';

export class SentinelLogsListServerCommand extends CommandBase<SentinelLogsListParams, SentinelLogsListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/logs/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelLogsListResult> {
    const listParams = params as SentinelLogsListParams;
    const { handle } = listParams;

    if (!handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        logsDir: '',
        streams: [],
        error: 'Missing required parameter: handle',
      });
    }

    const logsDir = path.join(BASE_DIR, handle, 'logs');

    try {
      // Check if directory exists
      await fs.access(logsDir);

      // List all .log files
      const files = await fs.readdir(logsDir);
      const logFiles = files.filter(f => f.endsWith('.log'));

      // Get info for each file
      const streams: LogStreamInfo[] = await Promise.all(
        logFiles.map(async (filename) => {
          const filePath = path.join(logsDir, filename);
          const stats = await fs.stat(filePath);
          return {
            name: filename.replace('.log', ''),
            path: filePath,
            size: stats.size,
            modifiedAt: stats.mtime.toISOString(),
          };
        })
      );

      // Sort by modified time (most recent first)
      streams.sort((a, b) => new Date(b.modifiedAt).getTime() - new Date(a.modifiedAt).getTime());

      return transformPayload(params, {
        success: true,
        handle,
        logsDir,
        streams,
      });
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return transformPayload(params, {
          success: false,
          handle,
          logsDir,
          streams: [],
          error: `No logs found for sentinel: ${handle}`,
        });
      }
      return transformPayload(params, {
        success: false,
        handle,
        logsDir,
        streams: [],
        error: error.message,
      });
    }
  }
}
