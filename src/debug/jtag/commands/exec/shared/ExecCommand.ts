/**
 * ExecCommand - Universal Script Execution Meta-Command
 * 
 * The primary automation tool for AI agents - enables custom script execution
 * with comprehensive error handling, visual feedback, and cross-context orchestration.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import { type JTAGContext, type JTAGEnvironment, JTAG_ENVIRONMENTS } from '../../../system/core/types/JTAGTypes';
import type { 
  ExecCommandParams, 
  ExecCommandResult, 
  ExecTransportPayload,
  ExecError,
  ExecErrorType,
  BrowserExecutionInfo,
  ServerExecutionInfo,
  ExecRuntimeEnvironment,
  SupportedLanguage
} from './ExecTypes';

import { ExecTransportEncoder, ExecResultTransporter, ExecNetworkSafety } from './ExecTransportUtils';

/**
 * Base ExecCommand implementation - extends JTAG CommandBase for proper registration
 */
export abstract class ExecCommand extends CommandBase<ExecCommandParams, ExecCommandResult> {
  protected readonly correlationId: string;
  protected readonly startTime: number;
  protected readonly logs: string[] = [];
  protected readonly screenshots: string[] = [];
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('exec', context, subpath, commander);
    this.correlationId = this.generateCorrelationId();
    this.startTime = Date.now();
  }
  
  /**
   * Main execution entry point - handles all error cases and provides comprehensive results
   * Implements the CommandBase interface for JTAG compatibility
   */
  async execute(params: ExecCommandParams): Promise<ExecCommandResult> {
    console.log(`🚀 ExecCommand: Starting execution ${this.correlationId}`);
    console.log(`📍 ExecCommand: Executing in ${this.getLocalEnvironment()} environment`);
    console.log(`🎯 ExecCommand: Request context.environment = ${params.context.environment}`);
    console.log(`🎯 ExecCommand: Request targetEnvironment = ${params.targetEnvironment || 'auto'}`);
    console.log(`🎯 ExecCommand: Full params received:`, JSON.stringify({
      sessionId: params.sessionId,
      context: params.context,
      targetEnvironment: params.targetEnvironment,
      codeType: params.code.type
    }, null, 2));
    
    try {
      // 1. Validate parameters
      const validationResult = this.validateParams(params);
      if (!validationResult.valid) {
        return this.createErrorResult('validation', `Parameter validation failed: ${validationResult.errors.join(', ')}`);
      }
      
      // 2. Encode for transport (even for local execution - ensures consistency)
      const transportPayload = await ExecTransportEncoder.encodeForTransport(params, this.correlationId);
      
      // 3. Validate transport payload
      const transportValidation = ExecTransportEncoder.validateTransportPayload(transportPayload);
      if (!transportValidation.valid) {
        return this.createErrorResult('validation', `Transport validation failed: ${transportValidation.errors.join(', ')}`);
      }
      
      // 4. Determine execution environment
      const requestedEnvironment = params.targetEnvironment || 'auto';
      const actualEnvironment = await this.resolveExecutionEnvironment(requestedEnvironment);
      console.log(`🔄 ExecCommand: Requested environment: ${requestedEnvironment}`);
      console.log(`🔄 ExecCommand: Resolved environment: ${actualEnvironment}`);
      console.log(`🔄 ExecCommand: Local environment: ${this.getLocalEnvironment()}`);
      
      // 5. Execute in appropriate context
      let executionResult: any;
      
      if (actualEnvironment === this.getLocalEnvironment()) {
        // Execute locally
        console.log(`📍 ExecCommand: Executing locally in ${actualEnvironment} environment`);
        executionResult = await this.executeLocally(transportPayload);
      } else {
        // Execute remotely (cross-context or P2P)
        console.log(`🌐 ExecCommand: Routing to ${actualEnvironment} environment`);
        executionResult = await this.executeRemotely(transportPayload, actualEnvironment);
      }
      
      // 6. Create success result
      return this.createSuccessResult(executionResult, actualEnvironment, transportPayload.language, params.sessionId, params.context);
      
    } catch (error) {
      console.error(`❌ ExecCommand: Execution failed for ${this.correlationId}:`, error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult('runtime', errorMessage, error instanceof Error ? error : undefined, params.sessionId, params.context);
    }
  }
  
  /**
   * Execute script in local context (browser or server)
   */
  protected async executeLocally(payload: ExecTransportPayload): Promise<any> {
    // 1. Decode payload
    const { sourceCode, parameters, language, permissions } = ExecTransportEncoder.decodeFromTransport(payload);
    
    // 2. Resolve source code (handle file/template inputs)
    const resolvedSource = await this.resolveSourceCode(sourceCode, payload.originalType);
    
    // 3. Check network safety
    const safetyCheck = ExecNetworkSafety.isSafeForTransport(resolvedSource);
    if (!safetyCheck.safe && safetyCheck.issues.some(issue => issue.includes('security'))) {
      console.warn(`⚠️ ExecCommand: Security issues detected:`, safetyCheck.issues);
    }
    
    // 4. Create runtime environment
    const runtime = await this.createRuntimeEnvironment(parameters, permissions);
    
    // 5. Execute with comprehensive error handling
    return await this.executeInSandbox(resolvedSource, language, runtime, payload.timeout);
  }
  
  /**
   * Execute script in remote context (cross-context or P2P mesh)
   */
  protected async executeRemotely(payload: ExecTransportPayload, targetEnvironment: JTAGEnvironment): Promise<any> {
    // This will be implemented by specific router integration
    // For now, throw an error to indicate remote execution is not yet implemented
    throw new Error(`Remote execution to ${targetEnvironment} not yet implemented`);
  }
  
  /**
   * Create runtime environment with all necessary globals
   */
  protected async createRuntimeEnvironment(
    parameters: Record<string, any>, 
    permissions: any
  ): Promise<ExecRuntimeEnvironment> {
    return {
      // JTAG system access
      jtag: await this.createJTAGAccess(permissions),
      continuum: await this.createContinuumAccess(permissions),
      
      // Script parameters
      params: parameters,
      
      // Context-specific APIs
      dom: this.getLocalEnvironment() === 'browser' ? await this.createDOMAccess(permissions) : undefined,
      fs: this.getLocalEnvironment() === 'server' ? await this.createFileSystemAccess(permissions) : undefined,
      
      // Utility functions
      utils: this.createUtilities()
    };
  }
  
  /**
   * Execute script in sandboxed environment with timeout
   */
  protected async executeInSandbox(
    sourceCode: string, 
    language: SupportedLanguage, 
    runtime: ExecRuntimeEnvironment,
    timeout: number
  ): Promise<any> {
    // Wrap execution in timeout
    return await Promise.race([
      this.executeWithRuntime(sourceCode, language, runtime),
      this.createTimeoutPromise(timeout)
    ]);
  }
  
  /**
   * Actually execute the user's script with runtime environment
   */
  protected async executeWithRuntime(
    sourceCode: string, 
    language: SupportedLanguage, 
    runtime: ExecRuntimeEnvironment
  ): Promise<any> {
    try {
      // Capture console output
      const originalConsole = console.log;
      console.log = (...args) => {
        const message = args.map(arg => typeof arg === 'string' ? arg : JSON.stringify(arg)).join(' ');
        this.logs.push(message);
        originalConsole(...args); // Still log to actual console
      };
      
      let result: any;
      
      if (language === 'typescript') {
        // For TypeScript, we need to compile first
        result = await this.executeTypeScript(sourceCode, runtime);
      } else if (language === 'javascript') {
        result = await this.executeJavaScript(sourceCode, runtime);
      } else if (language === 'python') {
        result = await this.executePython(sourceCode, runtime);
      } else {
        throw new Error(`Unsupported language: ${language}`);
      }
      
      // Restore console
      console.log = originalConsole;
      
      return result;
      
    } catch (error) {
      // Restore console on error
      console.log = console.log;
      throw error;
    }
  }
  
  /**
   * Execute JavaScript code with runtime environment
   */
  protected async executeJavaScript(sourceCode: string, runtime: ExecRuntimeEnvironment): Promise<any> {
    // Create function with runtime as parameters
    const wrappedCode = `
      (async function(jtag, continuum, params, dom, fs, utils) {
        ${sourceCode}
      })
    `;
    
    try {
      const func = eval(wrappedCode);
      return await func(runtime.jtag, runtime.continuum, runtime.params, runtime.dom, runtime.fs, runtime.utils);
    } catch (error) {
      throw this.enhanceExecutionError(error instanceof Error ? error : new Error(String(error)), sourceCode);
    }
  }
  
  /**
   * Execute TypeScript code (compile first, then execute)
   */
  protected async executeTypeScript(sourceCode: string, runtime: ExecRuntimeEnvironment): Promise<any> {
    // For now, treat TypeScript as JavaScript (basic transpilation)
    // In a full implementation, we'd use the TypeScript compiler API
    console.log('⚠️ ExecCommand: TypeScript execution using JavaScript fallback (full TS compilation not yet implemented)');
    return await this.executeJavaScript(sourceCode, runtime);
  }
  
  /**
   * Execute Python code (future implementation)
   */
  protected async executePython(sourceCode: string, runtime: ExecRuntimeEnvironment): Promise<any> {
    throw new Error('Python execution not yet implemented');
  }
  
  /**
   * Enhance execution errors with helpful context
   */
  protected enhanceExecutionError(error: Error, sourceCode: string): Error {
    // Try to extract line number from error
    const lines = sourceCode.split('\n');
    let lineNumber: number | undefined;
    
    // Basic line number extraction (works for simple cases)
    const lineMatch = error.stack?.match(/at eval.*:(\d+):/);
    if (lineMatch) {
      lineNumber = parseInt(lineMatch[1]) - 3; // Account for wrapper function
    }
    
    const enhancedError = new Error(`Script execution failed: ${error.message}`);
    enhancedError.stack = error.stack;
    (enhancedError as any).scriptLineNumber = lineNumber;
    (enhancedError as any).scriptSource = sourceCode;
    
    return enhancedError;
  }
  
  /**
   * Create timeout promise that rejects after specified time
   */
  protected createTimeoutPromise(timeout: number): Promise<never> {
    return new Promise((_, reject) => {
      setTimeout(() => {
        reject(new Error(`Script execution timeout after ${timeout}ms`));
      }, timeout);
    });
  }
  
  /**
   * Create success result with comprehensive information
   */
  protected createSuccessResult(
    result: any, 
    environment: JTAGEnvironment, 
    language: SupportedLanguage,
    sessionId: string,
    context: JTAGContext
  ): ExecCommandResult {
    return {
      success: true,
      result,
      sessionId,
      context,
      executionTime: Date.now() - this.startTime,
      environment: environment,
      language,
      logs: [...this.logs],
      screenshots: [...this.screenshots],
      browserInfo: environment === 'browser' ? this.getBrowserInfo() : undefined,
      serverInfo: environment === 'server' ? this.getServerInfo() : undefined
    };
  }
  
  /**
   * Create error result with comprehensive debugging information
   */
  protected createErrorResult(
    errorType: ExecErrorType, 
    message: string, 
    originalError?: Error,
    sessionId?: string,
    context?: any
  ): ExecCommandResult {
    const execError: ExecError = {
      type: errorType,
      message,
      stack: originalError?.stack,
    };
    
    // Add enhanced error information
    if (originalError && (originalError as any).scriptLineNumber) {
      execError.line = (originalError as any).scriptLineNumber;
    }
    
    return {
      success: false,
      error: execError,
      sessionId: sessionId || 'unknown',
      context: context || { uuid: 'unknown', environment: this.getLocalEnvironment() },
      executionTime: Date.now() - this.startTime,
      environment: this.getLocalEnvironment(),
      language: 'javascript', // Default when we can't determine
      logs: [...this.logs],
      screenshots: [...this.screenshots],
      browserInfo: this.getLocalEnvironment() === 'browser' ? this.getBrowserInfo() : undefined,
      serverInfo: this.getLocalEnvironment() === 'server' ? this.getServerInfo() : undefined
    };
  }
  
  // Abstract methods to be implemented by browser/server specific versions
  abstract getLocalEnvironment(): JTAGEnvironment;
  abstract resolveExecutionEnvironment(requested: JTAGEnvironment | 'auto' | 'both'): Promise<JTAGEnvironment>;
  abstract resolveSourceCode(sourceCode: string, originalType: string): Promise<string>;
  abstract createJTAGAccess(permissions: any): Promise<any>;
  abstract createContinuumAccess(permissions: any): Promise<any>;
  abstract createDOMAccess(permissions: any): Promise<any>;
  abstract createFileSystemAccess(permissions: any): Promise<any>;
  abstract getBrowserInfo(): BrowserExecutionInfo | undefined;
  abstract getServerInfo(): ServerExecutionInfo | undefined;
  
  /**
   * Create utility functions available to all scripts
   */
  protected createUtilities() {
    return {
      sleep: (ms: number): Promise<void> => new Promise(resolve => setTimeout(resolve, ms)),
      timestamp: () => new Date().toISOString(),
      uuid: () => this.generateCorrelationId(),
      base64Encode: (str: string) => Buffer.from(str).toString('base64'),
      base64Decode: (str: string) => Buffer.from(str, 'base64').toString('utf8'),
      safeStringify: (obj: any) => {
        try {
          return JSON.stringify(obj, null, 2);
        } catch (error) {
          return `[Unserializable object: ${error instanceof Error ? error.message : String(error)}]`;
        }
      }
    };
  }
  
  /**
   * Validate ExecCommandParams before execution
   */
  protected validateParams(params: ExecCommandParams): { valid: boolean; errors: string[] } {
    const errors: string[] = [];
    
    if (!params.code) {
      errors.push('Missing required "code" parameter');
      return { valid: false, errors };
    }
    
    // Validate code input based on type
    switch (params.code.type) {
      case 'inline':
        if (!params.code.source?.trim()) {
          errors.push('Inline code source cannot be empty');
        }
        if (!params.code.language) {
          errors.push('Language required for inline code');
        }
        break;
        
      case 'base64':
        if (!params.code.encoded?.trim()) {
          errors.push('Base64 encoded source cannot be empty');
        }
        if (!params.code.language) {
          errors.push('Language required for base64 code');
        }
        break;
        
      case 'file':
        if (!params.code.path?.trim()) {
          errors.push('File path cannot be empty');
        }
        break;
        
      case 'template':
        if (!params.code.name?.trim()) {
          errors.push('Template name cannot be empty');
        }
        break;
        
      default:
        errors.push(`Unknown code input type: ${(params.code as any).type}`);
    }
    
    // Validate timeout
    if (params.timeout !== undefined && (params.timeout <= 0 || params.timeout > 600000)) {
      errors.push('Timeout must be between 1ms and 10 minutes');
    }
    
    return {
      valid: errors.length === 0,
      errors
    };
  }
  
  /**
   * Generate unique correlation ID for request tracking
   */
  protected generateCorrelationId(): string {
    return `exec_${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }
}