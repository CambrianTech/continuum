/**
 * Sentinel Logs Tail Command - Server Implementation
 *
 * Get the last N lines of a log stream (like Unix tail).
 * Uses async file operations - NEVER blocks.
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { SentinelLogsTailParams, SentinelLogsTailResult } from '../shared/SentinelLogsTailTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

const BASE_DIR = '.sentinel-workspaces';
const DEFAULT_LINES = 20;

export class SentinelLogsTailServerCommand extends CommandBase<SentinelLogsTailParams, SentinelLogsTailResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/logs/tail', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelLogsTailResult> {
    const tailParams = params as SentinelLogsTailParams;
    const { handle, stream, lines = DEFAULT_LINES } = tailParams;

    if (!handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        stream: '',
        content: '',
        lineCount: 0,
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
        error: 'Missing required parameter: stream',
      });
    }

    const logPath = path.join(BASE_DIR, handle, 'logs', `${stream}.log`);

    try {
      const content = await fs.readFile(logPath, 'utf-8');
      const allLines = content.split('\n');

      // Get last N lines
      const tailLines = allLines.slice(-lines);

      return transformPayload(params, {
        success: true,
        handle,
        stream,
        content: tailLines.join('\n'),
        lineCount: tailLines.length,
      });
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return transformPayload(params, {
          success: false,
          handle,
          stream,
          content: '',
          lineCount: 0,
          error: `Log stream not found: ${stream}`,
        });
      }
      return transformPayload(params, {
        success: false,
        handle,
        stream,
        content: '',
        lineCount: 0,
        error: error.message,
      });
    }
  }
}
