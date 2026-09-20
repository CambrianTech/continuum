/**
 * AI Adapter Self-Diagnostic Command
 * ===================================
 *
 * Tests adapter capabilities to validate infrastructure before training.
 * Each adapter can self-report what it supports and run diagnostic tests.
 *
 * ASYNC PATTERN: Command returns testId immediately, tests run in background.
 *
 * To check status/results:
 *   data/read --collection="test_executions" --id="<testId>"
 *
 * Status values: queued → running → completed (or failed)
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import type { ModelCapability } from '../../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface AdapterTestParams extends CommandParams {
  /** Which adapter to test (e.g., 'candle', 'openai', 'anthropic') */
  adapter?: string;

  /** Specific model to test with (optional) */
  model?: string;

  /** Test specific capability only */
  capability?: ModelCapability;

  /** Run full diagnostic suite (slower but comprehensive) */
  full?: boolean;

  /** Test all registered adapters */
  all?: boolean;
}

/**
 * Async test execution handle - returned immediately
 */
export interface AsyncTestResult extends CommandResult {
  testId: UUID;
  status: 'queued';
  message: string;
}

export interface CapabilityTestResult {
  capability: ModelCapability;
  supported: boolean;
  tested: boolean;
  success?: boolean;
  responseTimeMs?: number;
  error?: string;
  details?: unknown;
}

export interface AdapterTestResult extends CommandResult {
  adapter: string;
  available: boolean;
  healthy: boolean;

  /** Capabilities this adapter claims to support */
  declaredCapabilities: ModelCapability[];

  /** Results of capability tests */
  testResults: CapabilityTestResult[];

  /** Available models for this adapter */
  models?: string[];

  /** Overall test summary */
  summary: {
    total: number;
    passed: number;
    failed: number;
    skipped: number;
  };

  /** Performance metrics */
  performance?: {
    healthCheckTime: number;
    totalTestTime: number;
  };
}

export interface AllAdaptersTestResult extends CommandResult {
  adapters: AdapterTestResult[];
  summary: {
    totalAdapters: number;
    healthyAdapters: number;
    unhealthyAdapters: number;
  };
}

/**
 * Diagnostic test suite for each capability
 */
export interface CapabilityTest {
  capability: ModelCapability;

  /** Human-readable description */
  description: string;

  /** Quick smoke test (< 5 seconds) */
  smokeTest: (adapter: unknown) => Promise<boolean>;

  /** Full validation test (can be slower) */
  fullTest?: (adapter: unknown) => Promise<{
    success: boolean;
    details: unknown;
  }>;
}

/**
 * AdapterTest — Type-safe command executor
 *
 * Usage:
 *   import { AdapterTest } from '...shared/AdapterTestTypes';
 *   const result = await AdapterTest.execute({ ... });
 */
export const AdapterTest = {
  execute(params: CommandInput<AdapterTestParams>): Promise<AdapterTestResult> {
    return Commands.execute<AdapterTestParams, AdapterTestResult>('ai/adapter/test', params as Partial<AdapterTestParams>);
  },
  commandName: 'ai/adapter/test' as const,
} as const;
