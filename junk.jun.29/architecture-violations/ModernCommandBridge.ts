/**
 * Modern Command Bridge - Routes TypeScript commands through proper daemon architecture
 * Bridges legacy CommandRegistry with modern CommandProcessorDaemon
 */

import { BaseCommand, CommandContext } from './BaseCommand';

export interface ModernCommandRequest<T = any> {
  command: string;
  params: T;
  context?: CommandContext;
}

export interface ModernCommandResponse<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  message: string;
}

/**
 * Modern Command Bridge - Ensures TypeScript commands get proper context
 * This bridges the gap between legacy CommandRegistry and modern daemon system
 */
export class ModernCommandBridge {
  private static continuonStatus: any = null;
  private static systemContext: CommandContext = {};

  /**
   * Initialize bridge with system components
   */
  static initialize(continuum: any) {
    if (continuum?.continuonStatus) {
      this.continuonStatus = continuum.continuonStatus;
    }
    
    // Create comprehensive context for TypeScript commands
    this.systemContext = {
      continuum,
      continuonStatus: this.continuonStatus,
      timestamp: new Date(),
      sessionId: `bridge_${Date.now()}_${Math.random().toString(36).substr(2, 8)}`
    };

    console.log('🌉 ModernCommandBridge: Initialized with proper context');
    console.log(`   ✅ ContinuonStatus: ${!!this.continuonStatus}`);
    console.log(`   ✅ Continuum: ${!!continuum}`);
    console.log(`   🆔 Session: ${this.systemContext.sessionId}`);
  }

  /**
   * Execute TypeScript command with proper context
   */
  static async executeCommand<T, R>(
    CommandClass: typeof BaseCommand,
    params: T,
    continuum?: any
  ): Promise<ModernCommandResponse<R>> {
    try {
      // Ensure bridge is initialized
      if (!this.systemContext.continuum && continuum) {
        this.initialize(continuum);
      }

      // Create enriched context for command execution
      const commandContext: CommandContext = {
        ...this.systemContext,
        continuum: continuum || this.systemContext.continuum,
        continuonStatus: this.continuonStatus,
        executionId: `cmd_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
        timestamp: new Date()
      };

      console.log(`🌉 ModernCommandBridge: Executing ${CommandClass.name} with context`);
      console.log(`   🆔 ExecutionId: ${commandContext.executionId}`);
      console.log(`   ✅ Context: ${Object.keys(commandContext).join(', ')}`);

      // Execute command with proper context
      const result = await CommandClass.execute(params, commandContext);

      console.log(`✅ ModernCommandBridge: ${CommandClass.name} completed successfully`);
      return {
        success: result.success,
        data: result.data,
        message: result.message
      };

    } catch (error) {
      console.error(`❌ ModernCommandBridge: ${CommandClass.name} failed:`, error);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        message: `Command execution failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Get current system context
   */
  static getContext(): CommandContext {
    return { ...this.systemContext };
  }

  /**
   * Update system context (for testing or runtime updates)
   */
  static updateContext(updates: Partial<CommandContext>) {
    this.systemContext = { ...this.systemContext, ...updates };
    console.log(`🌉 ModernCommandBridge: Context updated with keys: ${Object.keys(updates).join(', ')}`);
  }
}

export default ModernCommandBridge;