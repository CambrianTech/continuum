/**
 * CLI Client Parser - Handles CLI input parsing and output formatting
 * 
 * This parser handles the client-side CLI integration:
 * - Parses CLI arguments into universal Continuum types
 * - Formats command results for CLI display
 * - Provides user-friendly output formatting
 */

import { ParserBase } from '../../../shared/ParserBase';
import { ValidationResult } from '../../../../types/ValidationTypes';
import { CLIInputFormat, CLIOutputFormat, CLIParserConfig, CLIFormattingOptions, DEFAULT_CLI_FORMATTING } from '../shared/CLIParserTypes';

export class CLIClientParser extends ParserBase<CLIInputFormat, CLIOutputFormat> {
  private formatting: CLIFormattingOptions;

  constructor(config: Partial<CLIParserConfig> = {}, formatting: Partial<CLIFormattingOptions> = {}) {
    super();
    // Config available for future use when needed
    config; // Acknowledge config parameter
    this.formatting = { ...DEFAULT_CLI_FORMATTING, ...formatting };
  }

  /**
   * Parse CLI arguments into universal parameters
   */
  parseInput(input: CLIInputFormat): Record<string, any> {
    const { args } = input;
    const params: Record<string, any> = {};

    for (let i = 0; i < args.length; i++) {
      const arg = args[i];
      
      // Handle --key=value format
      if (arg.startsWith('--') && arg.includes('=')) {
        const [key, ...valueParts] = arg.substring(2).split('=');
        params[key] = valueParts.join('=');
      }
      // Handle --key value format
      else if (arg.startsWith('--') && i + 1 < args.length && !args[i + 1].startsWith('--')) {
        const key = arg.substring(2);
        params[key] = args[i + 1];
        i++; // Skip next arg as it's the value
      }
      // Handle --flag format (boolean)
      else if (arg.startsWith('--')) {
        const key = arg.substring(2);
        params[key] = true;
      }
      // Handle positional arguments
      else {
        if (!params.args) params.args = [];
        params.args.push(arg);
      }
    }

    return params;
  }

  /**
   * Format command output for CLI display
   */
  formatOutput(output: CLIOutputFormat, command: string): void {
    // Handle different command types with appropriate formatting
    switch (command) {
      case 'screenshot':
        this.formatScreenshotOutput(output);
        break;
      case 'chat':
        this.formatChatOutput(output);
        break;
      case 'help':
        this.formatHelpOutput(output);
        break;
      default:
        this.formatGenericOutput(output, command);
    }
  }

  /**
   * Format screenshot command output
   */
  private formatScreenshotOutput(output: CLIOutputFormat): void {
    const { success, data } = output;
    const status = this.getStatusDisplay(success);

    console.log(`\n📸 SCREENSHOT CAPTURED`);
    console.log(`${status.icon} Status: ${status.text}`);

    if (success && data) {
      if (data.broadcastSent) {
        console.log(`📡 Broadcast sent to ${data.connectionCount} browser connection(s)`);
        console.log(`💾 Screenshot saved to session directory`);
        console.log(`📁 Check your screenshots folder for the captured image`);
      } else if (data.filename) {
        console.log(`📁 File: ${data.filename}`);
        if (data.filePath) console.log(`💾 Saved to: ${data.filePath}`);
        if (data.width && data.height) console.log(`📏 Dimensions: ${data.width}x${data.height}px`);
      }
    }

    if (output.executionTime && this.formatting.showExecutionTime) {
      console.log(`⏱️ Execution time: ${this.formatExecutionTime(output.executionTime)}`);
    }

    console.log(`\n🤖 AI-Friendly: Full JSON data available programmatically`);
  }

  /**
   * Format chat command output
   */
  private formatChatOutput(output: CLIOutputFormat): void {
    const { success, data } = output;
    
    if (success && data) {
      if (data.response) {
        console.log(data.response);
      } else if (data.message) {
        console.log(data.message);
      }
    }
  }

  /**
   * Format help command output
   */
  private formatHelpOutput(output: CLIOutputFormat): void {
    const { success, data } = output;
    
    if (success && data) {
      if (data.commands) {
        console.log('\n🌐 CONTINUUM COMMANDS\n');
        Object.entries(data.commands).forEach(([category, commands]: [string, any]) => {
          console.log(`📋 ${category.toUpperCase()}:`);
          (commands as string[]).forEach(cmd => {
            console.log(`  continuum ${cmd}`);
          });
          console.log('');
        });
      }
    }
  }

  /**
   * Format generic command output
   */
  private formatGenericOutput(output: CLIOutputFormat, command: string): void {
    const { success, data, error } = output;
    const status = this.getStatusDisplay(success);

    console.log(`\n⚡ ${command.toUpperCase()} RESULT`);
    console.log(`${status.icon} Status: ${status.text}`);

    if (success && data) {
      if (typeof data === 'string') {
        console.log(data);
      } else {
        console.log(JSON.stringify(data, null, 2));
      }
    }

    if (error) {
      console.error(`❌ Error: ${error}`);
    }

    if (output.executionTime && this.formatting.showExecutionTime) {
      console.log(`⏱️ Execution time: ${this.formatExecutionTime(output.executionTime)}`);
    }
  }

  /**
   * Validate CLI input format
   */
  validateInput(input: CLIInputFormat): ValidationResult {
    if (!input || typeof input !== 'object') {
      return { success: false, error: 'Invalid input format' };
    }

    if (!Array.isArray(input.args)) {
      return { success: false, error: 'Args must be an array' };
    }

    return { success: true };
  }

  /**
   * Validate CLI output format
   */
  validateOutput(output: CLIOutputFormat): ValidationResult {
    if (!output || typeof output !== 'object') {
      return { success: false, error: 'Invalid output format' };
    }

    if (typeof output.success !== 'boolean') {
      return { success: false, error: 'Success field must be boolean' };
    }

    return { success: true };
  }

  /**
   * Get parser information
   */
  getParserInfo() {
    return {
      name: 'CLI Client Parser',
      version: '1.0.0',
      description: 'Parses CLI arguments and formats output for command-line display',
      supportedCommands: ['*'] // Supports all commands
    };
  }

  /**
   * Check if parser can handle input
   */
  canHandle(input: CLIInputFormat): boolean {
    return input && typeof input === 'object' && Array.isArray(input.args);
  }

  /**
   * Parser priority for selection
   */
  getPriority(): number {
    return 100; // High priority for CLI integration
  }
}