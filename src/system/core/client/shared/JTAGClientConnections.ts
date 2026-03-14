/**
 * JTAGClientConnections — Connection implementations for JTAGClient
 *
 * Extracted from JTAGClient. Contains LocalConnection and RemoteConnection,
 * plus the interfaces they implement/consume.
 */

import type { UUID } from '../../types/CrossPlatformUUID';
import type { CommandsInterface } from '../../shared/JTAGBase';
import type { JTAGContext, JTAGMessage, JTAGPayload, CommandParams, CommandResult } from '../../types/JTAGTypes';
import { JTAGMessageFactory } from '../../types/JTAGTypes';
import type { JTAGTransport, TransportSendResult } from '../../../transports';
import type { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGSystem } from '../../system/shared/JTAGSystem';
import type { CommandSignature } from '../../../../commands/list/shared/ListTypes';

// Verbose logging utility - mirrors JTAGClient's verbose helper
const verbose = (message: string, ...args: unknown[]) => {
  const isVerbose = (typeof process !== 'undefined' && process.env?.JTAG_VERBOSE === '1') ||
                    (typeof globalThis !== 'undefined' && (globalThis as Record<string, unknown>).JTAG_VERBOSE === true);
  if (isVerbose) {
    console.log(message, ...args);
  }
};

/**
 * Connection abstraction - local vs remote execution strategy
 */
export interface JTAGConnection {
  executeCommand(commandName: string, params: CommandParams | JTAGPayload): Promise<JTAGPayload>;
  getCommandsInterface(): CommandsInterface;
  readonly sessionId: UUID;
  readonly context: JTAGContext;
}

/**
 * Interface for the subset of JTAGClient that RemoteConnection needs.
 * Breaks the circular dependency between JTAGClient and RemoteConnection.
 */
export interface RemoteConnectionHost {
  readonly sessionId: UUID;
  readonly context: JTAGContext;
  getSystemTransport(): JTAGTransport | undefined;
  getDiscoveredCommands(): Map<string, CommandSignature>;
}

/**
 * Correlation interface for remote command execution.
 * Implemented by environment-specific correlators.
 */
export interface ICommandCorrelator {
  waitForResponse(correlationId: string, timeoutMs?: number): Promise<JTAGPayload>;
}

/**
 * Local connection - direct calls to local JTAG system (no transport)
 */
export class LocalConnection implements JTAGConnection {
  public readonly sessionId: UUID;
  public readonly context: JTAGContext;
  public readonly localSystem: JTAGSystem;

  constructor(
    localSystem: JTAGSystem,
    context: JTAGContext,
    sessionId: UUID
  ) {
    this.localSystem = localSystem;
    this.context = context;
    this.sessionId = sessionId;
  }

  async executeCommand(commandName: string, params: CommandParams | JTAGPayload): Promise<JTAGPayload> {
    // Direct call to local system - zero transport overhead
    const commandFn = this.localSystem.commands[commandName];
    if (!commandFn) {
      throw new Error(`Command '${commandName}' not available in local system`);
    }
    // commands proxy accepts CommandParams; JTAGPayload at wire boundary narrows here
    return await commandFn(params as CommandParams) as JTAGPayload;
  }

  getCommandsInterface(): CommandsInterface {
    // Delegate to local system's getCommandsInterface - zero transport overhead
    return this.localSystem.getCommandsInterface();
  }
}

/**
 * Remote connection - transport-based calls to remote JTAG system
 */
export class RemoteConnection implements JTAGConnection {
  public get sessionId(): UUID {
    return this.host.sessionId; // Dynamic getter - always uses host's current session
  }
  public readonly context: JTAGContext;

  constructor(
    private readonly host: RemoteConnectionHost,
    private readonly correlator: ICommandCorrelator
  ) {
    this.context = host.context;
  }

  async executeCommand(commandName: string, params: CommandParams | JTAGPayload): Promise<JTAGPayload> {
    // Create strongly-typed request message
    // Use client_ prefix for external client detection
    const correlationId = `client_${Date.now()}_${Math.random().toString(36).substr(2, 8)}`;

    const requestMessage: JTAGMessage = JTAGMessageFactory.createRequest(
      this.context,
      this.context.environment, // origin: current environment
      `server/commands/${commandName}`, // target: server command
      params,
      correlationId
    );

    // Send via transport - correlation handled by shared ResponseCorrelator
    const transport = this.host.getSystemTransport();
    if (!transport) {
      throw new Error('No transport available for remote command execution');
    }

    verbose(`📤 RemoteConnection: Sending command '${commandName}' with correlation ${correlationId}`);
    const sendResult: TransportSendResult = await transport.send(requestMessage);

    if (!sendResult.success) {
      throw new Error(`Transport failed to send command at ${sendResult.timestamp}`);
    }

    // Wait for correlated response — 10min safety net, CLI enforces the real per-command timeout
    const response = await this.correlator.waitForResponse(correlationId, 600000);
    return response;
  }

  getCommandsInterface(): CommandsInterface {
    const map = new Map();

    // Convert discovered commands to CommandsInterface format
    for (const [name, signature] of this.host.getDiscoveredCommands()) {
      // Create a command function that routes through this remote connection
      const commandFn = async (params?: JTAGPayload): Promise<JTAGPayload> => {
        return await this.executeCommand(name, params || { context: this.context, sessionId: this.sessionId });
      };

      // Remote commands are functions proxying through transport, not real CommandBase instances.
      // This cast is a wire boundary: the CommandsInterface Map expects CommandBase but remote
      // connections provide function proxies. TODO: Create proper CommandBase proxy adapter.
      map.set(name, commandFn as unknown as CommandBase<CommandParams, CommandResult>);
    }

    return map;
  }
}
