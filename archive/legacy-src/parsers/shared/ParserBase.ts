/**
 * Base Parser - Abstract foundation for all Continuum parsers
 * 
 * Defines the contract for parsing input/output across all integrations:
 * - CLI parsers for command-line interface
 * - MCP parsers for Model Context Protocol
 * - WebSocket parsers for browser communication
 * - REST API parsers for HTTP endpoints
 * 
 * All parsers extend this base to ensure consistent interface across
 * client, server, and shared environments.
 */

import { ValidationResult } from '../../types/ValidationTypes';

export abstract class ParserBase<TInput = any, TOutput = any> {
  /**
   * Parse input from integration-specific format to universal Continuum types
   * @param input - Raw input in integration format (CLI args, MCP message, etc.)
   * @returns Parsed parameters in universal format
   */
  abstract parseInput(input: TInput): Record<string, any>;

  /**
   * Format output from universal Continuum types to integration-specific format
   * @param output - Command result in universal format
   * @param command - Command name for context
   * @returns Formatted output ready for integration display
   */
  abstract formatOutput(output: TOutput, command: string): void;

  /**
   * Validate input parameters before processing
   * @param input - Input to validate
   * @returns Validation result with success/error details
   */
  abstract validateInput(input: TInput): ValidationResult;

  /**
   * Validate output before formatting
   * @param output - Output to validate
   * @returns Validation result with success/error details
   */
  abstract validateOutput(output: TOutput): ValidationResult;

  /**
   * Get parser metadata for registration and discovery
   */
  abstract getParserInfo(): {
    name: string;
    version: string;
    description: string;
    supportedCommands: string[];
  };

  /**
   * Check if this parser can handle the given input format
   * @param input - Input to check
   * @returns True if parser can handle this input
   */
  abstract canHandle(input: TInput): boolean;

  /**
   * Priority for parser selection when multiple parsers can handle input
   * Higher numbers = higher priority
   */
  abstract getPriority(): number;

  /**
   * Common utility for safe JSON parsing
   */
  protected safeJsonParse(input: string): any {
    try {
      return JSON.parse(input);
    } catch (error) {
      return null;
    }
  }

  /**
   * Common utility for extracting command name from various input formats
   */
  protected extractCommandName(input: TInput): string | null {
    // Default implementation - parsers can override
    if (typeof input === 'object' && input !== null) {
      return (input as any).command || (input as any).commandName || null;
    }
    return null;
  }

  /**
   * Common utility for parameter validation
   */
  protected validateParameters(params: Record<string, any>, required: string[]): ValidationResult {
    const missing = required.filter(key => !(key in params) || params[key] === undefined);
    
    if (missing.length > 0) {
      return {
        success: false,
        error: `Missing required parameters: ${missing.join(', ')}`,
        details: { missingParameters: missing }
      };
    }

    return { success: true };
  }

  /**
   * Common utility for output formatting status display
   */
  protected getStatusDisplay(success: boolean): { icon: string; text: string } {
    return success 
      ? { icon: '✅', text: 'Success' }
      : { icon: '❌', text: 'Failed' };
  }

  /**
   * Common utility for formatting file sizes
   */
  protected formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)}GB`;
  }

  /**
   * Common utility for formatting execution time
   */
  protected formatExecutionTime(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
    return `${(ms / 60000).toFixed(1)}m`;
  }
}