// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Click Command - Shared Types for Element Interaction
 * 
 * Minimal types for clicking DOM elements. Follows screenshot/navigate pattern
 * with clean inheritance and Object.assign() initialization.
 * 
 * DESIGN ANALYSIS:
 * ✅ Focused on single action - clicking elements
 * ✅ Clean parameter interface with optional properties
 * ✅ Proper constructor pattern with Object.assign()
 * ✅ Result type includes success state and metadata
 * ✅ No over-engineering - just what's needed for clicks
 * 
 * SCOPE:
 * - Browser: Direct DOM element.click() calls
 * - Server: Delegates to browser context
 * - Consistent interface across contexts
 */

import { CommandParams, CommandResult } from '../../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';

export class ClickParams extends CommandParams {
  selector!: string;
  button?: 'left' | 'right' | 'middle';
  timeout?: number;

  constructor(data: Partial<ClickParams> = {}) {
    super();
    Object.assign(this, {
      selector: '',
      button: 'left',
      timeout: 30000,
      ...data
    });
  }
}

export class ClickResult extends CommandResult {
  success: boolean;
  selector: string;
  clicked: boolean;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<ClickResult>) {
    super();
    this.success = data.success ?? false;
    this.selector = data.selector ?? '';
    this.clicked = data.clicked ?? false;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}