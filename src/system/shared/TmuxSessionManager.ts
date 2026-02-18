/**
 * Tmux Session Manager
 * 
 * Provides workdir-specific tmux session naming to prevent conflicts
 * when running multiple JTAG instances simultaneously.
 */

import { WorkingDirConfig } from '../core/config/WorkingDirConfig';

export class TmuxSessionManager {
  
  /**
   * Generate a unique tmux session name for the current working directory
   * Format: jtag-{workdir-sanitized}-{short-hash}
   */
  static getSessionName(): string {
    try {
      const workingDir = WorkingDirConfig.getWorkingDir();
      const exampleName = this.getActiveInstanceName();
      
      // Create a short, unique identifier based on the working directory path
      const pathHash = this.createShortHash(workingDir);
      
      // Use example name if available, otherwise use the last part of working dir
      const baseName = exampleName || this.extractBaseName(workingDir);
      
      // Sanitize for tmux session name (alphanumeric, hyphens only)
      const sanitizedBaseName = baseName.replace(/[^a-zA-Z0-9-]/g, '-').toLowerCase();
      
      return `jtag-${sanitizedBaseName}-${pathHash}`;
    } catch (error) {
      // Fallback to PID-based naming if config fails
      return `jtag-fallback-${process.pid}`;
    }
  }
  
  /**
   * Get session name for a specific working directory (useful for cross-context operations)
   */
  static getSessionNameForWorkdir(workdir: string): string {
    const pathHash = this.createShortHash(workdir);
    const baseName = this.extractBaseName(workdir);
    const sanitizedBaseName = baseName.replace(/[^a-zA-Z0-9-]/g, '-').toLowerCase();
    
    return `jtag-${sanitizedBaseName}-${pathHash}`;
  }
  
  /**
   * List all JTAG tmux sessions
   */
  static async listJtagSessions(): Promise<string[]> {
    const { spawn } = await import('child_process');
    const { promisify } = await import('util');
    const execAsync = promisify(spawn);
    
    try {
      const result = await execAsync('tmux', ['list-sessions', '-F', '#{session_name}']);
      const sessions = result.stdout?.toString().trim().split('\n') || [];
      return sessions.filter(session => session.startsWith('jtag-'));
    } catch {
      return []; // No tmux server running or no sessions
    }
  }
  
  /**
   * Kill all JTAG sessions (useful for cleanup)
   */
  static async killAllJtagSessions(): Promise<void> {
    const sessions = await this.listJtagSessions();
    const { spawn } = await import('child_process');
    
    for (const session of sessions) {
      try {
        spawn('tmux', ['kill-session', '-t', session], { stdio: 'ignore' });
      } catch {
        // Ignore errors when killing sessions
      }
    }
  }
  
  // Create a short hash from a string (6 characters, URL-safe)
  private static createShortHash(input: string): string {
    let hash = 0;
    for (let i = 0; i < input.length; i++) {
      const char = input.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash; // Convert to 32-bit integer
    }
    
    // Convert to base36 and take first 6 characters
    return Math.abs(hash).toString(36).substring(0, 6);
  }
  
  // Extract base name from path (last directory component)
  private static extractBaseName(workdir: string): string {
    const parts = workdir.split('/').filter(p => p.length > 0);
    return parts[parts.length - 1] || 'unknown';
  }

  /**
   * Get active instance name from package.json
   * Used by infrastructure scripts that don't have JTAGContext
   */
  private static getActiveInstanceName(): string {
    try {
      const fs = eval('require')('fs');
      const path = eval('require')('path');
      const packagePath = path.join(__dirname, '../../package.json');
      const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf-8'));
      
      return packageData.active_example || 'unknown';
    } catch (error) {
      console.warn(`⚠️ TmuxSessionManager: Failed to load active instance name: ${(error as Error).message}`);
      return 'unknown';
    }
  }
}