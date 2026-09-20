/**
 * Universal Test Summary Types
 * 
 * Structured data interface for AI agents, personas, UI components, and CLI consumers.
 * Transforms test failures from human-only text into universal structured intelligence.
 */

export interface TestFailure {
  name: string;
  error: string;
  category: 'module-resolution' | 'database' | 'event-system' | 'network' | 'timeout' | 'unknown';
  testType: 'unit' | 'integration' | 'e2e';
  environment: 'browser' | 'server' | 'cross-context';
  severity: 'critical' | 'major' | 'minor';
  logPath?: string;
  stackTrace?: string;
  suggestedFix?: string;
}

export interface TestSummary {
  totalTests: number;
  passedTests: number;
  failedTests: number;
  duration: number;
  timestamp: string;
  testSuite: string;
  failures: TestFailure[];
  categories: {
    testTypes: Record<string, number>;
    environments: Record<string, number>;
    rootCauses: Record<string, number>;
    severity: Record<string, number>;
  };
  guidance: {
    actionItems: string[];
    debugCommands: string[];
    logPaths: string[];
    autoFixable: boolean;
  };
  machineReadable: {
    status: 'passed' | 'failed' | 'partial';
    criticalFailures: boolean;
    canProceed: boolean;
    blocksDeployment: boolean;
    aiActionable: boolean;
  };
}

export interface TestDisplayOptions {
  format: 'human' | 'json' | 'compact' | 'ai-friendly';
  showStackTraces: boolean;
  showGuidance: boolean;
  maxFailureDetail: number;
  colorOutput: boolean;
}

export interface TestMetrics {
  passRate: number;
  environmentHealth: Record<string, number>;
  categoryDistribution: Record<string, number>;
  trends: {
    improving: boolean;
    regression: boolean;
    newFailures: string[];
  };
}