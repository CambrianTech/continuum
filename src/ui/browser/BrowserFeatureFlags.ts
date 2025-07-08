/**
 * BrowserFeatureFlags - Safe migration controls for browser daemon system
 * 
 * Controls the gradual migration from monolithic continuum-browser.ts
 * to modular browser daemon architecture. Provides safe rollback
 * mechanisms and development-first testing.
 * 
 * Migration Strategy:
 * 1. All flags default to FALSE (legacy behavior)
 * 2. Enable individual daemons one at a time
 * 3. Test extensively before production enablement
 * 4. Easy rollback on any issues
 */

export class BrowserFeatureFlags {
  /**
   * Development environment detection
   */
  static get isDevelopment(): boolean {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      return process.env.NODE_ENV === 'development' || process.env.NODE_ENV === 'test';
    }
    
    return window.location.hostname === 'localhost' || 
           window.location.hostname === '127.0.0.1' ||
           window.location.hostname.endsWith('.local');
  }

  /**
   * Debug mode - enables detailed logging
   */
  static get isDebugMode(): boolean {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      return BrowserFeatureFlags.isDevelopment && process.env.CONTINUUM_DEBUG === 'true';
    }
    
    return BrowserFeatureFlags.isDevelopment && 
           (localStorage.getItem('continuum_debug') === 'true' ||
            new URLSearchParams(window.location.search).has('debug'));
  }

  /**
   * Console Daemon - handles console capture and forwarding
   * Phase 2 migration target (40% of monolithic code)
   */
  static get CONSOLE_DAEMON_ENABLED(): boolean {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      return process.env.CONTINUUM_CONSOLE_DAEMON === 'true';
    }
    
    // Start disabled - only enable for development testing
    const localOverride = localStorage.getItem('continuum_console_daemon');
    if (localOverride !== null) {
      return localOverride === 'true';
    }
    
    return false; // Default: use legacy implementation
  }

  /**
   * WebSocket Daemon - handles connection management
   * Phase 3 migration target (15% of monolithic code)
   */
  static get WEBSOCKET_DAEMON_ENABLED(): boolean {
    if (typeof window === 'undefined') {
      return process.env.CONTINUUM_WEBSOCKET_DAEMON === 'true';
    }
    
    const localOverride = localStorage.getItem('continuum_websocket_daemon');
    if (localOverride !== null) {
      return localOverride === 'true';
    }
    
    return false; // Default: use legacy implementation
  }

  /**
   * Command Daemon - handles command execution
   * Phase 4 migration target (10% of monolithic code)
   */
  static get COMMAND_DAEMON_ENABLED(): boolean {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      return process.env.CONTINUUM_COMMAND_DAEMON === 'true';
    }
    
    const localOverride = localStorage.getItem('continuum_command_daemon');
    if (localOverride !== null) {
      return localOverride === 'true';
    }
    
    return false; // Default: use legacy implementation
  }

  /**
   * Widget Daemon - handles widget lifecycle
   * Phase 5 migration target (5% of monolithic code)
   */
  static get WIDGET_DAEMON_ENABLED(): boolean {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      return process.env.CONTINUUM_WIDGET_DAEMON === 'true';
    }
    
    const localOverride = localStorage.getItem('continuum_widget_daemon');
    if (localOverride !== null) {
      return localOverride === 'true';
    }
    
    return false; // Default: use legacy implementation
  }

  /**
   * Session Daemon - handles session state management
   * Phase 6 migration target (3% of monolithic code)
   */
  static get SESSION_DAEMON_ENABLED(): boolean {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      return process.env.CONTINUUM_SESSION_DAEMON === 'true';
    }
    
    const localOverride = localStorage.getItem('continuum_session_daemon');
    if (localOverride !== null) {
      return localOverride === 'true';
    }
    
    return false; // Default: use legacy implementation
  }

  /**
   * Health Daemon - handles health validation
   * Phase 7 migration target (12% of monolithic code)
   */
  static get HEALTH_DAEMON_ENABLED(): boolean {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      return process.env.CONTINUUM_HEALTH_DAEMON === 'true';
    }
    
    const localOverride = localStorage.getItem('continuum_health_daemon');
    if (localOverride !== null) {
      return localOverride === 'true';
    }
    
    return false; // Default: use legacy implementation
  }

  /**
   * Master switch - enable all daemon features
   * Only for advanced testing
   */
  static get ALL_DAEMONS_ENABLED(): boolean {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      return process.env.CONTINUUM_ALL_DAEMONS === 'true';
    }
    
    const localOverride = localStorage.getItem('continuum_all_daemons');
    if (localOverride === 'true') {
      return true;
    }
    
    return false; // Default: use legacy implementation
  }

  /**
   * Emergency rollback - disable all new features
   * Call this if any daemon causes issues
   */
  static disableAllFeatures(): void {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      console.warn('🚨 disableAllFeatures() not available in Node.js environment');
      console.warn('Use environment variables instead: CONTINUUM_*_DAEMON=false');
      return;
    }
    
    const flags = [
      'continuum_console_daemon',
      'continuum_websocket_daemon', 
      'continuum_command_daemon',
      'continuum_widget_daemon',
      'continuum_session_daemon',
      'continuum_health_daemon',
      'continuum_all_daemons'
    ];

    flags.forEach(flag => {
      localStorage.setItem(flag, 'false');
    });

    console.warn('🚨 Browser daemon system disabled - rolled back to legacy implementation');
    console.warn('Reload page to apply rollback');
  }

  /**
   * Development testing helpers
   */
  static enableConsoleeDaemonTesting(): void {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      console.warn('enableConsoleeDaemonTesting() not available in Node.js environment');
      console.warn('Use environment variable instead: CONTINUUM_CONSOLE_DAEMON=true');
      return;
    }
    
    if (!BrowserFeatureFlags.isDevelopment) {
      console.warn('Console daemon testing only available in development');
      return;
    }
    
    localStorage.setItem('continuum_console_daemon', 'true');
    console.log('✅ Console daemon testing enabled - reload page to apply');
  }

  static enableWebSocketDaemonTesting(): void {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      console.warn('enableWebSocketDaemonTesting() not available in Node.js environment');
      console.warn('Use environment variable instead: CONTINUUM_WEBSOCKET_DAEMON=true');
      return;
    }
    
    if (!BrowserFeatureFlags.isDevelopment) {
      console.warn('WebSocket daemon testing only available in development');
      return;
    }
    
    localStorage.setItem('continuum_websocket_daemon', 'true');
    console.log('✅ WebSocket daemon testing enabled - reload page to apply');
  }

  static enableCommandDaemonTesting(): void {
    // Handle Node.js environment for testing
    if (typeof window === 'undefined') {
      console.warn('enableCommandDaemonTesting() not available in Node.js environment');
      console.warn('Use environment variable instead: CONTINUUM_COMMAND_DAEMON=true');
      return;
    }
    
    if (!BrowserFeatureFlags.isDevelopment) {
      console.warn('Command daemon testing only available in development');
      return;
    }
    
    localStorage.setItem('continuum_command_daemon', 'true');
    console.log('✅ Command daemon testing enabled - reload page to apply');
  }

  /**
   * Get current feature flag status
   */
  static getStatus() {
    return {
      environment: BrowserFeatureFlags.isDevelopment ? 'development' : 'production',
      debugMode: BrowserFeatureFlags.isDebugMode,
      flags: {
        consoleDaemon: BrowserFeatureFlags.CONSOLE_DAEMON_ENABLED,
        webSocketDaemon: BrowserFeatureFlags.WEBSOCKET_DAEMON_ENABLED,
        commandDaemon: BrowserFeatureFlags.COMMAND_DAEMON_ENABLED,
        widgetDaemon: BrowserFeatureFlags.WIDGET_DAEMON_ENABLED,
        sessionDaemon: BrowserFeatureFlags.SESSION_DAEMON_ENABLED,
        healthDaemon: BrowserFeatureFlags.HEALTH_DAEMON_ENABLED,
        allDaemons: BrowserFeatureFlags.ALL_DAEMONS_ENABLED
      }
    };
  }

  /**
   * Log current status to console
   */
  static logStatus(): void {
    console.log('🏁 Browser Feature Flags Status:', BrowserFeatureFlags.getStatus());
  }
}

// Make available globally for debugging
if (typeof window !== 'undefined' && BrowserFeatureFlags.isDevelopment) {
  (window as any).BrowserFeatureFlags = BrowserFeatureFlags;
}