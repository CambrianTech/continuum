/**
 * Test Run Suite Command Types
 * Replaces shell script-based test runners with configurable JTAG commands
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';

export interface TestRunSuiteParams extends CommandParams {
  /** Test profile to run (comprehensive, chat, integration, etc.) */
  profile?: string;

  /** Custom test pattern list (comma-separated) */
  tests?: string;

  /** Maximum execution time for entire suite (ms) */
  timeout?: number;

  /** Run tests in parallel */
  parallel?: boolean;

  /** Number of parallel workers (if parallel enabled) */
  parallelism?: number;

  /** Output format for results */
  format?: 'json' | 'text' | 'summary' | 'detailed';

  /** Show verbose output during execution */
  verbose?: boolean;

  /** Stop on first failure */
  failFast?: boolean;

  /** Save this configuration as a named profile */
  save?: boolean;

  /** Name for saved configuration */
  name?: string;
}

export interface TestRunSuiteResult extends CommandResult {
  success: boolean;
  profile: string;
  testsRun: number;
  testsPassed: number;
  testsFailed: number;
  totalExecutionTime: number;
  results: TestFileResult[];
  error?: string;
}

export interface TestFileResult {
  file: string;
  success: boolean;
  output: string;
  executionTime: number;
  error?: string;
}

export interface TestProfile {
  name: string;
  description: string;
  tests: string[];
  deployBrowser: boolean;
  parallelism: number;
  timeout: number;
  prerequisites?: string[];
}

// Predefined test profiles migrated from shell script system
export const DEFAULT_TEST_PROFILES: Record<string, TestProfile> = {
  comprehensive: {
    name: 'comprehensive',
    description: 'All tests including integration and UI',
    tests: ['**/*.test.ts'],
    deployBrowser: true,
    parallelism: 4,
    timeout: 300000
  },
  integration: {
    name: 'integration',
    description: 'Integration tests only',
    tests: ['tests/integration/**/*.test.ts'],
    deployBrowser: true,
    parallelism: 2,
    timeout: 120000
  },
  unit: {
    name: 'unit',
    description: 'Unit tests only',
    tests: ['tests/unit/**/*.test.ts'],
    deployBrowser: false,
    parallelism: 4,
    timeout: 60000
  },
  chat: {
    name: 'chat',
    description: 'Chat system tests only',
    tests: [
      'tests/chat-*.test.ts',
      'tests/integration/crud-db-widget.test.ts',
      'tests/chat-scenarios/**/*.test.ts'
    ],
    deployBrowser: true,
    parallelism: 2,
    timeout: 60000
  },
  screenshots: {
    name: 'screenshots',
    description: 'Screenshot tests only',
    tests: ['tests/screenshot/**/*.test.ts'],
    deployBrowser: true,
    parallelism: 1,
    timeout: 90000
  },
  themes: {
    name: 'themes',
    description: 'Theme system tests only',
    tests: ['tests/theme/**/*.test.ts'],
    deployBrowser: true,
    parallelism: 2,
    timeout: 30000
  },
  transport: {
    name: 'transport',
    description: 'Transport tests only',
    tests: ['tests/transport/**/*.test.ts'],
    deployBrowser: false,
    parallelism: 2,
    timeout: 45000
  },
  events: {
    name: 'events',
    description: 'Event system tests only',
    tests: ['tests/events/**/*.test.ts'],
    deployBrowser: true,
    parallelism: 2,
    timeout: 45000
  },
  performance: {
    name: 'performance',
    description: 'Grid P2P performance tests',
    tests: ['tests/performance/**/*.test.ts'],
    deployBrowser: true,
    parallelism: 1,
    timeout: 180000
  },
  blocker: {
    name: 'blocker',
    description: 'Blocker-level tests only',
    tests: ['tests/**/*blocker*.test.ts'],
    deployBrowser: true,
    parallelism: 1,
    timeout: 120000
  },
  critical: {
    name: 'critical',
    description: 'Critical tests only',
    tests: ['tests/**/*critical*.test.ts'],
    deployBrowser: true,
    parallelism: 2,
    timeout: 90000
  },
  precommit: {
    name: 'precommit',
    description: 'Git precommit hook - CRUD (100% required)',
    tests: [
      'tests/integration/crud-db-widget.test.ts'
    ],
    deployBrowser: true, // Required for CRUD widget validation
    parallelism: 1, // Sequential to ensure stability
    timeout: 60000 // Longer timeout for browser operations
  },
  precommitFast: {
    name: 'precommitFast',
    description: 'Fast precommit validation (basic checks only)',
    tests: [
      'tests/bootstrap-detection.test.ts',
      'tests/compiler-error-detection.test.ts'
    ],
    deployBrowser: false,
    parallelism: 1,
    timeout: 30000
  }
};

/**
 * Factory function for creating TestRunSuiteParams
 */
export const createTestRunSuiteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<TestRunSuiteParams, 'context' | 'sessionId'>
): TestRunSuiteParams => createPayload(context, sessionId, data);

/**
 * Factory function for creating TestRunSuiteResult from params
 */
export const createTestRunSuiteResult = (
  params: TestRunSuiteParams,
  differences: Omit<Partial<TestRunSuiteResult>, 'context' | 'sessionId'>
): TestRunSuiteResult => transformPayload(params, {
  success: false,
  profile: 'unknown',
  testsRun: 0,
  testsPassed: 0,
  testsFailed: 0,
  totalExecutionTime: 0,
  results: [],
  ...differences
});