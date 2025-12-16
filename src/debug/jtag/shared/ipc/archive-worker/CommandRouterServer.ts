/**
 * CommandRouterServer - Handles Commands.execute() calls FROM Rust workers
 *
 * TEMPLATE: This pattern handles bidirectional communication
 * - Rust calls Commands.execute() via Unix socket
 * - This server executes command and returns result
 * - Keeps Rust workers as first-class citizens
 *
 * Flow:
 * Rust → Socket → CommandRouterServer → Commands.execute() → Result → Socket → Rust
 */

import * as net from 'net';
import * as fs from 'fs';
import { Commands } from '../../../system/core/shared/Commands';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';

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
   */
  private async handleRequest(socket: net.Socket, line: string): Promise<void> {
    try {
      const request: CommandRequest = JSON.parse(line);
      this.log.info(`Executing command from Rust: ${request.command}`);

      // Execute command via Commands.execute()
      // Type assertion needed since we don't know command type at runtime
      const result = await Commands.execute(request.command, request.params as Record<string, unknown>);

      const response: CommandResponse = {
        success: true,
        result
      };

      // Send response back to Rust
      socket.write(JSON.stringify(response) + '\n');
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
