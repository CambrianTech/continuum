/**
 * System Signaling Types
 * 
 * Shared types and interfaces for the system readiness signaling system.
 * These types are used across browser, server, and test environments.
 */

export type SystemHealth = 'healthy' | 'degraded' | 'unhealthy' | 'error';
export type BuildStatus = 'success' | 'failed' | 'in-progress' | 'unknown';

export interface SystemReadySignal {
  readonly timestamp: string;
  readonly bootstrapComplete: boolean;
  readonly commandCount: number;
  readonly portsActive: readonly number[];
  readonly systemHealth: SystemHealth;
  readonly readySignalVersion: string;
  readonly errors: readonly string[];
  readonly startupLogs: string;
  readonly nodeErrors: readonly string[];
  readonly compilationStatus: BuildStatus;
  readonly browserReady: boolean;
  readonly buildStatus: BuildStatus;
  readonly autonomousGuidance: readonly string[];
  readonly generatorPid: number;        // PID of the process that generated this signal
  readonly consumerPids: readonly number[];  // PIDs of processes that have consumed this signal
}

// Configurable milestone system for flexible progress tracking
export interface SystemMilestone {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly required: boolean;  // Required for "healthy" vs optional for "degraded"
  readonly checkFn: (signal: SystemReadySignal) => boolean;
  readonly emoji: string;
}

// Milestone configuration - easily extensible for future requirements
export interface MilestoneConfig {
  readonly core: readonly SystemMilestone[];      // Core system milestones
  readonly performance?: readonly SystemMilestone[];  // Performance-related milestones
  readonly integration?: readonly SystemMilestone[];  // Integration milestones
  readonly custom?: readonly SystemMilestone[];   // Project-specific milestones
}

// Progress calculation results
export interface ProgressInfo {
  readonly completed: number;
  readonly total: number;
  readonly requiredCompleted: number;
  readonly requiredTotal: number;
  readonly details: readonly string[];
  readonly milestones: readonly { milestone: SystemMilestone; ready: boolean }[];
}

// Configuration for signal generation and checking
export interface SignalConfig {
  readonly VERSION: string;
  readonly EXPECTED_PORTS: readonly number[];
  readonly MIN_COMMAND_COUNT: number;
  readonly DEFAULT_TIMEOUT_MS: number;
}