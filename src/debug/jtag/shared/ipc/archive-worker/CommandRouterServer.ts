/**
 * CommandRouterServer - Handles command execution calls FROM Rust workers
 *
 * TEMPLATE: This pattern handles bidirectional communication
 * - Rust calls commands via Unix socket
 * - This server executes command through CommandDaemon and returns result
 * - Keeps Rust workers as first-class citizens
 *
 * Flow:
 * Rust → Socket → CommandRouterServer → CommandDaemon.execute() → Result → Socket → Rust
 */

import * as net from 'net';
import * as fs from 'fs';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '../../../system/core/types/SystemScopes';

interface CommandRequest {
  command: string;
  params: Record<string, unknown>;
}

interface CommandResponse {
  success: boolean;
  result?: unknown;
  error?: string;
}

export class CommandRouterServer {
  private server: net.Server | null = null;
  private readonly socketPath: string;
  private log: ComponentLogger;

  constructor(socketPath: string) {
    this.socketPath = socketPath;
    this.log = Logger.create('CommandRouterServer', 'workers/CommandRouter');
  }

  /**
   * Start listening for Rust command requests
   */
  async start(): Promise<void> {
    // Remove socket if exists
    if (fs.existsSync(this.socketPath)) {
      fs.unlinkSync(this.socketPath);
    }

    this.server = net.createServer((socket) => {
      this.handleConnection(socket);
    });

    return new Promise((resolve, reject) => {
      this.server!.listen(this.socketPath, () => {
        this.log.info(`📡 Command Router listening on ${this.socketPath}`);
        resolve();
      });

      this.server!.on('error', (error) => {
        this.log.error('Server error:', error);
        reject(error);
      });
    });
  }

  /**
   * Stop server
   */
  async stop(): Promise<void> {
    if (this.server) {
      return new Promise((resolve) => {
        this.server!.close(() => {
          this.log.info('Command Router stopped');
          resolve();
        });
      });
    }
  }

  /**
   * Handle connection from Rust worker
   */
  private handleConnection(socket: net.Socket): void {
    this.log.info('🔗 Rust worker connected');

    let buffer = '';

    socket.on('data', async (chunk) => {
      buffer += chunk.toString();

      // Process complete lines (JSON messages end with \n)
      const lines = buffer.split('\n');
      buffer = lines.pop() || ''; // Keep incomplete line in buffer

      for (const line of lines) {
        if (line.trim()) {
          await this.handleRequest(socket, line);
        }
      }
    });

    socket.on('end', () => {
      this.log.info('Rust worker disconnected');
    });

    socket.on('error', (error) => {
      this.log.error('Socket error:', error);
    });
  }

  /**
   * Handle single command request from Rust
   *
   * Uses JTAGClient-style routing to properly handle both browser and server commands.
   * Commands are sent through the router's transport layer, which handles cross-context routing.
   */
  private async handleRequest(socket: net.Socket, line: string): Promise<void> {
    try {
      const request: CommandRequest = JSON.parse(line);
      this.log.info(`Executing command from Rust: ${request.command}`);

      // Get JTAGSystemServer instance
      const { JTAGSystemServer } = await import('../../../system/core/system/server/JTAGSystemServer');
      const system = JTAGSystemServer.instance;

      if (!system) {
        throw new Error('JTAGSystemServer not initialized');
      }

      // Use getCommandsInterface() which returns the server-side command interface
      // This includes routing capabilities for browser commands via the router
      const commandsInterface = system.getCommandsInterface();
      const commandFn = commandsInterface.get(request.command);

      if (commandFn) {
        // Server-side command - execute directly
        const sessionId = (request.params.sessionId as UUID) || SYSTEM_SCOPES.SYSTEM as UUID;

        // Use the system's context, which is a proper JTAGContext
        const fullParams = {
          context: system.context,
          sessionId,
          ...request.params
        };

        const result = await commandFn.execute(fullParams);

        const response: CommandResponse = {
          success: true,
          result
        };

        socket.write(JSON.stringify(response) + '\n');
      } else {
        // Command not in server CommandDaemon - might be browser-only
        // Return informative error (browser commands require CLI/WebSocket routing)
        const available = Array.from(commandsInterface.keys()).slice(0, 20);
        throw new Error(
          `Command '${request.command}' not available in server context. ` +
          `Server commands available: ${available.join(', ')}... ` +
          `(Browser-only commands like 'screenshot' require CLI routing)`
        );
      }
    } catch (error) {
      this.log.error('Command execution error:', error);

      const response: CommandResponse = {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };

      socket.write(JSON.stringify(response) + '\n');
    }
  }
}
