/**
 * Audit System Types
 *
 * Defines the core types for module auditing and fixing.
 */

export type ModuleType = 'command' | 'widget' | 'daemon';

export type IssueSeverity = 'error' | 'warning';

export type IssueCategory =
  | 'lint'
  | 'missing-file'
  | 'outdated-pattern'
  | 'package-json'
  | 'readme'
  | 'test-coverage'
  | 'hibernation-pollution';

export interface Issue {
  severity: IssueSeverity;
  category: IssueCategory;
  message: string;
  filePath?: string;
  lineNumber?: number;
  fixable: boolean;
  suggestedFix?: string;
}

export interface AuditReport {
  modulePath: string;
  moduleType: ModuleType;
  timestamp: number;
  issues: Issue[];
  summary: {
    errors: number;
    warnings: number;
    fixable: number;
  };
}

export interface AuditOptions {
  fix?: boolean;
  hibernateFailures?: boolean;
}

/**
 * Base interface for audit checks
 */
export interface IAuditCheck {
  readonly name: string;
  readonly category: IssueCategory;

  /**
   * Run the check and return issues found
   */
  check(modulePath: string, moduleType: ModuleType): Promise<Issue[]>;

  /**
   * Fix the issues (if fixable)
   */
  fix(modulePath: string, issues: Issue[]): Promise<void>;
}
