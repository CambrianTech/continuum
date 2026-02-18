/**
 * ExecCommand Transport Utilities - Safe Network Transmission
 * 
 * Handles base64 encoding/decoding and safe transport of exec payloads
 * across WebSocket, HTTP, P2P mesh, and any other transport protocol.
 */

import { type JTAGEnvironment } from '@system/core/types/JTAGTypes';
import type { 
  CodeInput, 
  ExecCommandParams, 
  ExecTransportPayload, 
  ExecPermissions,
  SupportedLanguage
} from './ExecTypes';

import { 
  DEFAULT_EXEC_PERMISSIONS,
  DEFAULT_EXEC_TIMEOUT
} from './ExecTypes';

/**
 * Converts user-friendly ExecCommandParams to network-safe transport payload
 * All code gets base64 encoded regardless of input type
 */
export class ExecTransportEncoder {
  
  /**
   * Prepare exec command for safe transport across any network protocol
   */
  static async encodeForTransport(
    params: ExecCommandParams,
    correlationId: string
  ): Promise<ExecTransportPayload> {
    try {
      // 1. Resolve source code from any input type
      const sourceCode = await this.resolveSourceCode(params.code);
      
      // 2. Extract language from code input
      const language = this.extractLanguage(params.code);
      
      // 3. Base64 encode everything for safe transport
      const sourceBase64 = this.safeEncode(sourceCode);
      const parametersBase64 = params.parameters ? 
        this.safeEncode(JSON.stringify(params.parameters)) : undefined;
      
      // 4. Resolve permissions with defaults
      const permissions = this.resolvePermissions(params);
      
      // 5. Create transport-safe payload
      const transportPayload: ExecTransportPayload = {
        sourceBase64,
        parametersBase64,
        language,
        targetEnvironment: params.targetEnvironment || 'auto',
        timeout: params.timeout || DEFAULT_EXEC_TIMEOUT,
        permissions,
        originalType: params.code.type,
        correlationId
      };
      
      return transportPayload;
      
    } catch (error) {
      throw new Error(`Failed to encode exec command for transport: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  
  /**
   * Decode transport payload back to executable form on target context
   */
  static decodeFromTransport(payload: ExecTransportPayload): {
    sourceCode: string;
    parameters: Record<string, any>;
    language: SupportedLanguage;
    targetEnvironment: JTAGEnvironment | 'auto' | 'both';
    timeout: number;
    permissions: ExecPermissions;
  } {
    try {
      // 1. Decode source code safely
      const sourceCode = this.safeDecode(payload.sourceBase64);
      
      // 2. Decode parameters safely  
      const parameters = payload.parametersBase64 ? 
        JSON.parse(this.safeDecode(payload.parametersBase64)) : {};
      
      return {
        sourceCode,
        parameters,
        language: payload.language,
        targetEnvironment: payload.targetEnvironment,
        timeout: payload.timeout,
        permissions: payload.permissions
      };
      
    } catch (error) {
      throw new Error(`Failed to decode exec transport payload: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  
  /**
   * Resolve source code from any CodeInput type
   */
  private static async resolveSourceCode(code: CodeInput): Promise<string> {
    switch (code.type) {
      case 'inline':
        return code.source;
        
      case 'base64':
        return this.safeDecode(code.encoded);
        
      case 'file':
        // Note: File reading needs to be implemented in the execution context
        // Here we just pass the filepath, actual reading happens in the executor
        return `// FILE_PATH: ${code.filepath}`;

      case 'template':
        // Note: Template resolution needs to be implemented in the execution context
        return `// TEMPLATE: ${code.name}`;

      default:
        const exhaustive: never = code;
        throw new Error(`Unknown code input type: ${(exhaustive as any).type}`);
    }
  }

  /**
   * Extract language from CodeInput (with intelligent defaults)
   */
  private static extractLanguage(code: CodeInput): SupportedLanguage {
    switch (code.type) {
      case 'inline':
      case 'base64':
        return code.language;

      case 'file':
        // Infer from file extension
        if (code.filepath.endsWith('.ts')) return 'typescript';
        if (code.filepath.endsWith('.js')) return 'javascript';
        if (code.filepath.endsWith('.py')) return 'python';
        return 'javascript'; // Default fallback
        
      case 'template':
        return 'javascript'; // Default for templates (can be overridden in template definition)
        
      default:
        return 'javascript';
    }
  }
  
  /**
   * Resolve permissions with intelligent defaults
   */
  private static resolvePermissions(params: ExecCommandParams): ExecPermissions {
    return {
      allowNetworkRequests: params.allowNetworkRequests ?? DEFAULT_EXEC_PERMISSIONS.allowNetworkRequests,
      allowFileSystemAccess: params.allowFileSystemAccess ?? DEFAULT_EXEC_PERMISSIONS.allowFileSystemAccess, 
      allowDOMManipulation: params.allowDOMManipulation ?? DEFAULT_EXEC_PERMISSIONS.allowDOMManipulation,
      allowJTAGCommandAccess: params.allowJTAGCommandAccess ?? DEFAULT_EXEC_PERMISSIONS.allowJTAGCommandAccess
    };
  }
  
  /**
   * Safe base64 encoding with error handling
   */
  private static safeEncode(content: string): string {
    try {
      return Buffer.from(content, 'utf8').toString('base64');
    } catch (error) {
      throw new Error(`Base64 encoding failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  
  /**
   * Safe base64 decoding with error handling
   */
  private static safeDecode(encoded: string): string {
    try {
      return Buffer.from(encoded, 'base64').toString('utf8');
    } catch (error) {
      throw new Error(`Base64 decoding failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  
  /**
   * Validate transport payload before sending
   */
  static validateTransportPayload(payload: ExecTransportPayload): {
    valid: boolean;
    errors: string[];
  } {
    const errors: string[] = [];
    
    // Check required fields
    if (!payload.sourceBase64) {
      errors.push('Missing sourceBase64');
    }
    
    if (!payload.language) {
      errors.push('Missing language');
    }
    
    if (!payload.correlationId) {
      errors.push('Missing correlationId');
    }
    
    // Validate base64 encoding
    try {
      this.safeDecode(payload.sourceBase64);
    } catch (error) {
      errors.push(`Invalid sourceBase64 encoding: ${error instanceof Error ? error.message : String(error)}`);
    }
    
    if (payload.parametersBase64) {
      try {
        this.safeDecode(payload.parametersBase64);
        JSON.parse(this.safeDecode(payload.parametersBase64));
      } catch (error) {
        errors.push(`Invalid parametersBase64: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
    
    // Validate timeout
    if (payload.timeout <= 0 || payload.timeout > 600000) { // Max 10 minutes
      errors.push('Invalid timeout (must be between 1ms and 10 minutes)');
    }
    
    return {
      valid: errors.length === 0,
      errors
    };
  }
}

/**
 * Utility class for working with exec results in transport layer
 */
export class ExecResultTransporter {
  
  /**
   * Encode exec result for safe transport back to caller
   */
  static encodeResult(result: any): string {
    try {
      // Handle circular references and non-serializable objects
      const safeResult = this.makeSafeForSerialization(result);
      const jsonResult = JSON.stringify(safeResult);
      return ExecTransportEncoder['safeEncode'](jsonResult);
    } catch (error) {
      // Fallback for completely unserializable results
      const fallback = {
        error: 'Result could not be serialized',
        originalError: error instanceof Error ? error.message : String(error),
        resultType: typeof result,
        resultConstructor: result?.constructor?.name || 'unknown'
      };
      const jsonFallback = JSON.stringify(fallback);
      return ExecTransportEncoder['safeEncode'](jsonFallback);
    }
  }
  
  /**
   * Decode exec result from transport back to usable form
   */
  static decodeResult(encodedResult: string): any {
    try {
      const jsonResult = ExecTransportEncoder['safeDecode'](encodedResult);
      return JSON.parse(jsonResult);
    } catch (error) {
      return {
        error: 'Could not decode result from transport',
        transportError: error instanceof Error ? error.message : String(error),
        rawResult: encodedResult.substring(0, 100) + '...' // First 100 chars for debugging
      };
    }
  }
  
  /**
   * Make object safe for JSON serialization by handling problematic values
   */
  private static makeSafeForSerialization(obj: any, visited = new WeakSet()): any {
    // Handle primitive types
    if (obj === null || obj === undefined) return obj;
    if (typeof obj !== 'object') return obj;
    
    // Handle circular references
    if (visited.has(obj)) {
      return '[Circular Reference]';
    }
    visited.add(obj);
    
    // Handle arrays
    if (Array.isArray(obj)) {
      return obj.map(item => this.makeSafeForSerialization(item, visited));
    }
    
    // Handle special objects
    if (obj instanceof Error) {
      return {
        name: obj.name,
        message: obj.message,
        stack: obj.stack
      };
    }
    
    if (obj instanceof Date) {
      return obj.toISOString();
    }
    
    // Handle DOM elements (browser context)
    if (typeof window !== 'undefined' && obj instanceof Element) {
      return {
        tagName: obj.tagName,
        className: obj.className,
        id: obj.id,
        textContent: obj.textContent?.substring(0, 100) // Limit text content
      };
    }
    
    // Handle functions
    if (typeof obj === 'function') {
      return `[Function: ${obj.name || 'anonymous'}]`;
    }
    
    // Handle regular objects
    const safeObj: any = {};
    for (const key in obj) {
      if (obj.hasOwnProperty(key)) {
        try {
          safeObj[key] = this.makeSafeForSerialization(obj[key], visited);
        } catch (error) {
          safeObj[key] = `[Unserializable: ${error instanceof Error ? error.message : String(error)}]`;
        }
      }
    }
    
    return safeObj;
  }
}

/**
 * Network transport safety utilities
 */
export class ExecNetworkSafety {
  
  /**
   * Check if content is safe for network transmission
   */
  static isSafeForTransport(content: string): {
    safe: boolean;
    issues: string[];
    recommendations: string[];
  } {
    const issues: string[] = [];
    const recommendations: string[] = [];
    
    // Check for problematic characters
    if (content.includes('\0')) {
      issues.push('Contains null characters');
      recommendations.push('Remove null characters from source');
    }
    
    // Check size limits
    const sizeInBytes = Buffer.byteLength(content, 'utf8');
    const maxSize = 1024 * 1024; // 1MB limit
    
    if (sizeInBytes > maxSize) {
      issues.push(`Content too large: ${sizeInBytes} bytes (max: ${maxSize})`);
      recommendations.push('Consider using file input or breaking into smaller scripts');
    }
    
    // Check for potential security issues (basic checks)
    const securityPatterns = [
      { pattern: /eval\s*\(/, issue: 'Contains eval() calls', recommendation: 'Avoid eval() for security' },
      { pattern: /Function\s*\(/, issue: 'Contains Function() constructor', recommendation: 'Use regular function declarations' },
      { pattern: /document\.write/, issue: 'Contains document.write', recommendation: 'Use modern DOM manipulation' }
    ];
    
    for (const { pattern, issue, recommendation } of securityPatterns) {
      if (pattern.test(content)) {
        issues.push(issue);
        recommendations.push(recommendation);
      }
    }
    
    return {
      safe: issues.length === 0,
      issues,
      recommendations
    };
  }
  
  /**
   * Sanitize content for safer transport (basic sanitization)
   */
  static sanitizeForTransport(content: string): string {
    return content
      .replace(/\0/g, '') // Remove null characters
      .trim(); // Remove leading/trailing whitespace
  }
}