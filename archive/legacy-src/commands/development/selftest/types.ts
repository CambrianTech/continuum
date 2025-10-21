/**
 * SelfTest Command Types
 * 
 * Type definitions for the system self-testing functionality
 */

export interface SelfTestResult {
  success: boolean;
  message: string;
  checks: HealthCheckResult[];
  timestamp: string;
  duration: number;
}

export interface HealthCheckResult {
  name: string;
  status: 'passed' | 'failed' | 'skipped';
  message: string;
  details?: Record<string, any>;
}

export interface HealthCheckOptions {
  verbose?: boolean;
  skipDaemonChecks?: boolean;
  skipWebSocketChecks?: boolean;
  skipWidgetChecks?: boolean;
  timeout?: number;
}

export interface SelfTestParams {
  verbose?: boolean;
  checks?: string[];
  timeout?: number;
}