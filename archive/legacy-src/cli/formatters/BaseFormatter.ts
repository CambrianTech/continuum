/**
 * Base class for CLI command result formatters
 */
export abstract class BaseFormatter {
  /**
   * Check if this formatter can handle the given result
   */
  abstract canHandle(result: any, command: string): boolean;

  /**
   * Format the result for user-friendly CLI output
   */
  abstract format(result: any): void;

  /**
   * Format file size in human-readable format
   */
  protected formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)}GB`;
  }

  /**
   * Format execution time in human-readable format
   */
  protected formatExecutionTime(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
    return `${(ms / 60000).toFixed(1)}m`;
  }

  /**
   * Get status icon and text
   */
  protected getStatusDisplay(success: boolean): { icon: string; text: string } {
    return {
      icon: success ? '✅' : '❌',
      text: success ? 'Success' : 'Failed'
    };
  }
}