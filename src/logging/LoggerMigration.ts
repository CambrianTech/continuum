/**
 * Logger Migration - Gradual transition from sync to async logging
 * Provides compatibility layer and migration utilities
 */

import { ContinuumContext, ContinuumEnvironment } from '../types/shared/core/ContinuumTypes';
import { UniversalLogger } from './UniversalLogger';
import { UniversalLoggerAsync } from './UniversalLoggerAsync';

export enum LoggerMode {
  SYNC = 'sync',
  ASYNC = 'async',
  HYBRID = 'hybrid'
}

export class LoggerMigration {
  private static mode: LoggerMode = LoggerMode.SYNC;
  private static migrationInProgress = false;

  /**
   * Set logger mode for gradual migration
   */
  static setMode(mode: LoggerMode) {
    this.mode = mode;
  }

  /**
   * Get current logger mode
   */
  static getMode(): LoggerMode {
    return this.mode;
  }

  /**
   * Unified logging interface - routes to appropriate logger
   */
  static async log(
    name: ContinuumEnvironment,
    source: string,
    message: string,
    level: 'info' | 'warn' | 'error' | 'debug' = 'info',
    context: ContinuumContext
  ) {
    switch (this.mode) {
      case LoggerMode.SYNC:
        // Use original sync logger
        UniversalLogger.log(name, source, message, level, context);
        break;
        
      case LoggerMode.ASYNC:
        // Use new async logger
        await UniversalLoggerAsync.log(name, source, message, level, context);
        break;
        
      case LoggerMode.HYBRID:
        // Use both for comparison/testing
        UniversalLogger.log(name, source, message, level, context);
        await UniversalLoggerAsync.log(name, source, message, level, context);
        break;
    }
  }

  /**
   * Initialize console overrides based on mode
   */
  static async initializeConsoleOverrides() {
    switch (this.mode) {
      case LoggerMode.SYNC:
        UniversalLogger.overrideConsole();
        break;
        
      case LoggerMode.ASYNC:
        await UniversalLoggerAsync.init();
        UniversalLoggerAsync.overrideConsole();
        break;
        
      case LoggerMode.HYBRID:
        UniversalLogger.overrideConsole();
        await UniversalLoggerAsync.init();
        // Don't override console twice - sync will handle it
        break;
    }
  }

  /**
   * Migrate from sync to async logging
   */
  static async migrateToAsync() {
    if (this.migrationInProgress) {
      return;
    }

    this.migrationInProgress = true;
    
    try {
      console.log('Starting migration from sync to async logging...');
      
      // Step 1: Initialize async logger
      await UniversalLoggerAsync.init();
      
      // Step 2: Switch to hybrid mode
      this.setMode(LoggerMode.HYBRID);
      console.log('Migration: Switched to hybrid mode');
      
      // Step 3: Wait for existing logs to flush
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      // Step 4: Switch console overrides to async
      UniversalLogger.restoreConsole();
      UniversalLoggerAsync.overrideConsole();
      console.log('Migration: Switched console overrides to async');
      
      // Step 5: Switch to full async mode
      this.setMode(LoggerMode.ASYNC);
      console.log('Migration: Completed - now using async logging');
      
    } catch (error) {
      console.error('Migration failed:', error);
      // Rollback to sync mode
      this.setMode(LoggerMode.SYNC);
      UniversalLoggerAsync.restoreConsole();
      UniversalLogger.overrideConsole();
    } finally {
      this.migrationInProgress = false;
    }
  }

  /**
   * Rollback to sync logging
   */
  static async rollbackToSync() {
    console.log('Rolling back to sync logging...');
    
    // Restore sync console overrides
    UniversalLoggerAsync.restoreConsole();
    await UniversalLogger.overrideConsole();
    
    // Switch mode
    this.setMode(LoggerMode.SYNC);
    
    // Shutdown async logger
    await UniversalLoggerAsync.shutdown();
    
    console.log('Rollback completed - using sync logging');
  }

  /**
   * Get migration status
   */
  static getStatus(): {
    mode: LoggerMode;
    migrationInProgress: boolean;
    asyncInitialized: boolean;
  } {
    return {
      mode: this.mode,
      migrationInProgress: this.migrationInProgress,
      asyncInitialized: true // This would check if async logger is initialized
    };
  }
}

/**
 * Convenient wrapper that uses migration layer
 */
export class Logger {
  static async log(
    name: ContinuumEnvironment,
    source: string,
    message: string,
    level: 'info' | 'warn' | 'error' | 'debug' = 'info',
    context: ContinuumContext
  ) {
    await LoggerMigration.log(name, source, message, level, context);
  }

  static async info(name: ContinuumEnvironment, source: string, message: string, context: ContinuumContext) {
    await LoggerMigration.log(name, source, message, 'info', context);
  }

  static async warn(name: ContinuumEnvironment, source: string, message: string, context: ContinuumContext) {
    await LoggerMigration.log(name, source, message, 'warn', context);
  }

  static async error(name: ContinuumEnvironment, source: string, message: string, context: ContinuumContext) {
    await LoggerMigration.log(name, source, message, 'error', context);
  }

  static async debug(name: ContinuumEnvironment, source: string, message: string, context: ContinuumContext) {
    await LoggerMigration.log(name, source, message, 'debug', context);
  }
}