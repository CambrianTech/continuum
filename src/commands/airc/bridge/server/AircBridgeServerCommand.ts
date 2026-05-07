import { spawn } from 'node:child_process';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { ValidationError } from '@system/core/types/ErrorTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import type { RoomEntity } from '@system/data/entities/RoomEntity';
import { ChatSend } from '@commands/collaboration/chat/send/shared/ChatSendTypes';
import { ChatExport } from '@commands/collaboration/chat/export/shared/ChatExportTypes';
import { ActivityList } from '@commands/collaboration/activity/list/shared/ActivityListTypes';
import {
  formatAircBridgeChatText,
  parseAircBridgeMessage,
  summarizeBridgeResponse,
} from '@system/airc-bridge/shared/AircBridgeProtocol';
import type { ParsedAircBridgeMessage } from '@system/airc-bridge/shared/AircBridgeProtocol';
import { AircBridgeCommand } from '../shared/AircBridgeCommand';
import type { AircBridgeParams, AircBridgeResult } from '../shared/AircBridgeTypes';
import { createAircBridgeResultFromParams } from '../shared/AircBridgeTypes';

interface BridgeHandlerResult {
  responseText: string;
  commandResult?: unknown;
  mirrorError?: string;
}

export class AircBridgeServerCommand extends AircBridgeCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeAircBridge(params: AircBridgeParams): Promise<AircBridgeResult> {
    this.validateParams(params);

    const parsed = parseAircBridgeMessage(params.message, {
      senderNick: params.senderNick,
      channel: params.channel,
      room: params.room,
      commandPrefix: params.commandPrefix,
    });

    if (params.dryRun) return this.dryRun(params, parsed);

    try {
      const result = await this.handleParsedMessage(params, parsed);
      const mirror = await this.mirrorResponseIfRequested(params, parsed.channel, result.responseText);
      return createAircBridgeResultFromParams(params, {
        success: true,
        handled: true,
        parsed,
        ...result,
        mirrored: mirror.mirrored,
        mirrorError: mirror.error,
      });
    } catch (error) {
      return this.failed(params, parsed, error);
    }
  }

  private validateParams(params: AircBridgeParams): void {
    if (!params.message || params.message.trim() === '') {
      throw new ValidationError(
        'message',
        'Missing required parameter message. Pass the raw AIRC message body to ingest.',
      );
    }
  }

  private dryRun(params: AircBridgeParams, parsed: ParsedAircBridgeMessage): AircBridgeResult {
    return createAircBridgeResultFromParams(params, {
      success: true,
      handled: false,
      parsed,
      responseText: `dry-run: ${parsed.action} -> ${parsed.room}`,
    });
  }

  private failed(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
    error: unknown,
  ): AircBridgeResult {
    const message = error instanceof Error ? error.message : String(error);
    return createAircBridgeResultFromParams(params, {
      success: false,
      handled: false,
      parsed,
      error: message,
      responseText: `airc bridge failed: ${message}`,
    });
  }

  private async handleParsedMessage(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<BridgeHandlerResult> {
    const handlers: Record<string, () => Promise<BridgeHandlerResult>> = {
      skip: () => Promise.resolve({ responseText: 'skipped Continuum-origin mirror echo' }),
      chat: () => this.handleChat(params, parsed),
      ping: () => Promise.resolve({ responseText: `continuum-airc-bridge ok (${parsed.room})`, commandResult: { ok: true } }),
      status: () => this.handleStatus(params, parsed),
      rooms: () => this.handleRooms(params, parsed),
      'activity-list': () => this.handleActivityList(params, parsed),
      export: () => this.handleExport(params, parsed),
      'assert-seen': () => this.handleAssertSeen(params, parsed),
    };

    const handler = handlers[parsed.action];
    if (!handler) {
      throw new Error(parsed.error ?? 'unknown AIRC bridge directive');
    }
    return handler();
  }

  private async handleChat(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<BridgeHandlerResult> {
    const commandResult = await ChatSend.execute({
      room: parsed.room,
      message: formatAircBridgeChatText(parsed),
      context: params.context,
      sessionId: params.sessionId,
    });
    return {
      commandResult,
      responseText: `bridged chat from ${parsed.senderNick} into ${parsed.room}`,
    };
  }

  private async handleStatus(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<BridgeHandlerResult> {
    const rooms = await this.listRooms(parsed.limit ?? 25, params);
    return {
      commandResult: rooms,
      responseText: `continuum-airc-bridge ok; rooms=${rooms.length}; room=${parsed.room}`,
    };
  }

  private async handleRooms(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<BridgeHandlerResult> {
    const rooms = await this.listRooms(parsed.limit ?? 50, params);
    const labels = rooms.map(room => room.name || room.uniqueId || room.id).join(', ');
    return {
      commandResult: rooms,
      responseText: labels ? `rooms: ${labels}` : 'rooms: none',
    };
  }

  private async handleActivityList(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<BridgeHandlerResult> {
    const commandResult = await ActivityList.execute({
      limit: parsed.limit ?? 50,
      context: params.context,
      sessionId: params.sessionId,
    });
    const result = commandResult as { success?: boolean; activities?: Array<{ displayName?: string; id?: string }> };
    return {
      commandResult,
      responseText: result.success
        ? `activities: ${this.formatActivityLabels(result.activities)}`
        : 'activity list failed',
    };
  }

  private async handleExport(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<BridgeHandlerResult> {
    const commandResult = await ChatExport.execute({
      room: parsed.room,
      limit: parsed.limit ?? 50,
      context: params.context,
      sessionId: params.sessionId,
    });
    const result = commandResult as { success?: boolean; markdown?: string; message?: string };
    return {
      commandResult,
      responseText: result.success
        ? summarizeBridgeResponse(result.markdown ?? result.message ?? '')
        : `export failed: ${result.message ?? 'unknown error'}`,
    };
  }

  private async handleAssertSeen(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<BridgeHandlerResult> {
    const commandResult = await ChatExport.execute({
      room: parsed.room,
      limit: parsed.limit ?? 50,
      includeSystem: true,
      includeTests: true,
      context: params.context,
      sessionId: params.sessionId,
    });
    const result = commandResult as { markdown?: string };
    const found = Boolean(parsed.marker && result.markdown?.includes(parsed.marker));
    if (!found) throw new Error(`assert seen failed: ${parsed.marker ?? '(missing marker)'}`);
    return { commandResult, responseText: `assert seen ok: ${parsed.marker}` };
  }

  private async listRooms(limit: number, params: AircBridgeParams): Promise<RoomEntity[]> {
    const result = await DataList.execute<RoomEntity>({
      collection: 'rooms',
      limit,
      orderBy: [{ field: 'lastMessageAt', direction: 'desc' }],
      context: params.context,
      sessionId: params.sessionId,
    });
    return result.success ? [...result.items] : [];
  }

  private formatActivityLabels(activities?: Array<{ displayName?: string; id?: string }>): string {
    const labels = activities?.map(a => a.displayName ?? a.id).filter(Boolean).join(', ') ?? '';
    return labels.length > 0 ? labels : 'none';
  }

  private async mirrorResponseIfRequested(
    params: AircBridgeParams,
    channel: string,
    responseText: string,
  ): Promise<{ mirrored: boolean; error?: string }> {
    if (!params.mirrorResponse || !responseText.trim()) return { mirrored: false };
    try {
      const result = await this.spawnAirc([
        'msg',
        '--channel',
        channel,
        `[continuum] ${summarizeBridgeResponse(responseText, 1200)}`,
      ]);
      return result.exitCode === 0
        ? { mirrored: true }
        : { mirrored: false, error: result.stderr || `airc exited ${result.exitCode}` };
    } catch (error) {
      return {
        mirrored: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  private spawnAirc(argv: string[]): Promise<{ exitCode: number; stderr: string }> {
    return new Promise((resolve, reject) => {
      const child = spawn('airc', argv, { stdio: ['ignore', 'ignore', 'pipe'] });
      let stderr = '';

      child.stderr.on('data', (chunk: Buffer) => { stderr += chunk.toString('utf8'); });
      child.on('error', reject);
      child.on('close', exitCode => resolve({ exitCode: exitCode ?? -1, stderr }));
    });
  }
}
