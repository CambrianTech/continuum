/**
 * Airc Bridge Command - Server Implementation
 *
 * Ingest one AIRC message into Continuum. Normal messages become chat;
 * explicit !continuum directives become bounded development/test commands.
 */

import { spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import { ValidationError } from '@system/core/types/ErrorTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import {
  formatAircBridgeChatText,
  parseAircBridgeMessage,
  summarizeBridgeResponse,
  type ParsedAircBridgeMessage,
} from '@system/airc-bridge/shared/AircBridgeProtocol';
import type { AircBridgeParams, AircBridgeResult } from '../shared/AircBridgeTypes';
import { createAircBridgeResultFromParams } from '../shared/AircBridgeTypes';

interface CommandLikeResult {
  success?: boolean;
  error?: unknown;
  message?: unknown;
  markdown?: unknown;
  commands?: unknown;
  totalCount?: unknown;
}

function isCommandLikeResult(value: unknown): value is CommandLikeResult {
  return typeof value === 'object' && value !== null;
}

export class AircBridgeServerCommand extends CommandBase<AircBridgeParams, AircBridgeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('airc/bridge', context, subpath, commander);
  }

  async execute(params: AircBridgeParams): Promise<AircBridgeResult> {
    if (!params.message?.trim()) {
      throw new ValidationError('message', 'Missing required AIRC message body.');
    }

    const parsed = parseAircBridgeMessage(params.message, {
      senderNick: params.senderNick,
      channel: params.channel,
      room: params.room,
      commandPrefix: params.commandPrefix,
    });

    if (params.dryRun) {
      return createAircBridgeResultFromParams(params, {
        success: true,
        handled: false,
        parsed,
        responseText: `dry-run: ${parsed.action} -> ${parsed.room}`,
      });
    }

    const handled = await this.handleParsedMessage(params, parsed);

    if (params.mirrorResponse && handled.responseText) {
      await this.mirrorToAirc(handled.responseText);
      return createAircBridgeResultFromParams(params, {
        ...handled,
        mirrored: true,
      });
    }

    return createAircBridgeResultFromParams(params, handled);
  }

  private async handleParsedMessage(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<Omit<AircBridgeResult, 'context' | 'sessionId' | 'userId'>> {
    switch (parsed.action) {
      case 'skip':
        return { success: true, handled: false, parsed, responseText: 'skipped continuum-origin echo' };
      case 'ping':
        return { success: true, handled: true, parsed, responseText: 'pong from Continuum airc/bridge' };
      case 'chat':
        return this.bridgeChat(params, parsed);
      case 'status':
        return this.commandResponse(params, parsed, 'system/resources', {}, 'Continuum status');
      case 'rooms':
        return this.commandResponse(params, parsed, 'workspace/list', {}, 'Continuum rooms/workspaces');
      case 'activity-list':
        return this.commandResponse(params, parsed, 'list', { includeDescription: false }, 'Continuum command list');
      case 'export':
        return this.exportChat(params, parsed);
      case 'assert-seen':
        return this.assertSeen(params, parsed);
      case 'unknown':
        throw new ValidationError('message', parsed.error ?? 'Unknown AIRC bridge directive.');
    }
  }

  private async bridgeChat(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<Omit<AircBridgeResult, 'context' | 'sessionId' | 'userId'>> {
    const commandResult = await this.executeContinuumCommand(params, 'collaboration/chat/send', {
      message: formatAircBridgeChatText(parsed),
      room: parsed.room,
      isSystemTest: false,
    });
    this.assertCommandSuccess(commandResult, 'collaboration/chat/send');

    return {
      success: true,
      handled: true,
      parsed,
      responseText: `bridged chat into #${parsed.room}`,
      commandResult,
    };
  }

  private async exportChat(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<Omit<AircBridgeResult, 'context' | 'sessionId' | 'userId'>> {
    const commandResult = await this.executeContinuumCommand(params, 'collaboration/chat/export', {
      room: parsed.room,
      limit: parsed.limit,
      includeSystem: true,
      includeTests: true,
    });
    this.assertCommandSuccess(commandResult, 'collaboration/chat/export');

    const text = this.readStringField(commandResult, 'markdown') ?? this.readStringField(commandResult, 'message') ?? 'export completed';
    return {
      success: true,
      handled: true,
      parsed,
      responseText: summarizeBridgeResponse(text),
      commandResult,
    };
  }

  private async assertSeen(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
  ): Promise<Omit<AircBridgeResult, 'context' | 'sessionId' | 'userId'>> {
    if (!parsed.marker) {
      throw new ValidationError('message', 'Expected: !continuum assert seen <marker>');
    }

    const commandResult = await this.executeContinuumCommand(params, 'collaboration/chat/export', {
      room: parsed.room,
      limit: parsed.limit,
      includeSystem: true,
      includeTests: true,
    });
    this.assertCommandSuccess(commandResult, 'collaboration/chat/export');

    const exported = this.readStringField(commandResult, 'markdown') ?? '';
    if (!exported.includes(parsed.marker)) {
      throw new ValidationError('marker', `Marker not found in #${parsed.room}: ${parsed.marker}`);
    }

    return {
      success: true,
      handled: true,
      parsed,
      responseText: `marker seen in #${parsed.room}: ${parsed.marker}`,
      commandResult,
    };
  }

  private async commandResponse(
    params: AircBridgeParams,
    parsed: ParsedAircBridgeMessage,
    commandName: string,
    data: Record<string, unknown>,
    label: string,
  ): Promise<Omit<AircBridgeResult, 'context' | 'sessionId' | 'userId'>> {
    const commandResult = await this.executeContinuumCommand(params, commandName, data);
    this.assertCommandSuccess(commandResult, commandName);

    return {
      success: true,
      handled: true,
      parsed,
      responseText: summarizeBridgeResponse(`${label}: ${JSON.stringify(commandResult)}`),
      commandResult,
    };
  }

  private async executeContinuumCommand(
    params: AircBridgeParams,
    commandName: string,
    data: Record<string, unknown>,
  ): Promise<unknown> {
    return Commands.execute<CommandParams, CommandResult>(commandName, {
      context: params.context,
      sessionId: params.sessionId,
      userId: params.userId ?? SYSTEM_SCOPES.SYSTEM,
      ...data,
    });
  }

  private assertCommandSuccess(result: unknown, commandName: string): void {
    if (!isCommandLikeResult(result)) return;
    if (result.success === false) {
      const detail = result.error ?? result.message ?? 'no error detail';
      throw new Error(`${commandName} failed: ${String(detail)}`);
    }
  }

  private readStringField(result: unknown, fieldName: keyof CommandLikeResult): string | undefined {
    if (!isCommandLikeResult(result)) return undefined;
    const value = result[fieldName];
    return typeof value === 'string' ? value : undefined;
  }

  private async mirrorToAirc(responseText: string): Promise<void> {
    const message = `[continuum] ${summarizeBridgeResponse(responseText, 1200)}`;
    const result = await this.spawnAirc(['msg', message]);
    if (result.exitCode !== 0) {
      throw new Error(`AIRC mirror failed: ${result.stderr || result.stdout || `exit ${result.exitCode}`}`);
    }
  }

  private spawnAirc(args: string[]): Promise<{ exitCode: number; stdout: string; stderr: string }> {
    return new Promise((resolve, reject) => {
      const repoRoot = this.findRepoRoot(process.cwd());
      const child = spawn('airc', args, {
        cwd: repoRoot,
        env: {
          ...process.env,
          AIRC_HOME: path.join(repoRoot, '.airc'),
        },
        stdio: ['ignore', 'pipe', 'pipe'],
      });

      let stdout = '';
      let stderr = '';
      child.stdout.on('data', chunk => { stdout += chunk.toString(); });
      child.stderr.on('data', chunk => { stderr += chunk.toString(); });
      child.on('error', reject);
      child.on('close', code => {
        resolve({ exitCode: code ?? 1, stdout: stdout.trim(), stderr: stderr.trim() });
      });
    });
  }

  private findRepoRoot(startDir: string): string {
    let current = startDir;
    while (current !== path.dirname(current)) {
      if (path.basename(current) === 'src' && this.pathExists(path.join(current, '..', '.git'))) {
        return path.dirname(current);
      }
      if (this.pathExists(path.join(current, '.git'))) {
        return current;
      }
      current = path.dirname(current);
    }
    return startDir;
  }

  private pathExists(targetPath: string): boolean {
    return fs.existsSync(targetPath);
  }
}
