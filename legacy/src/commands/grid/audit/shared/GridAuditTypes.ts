/**
 * Grid Audit Command - Shared Types
 *
 * Returns the audit trail of grid operations (sends, receives, ACL decisions).
 * Routes to Rust GridModule via continuum-core IPC.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface GridAuditParams extends CommandParams {
	limit?: number;
}

export interface GridAuditResult extends CommandResult {
	entries: unknown[];
}
