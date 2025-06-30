/**
 * Safe Browser Manager - Critical browser operations with bulletproof safety
 * 
 * CRITICAL REQUIREMENTS:
 * - Never crash the system
 * - Safe browser launching (no tab spam)
 * - Reliable connection detection
 * - Fail gracefully with clear errors
 * - No side effects on failure
 */

export interface BrowserConnection {
  isConnected: boolean;
  connectionId: string | null;
  lastSeen: Date | null;
  isHealthy: boolean;
}

export interface BrowserLaunchResult {
  success: boolean;
  action: 'launched' | 'already_running' | 'failed';
  error?: string;
  details?: {
    pid?: number;
    url?: string;
    reason?: string;
  };
}

export class SafeBrowserManager {
  private connection: BrowserConnection = {
    isConnected: false,
    connectionId: null,
    lastSeen: null,
    isHealthy: false
  };

  private launchInProgress = false;
  private readonly maxLaunchAttempts = 3;
  private launchAttempts = 0;

  /**
   * Get current browser connection status (always safe)
   */
  getConnectionStatus(): BrowserConnection {
    return { ...this.connection }; // Return copy to prevent mutation
  }

  /**
   * Register browser connection (safe state update)
   */
  registerConnection(connectionId: string): void {
    try {
      this.connection = {
        isConnected: true,
        connectionId,
        lastSeen: new Date(),
        isHealthy: true
      };
      console.log(`🔌 Browser connected safely: ${connectionId}`);
    } catch (error) {
      console.error('❌ Failed to register browser connection:', error);
    }
  }

  /**
   * Register browser disconnection (safe state update)
   */
  registerDisconnection(connectionId?: string): void {
    try {
      const previousId = this.connection.connectionId;
      this.connection = {
        isConnected: false,
        connectionId: null,
        lastSeen: this.connection.lastSeen, // Keep last seen time
        isHealthy: false
      };
      console.log(`🔌 Browser disconnected safely: ${connectionId || previousId || 'unknown'}`);
    } catch (error) {
      console.error('❌ Failed to register browser disconnection:', error);
    }
  }

  /**
   * Safe browser launching with anti-spam protection
   */
  async launchBrowser(url: string = 'http://localhost:9000'): Promise<BrowserLaunchResult> {
    // Safety check: prevent concurrent launches
    if (this.launchInProgress) {
      return {
        success: false,
        action: 'failed',
        error: 'Launch already in progress',
        details: { reason: 'concurrent_launch_prevented' }
      };
    }

    // Safety check: don't launch if already connected
    if (this.connection.isConnected) {
      return {
        success: true,
        action: 'already_running',
        details: { 
          reason: 'browser_already_connected',
          url 
        }
      };
    }

    // Safety check: respect launch attempt limits
    if (this.launchAttempts >= this.maxLaunchAttempts) {
      return {
        success: false,
        action: 'failed',
        error: `Max launch attempts (${this.maxLaunchAttempts}) exceeded`,
        details: { reason: 'max_attempts_reached' }
      };
    }

    this.launchInProgress = true;
    this.launchAttempts++;

    try {
      console.log(`🚀 Safely launching browser (attempt ${this.launchAttempts}/${this.maxLaunchAttempts}): ${url}`);

      const launchResult = await this.performSafeLaunch(url);
      
      this.launchInProgress = false;
      
      if (launchResult.success) {
        console.log('✅ Browser launched successfully');
        // Reset attempts on success
        this.launchAttempts = 0;
      }

      return launchResult;

    } catch (error) {
      this.launchInProgress = false;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      console.error(`❌ Browser launch failed (attempt ${this.launchAttempts}):`, errorMessage);
      
      return {
        success: false,
        action: 'failed',
        error: `Launch failed: ${errorMessage}`,
        details: { 
          reason: 'launch_exception',
          url 
        }
      };
    }
  }

  /**
   * Perform actual browser launch with platform safety
   */
  private async performSafeLaunch(url: string): Promise<BrowserLaunchResult> {
    try {
      const { spawn } = await import('child_process');
      
      // Get safe platform-specific command
      const command = this.getSafeLaunchCommand();
      
      if (!command) {
        return {
          success: false,
          action: 'failed',
          error: 'Unsupported platform for browser launch',
          details: { reason: 'unsupported_platform' }
        };
      }

      // Spawn process safely with timeouts and error handling
      const process = spawn(command.cmd, [...command.args, url], {
        detached: true,
        stdio: 'ignore', // Prevent process attachment
        timeout: 10000 // 10 second timeout
      });

      // Detach process immediately to prevent hanging
      process.unref();

      const pid = process.pid;

      // Return immediately - don't wait for browser to fully start
      return {
        success: true,
        action: 'launched',
        details: {
          pid,
          url,
          reason: 'process_spawned'
        }
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        action: 'failed',
        error: errorMessage,
        details: { reason: 'spawn_failed' }
      };
    }
  }

  /**
   * Get safe platform-specific browser launch command
   */
  private getSafeLaunchCommand(): { cmd: string; args: string[] } | null {
    const platform = process.platform;
    
    switch (platform) {
      case 'darwin': // macOS - use open command (very safe)
        return { cmd: 'open', args: ['-a', 'Safari'] };
      
      case 'win32': // Windows - use start command
        return { cmd: 'cmd', args: ['/c', 'start', '""'] };
      
      case 'linux': // Linux - use xdg-open (standard)
        return { cmd: 'xdg-open', args: [] };
      
      default:
        console.warn(`⚠️ Unsupported platform: ${platform}`);
        return null;
    }
  }

  /**
   * Wait for browser connection with timeout (safe blocking)
   */
  async waitForConnection(timeoutMs: number = 10000): Promise<boolean> {
    const startTime = Date.now();
    
    return new Promise((resolve) => {
      const checkInterval = setInterval(() => {
        // Check if connected
        if (this.connection.isConnected) {
          clearInterval(checkInterval);
          resolve(true);
          return;
        }

        // Check timeout
        if (Date.now() - startTime >= timeoutMs) {
          clearInterval(checkInterval);
          resolve(false);
          return;
        }
      }, 100); // Check every 100ms
    });
  }

  /**
   * Ensure browser is ready (launch if needed, wait for connection)
   */
  async ensureBrowserReady(timeoutMs: number = 15000): Promise<{ ready: boolean; error?: string }> {
    try {
      // If already connected, we're ready
      if (this.connection.isConnected) {
        return { ready: true };
      }

      // Launch browser if needed
      const launchResult = await this.launchBrowser();
      
      if (!launchResult.success && launchResult.action !== 'already_running') {
        return { 
          ready: false, 
          error: launchResult.error || 'Launch failed' 
        };
      }

      // Wait for connection
      const connected = await this.waitForConnection(timeoutMs);
      
      if (connected) {
        return { ready: true };
      } else {
        return { 
          ready: false, 
          error: `Browser connection timeout after ${timeoutMs}ms` 
        };
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { 
        ready: false, 
        error: `Browser ready check failed: ${errorMessage}` 
      };
    }
  }

  /**
   * Reset launch attempts (for testing or manual recovery)
   */
  resetLaunchAttempts(): void {
    this.launchAttempts = 0;
    this.launchInProgress = false;
    console.log('🔄 Browser launch attempts reset');
  }

  /**
   * Check if browser connection is healthy
   */
  isConnectionHealthy(stalenessThresholdMs: number = 30000): boolean {
    if (!this.connection.isConnected || !this.connection.lastSeen) {
      return false;
    }

    const now = Date.now();
    const lastSeenTime = this.connection.lastSeen.getTime();
    const age = now - lastSeenTime;

    return age < stalenessThresholdMs;
  }

  /**
   * Update connection health status
   */
  updateConnectionHealth(): void {
    this.connection.isHealthy = this.isConnectionHealthy();
  }
}