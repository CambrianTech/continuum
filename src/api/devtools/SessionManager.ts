/**
 * Clean TypeScript SessionManager - main orchestrator
 * Event-driven architecture, dependency injection, clean interfaces
 */

import { ISessionManager, ISession, SessionConfig, SessionPurpose, SessionStatus } from './interfaces.js';
import { Session } from './Session.js';
import { BrowserCoordinator } from './BrowserCoordinator.js';

export class SessionManager implements ISessionManager {
  private sessions = new Map<string, Session>();
  private browserCoordinator = new BrowserCoordinator();

  /**
   * Request a session with clean async/await - no polling, no timeouts
   */
  async requestSession(config: SessionConfig): Promise<ISession> {
    const sessionKey = this.generateSessionKey(config.purpose, config.persona);
    
    // Check if session already exists
    const existingSession = this.sessions.get(sessionKey);
    if (existingSession && existingSession.status !== SessionStatus.CLOSED) {
      return existingSession;
    }

    // Try to reuse existing browser if shared mode
    if (config.shared !== false) {
      const existingBrowser = await this.browserCoordinator.findExistingBrowser();
      
      if (existingBrowser) {
        return await this.createSessionInExistingBrowser(config, existingBrowser);
      }
    }

    // Create new browser and session
    return await this.createNewBrowserSession(config);
  }

  /**
   * Find existing session by purpose and persona
   */
  findSession(purpose: SessionPurpose, persona: string): ISession | null {
    const sessionKey = this.generateSessionKey(purpose, persona);
    return this.sessions.get(sessionKey) || null;
  }

  /**
   * List all active sessions
   */
  listSessions(): ISession[] {
    return Array.from(this.sessions.values()).filter(
      session => session.status !== SessionStatus.CLOSED
    );
  }

  /**
   * Close all sessions cleanly
   */
  async closeAllSessions(): Promise<void> {
    const closingPromises = Array.from(this.sessions.values()).map(session => 
      session.close().catch(error => 
        console.warn(`Failed to close session ${session.id}:`, error)
      )
    );
    
    await Promise.all(closingPromises);
    this.sessions.clear();
  }

  /**
   * Create session in existing browser (clean, no polling)
   */
  private async createSessionInExistingBrowser(
    config: SessionConfig, 
    browser: { port: number; shared: boolean; tabs: any[] }
  ): Promise<Session> {
    const sessionId = this.generateSessionId(config.purpose, config.persona);
    const sessionUrl = this.buildSessionUrl(sessionId, config);
    
    try {
      // Create new tab in existing browser
      const tabInfo = await this.browserCoordinator.createTabInBrowser(browser, sessionUrl);
      
      // Create session object
      const session = new Session(
        sessionId,
        config.purpose,
        config.persona,
        browser.port,
        true, // isShared
        tabInfo
      );

      // Tab is already ready since createTabInBrowser waits for it
      session.markReady();
      
      this.registerSession(config.purpose, config.persona, session);
      return session;
      
    } catch (error) {
      throw new Error(`Failed to create session in existing browser: ${error}`);
    }
  }

  /**
   * Create new browser and session
   */
  private async createNewBrowserSession(config: SessionConfig): Promise<Session> {
    const sessionId = this.generateSessionId(config.purpose, config.persona);
    const port = await this.browserCoordinator.findAvailablePort();
    const sessionUrl = this.buildSessionUrl(sessionId, config);
    
    const browserConfig = {
      port,
      userDataDir: `/tmp/continuum-session-${sessionId}`,
      initialUrl: sessionUrl,
      windowTitle: config.windowTitle || `Continuum - ${config.purpose}`,
      shared: config.shared !== false
    };

    try {
      // Launch browser with event-driven ready detection
      const browserInfo = await this.browserCoordinator.launchBrowser(browserConfig);
      
      // Create session object
      const session = new Session(
        sessionId,
        config.purpose,
        config.persona,
        port,
        false, // isShared = false for new browser
        browserInfo.tabs[0] // First tab
      );

      // Browser launch already verified Continuum is loaded
      session.markReady();
      
      this.registerSession(config.purpose, config.persona, session);
      return session;
      
    } catch (error) {
      throw new Error(`Failed to create new browser session: ${error}`);
    }
  }

  /**
   * Register session in internal map
   */
  private registerSession(purpose: SessionPurpose, persona: string, session: Session): void {
    const sessionKey = this.generateSessionKey(purpose, persona);
    
    // Clean up old session if exists
    const oldSession = this.sessions.get(sessionKey);
    if (oldSession) {
      oldSession.close().catch(() => {}); // Fire and forget cleanup
    }
    
    this.sessions.set(sessionKey, session);
    
    // Auto-cleanup when session closes
    session.on('closed', () => {
      this.sessions.delete(sessionKey);
    });
  }

  /**
   * Generate unique session key
   */
  private generateSessionKey(purpose: SessionPurpose, persona: string): string {
    return `${purpose}_${persona}`;
  }

  /**
   * Generate unique session ID
   */
  private generateSessionId(purpose: SessionPurpose, persona: string): string {
    return `${purpose}_${persona}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * Build session URL with parameters
   */
  private buildSessionUrl(sessionId: string, config: SessionConfig): string {
    const params = new URLSearchParams({
      session: sessionId,
      purpose: config.purpose,
      persona: config.persona
    });
    
    return `http://localhost:9000?${params.toString()}`;
  }
}

// Singleton instance for global access
let sessionManagerInstance: SessionManager | null = null;

export function getSessionManager(): SessionManager {
  if (!sessionManagerInstance) {
    sessionManagerInstance = new SessionManager();
  }
  return sessionManagerInstance;
}