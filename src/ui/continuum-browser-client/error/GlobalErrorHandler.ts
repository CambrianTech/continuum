/**
 * Global Error Handler - Centralized Error Capture and Forwarding
 * 
 * Provides universal error wrapping for widgets, commands, and any async operations.
 * Ensures ALL errors make it to the browser logs for JTAG debugging.
 */

export interface ErrorContext {
  component?: string;
  operation?: string;
  widget?: string;
  command?: string;
  metadata?: Record<string, any>;
}

export class GlobalErrorHandler {
  private static instance: GlobalErrorHandler;
  private errorCount = 0;

  static getInstance(): GlobalErrorHandler {
    if (!GlobalErrorHandler.instance) {
      GlobalErrorHandler.instance = new GlobalErrorHandler();
    }
    return GlobalErrorHandler.instance;
  }

  /**
   * Wrap any function with comprehensive error handling
   */
  public wrapFunction<T extends (...args: any[]) => any>(
    fn: T,
    context: ErrorContext
  ): T {
    return ((...args: any[]) => {
      try {
        const result = fn(...args);
        
        // Handle both sync and async functions
        if (result instanceof Promise) {
          return result.catch(error => {
            this.captureError(error, context);
            throw error; // Re-throw for caller handling
          });
        }
        
        return result;
      } catch (error) {
        this.captureError(error, context);
        throw error; // Re-throw for caller handling
      }
    }) as T;
  }

  /**
   * Wrap async functions with error handling
   */
  public wrapAsync<T extends (...args: any[]) => Promise<any>>(
    fn: T,
    context: ErrorContext
  ): T {
    return (async (...args: any[]) => {
      try {
        return await fn(...args);
      } catch (error) {
        this.captureError(error, context);
        throw error; // Re-throw for caller handling
      }
    }) as T;
  }

  /**
   * Safe execution wrapper - catches errors but doesn't re-throw
   */
  public async safeExecute<T>(
    fn: () => T | Promise<T>,
    context: ErrorContext,
    fallback?: T
  ): Promise<T | undefined> {
    try {
      const result = await fn();
      return result;
    } catch (error) {
      this.captureError(error, context);
      return fallback;
    }
  }

  /**
   * Manual error capture for explicit error handling
   */
  public captureError(error: any, context: ErrorContext): void {
    this.errorCount++;
    
    const errorInfo = {
      error: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined,
      context: context,
      timestamp: new Date().toISOString(),
      errorId: this.errorCount,
      url: window.location.href
    };

    // Always use console.error to ensure it goes through our capture system
    console.error(`🚨 Global Error Handler:`, errorInfo);
    
    // Also emit an event for any listeners
    window.dispatchEvent(new CustomEvent('continuum:error', {
      detail: errorInfo
    }));
  }

  /**
   * Widget-specific error wrapper
   */
  public wrapWidgetMethod<T extends (...args: any[]) => any>(
    widgetName: string,
    methodName: string,
    fn: T
  ): T {
    return this.wrapFunction(fn, {
      component: 'widget',
      widget: widgetName,
      operation: methodName
    });
  }

  /**
   * Command-specific error wrapper
   */
  public wrapCommandMethod<T extends (...args: any[]) => any>(
    commandName: string,
    methodName: string,
    fn: T
  ): T {
    return this.wrapFunction(fn, {
      component: 'command',
      command: commandName,
      operation: methodName
    });
  }

  /**
   * API call wrapper
   */
  public wrapAPICall<T extends (...args: any[]) => Promise<any>>(
    apiName: string,
    fn: T
  ): T {
    return this.wrapAsync(fn, {
      component: 'api',
      operation: apiName
    });
  }

  /**
   * Get error statistics
   */
  public getErrorStats(): { errorCount: number } {
    return { errorCount: this.errorCount };
  }
}

// Global instance for easy access
export const globalErrorHandler = GlobalErrorHandler.getInstance();

// Helper functions for common use cases
export const wrapWidget = (widgetName: string, methodName: string) => 
  <T extends (...args: any[]) => any>(fn: T): T => 
    globalErrorHandler.wrapWidgetMethod(widgetName, methodName, fn);

export const wrapCommand = (commandName: string, methodName: string) => 
  <T extends (...args: any[]) => any>(fn: T): T => 
    globalErrorHandler.wrapCommandMethod(commandName, methodName, fn);

export const safeExecute = <T>(
  fn: () => T | Promise<T>,
  context: ErrorContext,
  fallback?: T
): Promise<T | undefined> => 
  globalErrorHandler.safeExecute(fn, context, fallback);

export const captureError = (error: any, context: ErrorContext): void =>
  globalErrorHandler.captureError(error, context);