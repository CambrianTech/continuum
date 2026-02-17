/**
 * ProviderStatusTester - Handles API key testing logic
 *
 * Separated from SettingsWidget for better maintainability.
 * Tests API keys via ai/key/test command and parses results.
 */

import { Commands } from '@system/core/shared/Commands';

import { AiKeyTest } from '../../../commands/ai/key/test/shared/AiKeyTestTypes';
export type TestStatus = 'idle' | 'testing' | 'operational' | 'invalid' | 'out-of-funds' | 'rate-limited' | 'error';

export interface ProviderTestResult {
  status: TestStatus;
  message?: string;
  responseTimeMs?: number;
}

export interface TestKeyParams {
  provider: string;
  key: string;
  /** If true, use stored key from config.env instead of provided key */
  useStored?: boolean;
}

export class ProviderStatusTester {
  private testResults: Map<string, ProviderTestResult> = new Map();
  private onUpdate: () => void;

  constructor(onUpdate: () => void) {
    this.onUpdate = onUpdate;
  }

  getResult(key: string): ProviderTestResult | undefined {
    return this.testResults.get(key);
  }

  clearResult(key: string): void {
    this.testResults.delete(key);
  }

  clearAll(): void {
    this.testResults.clear();
  }

  async testKey(params: TestKeyParams, configKey: string): Promise<ProviderTestResult> {
    this.testResults.set(configKey, { status: 'testing' });
    this.onUpdate();

    try {
      const result = await AiKeyTest.execute({
        provider: params.provider,
        key: params.key,
        useStored: params.useStored
      } as any) as any;

      const testResult: ProviderTestResult = result?.valid
        ? {
            status: 'operational',
            responseTimeMs: result.responseTimeMs,
            message: result.models?.length ? `${result.models.length} models available` : undefined
          }
        : {
            status: this.parseErrorStatus(result?.errorMessage || ''),
            responseTimeMs: result?.responseTimeMs,
            message: result?.errorMessage
          };

      this.testResults.set(configKey, testResult);
      this.onUpdate();
      return testResult;
    } catch (error) {
      const testResult: ProviderTestResult = {
        status: 'error',
        message: error instanceof Error ? error.message : 'Test failed'
      };
      this.testResults.set(configKey, testResult);
      this.onUpdate();
      return testResult;
    }
  }

  private parseErrorStatus(errorMessage: string): TestStatus {
    const lowerError = errorMessage.toLowerCase();

    if (lowerError.includes('insufficient') || lowerError.includes('billing') ||
        lowerError.includes('funds') || lowerError.includes('quota') ||
        lowerError.includes('credit') || lowerError.includes('payment')) {
      return 'out-of-funds';
    }

    if (lowerError.includes('rate') || lowerError.includes('limit') ||
        lowerError.includes('too many') || lowerError.includes('429')) {
      return 'rate-limited';
    }

    if (lowerError.includes('invalid') || lowerError.includes('unauthorized') ||
        lowerError.includes('authentication') || lowerError.includes('401') ||
        lowerError.includes('incorrect')) {
      return 'invalid';
    }

    return 'error';
  }

  /**
   * Check if all pending changes have been tested and are operational
   */
  validateChanges(pendingChanges: Map<string, string>): { valid: boolean; untested: string[]; failed: string[] } {
    const untested: string[] = [];
    const failed: string[] = [];

    for (const [key] of pendingChanges) {
      const result = this.testResults.get(key);
      if (!result || result.status === 'idle') {
        untested.push(key);
      } else if (result.status !== 'operational') {
        failed.push(key);
      }
    }

    return {
      valid: untested.length === 0 && failed.length === 0,
      untested,
      failed
    };
  }
}
