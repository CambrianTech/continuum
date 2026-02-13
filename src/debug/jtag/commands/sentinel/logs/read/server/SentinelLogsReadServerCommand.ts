/**
 * Sentinel Logs Read Command - Server Implementation
 *
 * Read a log stream for a sentinel with optional offset/limit.
 * Uses async file operations - NEVER blocks.
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { SentinelLogsReadParams, SentinelLogsReadResult } from '../shared/SentinelLogsReadTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

const BASE_DIR = '.sentinel-workspaces';

export class SentinelLogsReadServerCommand extends CommandBase<SentinelLogsReadParams, SentinelLogsReadResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/logs/read', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelLogsReadResult> {
    const readParams = params as SentinelLogsReadParams;
    const { handle, stream, offset = 0, limit } = readParams;

    if (!handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        stream: '',
        content: '',
        lineCount: 0,
        totalLines: 0,
        truncated: false,
        error: 'Missing required parameter: handle',
      });
    }

    if (!stream) {
      return transformPayload(params, {
        success: false,
        handle,
        stream: '',
        content: '',
        lineCount: 0,
        totalLines: 0,
        truncated: false,
        error: 'Missing required parameter: stream',
      });
    }

    const logPath = path.join(BASE_DIR, handle, 'logs', `${stream}.log`);

    try {
      const content = await fs.readFile(logPath, 'utf-8');
      const allLines = content.split('\n');
      const totalLines = allLines.length;

      // Apply offset and limit
      let selectedLines = allLines.slice(offset);
      let truncated = false;

      if (limit !== undefined && limit > 0) {
        if (selectedLines.length > limit) {
          selectedLines = selectedLines.slice(0, limit);
          truncated = true;
        }
      }

      return transformPayload(params, {
        success: true,
        handle,
        stream,
        content: selectedLines.join('\n'),
        lineCount: selectedLines.length,
        totalLines,
        truncated,
      });
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return transformPayload(params, {
          success: false,
          handle,
          stream,
          content: '',
          lineCount: 0,
          totalLines: 0,
          truncated: false,
          error: `Log stream not found: ${stream}`,
        });
      }
      return transformPayload(params, {
        success: false,
        handle,
        stream,
        content: '',
        lineCount: 0,
        totalLines: 0,
        truncated: false,
        error: error.message,
      });
    }
  }
}
