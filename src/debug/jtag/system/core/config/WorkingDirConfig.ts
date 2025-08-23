/**
 * Working Directory Configuration - Single Source of Truth
 * 
 * Creates .continuum directories relative to consumer's working directory.
 * Works when installed as npm package or used in development.
 */

import { resolve } from 'path';

export class WorkingDirConfig {
  /**
   * Get the current working directory for JTAG operations
   * Uses consumer's working directory (where they run the command)
   * This makes it work properly when installed as npm package
   */
  static getWorkingDir(): string {
    // If explicitly set via env var, use that
    if (process.env.JTAG_WORKING_DIR) {
      return resolve(process.env.JTAG_WORKING_DIR);
    }
    
    // For npm package usage: use consumer's working directory
    // For development: can be overridden with JTAG_WORKING_DIR
    return process.cwd();
  }
  
  /**
   * Set working directory programmatically
   */
  static setWorkingDir(dir: string): void {
    process.env.JTAG_WORKING_DIR = resolve(dir);
  }
  
  /**
   * Get .continuum base path (always relative to working directory)
   */
  static getContinuumPath(): string {
    return `${this.getWorkingDir()}/.continuum`;
  }
  
  /**
   * Get JTAG session logs path
   */
  static getLogsPath(sessionId: string = '00000000-0000-0000-0000-000000000000'): string {
    return `${this.getContinuumPath()}/jtag/sessions/system/${sessionId}/logs`;
  }
  
  /**
   * Get current user logs path
   */
  static getCurrentUserLogsPath(): string {
    return `${this.getContinuumPath()}/jtag/currentUser/logs`;
  }
  
  /**
   * Get screenshots path
   */
  static getScreenshotsPath(): string {
    return `${this.getContinuumPath()}/jtag/currentUser/screenshots`;
  }
  
  /**
   * Development mode: Switch to test-bench example 
   */
  static useTestBench(): void {
    this.setWorkingDir('examples/test-bench');
  }
  
  /**
   * Development mode: Switch to widget-ui example
   */
  static useWidgetUI(): void {
    this.setWorkingDir('examples/widget-ui');
  }
}