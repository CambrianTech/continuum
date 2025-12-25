/**
 * AI Providers Status - Check which API keys are configured
 *
 * Returns status for each provider WITHOUT exposing actual key values.
 * Safe to call from browser - only returns boolean status.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface AIProvidersStatusParams extends CommandParams {
  // No additional params needed - returns all provider statuses
}

export interface ProviderStatus {
  provider: string;
  key: string;
  isConfigured: boolean;
  category: 'local' | 'cloud';
  description: string;
  getKeyUrl?: string;
  billingUrl?: string;
  /** Masked key preview like "sk-...QfQA" - safe to display */
  maskedKey?: string;
}

export interface AIProvidersStatusResult extends CommandResult {
  providers: ProviderStatus[];
  configuredCount: number;
  totalCount: number;
}
