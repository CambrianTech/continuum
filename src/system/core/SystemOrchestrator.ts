/**
 * JTAG System Orchestrator - Central coordination for all system operations
 * 
 * This replaces the scattered startup scripts with a single, robust system manager
 * that handles building, starting, monitoring, and cleanup consistently across
 * all entry points.
 */

import { spawn, ChildProcess } from 'child_process';
import fs from 'fs';
import path from 'path';

export interface SystemState {
  readonly isRunning: boolean;
  readonly health: 'healthy' | 'degraded' | 'unhealthy';
  readonly pid?: number;
  readonly ports: number[];
  readonly buildStatus: 'current' | 'needs_rebuild' | 'building' | 'failed';
  readonly errors: string[];
}

export interface SystemStartupOptions {
  readonly mode: 'development' | 'testing' | 'production';
  readonly persistent: boolean; // Use tmux or run directly?
  readonly captureOutput: 'stdout' | 'logs' | 'both';
  readonly buildIfNeeded: boolean;
  readonly timeout: number;
}

export interface SystemStartupResult {
  readonly success: boolean;
  readonly state: SystemState;
  readonly pid?: number;
  readonly logFile?: string;
  readonly errorMessage?: string;
}

/**
 * Central System Orchestrator
 * 
 * Handles all system lifecycle operations:
 * - Build management (when to rebuild, how to rebuild)
 * - Process management (tmux vs direct, cleanup)
 * - Output management (stdout vs logs vs both)
 * - Health monitoring (readiness, signals)
 * - Error handling (consistent across all entry points)
 */
export class SystemOrchestrator {
  
  /**
   * Get current system state without making any changes
   */
  async getSystemState(): Promise<SystemState> {
    // TODO: Check running processes, build status, health signals
    throw new Error('SystemOrchestrator.getSystemState() - Not implemented');
  }
  
  /**
   * Ensure system is running and ready for the given mode
   * 
   * This is the main entry point that all scripts should use.
   * It determines what actions are needed and executes them consistently.
   */
  async ensureSystemReady(options: SystemStartupOptions): Promise<SystemStartupResult> {
    try {
      console.log(`🎯 System Orchestrator: Ensuring system ready for ${options.mode} mode`);
      
      // 1. Check current state
      const currentState = await this.getSystemState();
      
      // 2. Determine required actions
      const actions = await this.planRequiredActions(currentState, options);
      
      // 3. Execute actions in order
      for (const action of actions) {
        await this.executeAction(action, options);
      }
      
      // 4. Verify final state
      const finalState = await this.getSystemState();
      
      return {
        success: finalState.health !== 'unhealthy',
        state: finalState,
        pid: finalState.pid
      };
      
    } catch (error) {
      return {
        success: false,
        state: await this.getSystemState(),
        errorMessage: error instanceof Error ? error.message : String(error)
      };
    }
  }
  
  /**
   * Determine what actions are needed based on current state and requirements
   */
  private async planRequiredActions(state: SystemState, options: SystemStartupOptions): Promise<string[]> {
    const actions: string[] = [];
    
    // Build logic
    if (options.buildIfNeeded && state.buildStatus === 'needs_rebuild') {
      actions.push('build');
    }
    
    // Process management logic
    if (!state.isRunning) {
      if (options.persistent) {
        actions.push('start_persistent');
      } else {
        actions.push('start_direct');
      }
    } else if (state.health === 'unhealthy') {
      actions.push('restart');
    }
    
    // Health check
    actions.push('wait_for_ready');
    
    return actions;
  }
  
  /**
   * Execute a single action with proper error handling and output management
   */
  private async executeAction(action: string, options: SystemStartupOptions): Promise<void> {
    console.log(`🔧 Executing action: ${action}`);
    
    switch (action) {
      case 'build':
        await this.executeBuild(options);
        break;
      case 'start_persistent':
        await this.startSystemPersistent(options);
        break;
      case 'start_direct':
        await this.startSystemDirect(options);
        break;
      case 'restart':
        await this.restartSystem(options);
        break;
      case 'wait_for_ready':
        await this.waitForSystemReady(options);
        break;
      default:
        throw new Error(`Unknown action: ${action}`);
    }
  }
  
  /**
   * Build system with unified build logic
   */
  private async executeBuild(options: SystemStartupOptions): Promise<void> {
    console.log('🔨 Building system...');
    // TODO: Centralized build logic from smart-build.ts
    throw new Error('SystemOrchestrator.executeBuild() - Not implemented');
  }
  
  /**
   * Start system in persistent mode (tmux)
   */
  private async startSystemPersistent(options: SystemStartupOptions): Promise<void> {
    console.log('🚀 Starting system in persistent mode...');
    // TODO: Tmux session management
    throw new Error('SystemOrchestrator.startSystemPersistent() - Not implemented');
  }
  
  /**
   * Start system in direct mode (no tmux)
   */
  private async startSystemDirect(options: SystemStartupOptions): Promise<void> {
    console.log('🚀 Starting system in direct mode...');
    // TODO: Direct process management
    throw new Error('SystemOrchestrator.startSystemDirect() - Not implemented');
  }
  
  /**
   * Restart system regardless of current state
   */
  private async restartSystem(options: SystemStartupOptions): Promise<void> {
    console.log('🔄 Restarting system...');
    // TODO: Cleanup + restart logic
    throw new Error('SystemOrchestrator.restartSystem() - Not implemented');
  }
  
  /**
   * Wait for system to be ready with unified readiness detection
   */
  private async waitForSystemReady(options: SystemStartupOptions): Promise<void> {
    console.log('⏳ Waiting for system ready...');
    // TODO: Unified readiness detection
    throw new Error('SystemOrchestrator.waitForSystemReady() - Not implemented');
  }
}

/**
 * Factory function for different entry point scenarios
 */
export class SystemOrchestration {
  
  /**
   * For npm start - Simple development startup
   */
  static async forDevelopment(): Promise<SystemStartupResult> {
    const orchestrator = new SystemOrchestrator();
    return orchestrator.ensureSystemReady({
      mode: 'development',
      persistent: false,  // No tmux for simple development
      captureOutput: 'both',  // See output AND capture logs
      buildIfNeeded: true,
      timeout: 30000
    });
  }
  
  /**
   * For npm test - Testing with persistent background system
   */
  static async forTesting(): Promise<SystemStartupResult> {
    const orchestrator = new SystemOrchestrator();
    return orchestrator.ensureSystemReady({
      mode: 'testing',
      persistent: true,   // Tmux for tests that need background system
      captureOutput: 'logs',  // Clean test output
      buildIfNeeded: true,
      timeout: 60000
    });
  }
  
  /**
   * For git hooks - Fast validation
   */
  static async forValidation(): Promise<SystemStartupResult> {
    const orchestrator = new SystemOrchestrator();
    return orchestrator.ensureSystemReady({
      mode: 'production',
      persistent: true,
      captureOutput: 'logs',
      buildIfNeeded: true,
      timeout: 45000
    });
  }
  
  /**
   * For CLI commands - Adaptive based on current state
   */
  static async forCLI(): Promise<SystemStartupResult> {
    const orchestrator = new SystemOrchestrator();
    
    // First check if system is already running
    const state = await orchestrator.getSystemState();
    
    if (state.isRunning && state.health === 'healthy') {
      // System already ready - just return state
      return {
        success: true,
        state: state,
        pid: state.pid
      };
    }
    
    // Need to start system for CLI
    return orchestrator.ensureSystemReady({
      mode: 'development',
      persistent: true,   // CLI commands expect persistent system
      captureOutput: 'stdout',  // User wants to see what's happening
      buildIfNeeded: true,
      timeout: 45000
    });
  }
}