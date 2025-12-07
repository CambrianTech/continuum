/**
 * Generate/Audit Browser Command
 *
 * Browser-side implementation (primarily server-side operation)
 */

import type { GenerateAuditParams, GenerateAuditResult } from '../shared/GenerateAuditTypes';
import { createGenerateAuditResultFromParams } from '../shared/GenerateAuditTypes';

export class GenerateAuditBrowserCommand {
  /**
   * Execute audit command from browser
   * (Delegates to server)
   */
  static async execute(params: GenerateAuditParams): Promise<GenerateAuditResult> {
    // Audit is a server-side operation
    // This will be handled by the command routing system
    return createGenerateAuditResultFromParams(params, {
      success: false,
      error: 'Audit must be run from server (use ./jtag generate/audit)',
    });
  }
}
