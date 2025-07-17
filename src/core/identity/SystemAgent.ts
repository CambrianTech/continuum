// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * System Agent - Specialized identity for system-level operations
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: System-specific message handling and operations
 * - Integration tests: System + daemon interaction
 * - Security tests: System permission enforcement
 * 
 * ARCHITECTURAL INSIGHTS:
 * - 90% shared logic from UniversalIdentity foundation
 * - 10% system-specific implementation (this class)
 * - Separation of burden: Foundation handles infrastructure, this handles system specifics
 * - Modular design: System behavior isolated from other identity types
 * 
 * SYSTEM-SPECIFIC FEATURES:
 * - Daemon integration and system operations
 * - Administrative capabilities
 * - System monitoring and health checks
 * - Resource management
 * - Security enforcement
 */

import { UniversalIdentity, BaseMetadata, BaseCapabilities } from './UniversalIdentity';

/**
 * System-specific metadata - extends base with system-relevant properties
 */
export interface SystemMetadata extends BaseMetadata {
  systemLevel?: boolean;
  permissions?: string[];
  processCount?: number;
  uptime?: number;
}

/**
 * System-specific capabilities - extends base with system-relevant capabilities
 */
export interface SystemCapabilities extends BaseCapabilities {
  // Limited capabilities for system agents
  receiveMessages: boolean;
  sendMessages: boolean;
  execute: boolean;
  monitor: boolean;
  cleanup: boolean;
}

/**
 * System Agent - Specialized for system-level operations
 * Generic types follow inheritance naturally - no type overrides needed
 */
export class SystemAgent extends UniversalIdentity<SystemMetadata, SystemCapabilities> {
  private systemProcesses: Map<string, any> = new Map();
  private permissions: Set<string> = new Set();
  
  constructor(config: {
    id?: string;
    name: string;
    permissions?: string[];
    metadata?: Partial<SystemMetadata>;
  }) {
    // System-specific capabilities - simple assignment
    const systemCapabilities: SystemCapabilities = {
      communicate: true,
      serialize: true,
      receiveMessages: true,
      sendMessages: false,
      execute: true,
      monitor: true,
      cleanup: true
    };
    
    // System-specific metadata - simple assignment
    const systemMetadata: SystemMetadata = {
      description: config.metadata?.description || 'System agent for operations',
      isActive: true,
      systemLevel: true,
      permissions: config.permissions || [],
      processCount: 0,
      uptime: 0,
      ...config.metadata
    };
    
    super({
      ...(config.id && { id: config.id }),
      name: config.name,
      type: 'system',
      capabilities: systemCapabilities,
      metadata: systemMetadata
    });
    
    // Set permissions
    if (config.permissions) {
      config.permissions.forEach(p => this.permissions.add(p));
    }
    
    this.logMessage('⚙️ System agent created');
  }
  
  // ==================== SYSTEM-SPECIFIC IMPLEMENTATIONS ====================
  
  /**
   * Handle message - System-specific message processing
   */
  async handleMessage(message: any): Promise<any> {
    if (!this.hasCapability('receiveMessages')) {
      this.logMessage('⚠️ Cannot receive messages, ignoring');
      return;
    }
    
    this.logMessage(`📨 System received message: ${message.type}`);
    
    // System messages are typically administrative
    if (message.type === 'system_command') {
      return await this.handleSystemCommand(message);
    }
    
    if (message.type === 'health_check') {
      return await this.handleHealthCheck(message);
    }
    
    if (message.type === 'resource_query') {
      return await this.handleResourceQuery(message);
    }
    
    // Record system message interaction
    this.recordEvent('system_message_received', {
      success: true,
      messageType: message.type,
      fromId: message.senderId
    });
    
    return {
      type: 'system_response',
      content: `System processed: ${message.type}`,
      systemId: this.id
    };
  }
  
  /**
   * Initialize system-specific functionality
   */
  async initializeSpecific(): Promise<void> {
    this.logMessage('🚀 Initializing system agent');
    
    // Initialize system processes
    await this.initializeSystemProcesses();
    
    // Start system monitoring
    await this.startSystemMonitoring();
    
    this.recordEvent('system_initialized', { 
      success: true, 
      permissions: Array.from(this.permissions),
      processCount: this.systemProcesses.size
    });
    
    this.logMessage('✅ System agent initialized');
  }
  
  /**
   * Cleanup system-specific resources
   */
  async destroySpecific(): Promise<void> {
    this.logMessage('🛑 Destroying system agent');
    
    // Stop system monitoring
    await this.stopSystemMonitoring();
    
    // Cleanup system processes
    await this.cleanupSystemProcesses();
    
    this.recordEvent('system_destroyed', { success: true });
    this.logMessage('✅ System agent destroyed');
  }
  
  // ==================== SYSTEM-SPECIFIC METHODS ====================
  
  /**
   * Handle system command
   */
  private async handleSystemCommand(message: any): Promise<any> {
    const command = message.command;
    
    if (!this.hasPermission(command)) {
      return {
        type: 'system_error',
        error: 'Insufficient permissions',
        command,
        systemId: this.id
      };
    }
    
    this.logMessage(`⚙️ Executing system command: ${command}`);
    
    // Execute command based on type
    switch (command) {
      case 'status':
        return await this.getSystemStatus();
      case 'restart':
        return await this.restartSystem();
      case 'cleanup':
        return await this.cleanupSystem();
      default:
        return {
          type: 'system_error',
          error: 'Unknown command',
          command,
          systemId: this.id
        };
    }
  }
  
  /**
   * Handle health check
   */
  private async handleHealthCheck(message: any): Promise<any> {
    this.logMessage('🔍 Performing health check');
    
    const health = {
      status: 'healthy',
      processes: this.systemProcesses.size,
      uptime: Date.now() - this.created,
      memory: process.memoryUsage ? process.memoryUsage() : 'unavailable',
      permissions: Array.from(this.permissions)
    };
    
    this.recordEvent('health_check_performed', {
      success: true,
      health,
      requestedBy: message.senderId
    });
    
    return {
      type: 'health_check_response',
      health,
      systemId: this.id
    };
  }
  
  /**
   * Handle resource query
   */
  private async handleResourceQuery(_message: any): Promise<any> {
    this.logMessage('📊 Handling resource query');
    
    const resources = {
      activeProcesses: this.systemProcesses.size,
      systemLoad: 'normal', // Would be actual system load
      availableMemory: 'sufficient', // Would be actual memory info
      diskSpace: 'adequate' // Would be actual disk space
    };
    
    return {
      type: 'resource_response',
      resources,
      systemId: this.id
    };
  }
  
  /**
   * Check if system has permission
   */
  private hasPermission(permission: string): boolean {
    return this.permissions.has(permission) || this.permissions.has('admin');
  }
  
  /**
   * Add permission
   */
  addPermission(permission: string): void {
    this.permissions.add(permission);
    this.recordEvent('permission_added', {
      success: true,
      permission
    });
  }
  
  /**
   * Remove permission
   */
  removePermission(permission: string): void {
    this.permissions.delete(permission);
    this.recordEvent('permission_removed', {
      success: true,
      permission
    });
  }
  
  /**
   * Get system status
   */
  private async getSystemStatus(): Promise<any> {
    return {
      type: 'system_status',
      status: {
        state: this.getState(),
        processes: this.systemProcesses.size,
        uptime: Date.now() - this.created,
        permissions: Array.from(this.permissions)
      },
      systemId: this.id
    };
  }
  
  /**
   * Restart system
   */
  private async restartSystem(): Promise<any> {
    this.logMessage('🔄 Restarting system');
    
    // Would trigger actual system restart
    this.recordEvent('system_restart_requested', { success: true });
    
    return {
      type: 'system_restart_response',
      message: 'System restart initiated',
      systemId: this.id
    };
  }
  
  /**
   * Cleanup system
   */
  private async cleanupSystem(): Promise<any> {
    this.logMessage('🧹 Cleaning up system');
    
    // Cleanup temporary files, logs, etc.
    await this.cleanupSystemProcesses();
    
    this.recordEvent('system_cleanup_performed', { success: true });
    
    return {
      type: 'system_cleanup_response',
      message: 'System cleanup completed',
      systemId: this.id
    };
  }
  
  // ==================== PRIVATE METHODS ====================
  
  /**
   * Initialize system processes
   */
  private async initializeSystemProcesses(): Promise<void> {
    this.logMessage('🔧 Initializing system processes');
    
    // Initialize monitoring processes
    this.systemProcesses.set('monitor', {
      name: 'System Monitor',
      status: 'active',
      started: Date.now()
    });
    
    // Initialize cleanup processes
    this.systemProcesses.set('cleanup', {
      name: 'System Cleanup',
      status: 'active',
      started: Date.now()
    });
  }
  
  /**
   * Start system monitoring
   */
  private async startSystemMonitoring(): Promise<void> {
    this.logMessage('👁️ Starting system monitoring');
    
    // Would start actual monitoring
    this.recordEvent('monitoring_started', { success: true });
  }
  
  /**
   * Stop system monitoring
   */
  private async stopSystemMonitoring(): Promise<void> {
    this.logMessage('👁️ Stopping system monitoring');
    
    // Would stop actual monitoring
    this.recordEvent('monitoring_stopped', { success: true });
  }
  
  /**
   * Cleanup system processes
   */
  private async cleanupSystemProcesses(): Promise<void> {
    this.logMessage('🧹 Cleaning up system processes');
    
    this.systemProcesses.clear();
    this.recordEvent('processes_cleaned', { success: true });
  }
}

/**
 * Factory function for creating system agents
 */
export function createSystemAgent(config: {
  id?: string;
  name: string;
  permissions?: string[];
  metadata?: Partial<SystemMetadata>;
}): SystemAgent {
  return new SystemAgent(config);
}