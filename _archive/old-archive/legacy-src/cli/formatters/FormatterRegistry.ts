/**
 * Registry for CLI command result formatters
 */
import { BaseFormatter } from './BaseFormatter';
import { ScreenshotFormatter } from './ScreenshotFormatter';

export class FormatterRegistry {
  private formatters: BaseFormatter[] = [];

  constructor() {
    this.registerDefaultFormatters();
  }

  /**
   * Register default formatters
   */
  private registerDefaultFormatters(): void {
    this.register(new ScreenshotFormatter());
  }

  /**
   * Register a new formatter
   */
  register(formatter: BaseFormatter): void {
    this.formatters.push(formatter);
  }

  /**
   * Find and use appropriate formatter for the result
   */
  format(result: any, command: string): boolean {
    const formatter = this.formatters.find(f => f.canHandle(result, command));
    
    if (formatter) {
      formatter.format(result);
      return true;
    }
    
    return false;
  }

  /**
   * Get list of registered formatters
   */
  getFormatters(): BaseFormatter[] {
    return [...this.formatters];
  }
}