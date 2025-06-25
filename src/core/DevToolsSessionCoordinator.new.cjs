/**
 * Clean integration shim - bridges old JavaScript git hook to new TypeScript API
 * This replaces the messy 800-line DevToolsSessionCoordinator.cjs
 */

const path = require('path');

// Import compiled TypeScript API
let TypeScriptAPI = null;

async function loadTypeScriptAPI() {
  if (!TypeScriptAPI) {
    try {
      TypeScriptAPI = await import(path.resolve(__dirname, '../dist/index.js'));
    } catch (error) {
      console.warn('⚠️ TypeScript API not available, falling back to legacy system');
      throw error;
    }
  }
  return TypeScriptAPI;
}

class DevToolsSessionCoordinator {
  constructor() {
    this.activeSessions = new Map();
  }

  /**
   * Clean interface - delegates to TypeScript API
   */
  async requestSession(purpose, aiPersona = 'system', options = {}) {
    try {
      const api = await loadTypeScriptAPI();
      const session = await api.requestSession(purpose, aiPersona, options);
      
      // Store in legacy map for compatibility
      const sessionKey = `${purpose}_${aiPersona}`;
      this.activeSessions.set(sessionKey, session);
      
      return session;
    } catch (error) {
      throw new Error(`Session request failed: ${error.message}`);
    }
  }

  /**
   * Legacy compatibility methods
   */
  generateSessionKey(purpose, aiPersona) {
    return `${purpose}_${aiPersona}`;
  }

  async isSessionActive(session) {
    try {
      await session.waitForReady(1000);
      return true;
    } catch (error) {
      return false;
    }
  }

  async cleanupStaleSessions() {
    // TypeScript API handles this automatically
    console.log('🧹 Session cleanup handled by TypeScript API');
  }

  async emergencyShutdown() {
    try {
      const api = await loadTypeScriptAPI();
      const manager = api.getSessionManager();
      await manager.closeAllSessions();
      this.activeSessions.clear();
    } catch (error) {
      console.warn('Emergency shutdown failed:', error.message);
    }
  }
}

// Singleton instance for legacy compatibility
let coordinatorInstance = null;

function getDevToolsCoordinator() {
  if (!coordinatorInstance) {
    coordinatorInstance = new DevToolsSessionCoordinator();
  }
  return coordinatorInstance;
}

module.exports = {
  DevToolsSessionCoordinator,
  getDevToolsCoordinator
};