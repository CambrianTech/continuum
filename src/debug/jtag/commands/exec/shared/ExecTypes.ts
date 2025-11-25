/**
 * ExecCommand Types - Universal Script Execution System
 * 
 * Defines types for JTAG's meta-command that enables AI agents to write
 * custom automation scripts with visual feedback and cross-context execution.
 */

import { type CommandParams, type CommandResult, type JTAGEnvironment, JTAG_ENVIRONMENTS } from '../../../system/core/types/JTAGTypes';

export interface ExecCommandParams extends CommandParams {
  code: CodeInput;                              // REQUIRED - discriminated union for all input types
  targetEnvironment?: JTAGEnvironment | 'auto' | 'both';  // Where to execute (defaults to context.environment)
  timeout?: number;                             // Max execution time in milliseconds
  parameters?: Record<string, any>;             // Parameters passed to script as 'params' global
  
  // Security permissions
  allowNetworkRequests?: boolean;               // Allow fetch, XMLHttpRequest, etc.
  allowFileSystemAccess?: boolean;              // Allow file operations (server context only)
  allowDOMManipulation?: boolean;               // Allow DOM modifications (browser context only)
  allowJTAGCommandAccess?: boolean;            // Allow calling other JTAG commands (default: true)
  
  // Cross-environment routing properties (added by browser execution)
  result?: any;                                 // Result from browser execution
  executedAt?: number;                         // Timestamp of execution
  executedIn?: 'browser' | 'server';          // Environment where execution happened
}

/**
 * Discriminated union for code input - exactly one way to specify what to execute
 * TypeScript provides perfect type safety with this approach
 */
export type CodeInput = 
  | InlineCodeInput
  | FileCodeInput  
  | Base64CodeInput
  | TemplateCodeInput;

export interface InlineCodeInput {
  type: 'inline';
  language: SupportedLanguage;
  source: string;                               // Raw source code
}

export interface FileCodeInput {
  type: 'file';
  filepath: string;                             // Path to script file (relative to execution context)
}

export interface Base64CodeInput {
  type: 'base64';
  language: SupportedLanguage;
  encoded: string;                              // Base64 encoded source code
}

export interface TemplateCodeInput {
  type: 'template';
  name: string;                                 // Template name from template registry
  variables?: Record<string, any>;              // Template variable substitution
}

export type SupportedLanguage = 'typescript' | 'javascript' | 'python';
// Removed - using JTAGEnvironment instead

/**
 * Complete execution result with comprehensive debugging information
 */
export interface ExecCommandResult extends CommandResult {
  success: boolean;
  result?: any;                                 // Script return value (JSON-serializable)
  
  // Execution metadata
  executionTime: number;                        // Milliseconds taken to execute
  environment: JTAGEnvironment;                // Actual execution environment used
  language: SupportedLanguage;                 // Language that was executed
  
  // Debug information
  logs: string[];                              // All console output during execution
  screenshots: string[];                       // Screenshots taken during execution
  
  // Rich error information (if success = false)
  error?: ExecError;
  
  // Browser context information (for browser executions)
  browserInfo?: BrowserExecutionInfo;
  
  // Server context information (for server executions)
  serverInfo?: ServerExecutionInfo;
}

export interface ExecError {
  type: ExecErrorType;
  message: string;
  stack?: string;                              // Full error stack trace
  line?: number;                               // Line number where error occurred
  column?: number;                             // Column number where error occurred
  
  // Context-specific error details
  compilationError?: TypeScriptCompilationError;
  runtimeError?: RuntimeErrorDetails;
  permissionError?: PermissionErrorDetails;
}

export type ExecErrorType = 
  | 'syntax'           // Code syntax/compilation errors
  | 'validation'       // Parameter validation errors
  | 'runtime'          // Runtime execution errors
  | 'permission'       // Security/permission errors
  | 'timeout'          // Execution timeout errors
  | 'network'          // Network-related errors

// Helper function to create consistent error results
export function createExecErrorResult(
  errorType: ExecErrorType,
  message: string,
  environment: 'browser' | 'server',
  params: ExecCommandParams,
  executionTime: number = 0
): ExecCommandResult {
  return {
    success: false,
    error: { type: errorType, message },
    executionTime,
    environment,
    language: 'javascript',
    logs: [],
    screenshots: [],
    sessionId: params.sessionId,
    context: params.context
  };
}

// Helper function to create consistent success results
export function createExecSuccessResult(
  result: any,
  environment: 'browser' | 'server',
  params: ExecCommandParams,
  executionTime: number
): ExecCommandResult {
  return {
    success: true,
    result,
    executionTime,
    environment,
    language: 'javascript',
    logs: [],
    screenshots: [],
    sessionId: params.sessionId,
    context: params.context
  };
}

export interface TypeScriptCompilationError {
  diagnostics: {
    line: number;
    column: number;
    message: string;
    category: 'error' | 'warning' | 'info';
  }[];
}

export interface RuntimeErrorDetails {
  originalError: string;                       // Original error message
  scriptLine?: number;                         // Line in user script where error occurred
  nativeStack?: string;                        // Native JavaScript stack trace
}

export interface PermissionErrorDetails {
  attemptedAction: string;                     // What action was blocked
  requiredPermission: keyof ExecPermissions;   // What permission was needed
  suggestion: string;                          // How to fix the permission issue
}

export interface BrowserExecutionInfo {
  userAgent: string;
  url: string;
  title: string;
  viewport: { width: number; height: number; };
  readyState: DocumentReadyState;
  elementsCount: number;                       // Total DOM elements
}

export interface ServerExecutionInfo {
  nodeVersion: string;
  platform: string;
  architecture: string; 
  workingDirectory: string;
  memoryUsage: NodeJS.MemoryUsage;
}

/**
 * Internal transport payload - what gets sent across network
 * Always base64 encoded regardless of input type for safe transmission
 */
export interface ExecTransportPayload {
  // Encoded content (safe for network transmission)
  sourceBase64: string;                        // ALWAYS base64 encoded source
  parametersBase64?: string;                   // Base64 encoded JSON parameters
  
  // Execution metadata  
  language: SupportedLanguage;
  targetEnvironment: JTAGEnvironment | 'auto' | 'both';
  timeout: number;
  
  // Security permissions
  permissions: ExecPermissions;
  
  // Debug information
  originalType: CodeInput['type'];             // Original input type for logging
  correlationId: string;                       // For request/response correlation
}

export interface ExecPermissions {
  allowNetworkRequests: boolean;
  allowFileSystemAccess: boolean; 
  allowDOMManipulation: boolean;
  allowJTAGCommandAccess: boolean;
}

/**
 * Runtime environment injected into every script execution
 * These globals are available in all exec scripts
 */
export interface ExecRuntimeEnvironment {
  // JTAG system access
  jtag: JTAGClientAccess;                      // Full JTAG command access
  continuum?: ContinuumClientAccess;           // Continuum integration (if available)
  
  // Script parameters
  params: Record<string, any>;                 // Parameters from ExecCommandParams
  
  // Context-specific APIs
  dom?: DOMManipulatorAccess;                  // Browser-only DOM access
  fs?: FileSystemAccess;                       // Server-only file system access
  
  // Utility functions
  utils: ExecUtilities;                        // Helper functions for common tasks
}

/**
 * JTAG client interface available in exec scripts
 * Subset of full JTAG client focused on commonly used commands
 */
export interface JTAGClientAccess {
  commands: {
    screenshot: (filename?: string) => Promise<any>;
    log: (level: string, message: string, data?: any) => Promise<any>;
    exec: (params: ExecCommandParams) => Promise<ExecCommandResult>; // Self-referential!
  };
}

export interface ContinuumClientAccess {
  commands: {
    processData: (params: any) => Promise<any>;
    getData: () => Promise<any>;
    // Add more as needed
  };
}

export interface DOMManipulatorAccess {
  querySelector: (selector: string) => Element | null;
  querySelectorAll: (selector: string) => NodeList;
  waitForElement: (selector: string, timeout?: number) => Promise<Element>;
  waitForElementVisible: (selector: string, timeout?: number) => Promise<Element>;
  // Browser-specific utilities
}

export interface FileSystemAccess {
  readFile: (path: string, encoding?: string) => Promise<string>;
  writeFile: (path: string, data: string) => Promise<void>;
  exists: (path: string) => Promise<boolean>;
  // Server-specific utilities
}

export interface ExecUtilities {
  sleep: (ms: number) => Promise<void>;
  timestamp: () => string;
  uuid: () => string;
  base64Encode: (str: string) => string;
  base64Decode: (str: string) => string;
  safeStringify: (obj: any) => string;         // JSON.stringify with error handling
}

/**
 * Template system types (future expansion)
 */
export interface ExecTemplate {
  name: string;
  description: string;
  language: SupportedLanguage;
  source: string;                              // Template source with variable placeholders
  variables: ExecTemplateVariable[];
  examples: ExecTemplateExample[];
}

export interface ExecTemplateVariable {
  name: string;
  type: 'string' | 'number' | 'boolean' | 'object';
  required: boolean;
  description: string;
  defaultValue?: any;
}

export interface ExecTemplateExample {
  name: string;
  description: string;
  variables: Record<string, any>;
  expectedResult: any;
}

/**
 * Execution context detection helpers
 */
// Use JTAG_ENVIRONMENTS instead of custom ExecutionContexts

export const SupportedLanguages = {
  TYPESCRIPT: 'typescript' as const,
  JAVASCRIPT: 'javascript' as const,
  PYTHON: 'python' as const
};

export const CodeInputTypes = {
  INLINE: 'inline' as const,
  FILE: 'file' as const,
  BASE64: 'base64' as const,
  TEMPLATE: 'template' as const
};

/**
 * Default configurations
 */
export const DEFAULT_EXEC_TIMEOUT = 3000; // 3 seconds - reasonable timeout with Web Worker protection
export const DEFAULT_EXEC_PERMISSIONS: ExecPermissions = {
  allowNetworkRequests: true,
  allowFileSystemAccess: false,
  allowDOMManipulation: true, 
  allowJTAGCommandAccess: true
};