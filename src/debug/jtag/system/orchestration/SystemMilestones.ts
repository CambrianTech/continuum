/**
 * System Milestones - Well-defined constants for orchestration events
 * 
 * These milestones represent specific, measurable system states that can be 
 * detected and waited for. Each milestone has clear completion criteria and
 * emits events when reached.
 */

import { EventEmitter } from 'events';
import type { EventPriority, EventScope } from '../events/shared/EventSystemConstants';

/**
 * System milestone constants - single source of truth for all orchestration
 */
export const SYSTEM_MILESTONES = {
  // Build Phase Milestones
  BUILD_START: 'build_start',
  BUILD_TYPESCRIPT_COMPLETE: 'build_typescript_complete',
  BUILD_STRUCTURE_COMPLETE: 'build_structure_complete', 
  BUILD_COMPLETE: 'build_complete',
  
  // Deployment Phase Milestones
  DEPLOY_START: 'deploy_start',
  DEPLOY_FILES_COMPLETE: 'deploy_files_complete',
  DEPLOY_PORTS_ALLOCATED: 'deploy_ports_allocated',
  DEPLOY_COMPLETE: 'deploy_complete',
  
  // Server Phase Milestones
  SERVER_START: 'server_start',
  SERVER_PROCESS_READY: 'server_process_ready',
  SERVER_WEBSOCKET_READY: 'server_websocket_ready',
  SERVER_HTTP_READY: 'server_http_ready', 
  SERVER_BOOTSTRAP_COMPLETE: 'server_bootstrap_complete',
  SERVER_COMMANDS_LOADED: 'server_commands_loaded',
  SERVER_READY: 'server_ready',
  
  // Browser Phase Milestones (CRITICAL - happens AFTER server ready)
  BROWSER_LAUNCH_INITIATED: 'browser_launch_initiated', 
  BROWSER_PROCESS_STARTED: 'browser_process_started',
  BROWSER_WEBSOCKET_CONNECTED: 'browser_websocket_connected',
  BROWSER_INTERFACE_LOADED: 'browser_interface_loaded',
  BROWSER_READY: 'browser_ready',
  
  // System Phase Milestones
  SYSTEM_HEALTHY: 'system_healthy',
  SYSTEM_READY: 'system_ready'
} as const;

/**
 * Milestone dependency graph - defines execution order
 * 
 * SIMPLIFIED for initial implementation - minimal viable dependencies
 */
export const MILESTONE_DEPENDENCIES: Record<SystemMilestone, readonly SystemMilestone[]> = {
  // Build Phase - simplified for initial implementation
  [SYSTEM_MILESTONES.BUILD_START]: [],
  [SYSTEM_MILESTONES.BUILD_TYPESCRIPT_COMPLETE]: [SYSTEM_MILESTONES.BUILD_START],
  [SYSTEM_MILESTONES.BUILD_STRUCTURE_COMPLETE]: [SYSTEM_MILESTONES.BUILD_START],
  [SYSTEM_MILESTONES.BUILD_COMPLETE]: [SYSTEM_MILESTONES.BUILD_START],

  // Deployment Phase - simplified
  [SYSTEM_MILESTONES.DEPLOY_START]: [],
  [SYSTEM_MILESTONES.DEPLOY_PORTS_ALLOCATED]: [],
  [SYSTEM_MILESTONES.DEPLOY_FILES_COMPLETE]: [],
  [SYSTEM_MILESTONES.DEPLOY_COMPLETE]: [],
  
  // Essential server startup sequence
  [SYSTEM_MILESTONES.SERVER_START]: [],
  [SYSTEM_MILESTONES.SERVER_PROCESS_READY]: [SYSTEM_MILESTONES.SERVER_START],
  [SYSTEM_MILESTONES.SERVER_WEBSOCKET_READY]: [SYSTEM_MILESTONES.SERVER_START],
  [SYSTEM_MILESTONES.SERVER_HTTP_READY]: [SYSTEM_MILESTONES.SERVER_START], 
  [SYSTEM_MILESTONES.SERVER_BOOTSTRAP_COMPLETE]: [SYSTEM_MILESTONES.SERVER_START],
  [SYSTEM_MILESTONES.SERVER_COMMANDS_LOADED]: [SYSTEM_MILESTONES.SERVER_START],
  [SYSTEM_MILESTONES.SERVER_READY]: [SYSTEM_MILESTONES.SERVER_START],
  
  // CRITICAL: Browser launch MUST wait for server ready
  [SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED]: [SYSTEM_MILESTONES.SERVER_READY],
  [SYSTEM_MILESTONES.BROWSER_PROCESS_STARTED]: [SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED],
  [SYSTEM_MILESTONES.BROWSER_WEBSOCKET_CONNECTED]: [SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED],
  [SYSTEM_MILESTONES.BROWSER_INTERFACE_LOADED]: [SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED],
  [SYSTEM_MILESTONES.BROWSER_READY]: [SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED],
  
  [SYSTEM_MILESTONES.SYSTEM_HEALTHY]: [SYSTEM_MILESTONES.SERVER_READY],
  [SYSTEM_MILESTONES.SYSTEM_READY]: [SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.BROWSER_READY]
};

/**
 * Entry point milestone requirements - what each entry point needs
 * 
 * SIMPLIFIED for initial implementation - only essential milestones
 */
export const ENTRY_POINT_REQUIREMENTS = {
  'npm-start': [SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.BROWSER_READY],
  'npm-test': [SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.BROWSER_READY], 
  'cli-command': [SYSTEM_MILESTONES.SERVER_READY], // Browser optional for CLI
  'single-test': [SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.BROWSER_READY],
  'git-hook': [SYSTEM_MILESTONES.SERVER_READY],  // Simplified - build happens implicitly
  'agent-command': [SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.BROWSER_READY],
  'system-start': [SYSTEM_MILESTONES.SERVER_READY, SYSTEM_MILESTONES.BROWSER_READY]
} as const;

/**
 * Milestone event data structure
 */
export interface MilestoneEvent {
  readonly milestone: SystemMilestone;
  readonly timestamp: number;
  readonly success: boolean;
  readonly error?: string;
  readonly metadata?: Record<string, any>;
  readonly entryPoint: string;
  readonly priority: EventPriority;
  readonly scope: EventScope;
}

/**
 * Milestone progress information
 */
export interface MilestoneProgress {
  readonly total: number;
  readonly completed: number;
  readonly current: string;
  readonly percentage: number;
  readonly estimatedTimeRemaining?: number;
  readonly completedMilestones: string[];
  readonly pendingMilestones: string[];
}

/**
 * Milestone completion criteria - how to detect when each milestone is reached
 */
export const MILESTONE_COMPLETION_CRITERIA = {
  [SYSTEM_MILESTONES.BUILD_TYPESCRIPT_COMPLETE]: {
    description: 'TypeScript compilation completed without errors',
    checkFunction: 'checkTypeScriptBuild',
    files: ['dist/**/*.js', 'dist/**/*.d.ts'],
    processes: [],
    ports: [],
    signals: []
  },
  
  [SYSTEM_MILESTONES.SERVER_PROCESS_READY]: {
    description: 'JTAG server process started and responding',
    checkFunction: 'checkServerProcess', 
    files: [],
    processes: ['JTAGSystemServer'],
    ports: [],
    signals: ['server_process_started']
  },
  
  [SYSTEM_MILESTONES.SERVER_WEBSOCKET_READY]: {
    description: 'WebSocket server accepting connections',
    checkFunction: 'checkWebSocketServer',
    files: [],
    processes: [],
    ports: ['websocket_server'],
    signals: ['websocket_listening']
  },
  
  [SYSTEM_MILESTONES.SERVER_HTTP_READY]: {
    description: 'HTTP server serving requests',
    checkFunction: 'checkHTTPServer',
    files: [],
    processes: [],
    ports: ['http_server'],
    signals: ['http_listening']
  },
  
  [SYSTEM_MILESTONES.SERVER_BOOTSTRAP_COMPLETE]: {
    description: 'Server bootstrap process completed',
    checkFunction: 'checkServerBootstrap',
    files: [],
    processes: [],
    ports: [],
    signals: ['bootstrap_complete']
  },
  
  [SYSTEM_MILESTONES.SERVER_COMMANDS_LOADED]: {
    description: 'All server commands loaded and registered',
    checkFunction: 'checkCommandsLoaded',
    files: [],
    processes: [],
    ports: [],
    signals: ['commands_loaded']
  },
  
  [SYSTEM_MILESTONES.SERVER_READY]: {
    description: 'Server fully ready to accept requests',
    checkFunction: 'checkServerReady',
    files: [],
    processes: [],
    ports: ['websocket_server', 'http_server'],
    signals: ['server_ready', 'system_healthy']
  },
  
  // Browser milestones - CRITICAL ORDERING
  [SYSTEM_MILESTONES.BROWSER_LAUNCH_INITIATED]: {
    description: 'Browser launch command executed',
    checkFunction: 'checkBrowserLaunch',
    files: [],
    processes: [],
    ports: [],
    signals: ['browser_launch_initiated']
  },
  
  [SYSTEM_MILESTONES.BROWSER_PROCESS_STARTED]: {
    description: 'Browser process running and window opened',
    checkFunction: 'checkBrowserProcess',
    files: [],
    processes: ['browser'],
    ports: [],
    signals: ['browser_process_started']
  },
  
  [SYSTEM_MILESTONES.BROWSER_WEBSOCKET_CONNECTED]: {
    description: 'Browser WebSocket connected to server',
    checkFunction: 'checkBrowserWebSocket',
    files: [],
    processes: [],
    ports: [],
    signals: ['browser_websocket_connected']
  },
  
  [SYSTEM_MILESTONES.BROWSER_INTERFACE_LOADED]: {
    description: 'Browser interface fully loaded and interactive',
    checkFunction: 'checkBrowserInterface',
    files: [],
    processes: [],
    ports: [],
    signals: ['browser_interface_loaded']
  },
  
  [SYSTEM_MILESTONES.BROWSER_READY]: {
    description: 'Browser fully ready for user interaction',
    checkFunction: 'checkBrowserReady',
    files: [],
    processes: [],
    ports: [],
    signals: ['browser_ready']
  },
  
  [SYSTEM_MILESTONES.SYSTEM_READY]: {
    description: 'Complete system ready for all operations',
    checkFunction: 'checkSystemReady',
    files: [],
    processes: [],
    ports: ['websocket_server', 'http_server'],
    signals: ['system_ready', 'system_healthy', 'server_ready', 'browser_ready']
  }
} as const;

/**
 * Type definitions
 *
 * SystemMilestone is the VALUE type (e.g., 'build_start', 'server_ready')
 * SystemMilestoneKey is the KEY type (e.g., 'BUILD_START', 'SERVER_READY')
 */
export type SystemMilestoneKey = keyof typeof SYSTEM_MILESTONES;
export type SystemMilestone = typeof SYSTEM_MILESTONES[SystemMilestoneKey];
export type EntryPointType = keyof typeof ENTRY_POINT_REQUIREMENTS;

/**
 * Milestone event emitter - central coordination point for milestone events
 */
export class MilestoneEventEmitter extends EventEmitter {
  private completedMilestones: Set<string> = new Set();
  private milestoneTimestamps: Map<string, number> = new Map();
  private milestoneErrors: Map<string, string> = new Map();
  
  /**
   * Mark a milestone as completed and emit event
   */
  async completeMilestone(
    milestone: SystemMilestone, 
    entryPoint: string, 
    metadata?: Record<string, any>
  ): Promise<void> {
    const timestamp = Date.now();
    this.completedMilestones.add(milestone);
    this.milestoneTimestamps.set(milestone, timestamp);
    
    const event: MilestoneEvent = {
      milestone,
      timestamp,
      success: true,
      metadata,
      entryPoint,
      priority: 'HIGH', // Milestone events are high priority
      scope: 'system'    // Milestone events are system-scoped
    };
    
    console.log(`✅ Milestone completed: ${milestone} (${entryPoint})`);
    this.emit('milestone-completed', event);
    this.emit(`milestone:${milestone}`, event);
  }
  
  /**
   * Mark a milestone as failed and emit event
   */
  async failMilestone(
    milestone: SystemMilestone, 
    entryPoint: string, 
    error: string,
    metadata?: Record<string, any>
  ): Promise<void> {
    const timestamp = Date.now();
    this.milestoneErrors.set(milestone, error);
    
    const event: MilestoneEvent = {
      milestone,
      timestamp,
      success: false,
      error,
      metadata,
      entryPoint,
      priority: 'CRITICAL', // Milestone failures are critical
      scope: 'system'
    };
    
    console.error(`❌ Milestone failed: ${milestone} (${entryPoint}): ${error}`);
    this.emit('milestone-failed', event);
    this.emit(`milestone:${milestone}`, event);
  }
  
  /**
   * Check if milestone is completed
   */
  isMilestoneCompleted(milestone: SystemMilestone): boolean {
    return this.completedMilestones.has(milestone);
  }
  
  /**
   * Get progress for a set of milestones
   */
  getProgress(requiredMilestones: SystemMilestone[]): MilestoneProgress {
    const completed = requiredMilestones.filter(m => this.completedMilestones.has(m));
    const pending = requiredMilestones.filter(m => !this.completedMilestones.has(m));
    
    return {
      total: requiredMilestones.length,
      completed: completed.length,
      current: pending[0] || 'complete',
      percentage: Math.round((completed.length / requiredMilestones.length) * 100),
      completedMilestones: completed,
      pendingMilestones: pending
    };
  }
  
  /**
   * Wait for a specific milestone to complete
   */
  async waitForMilestone(milestone: SystemMilestone, timeoutMs: number = 60000): Promise<boolean> {
    if (this.completedMilestones.has(milestone)) {
      return true;
    }
    
    return new Promise((resolve) => {
      const timeout = setTimeout(() => {
        this.removeAllListeners(`milestone:${milestone}`);
        resolve(false);
      }, timeoutMs);
      
      this.once(`milestone:${milestone}`, (event: MilestoneEvent) => {
        clearTimeout(timeout);
        resolve(event.success);
      });
    });
  }
  
  /**
   * Reset milestone state (for testing)
   */
  reset(): void {
    this.completedMilestones.clear();
    this.milestoneTimestamps.clear();
    this.milestoneErrors.clear();
    this.removeAllListeners();
  }
}

/**
 * Global milestone event emitter instance
 */
export const milestoneEmitter = new MilestoneEventEmitter();