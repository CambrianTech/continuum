/**
 * Process Orchestrator Interface
 * The master daemon that coordinates all other daemon processes
 * Runs in its own process and routes messages between daemons
 */

import { ProcessMessage, ProcessResult, ProcessHealth } from './IProcessCoordinator.js';
import { IProcessDaemon } from './IProcessDaemon.js';

export interface RouteTable {
  capability: string;
  processId: string;
  priority: number;
  lastUsed: number;
}

export interface ProcessRoute {
  processId: string;
  capabilities: string[];
  messageQueue: ProcessMessage[];
  lastHeartbeat: number;
  health: ProcessHealth;
}

/**
 * The orchestrator daemon - coordinates all other daemons
 * This IS a daemon itself, running in its own process
 */
export interface IProcessOrchestrator extends IProcessDaemon {
  // Daemon process management
  spawnDaemon(daemonType: string): Promise<string>;
  killDaemon(processId: string): Promise<void>;
  restartDaemon(processId: string): Promise<void>;
  
  // Message routing between daemons
  routeMessage(message: ProcessMessage): Promise<ProcessResult>;
  broadcastMessage(message: ProcessMessage): Promise<ProcessResult[]>;
  
  // Route management
  updateRoutes(): Promise<void>;
  getRouteTable(): Map<string, RouteTable[]>;
  findBestRoute(capability: string): string | null;
  
  // System coordination
  coordinateShutdown(): Promise<void>;
  coordinateStartup(): Promise<void>;
  monitorSystemHealth(): Promise<ProcessHealth[]>;
  
  // Load balancing
  balanceLoad(): Promise<void>;
  redistributeWork(overloadedProcess: string): Promise<void>;
}

/**
 * Router specific interface for message routing logic
 * The orchestrator delegates routing to this component
 */
export interface IProcessRouter {
  // Core routing logic
  route(message: ProcessMessage, routeTable: Map<string, RouteTable[]>): string | null;
  
  // Route optimization
  optimizeRoutes(routeTable: Map<string, RouteTable[]>): Map<string, RouteTable[]>;
  
  // Performance tracking
  recordRoutePerformance(processId: string, responseTime: number): void;
  getRouteMetrics(): Map<string, any>;
}