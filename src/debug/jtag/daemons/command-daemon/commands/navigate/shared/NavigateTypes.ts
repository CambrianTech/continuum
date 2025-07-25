// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Navigate Command - Shared Types for Browser Navigation
 * 
 * Minimal, focused types for URL navigation across browser/server contexts.
 * Follows the elegant pattern of screenshot command - simple params and results
 * with clean inheritance from CommandParams/CommandResult base classes.
 * 
 * DESIGN PRINCIPLES:
 * - Object.assign() in constructor for clean initialization
 * - Optional properties with sensible defaults
 * - Environment-aware results with timestamps
 * - Type safety without overkill complexity
 * 
 * USAGE:
 * - Browser: Direct window.location navigation
 * - Server: Delegates to browser context
 * - Symmetric interface across both contexts
 */

import { CommandParams, CommandResult } from '../../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';

export class NavigateParams extends CommandParams {
  url!: string;
  timeout?: number;
  waitForSelector?: string;

  constructor(data: Partial<NavigateParams> = {}) {
    super();
    Object.assign(this, {
      url: '',
      timeout: 30000,
      waitForSelector: undefined,
      ...data
    });
  }
}

export class NavigateResult extends CommandResult {
  success: boolean;
  url: string;
  title?: string;
  loadTime?: number;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<NavigateResult>) {
    super();
    this.success = data.success ?? false;
    this.url = data.url ?? '';
    this.title = data.title;
    this.loadTime = data.loadTime;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}