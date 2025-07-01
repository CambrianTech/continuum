/**
 * Session Manager - Focused module for browser session lifecycle management
 * Handles session creation, tracking, browser reuse decisions, and session cleanup
 */

import { BrowserConfig, DaemonManagedBrowser, BrowserPurpose } from '../types/index.js';

export interface PlacementStrategy {
  type: 'reuse' | 'add-tab' | 'create';
  browserId?: string;
  reasoning: string;
}

export interface SessionState {
  sessionId: string;
  browserId: string;
  url: string;
  created: Date;
  lastActivity: Date;
  config: BrowserConfig;
}

export class SessionManager {
  private browsers: Map<string, DaemonManagedBrowser>;
  private sessions = new Map<string, SessionState>();

  constructor(browsers: Map<string, DaemonManagedBrowser>) {
    this.browsers = browsers;
  }

  /**
   * Analyze placement strategy for new session
   */
  analyzePlacement(config: BrowserConfig): PlacementStrategy {
    // Check for reusable browsers
    if (config.requirements.isolation === 'shared') {
      const compatible = this.findCompatibleBrowser(config);
      if (compatible) {
        return {
          type: 'reuse',
          browserId: compatible.id,
          reasoning: 'Found compatible shared browser'
        };
      }
    }

    // Check resource constraints
    if (this.browsers.size >= this.getMaxBrowsers()) {
      const leastUsed = this.findLeastUsedBrowser();
      if (leastUsed && leastUsed.sessions.size < this.getMaxTabsPerBrowser()) {
        return {
          type: 'add-tab',
          browserId: leastUsed.id,
          reasoning: 'Resource limits reached, adding tab to existing browser'
        };
      }
    }

    // Default: create new browser
    return {
      type: 'create',
      reasoning: 'Creating new dedicated browser instance'
    };
  }

  /**
   * Calculate smart placement strategy for browser session
   */
  calculatePlacement(config: BrowserConfig): PlacementStrategy {
    // DevTools sessions always get new browser + new tab (isolation needed)
    if (config.requirements.devtools) {
      return {
        type: 'create',
        reasoning: `DevTools sessions always get fresh browser for clean debugging environment`
      };
    }
    
    // Check for existing browsers that could be reused (user sessions only)
    const compatibleBrowsers = Array.from(this.browsers.values()).filter(browser => 
      browser.state === 'ready' && 
      browser.config.purpose === config.purpose &&
      browser.resources.cpu < 70 && // Not overloaded
      !browser.config.requirements.devtools // Don't reuse DevTools browsers
    );
    
    if (compatibleBrowsers.length > 0) {
      const browser = compatibleBrowsers[0];
      
      // Decide between reuse and add-tab based on session count
      if (browser.sessions.size < 3) {
        return {
          type: 'add-tab',
          browserId: browser.id,
          reasoning: `Adding tab to existing ${config.purpose} browser with ${browser.sessions.size} sessions`
        };
      } else {
        return {
          type: 'reuse',
          browserId: browser.id,
          reasoning: `Reusing existing ${config.purpose} browser (refreshing current tab)`
        };
      }
    }
    
    // No compatible browsers found - create new one
    return {
      type: 'create',
      reasoning: `Creating new browser for ${config.purpose} (no compatible browsers available)`
    };
  }

  /**
   * Find existing browser for session/URL combination
   */
  findExistingBrowser(sessionId: string, url: string): DaemonManagedBrowser | null {
    for (const browser of this.browsers.values()) {
      if (browser.sessions.has(sessionId) && browser.state === 'ready') {
        return browser;
      }
    }
    return null;
  }

  /**
   * Determine if refreshing existing browser is sufficient
   */
  shouldRefreshExisting(browser: DaemonManagedBrowser, newConfig: any): boolean {
    // Refresh if same requirements and browser is healthy
    const devToolsMatch = (browser.config.requirements.devtools || false) === (newConfig?.requirements?.devtools || false);
    const browserHealthy = browser.state === 'ready' && browser.resources.cpu < 80;
    
    return devToolsMatch && browserHealthy;
  }

  /**
   * Add session to browser
   */
  addSession(sessionId: string, browserId: string, url: string, config: BrowserConfig): void {
    const browser = this.browsers.get(browserId);
    if (browser) {
      browser.sessions.add(sessionId);
      browser.lastActivity = new Date();
      
      this.sessions.set(sessionId, {
        sessionId,
        browserId,
        url,
        created: new Date(),
        lastActivity: new Date(),
        config
      });

      console.log(`📋 Session ${sessionId} added to browser ${browserId}`);
    }
  }

  /**
   * Remove session from browser
   */
  removeSession(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      const browser = this.browsers.get(session.browserId);
      if (browser) {
        browser.sessions.delete(sessionId);
        browser.lastActivity = new Date();
      }
      
      this.sessions.delete(sessionId);
      console.log(`🗑️ Session ${sessionId} removed`);
    }
  }

  /**
   * Get session information
   */
  getSession(sessionId: string): SessionState | null {
    return this.sessions.get(sessionId) || null;
  }

  /**
   * Get all sessions for a browser
   */
  getBrowserSessions(browserId: string): SessionState[] {
    return Array.from(this.sessions.values())
      .filter(session => session.browserId === browserId);
  }

  /**
   * Get all active sessions
   */
  getAllSessions(): SessionState[] {
    return Array.from(this.sessions.values());
  }

  /**
   * Clean up expired sessions
   */
  cleanupExpiredSessions(): number {
    const now = Date.now();
    const expirationTime = 24 * 60 * 60 * 1000; // 24 hours
    let cleaned = 0;

    for (const [sessionId, session] of this.sessions.entries()) {
      if (now - session.lastActivity.getTime() > expirationTime) {
        this.removeSession(sessionId);
        cleaned++;
      }
    }

    if (cleaned > 0) {
      console.log(`🧹 Cleaned up ${cleaned} expired sessions`);
    }

    return cleaned;
  }

  /**
   * Find compatible browser for config
   */
  private findCompatibleBrowser(config: BrowserConfig): DaemonManagedBrowser | null {
    for (const browser of this.browsers.values()) {
      if (this.isCompatible(browser, config)) {
        return browser;
      }
    }
    return null;
  }

  /**
   * Check if browser is compatible with config
   */
  private isCompatible(browser: DaemonManagedBrowser, config: BrowserConfig): boolean {
    // Check if browser can handle this session
    return browser.config.requirements.isolation === 'shared' &&
           browser.sessions.size < this.getMaxTabsPerBrowser() &&
           browser.state === 'ready' &&
           browser.config.purpose === config.purpose;
  }

  /**
   * Find least used browser
   */
  private findLeastUsedBrowser(): DaemonManagedBrowser | null {
    return Array.from(this.browsers.values())
      .sort((a, b) => a.sessions.size - b.sessions.size)[0] || null;
  }

  /**
   * Get maximum number of browsers
   */
  private getMaxBrowsers(): number {
    return 5;
  }

  /**
   * Get maximum tabs per browser
   */
  private getMaxTabsPerBrowser(): number {
    return 10;
  }

  /**
   * Update session activity
   */
  updateSessionActivity(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.lastActivity = new Date();
      
      const browser = this.browsers.get(session.browserId);
      if (browser) {
        browser.lastActivity = new Date();
      }
    }
  }

  /**
   * Get session statistics
   */
  getSessionStats() {
    const totalSessions = this.sessions.size;
    const browserSessionCounts = new Map<string, number>();
    
    for (const session of this.sessions.values()) {
      const count = browserSessionCounts.get(session.browserId) || 0;
      browserSessionCounts.set(session.browserId, count + 1);
    }

    return {
      totalSessions,
      browsersWithSessions: browserSessionCounts.size,
      averageSessionsPerBrowser: browserSessionCounts.size > 0 ? totalSessions / browserSessionCounts.size : 0,
      sessionsByBrowser: Object.fromEntries(browserSessionCounts)
    };
  }
}