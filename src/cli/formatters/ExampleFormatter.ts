/**
 * Example formatter for other commands
 * Shows how to add new formatters to the registry
 */
import { BaseFormatter } from './BaseFormatter';

export class ExampleFormatter extends BaseFormatter {
  /**
   * Check if this formatter can handle the given result
   */
  canHandle(result: any, command: string): boolean {
    // Example: Handle any command that returns help information
    return command === 'example' && result.data && result.data.message;
  }

  /**
   * Format the result for user-friendly CLI output
   */
  format(result: any): void {
    const status = this.getStatusDisplay(result.success);
    
    console.log(`\n🔧 EXAMPLE COMMAND`);
    console.log(`${status.icon} Status: ${status.text}`);
    
    if (result.data && result.data.message) {
      console.log(`📝 Message: ${result.data.message}`);
    }
    
    if (result.executionTime) {
      console.log(`⏱️ Time: ${this.formatExecutionTime(result.executionTime)}`);
    }
    
    console.log(`\n💡 This is an example of modular CLI formatting\n`);
  }
}

// To register this formatter, add it to FormatterRegistry constructor:
// this.register(new ExampleFormatter());