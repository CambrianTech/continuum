import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';

export interface GitIssueCreateParams extends CommandParams {
  readonly title: string;
  readonly body: string;
  readonly labels?: string[] | string;  // Array or comma-separated string (CLI passes string)
  readonly assignee?: string;  // GitHub username
  readonly milestone?: number; // Milestone number
  readonly repo?: string;      // Defaults to current repo
}

export interface GitIssueCreateResult extends CommandResult {
  readonly success: boolean;
  readonly issue?: {
    readonly number: number;
    readonly url: string;
    readonly title: string;
    readonly state: string;
    readonly labels: string[];
  };
  readonly error?: string;
}
