/**
 * Tab Manager
 * Handles browser tab management, auto-reload, and focus control
 */

class TabManager {
  constructor() {
    this.activeTabs = new Map(); // tabId -> {ws, sessionId, version, url, timestamp}
    this.focusTimeouts = new Map(); // sessionId -> timeout
  }

  /**
   * Register a new tab connection
   */
  async registerTab(ws, data) {
    const { tabId, version, url, timestamp } = data;
    const sessionId = this.getSessionId(ws);
    
    console.log(`📱 Tab registration: ${tabId} (v${version})`);
    
    // Check if this is a duplicate tab for the same user
    const existingTab = this.findExistingTab(url);
    
    if (existingTab && existingTab.tabId !== tabId) {
      console.log(`🔄 Duplicate tab detected, focusing existing tab: ${existingTab.tabId}`);
      
      // Send focus command to existing tab
      await this.focusTab(existingTab.tabId);
      
      // Send refresh command to new tab to close it gracefully
      ws.send(JSON.stringify({
        type: 'tabRefresh',
        message: 'Focusing existing tab...'
      }));
      
      return false; // Don't register this tab
    }
    
    // Register the new tab
    this.activeTabs.set(tabId, {
      ws,
      sessionId,
      version,
      url,
      timestamp,
      lastActivity: Date.now()
    });
    
    // Send version update check
    await this.checkVersionUpdate(tabId, version);
    
    console.log(`✅ Tab registered successfully: ${tabId}`);
    return true;
  }

  /**
   * Find existing tab for the same URL/user
   */
  findExistingTab(url) {
    for (const [tabId, tabData] of this.activeTabs) {
      if (tabData.url === url && this.isTabActive(tabData)) {
        return { tabId, ...tabData };
      }
    }
    return null;
  }

  /**
   * Check if tab is still active (recent activity)
   */
  isTabActive(tabData) {
    const fiveMinutesAgo = Date.now() - (5 * 60 * 1000);
    return tabData.lastActivity > fiveMinutesAgo;
  }

  /**
   * Focus a specific tab
   */
  async focusTab(tabId) {
    const tabData = this.activeTabs.get(tabId);
    if (!tabData) {
      console.log(`❌ Tab not found: ${tabId}`);
      return false;
    }

    try {
      tabData.ws.send(JSON.stringify({
        type: 'tabFocus',
        message: 'Bringing tab to focus...'
      }));
      
      // Update last activity
      tabData.lastActivity = Date.now();
      
      console.log(`🎯 Focus command sent to tab: ${tabId}`);
      return true;
    } catch (error) {
      console.error(`❌ Failed to focus tab ${tabId}:`, error);
      this.unregisterTab(tabId);
      return false;
    }
  }

  /**
   * Check for version updates and trigger auto-reload
   */
  async checkVersionUpdate(tabId, clientVersion) {
    // Clear require cache to get fresh version info
    delete require.cache[require.resolve('../../package.json')];
    const packageInfo = require('../../package.json');
    const serverVersion = packageInfo.version;
    
    if (clientVersion !== serverVersion) {
      console.log(`🔄 Version mismatch detected for tab ${tabId}: ${clientVersion} -> ${serverVersion}`);
      
      const tabData = this.activeTabs.get(tabId);
      if (tabData) {
        try {
          tabData.ws.send(JSON.stringify({
            type: 'versionUpdate',
            version: serverVersion,
            message: `Updating from v${clientVersion} to v${serverVersion}...`
          }));
          
          console.log(`📤 Version update notification sent to tab: ${tabId}`);
        } catch (error) {
          console.error(`❌ Failed to send version update to tab ${tabId}:`, error);
          this.unregisterTab(tabId);
        }
      }
    }
  }

  /**
   * Broadcast version update to all tabs
   */
  async broadcastVersionUpdate(newVersion) {
    console.log(`📢 Broadcasting version update to all tabs: v${newVersion}`);
    
    const updatePromises = Array.from(this.activeTabs.entries()).map(async ([tabId, tabData]) => {
      if (tabData.version !== newVersion) {
        try {
          tabData.ws.send(JSON.stringify({
            type: 'versionUpdate',
            version: newVersion,
            message: `Continuum updated to v${newVersion} - Reloading...`
          }));
          
          console.log(`✅ Version update sent to tab: ${tabId}`);
        } catch (error) {
          console.error(`❌ Failed to send version update to tab ${tabId}:`, error);
          this.unregisterTab(tabId);
        }
      }
    });
    
    await Promise.all(updatePromises);
  }

  /**
   * Unregister a tab (cleanup)
   */
  unregisterTab(tabId) {
    if (this.activeTabs.has(tabId)) {
      console.log(`🗑️ Unregistering tab: ${tabId}`);
      this.activeTabs.delete(tabId);
    }
  }

  /**
   * Clean up tabs for a disconnected session
   */
  cleanupSession(sessionId) {
    const tabsToRemove = [];
    
    for (const [tabId, tabData] of this.activeTabs) {
      if (tabData.sessionId === sessionId) {
        tabsToRemove.push(tabId);
      }
    }
    
    tabsToRemove.forEach(tabId => this.unregisterTab(tabId));
    
    if (tabsToRemove.length > 0) {
      console.log(`🧹 Cleaned up ${tabsToRemove.length} tabs for session: ${sessionId}`);
    }
  }

  /**
   * Get session ID from WebSocket (helper method)
   */
  getSessionId(ws) {
    // Find session ID by matching WebSocket instance
    // This is a simple implementation - could be improved with better session tracking
    return `session_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * Get status of all active tabs
   */
  getStatus() {
    const tabs = Array.from(this.activeTabs.entries()).map(([tabId, tabData]) => ({
      tabId,
      version: tabData.version,
      url: tabData.url,
      timestamp: tabData.timestamp,
      lastActivity: tabData.lastActivity,
      isActive: this.isTabActive(tabData)
    }));

    return {
      totalTabs: tabs.length,
      activeTabs: tabs.filter(tab => tab.isActive).length,
      tabs
    };
  }
}

module.exports = TabManager;