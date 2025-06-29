/**
 * Command Queue - Manages queued commands during system initialization
 */

import { EventEmitter } from 'events';

interface QueuedCommand {
  command: string;
  params: any;
  promise: {
    resolve: (result: any) => void;
    reject: (error: Error) => void;
  };
  queuedAt: Date;
}

export class CommandQueue extends EventEmitter {
  private queue: QueuedCommand[] = [];

  /**
   * Add a command to the queue and return a promise that resolves when processed
   */
  add(command: string, params: any): Promise<any> {
    return new Promise((resolve, reject) => {
      const queuedCommand: QueuedCommand = {
        command,
        params,
        promise: { resolve, reject },
        queuedAt: new Date()
      };
      
      this.queue.push(queuedCommand);
      console.log(`⏳ QUEUE: Added command to queue: ${command} (queue size: ${this.queue.length})`);
      
      this.emit('command-queued', { command, params, queueSize: this.queue.length });
    });
  }

  /**
   * Process all queued commands using the provided executor
   */
  async processAll(executor: (command: string, params: any) => Promise<any>): Promise<void> {
    if (this.queue.length === 0) {
      console.log('📝 QUEUE: No commands to process');
      return;
    }

    console.log(`🔄 QUEUE: Processing ${this.queue.length} queued commands...`);
    
    const commands = [...this.queue];
    this.queue = []; // Clear queue
    
    for (const queuedCommand of commands) {
      try {
        console.log(`⚡ QUEUE: Processing queued command: ${queuedCommand.command}`);
        
        const result = await executor(queuedCommand.command, queuedCommand.params);
        queuedCommand.promise.resolve(result);
        
        console.log(`✅ QUEUE: Completed queued command: ${queuedCommand.command}`);
        this.emit('command-processed', queuedCommand.command, result);
        
      } catch (error) {
        console.error(`❌ QUEUE: Failed queued command: ${queuedCommand.command} - ${error.message}`);
        queuedCommand.promise.reject(error);
        
        this.emit('command-failed', queuedCommand.command, error);
      }
    }
    
    console.log(`✅ QUEUE: Processed all queued commands`);
    this.emit('queue-processed', commands.length);
  }

  /**
   * Get current queue size
   */
  size(): number {
    return this.queue.length;
  }

  /**
   * Check if queue is empty
   */
  isEmpty(): boolean {
    return this.queue.length === 0;
  }

  /**
   * Clear all queued commands (reject their promises)
   */
  clear(): void {
    const commands = [...this.queue];
    this.queue = [];
    
    for (const queuedCommand of commands) {
      queuedCommand.promise.reject(new Error('Command queue cleared'));
    }
    
    console.log(`🧹 QUEUE: Cleared ${commands.length} queued commands`);
    this.emit('queue-cleared', commands.length);
  }

  /**
   * Get queue statistics
   */
  getStats(): {
    size: number;
    commands: string[];
    oldestCommand?: { command: string; queuedAt: Date };
  } {
    const commands = this.queue.map(cmd => cmd.command);
    const oldestCommand = this.queue.length > 0 ? {
      command: this.queue[0].command,
      queuedAt: this.queue[0].queuedAt
    } : undefined;
    
    return {
      size: this.queue.length,
      commands,
      oldestCommand
    };
  }
}