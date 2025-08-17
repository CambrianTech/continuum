/**
 * Working Directory Configuration - Single Source of Truth
 * 
 * Configures where JTAG creates .continuum directories and serves content.
 * Switch between test-bench, widget-ui, or any other directory from one place.
 */

export class WorkingDirConfig {
  private static readonly DEFAULT_DIR = 'examples/test-bench';
  
  /**
   * Get the current working directory for JTAG operations
   * Priority: env var > package.json config > default
   */
  static getWorkingDir(): string {
    if (process.env.JTAG_WORKING_DIR) {
      return process.env.JTAG_WORKING_DIR;
    }
    
    try {
      const packageJson = require('../../../package.json');
      return packageJson.config?.workingDir || this.DEFAULT_DIR;
    } catch {
      return this.DEFAULT_DIR;
    }
  }
  
  /**
   * Set working directory programmatically
   */
  static setWorkingDir(dir: string): void {
    process.env.JTAG_WORKING_DIR = dir;
  }
  
  /**
   * Get .continuum base path for current working directory
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
   * Switch to widget-ui mode
   */
  static useWidgetUI(): void {
    this.setWorkingDir('examples/widget-ui');
  }
  
  /**
   * Switch to test-bench mode (default)
   */
  static useTestBench(): void {
    this.setWorkingDir('examples/test-bench');
  }
}