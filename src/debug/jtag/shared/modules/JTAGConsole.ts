/**
 * JTAG Console Module
 * 
 * Handles console interception and preservation
 */

export class JTAGConsole {
  
  /**
   * Attach console interception
   */
  static attach(
    consoleObject: Console,
    consoleOriginals: { [key: string]: any },
    log: (component: string, message: string, data?: any) => void,
    warn: (component: string, message: string, data?: any) => void,
    error: (component: string, message: string, data?: any) => void,
    context: 'browser' | 'server'
  ): void {
    
    // Preserve original console methods
    consoleOriginals.log = consoleObject.log;
    consoleOriginals.warn = consoleObject.warn;  
    consoleOriginals.error = consoleObject.error;
    consoleOriginals.info = consoleObject.info;
    consoleOriginals.debug = consoleObject.debug;
    consoleOriginals.trace = consoleObject.trace;

    // Override console.log
    consoleObject.log = (...args: any[]) => {
      // Call original first
      consoleOriginals.log?.apply(consoleObject, args);
      
      // Then route to JTAG
      const message = args.map(arg => 
        typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
      ).join(' ');
      
      log('CONSOLE', message, { 
        args, 
        intercepted: true,
        context,
        originalMethod: 'log'
      });
    };

    // Override console.warn
    consoleObject.warn = (...args: any[]) => {
      consoleOriginals.warn?.apply(consoleObject, args);
      
      const message = args.map(arg => 
        typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
      ).join(' ');
      
      warn('CONSOLE', message, { 
        args, 
        intercepted: true,
        context,
        originalMethod: 'warn'
      });
    };

    // Override console.error
    consoleObject.error = (...args: any[]) => {
      consoleOriginals.error?.apply(consoleObject, args);
      
      const message = args.map(arg => 
        typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
      ).join(' ');
      
      error('CONSOLE', message, { 
        args, 
        intercepted: true,
        context,
        originalMethod: 'error'
      });
    };

    // Override console.info
    consoleObject.info = (...args: any[]) => {
      consoleOriginals.info?.apply(consoleObject, args);
      
      const message = args.map(arg => 
        typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
      ).join(' ');
      
      log('CONSOLE_INFO', message, { 
        args, 
        intercepted: true,
        context,
        originalMethod: 'info'
      });
    };

    log('JTAG_CONSOLE', `Console interception attached for context: ${context}`, {
      methods: ['log', 'warn', 'error', 'info'],
      context
    });
  }

  /**
   * Detach console interception
   */
  static detach(
    consoleObject: Console,
    consoleOriginals: { [key: string]: any },
    log: (component: string, message: string, data?: any) => void,
    context: 'browser' | 'server'
  ): void {
    
    if (consoleOriginals.log) {
      consoleObject.log = consoleOriginals.log;
    }
    if (consoleOriginals.warn) {
      consoleObject.warn = consoleOriginals.warn;
    }
    if (consoleOriginals.error) {
      consoleObject.error = consoleOriginals.error;
    }
    if (consoleOriginals.info) {
      consoleObject.info = consoleOriginals.info;
    }
    if (consoleOriginals.debug) {
      consoleObject.debug = consoleOriginals.debug;
    }
    if (consoleOriginals.trace) {
      consoleObject.trace = consoleOriginals.trace;
    }

    log('JTAG_CONSOLE', `Console interception detached for context: ${context}`, {
      restored: ['log', 'warn', 'error', 'info', 'debug', 'trace']
    });
  }
}