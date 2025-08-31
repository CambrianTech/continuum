/**
 * Daemon Integration Module
 * 
 * Provides abstracted access to JTAG daemon operations for widgets.
 * Handles JTAG system communication, command execution, and error handling.
 */

import type { FileLoadResult } from '../../../../commands/file/load/shared/FileLoadTypes';
import type { FileSaveResult } from '../../../../commands/file/save/shared/FileSaveTypes';
import type { ScreenshotResult } from '../../../../commands/screenshot/shared/ScreenshotTypes';

export interface DaemonConfig {
  enableDebugging: boolean;
  retryAttempts: number;
  timeoutMs: number;
}

export interface JTAGSystemState {
  isReady: boolean;
  isConnected: boolean;
  lastOperation: string;
  operationCount: number;
}

export class DaemonIntegration {
  private config: DaemonConfig;
  private state: JTAGSystemState;
  private widgetName: string;

  constructor(widgetName: string, config: Partial<DaemonConfig> = {}) {
    this.widgetName = widgetName;
    
    this.config = {
      enableDebugging: false,
      retryAttempts: 3,
      timeoutMs: 10000,
      ...config
    };

    this.state = {
      isReady: false,
      isConnected: false,
      lastOperation: '',
      operationCount: 0
    };
  }

  /**
   * Execute JTAG operation with proper typing and error handling
   */
  async executeOperation<T>(command: string, params?: Record<string, any>): Promise<T> {
    try {
      this.state.lastOperation = command;
      this.state.operationCount++;

      // Wait for JTAG system to be ready
      await this.ensureSystemReady();
      
      // Get the JTAG client from window
      const jtagClient = (window as any).jtag;
      if (!jtagClient || !jtagClient.commands) {
        throw new Error('JTAG client not available even after system ready event');
      }
      
      // Execute command through the global JTAG system
      const result = await jtagClient.commands[command](params);
      
      if (this.config.enableDebugging) {
        console.log(`🔧 ${this.widgetName}: JTAG operation ${command} completed:`, result);
      }
      
      return result as T;
      
    } catch (error) {
      console.error(`❌ ${this.widgetName}: JTAG operation ${command} failed:`, error);
      throw error;
    }
  }

  /**
   * Load file via JTAG file/load command
   */
  async loadFile(filepath: string): Promise<FileLoadResult> {
    return this.executeOperation<FileLoadResult>('file/load', { filepath });
  }

  /**
   * Save file via JTAG file/save command
   */
  async saveFile(filepath: string, content: string | Blob, createDirs: boolean = true): Promise<FileSaveResult> {
    return this.executeOperation<FileSaveResult>('file/save', {
      filepath,
      content,
      createDirs
    });
  }

  /**
   * Take screenshot via JTAG screenshot command
   */
  async takeScreenshot(
    filename: string,
    querySelector?: string,
    includeContext: boolean = true
  ): Promise<ScreenshotResult> {
    return this.executeOperation<ScreenshotResult>('screenshot', {
      filename,
      querySelector,
      includeContext
    });
  }

  /**
   * Execute arbitrary browser code via JTAG exec command
   */
  async executeBrowserCode(code: string, environment: 'browser' | 'server' = 'browser'): Promise<any> {
    return this.executeOperation('exec', { code, environment });
  }

  /**
   * Get JTAG system health status
   */
  async getSystemHealth(): Promise<any> {
    return this.executeOperation('health');
  }

  /**
   * Test JTAG system connectivity
   */
  async ping(): Promise<any> {
    return this.executeOperation('ping');
  }

  /**
   * Ensure JTAG system is ready before operations
   */
  private async ensureSystemReady(): Promise<void> {
    if (this.state.isReady) {
      return;
    }

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`JTAG system ready timeout after ${this.config.timeoutMs}ms`));
      }, this.config.timeoutMs);

      const checkReady = () => {
        const jtagClient = (window as any).jtag;
        if (jtagClient && jtagClient.commands) {
          clearTimeout(timeout);
          this.state.isReady = true;
          this.state.isConnected = true;
          
          if (this.config.enableDebugging) {
            console.log(`✅ ${this.widgetName}: JTAG system ready`);
          }
          
          resolve();
        } else {
          setTimeout(checkReady, 100);
        }
      };

      if (this.config.enableDebugging) {
        console.log(`⏳ ${this.widgetName}: Waiting for JTAG system to be ready...`);
      }

      checkReady();
    });
  }

  /**
   * Get daemon integration state
   */
  getState(): JTAGSystemState {
    return { ...this.state };
  }

  /**
   * Update configuration
   */
  updateConfig(newConfig: Partial<DaemonConfig>): void {
    this.config = { ...this.config, ...newConfig };
  }

  /**
   * Check if system is ready
   */
  isSystemReady(): boolean {
    return this.state.isReady && this.state.isConnected;
  }

  /**
   * Reset connection state (useful for error recovery)
   */
  resetConnection(): void {
    this.state.isReady = false;
    this.state.isConnected = false;
    
    if (this.config.enableDebugging) {
      console.log(`🔄 ${this.widgetName}: Reset JTAG connection state`);
    }
  }

  /**
   * Get operation statistics
   */
  getStats(): { operationCount: number; lastOperation: string; uptime: number } {
    return {
      operationCount: this.state.operationCount,
      lastOperation: this.state.lastOperation,
      uptime: Date.now() // This would be more meaningful with a start timestamp
    };
  }
}