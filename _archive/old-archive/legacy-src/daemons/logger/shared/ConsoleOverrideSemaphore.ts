/**
 * Console Override Semaphore - GLOBAL RUN-ONCE enforcement for console overrides
 * Prevents loops and crashes by ensuring only ONE console override system is active
 * 
 * CRITICAL: This must be the ONLY way to override console methods system-wide
 */

interface OriginalConsoleMethods {
  log: typeof console.log;
  info: typeof console.info;
  warn: typeof console.warn;
  error: typeof console.error;
  debug: typeof console.debug;
}

export class ConsoleOverrideSemaphore {
  private static isOverrideActive = false;
  private static overrideSource: string | null = null;
  private static originalConsole: OriginalConsoleMethods | null = null;
  // private static readonly SEMAPHORE_LOCK = Symbol('ConsoleOverrideSemaphore'); // Reserved for future use

  /**
   * CRITICAL: Attempt to acquire console override semaphore - CRASHES if called twice
   * @param source - Source identifier for debugging (e.g., 'LoggerDaemon', 'UniversalLogger')
   * @throws Error if console override already active (INTENTIONAL CRASH)
   */
  static acquire(source: string): void {
    if (ConsoleOverrideSemaphore.isOverrideActive) {
      // INTENTIONAL CRASH - This is a critical violation
      const errorMsg = 
        `🚨 CONSOLE_OVERRIDE_SEMAPHORE_VIOLATION: Console override already active by '${ConsoleOverrideSemaphore.overrideSource}'. ` +
        `Cannot acquire from '${source}'. This prevents infinite loops and crashes. ` +
        `CRASHING IMMEDIATELY to prevent system damage.`;
      
      console.error(errorMsg);
      throw new Error(errorMsg);
    }

    // Store original console methods before any override
    if (!ConsoleOverrideSemaphore.originalConsole) {
      ConsoleOverrideSemaphore.originalConsole = {
        log: console.log.bind(console),
        info: console.info.bind(console),
        warn: console.warn.bind(console),
        error: console.error.bind(console),
        debug: console.debug.bind(console)
      };
    }

    ConsoleOverrideSemaphore.isOverrideActive = true;
    ConsoleOverrideSemaphore.overrideSource = source;
    
    console.log(`🔒 Console override semaphore acquired by: ${source}`);
  }

  /**
   * Release console override semaphore and restore original console methods
   * @param source - Source identifier (must match acquirer)
   * @throws Error if source doesn't match acquirer
   */
  static release(source: string): void {
    if (!ConsoleOverrideSemaphore.isOverrideActive) {
      throw new Error(
        `CONSOLE_OVERRIDE_SEMAPHORE_VIOLATION: No console override active. ` +
        `Cannot release from '${source}'.`
      );
    }

    if (ConsoleOverrideSemaphore.overrideSource !== source) {
      throw new Error(
        `CONSOLE_OVERRIDE_SEMAPHORE_VIOLATION: Console override owned by '${ConsoleOverrideSemaphore.overrideSource}'. ` +
        `Cannot release from '${source}'.`
      );
    }

    // Restore original console methods
    if (ConsoleOverrideSemaphore.originalConsole) {
      console.log = ConsoleOverrideSemaphore.originalConsole.log;
      console.info = ConsoleOverrideSemaphore.originalConsole.info;
      console.warn = ConsoleOverrideSemaphore.originalConsole.warn;
      console.error = ConsoleOverrideSemaphore.originalConsole.error;
      console.debug = ConsoleOverrideSemaphore.originalConsole.debug;
    }

    ConsoleOverrideSemaphore.isOverrideActive = false;
    ConsoleOverrideSemaphore.overrideSource = null;
    
    console.log(`🔓 Console override semaphore released by: ${source}`);
  }

  /**
   * Check if console override is currently active
   */
  static isActive(): boolean {
    return ConsoleOverrideSemaphore.isOverrideActive;
  }

  /**
   * Get current override source (for debugging)
   */
  static getCurrentSource(): string | null {
    return ConsoleOverrideSemaphore.overrideSource;
  }

  /**
   * Get original console methods (for overriding systems to use)
   */
  static getOriginalConsole(): OriginalConsoleMethods | null {
    return ConsoleOverrideSemaphore.originalConsole;
  }

  /**
   * Force reset semaphore (for testing only)
   * @internal
   */
  static _forceReset(): void {
    // Restore console methods first
    if (ConsoleOverrideSemaphore.originalConsole) {
      console.log = ConsoleOverrideSemaphore.originalConsole.log;
      console.info = ConsoleOverrideSemaphore.originalConsole.info;
      console.warn = ConsoleOverrideSemaphore.originalConsole.warn;
      console.error = ConsoleOverrideSemaphore.originalConsole.error;
      console.debug = ConsoleOverrideSemaphore.originalConsole.debug;
    }

    ConsoleOverrideSemaphore.isOverrideActive = false;
    ConsoleOverrideSemaphore.overrideSource = null;
    ConsoleOverrideSemaphore.originalConsole = null;
  }
}