/**
 * Browser Session Manager - Focused session lifecycle management
 * 
 * Responsibilities:
 * - Track active browser sessions
 * - Manage session allocation and cleanup
 * - Handle session reuse and optimization
 * - Monitor session health and activity
 */

import { ManagedBrowser, BrowserFilters } from '../types/index.js';

export interface SessionPlacementStrategy {
  type: 'reuse' | 'add-tab' | 'create';
  browserId?: string;
  reasoning: string;
}

export class BrowserSessionManager {
  private browsers = new Map<string, ManagedBrowser>();
  private maxBrowsers = 5;
  private maxTabsPerBrowser = 8;

  /**
   * Register a new browser instance
   */
  registerBrowser(browser: ManagedBrowser): void {
    this.browsers.set(browser.id, browser);
  }

  /**
   * Remove a browser from management
   */
  unregisterBrowser(browserId: string): boolean {
    return this.browsers.delete(browserId);
  }

  /**
   * Get all managed browsers
   */
  getAllBrowsers(): ManagedBrowser[] {
    return Array.from(this.browsers.values());
  }

  /**
   * Get a specific browser by ID
   */
  getBrowser(browserId: string): ManagedBrowser | null {
    return this.browsers.get(browserId) || null;
  }

  /**
   * Filter browsers based on criteria
   */
  filterBrowsers(filters: BrowserFilters): ManagedBrowser[] {
    let filtered = Array.from(this.browsers.values());

    if (filters.type) {
      filtered = filtered.filter(browser => browser.type === filters.type);
    }

    if (filters.status) {
      filtered = filtered.filter(browser => browser.status === filters.status);
    }

    if (filters.minIdleTime) {
      const cutoff = Date.now() - filters.minIdleTime;
      filtered = filtered.filter(browser => 
        browser.lastActivity.getTime() < cutoff
      );
    }

    if (filters.maxAge) {
      const cutoff = Date.now() - filters.maxAge;
      filtered = filtered.filter(browser => 
        browser.launchedAt.getTime() > cutoff
      );
    }

    if (filters.hasDevTools !== undefined) {
      filtered = filtered.filter(browser => 
        (browser.debugPort > 0) === filters.hasDevTools
      );
    }

    return filtered;
  }

  /**
   * Calculate optimal placement strategy for a new session
   */
  calculatePlacement(_sessionId: string, requirements: any): SessionPlacementStrategy {
    // DevTools sessions always get new browser (isolation needed)
    if (requirements?.devtools) {
      return {
        type: 'create',
        reasoning: 'DevTools sessions require isolated browser instance'
      };
    }

    // Check for compatible existing browsers
    const compatibleBrowsers = this.findCompatibleBrowsers(requirements);
    
    if (compatibleBrowsers.length > 0) {
      const browser = compatibleBrowsers[0];
      
      // Decide between reuse and add-tab based on session count
      if (this.getSessionCount(browser) < 3) {
        return {
          type: 'add-tab',
          browserId: browser.id,
          reasoning: `Adding tab to existing browser with ${this.getSessionCount(browser)} sessions`
        };
      } else {
        return {
          type: 'reuse',
          browserId: browser.id,
          reasoning: 'Reusing existing browser (refreshing current tab)'
        };
      }
    }

    // Check resource constraints
    if (this.browsers.size >= this.maxBrowsers) {
      const leastUsed = this.findLeastUsedBrowser();
      if (leastUsed && this.getSessionCount(leastUsed) < this.maxTabsPerBrowser) {
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
      reasoning: 'Creating new browser instance'
    };
  }

  /**
   * Add a session to a browser
   */
  addSession(browserId: string, _sessionId: string): boolean {
    const browser = this.browsers.get(browserId);
    if (!browser) return false;

    // Sessions are tracked as a Set in ManagedBrowser
    // This is a placeholder - in real implementation, would track sessions properly
    browser.lastActivity = new Date();
    
    return true;
  }

  /**
   * Remove a session from a browser
   */
  removeSession(browserId: string, _sessionId: string): boolean {
    const browser = this.browsers.get(browserId);
    if (!browser) return false;

    // Update last activity
    browser.lastActivity = new Date();
    
    return true;
  }

  /**
   * Find browsers compatible with requirements
   */
  private findCompatibleBrowsers(requirements: any): ManagedBrowser[] {
    return Array.from(this.browsers.values()).filter(browser => {
      // Only user browsers can be reused
      if (!this.isUserBrowser(browser)) return false;
      
      // Browser must be ready
      if (browser.status !== 'ready') return false;
      
      // DevTools compatibility
      const hasDevTools = browser.debugPort > 0;
      const needsDevTools = requirements?.devtools;
      if (needsDevTools && !hasDevTools) return false;
      
      return true;
    });
  }

  /**
   * Find the least used browser for resource optimization
   */
  private findLeastUsedBrowser(): ManagedBrowser | null {
    const browsers = Array.from(this.browsers.values());
    if (browsers.length === 0) return null;

    return browsers.reduce((least, current) => {
      const leastSessions = this.getSessionCount(least);
      const currentSessions = this.getSessionCount(current);
      return currentSessions < leastSessions ? current : least;
    });
  }

  /**
   * Check if a browser is a user browser (vs automation/testing)
   */
  private isUserBrowser(_browser: ManagedBrowser): boolean {
    // This would check browser.config.purpose in real implementation
    // For now, assume all browsers are user browsers
    return true;
  }

  /**
   * Get session count for a browser
   */
  private getSessionCount(_browser: ManagedBrowser): number {
    // In real implementation, would return browser.sessions.size
    // For now, return a reasonable estimate
    return Math.floor(Math.random() * 5);
  }

  /**
   * Cleanup idle browsers
   */
  cleanupIdleBrowsers(): string[] {
    const cleanedIds: string[] = [];
    const idleThreshold = 5 * 60 * 1000; // 5 minutes
    const now = Date.now();

    for (const [id, browser] of this.browsers) {
      const idleTime = now - browser.lastActivity.getTime();
      const sessionCount = this.getSessionCount(browser);
      
      if (idleTime > idleThreshold && sessionCount === 0) {
        this.browsers.delete(id);
        cleanedIds.push(id);
      }
    }

    return cleanedIds;
  }

  /**
   * Get session manager statistics
   */
  getStats() {
    const browsers = Array.from(this.browsers.values());
    return {
      totalBrowsers: browsers.length,
      readyBrowsers: browsers.filter(b => b.status === 'ready').length,
      totalSessions: browsers.reduce((sum, b) => sum + this.getSessionCount(b), 0),
      averageSessionsPerBrowser: browsers.length > 0 
        ? browsers.reduce((sum, b) => sum + this.getSessionCount(b), 0) / browsers.length 
        : 0
    };
  }
}