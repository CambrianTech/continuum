/**
 * Node.js Process Daemon Implementation
 * For daemons running in separate Node.js processes
 */

import { BaseProcessDaemon } from './BaseProcessDaemon.js';
import { INodeProcessDaemon } from '../interfaces/IProcessDaemon.js';
import { ProcessMessage } from '../interfaces/IProcessCoordinator.js';

export abstract class NodeProcessDaemon extends BaseProcessDaemon implements INodeProcessDaemon {
  private processListeners = new Map<string, (...args: any[]) => void>();

  constructor() {
    super();
    this.setupProcessListeners();
  }

  // Process communication
  onProcessMessage(message: any): void {
    if (message && message.type) {
      this.handleMessage(message as ProcessMessage).then(result => {
        // Send response back to coordinator
        if (message.responseId) {
          this.sendToParent({
            responseId: message.responseId,
            success: result.success,
            data: result.data,
            error: result.error,
            processId: result.processId
          });
        }
      }).catch(error => {
        this.log(`Process message handling error: ${error.message}`, 'error');
        if (message.responseId) {
          this.sendToParent({
            responseId: message.responseId,
            success: false,
            error: error.message,
            processId: this.processId
          });
        }
      });
    }
  }

  sendToParent(message: any): void {
    if (process.send) {
      process.send(message);
    } else {
      this.log('No parent process available for communication', 'warn');
    }
  }

  protected async sendToTarget(targetProcess: string, message: ProcessMessage): Promise<void> {
    // Send through parent coordinator
    this.sendToParent({
      type: 'route_message',
      targetProcess,
      message
    });
  }

  // Process management
  setupProcessListeners(): void {
    const messageHandler = this.onProcessMessage.bind(this);
    const exitHandler = this.gracefulShutdown.bind(this);
    const errorHandler = (error: Error) => {
      this.log(`Process error: ${error.message}`, 'error');
    };

    process.on('message', messageHandler);
    process.on('SIGINT', exitHandler);
    process.on('SIGTERM', exitHandler);
    process.on('uncaughtException', errorHandler);
    process.on('unhandledRejection', errorHandler);

    this.processListeners.set('message', messageHandler);
    this.processListeners.set('SIGINT', exitHandler);
    this.processListeners.set('SIGTERM', exitHandler);
    this.processListeners.set('uncaughtException', errorHandler);
    this.processListeners.set('unhandledRejection', errorHandler);

    this.log('Process listeners configured');
  }

  cleanupProcessListeners(): void {
    for (const [event, listener] of this.processListeners) {
      process.off(event as any, listener);
    }
    this.processListeners.clear();
    this.log('Process listeners cleaned up');
  }

  async gracefulShutdown(): Promise<void> {
    this.log('Received shutdown signal, gracefully shutting down...');
    
    try {
      await this.stop();
      this.cleanupProcessListeners();
      
      // Give a moment for cleanup
      setTimeout(() => {
        process.exit(0);
      }, 1000);
      
    } catch (error) {
      this.log(`Shutdown error: ${error.message}`, 'error');
      process.exit(1);
    }
  }

  // Override stop to cleanup listeners
  async stop(): Promise<void> {
    await super.stop();
    this.cleanupProcessListeners();
  }
}