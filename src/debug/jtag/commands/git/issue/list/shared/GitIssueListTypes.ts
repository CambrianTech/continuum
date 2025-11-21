import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';

export interface GitIssueListParams extends CommandParams {
  readonly state?: 'open' | 'closed' | 'all';  // Default: 'open'
  readonly label?: string;      // Filter by label
  readonly assignee?: string;   // Filter by assignee
  readonly limit?: number;      // Default: 30
  readonly repo?: string;       // Defaults to current repo
}

export interface GitIssue {
  readonly number: number;
  readonly title: string;
  readonly state: string;
  readonly url: string;
  readonly labels: string[];
  readonly assignee: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface GitIssueListResult extends CommandResult {
  readonly success: boolean;
  readonly issues?: GitIssue[];
  readonly count?: number;
  readonly error?: string;
}
