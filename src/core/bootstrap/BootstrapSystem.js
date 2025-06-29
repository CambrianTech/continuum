/**
 * Bootstrap System - Core OS bootstrap commands and initialization chain  
 * Simplified JavaScript version for testing the promise-based command queueing
 */

import { EventEmitter } from 'events';

class BootstrapSystem extends EventEmitter {
  constructor() {
    super();
    this.commandQueue = [];
    this.isSystemReady = false;
    this.moduleDiscoveryComplete = false;
    
    console.debug('🚀 BOOTSTRAP: System initialized');
  }

  async start() {
    console.debug('📋 BOOTSTRAP: Starting system initialization...');
    
    // Simulate module discovery delay
    setTimeout(() => {
      this.moduleDiscoveryComplete = true;
      this.isSystemReady = true;
      
      console.debug('✅ BOOTSTRAP: Module discovery complete - processing queued commands');
      this.emit('bootstrap-ready');
      
      // Process any queued commands
      this.processQueuedCommands();
    }, 100);
  }

  async executeCommand(command, params = {}) {
    console.debug(`📥 BOOTSTRAP: Received command: ${command}`);
    
    // Immediate commands (info, status) can execute right away
    if (this.isImmediateCommand(command)) {
      console.debug(`⚡ BOOTSTRAP: Executing immediate command: ${command}`);
      return this.executeImmediateCommand(command, params);
    }
    
    // Post-discovery commands (list, help) must wait for module discovery
    if (!this.moduleDiscoveryComplete) {
      console.debug(`⏳ BOOTSTRAP: Queueing command until modules discovered: ${command}`);
      
      return new Promise((resolve, reject) => {
        this.commandQueue.push({
          command,
          params, 
          resolve,
          reject,
          queuedAt: new Date()
        });
      });
    }
    
    // System is ready, execute immediately
    console.debug(`🔧 BOOTSTRAP: Executing command: ${command}`);
    return this.executePostDiscoveryCommand(command, params);
  }

  isImmediateCommand(command) {
    return ['info', 'status'].includes(command);
  }

  async executeImmediateCommand(command, params) {
    console.debug(`🏃 BOOTSTRAP: Processing immediate command: ${command}`);
    
    switch (command) {
      case 'info':
        return {
          success: true,
          data: {
            version: '0.2.2204',
            system: { platform: 'test' },
            server: { pid: process.pid },
            processedBy: 'bootstrap-command-processor'
          }
        };
      
      case 'status':
        return {
          success: true,
          data: {
            systemReady: this.isSystemReady,
            modulesDiscovered: this.moduleDiscoveryComplete,
            commandsLoaded: this.moduleDiscoveryComplete,
            daemonsReady: this.isSystemReady,
            discoveredCommandsCount: this.moduleDiscoveryComplete ? 8 : 0,
            processedBy: 'bootstrap-command-processor'
          }
        };
        
      default:
        throw new Error(`Unknown immediate command: ${command}`);
    }
  }

  async executePostDiscoveryCommand(command, params) {
    console.debug(`🔍 BOOTSTRAP: Processing post-discovery command: ${command}`);
    
    switch (command) {
      case 'list':
        const bootstrapCommands = ['info', 'status', 'list', 'help'];
        const discoveredCommands = ['screenshot', 'chat', 'workspace', 'preferences'];
        const allCommands = [...new Set([...bootstrapCommands, ...discoveredCommands])];
        
        return {
          success: true,
          data: {
            commands: allCommands.sort(),
            bootstrapCommands: bootstrapCommands.sort(),
            discoveredCommands: discoveredCommands.sort(), 
            totalCommands: allCommands.length,
            systemReady: true
          }
        };
      
      case 'help':
        return {
          success: true,
          data: {
            availableCommands: ['info', 'status', 'list', 'help', 'screenshot', 'chat'],
            systemState: {
              modulesDiscovered: true,
              commandsLoaded: true,
              totalCommands: 6
            },
            usage: 'Use "help <command>" for specific command help'
          }
        };
        
      default:
        throw new Error(`Unknown post-discovery command: ${command}`);
    }
  }

  async processQueuedCommands() {
    if (this.commandQueue.length === 0) {
      console.debug('📝 BOOTSTRAP: No queued commands to process');
      return;
    }

    console.debug(`🔄 BOOTSTRAP: Processing ${this.commandQueue.length} queued commands...`);
    
    const commands = [...this.commandQueue];
    this.commandQueue = [];
    
    for (const queuedCommand of commands) {
      try {
        console.debug(`⚡ BOOTSTRAP: Processing queued command: ${queuedCommand.command}`);
        
        const result = await this.executePostDiscoveryCommand(queuedCommand.command, queuedCommand.params);
        queuedCommand.resolve(result);
        
        console.debug(`✅ BOOTSTRAP: Completed queued command: ${queuedCommand.command}`);
      } catch (error) {
        console.debug(`❌ BOOTSTRAP: Failed queued command: ${queuedCommand.command} - ${error.message}`);
        queuedCommand.reject(error);
      }
    }
    
    console.debug(`✅ BOOTSTRAP: All queued commands processed`);
  }

  isReady() {
    return this.isSystemReady;
  }

  getSystemState() {
    return {
      phase: this.isSystemReady ? 'ready' : 'starting',
      systemReady: this.isSystemReady,
      modulesDiscovered: this.moduleDiscoveryComplete,
      queuedCommands: this.commandQueue.length,
      commandsAvailable: this.isSystemReady ? 
        ['info', 'status', 'list', 'help'] : 
        ['info', 'status']
    };
  }
}

export { BootstrapSystem };