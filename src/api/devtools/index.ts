/**
 * Clean TypeScript DevTools API - main export
 * Replace the messy DevToolsSessionCoordinator.cjs with this clean interface
 */

export { 
  SessionPurpose, 
  SessionStatus, 
  type ISession, 
  type ISessionManager, 
  type SessionConfig,
  type SessionEvents 
} from './interfaces.js';

export { Session } from './Session.js';
export { BrowserCoordinator } from './BrowserCoordinator.js';
export { SessionManager, getSessionManager } from './SessionManager.js';

// Clean API for external use
export async function requestSession(config: any): Promise<any> {
  const { getSessionManager } = await import('./SessionManager.js');
  const { SessionPurpose } = await import('./interfaces.js');
  
  const manager = getSessionManager();
  
  // Handle both legacy (purpose, persona, options) and new (config) calling styles
  let sessionConfig;
  if (typeof config === 'string') {
    // Legacy style: requestSession('git_verification', 'system', options)
    const [purpose, persona = 'system', options = {}] = arguments;
    sessionConfig = {
      purpose: SessionPurpose[purpose.toUpperCase() as keyof typeof SessionPurpose] || SessionPurpose.WORKSPACE,
      persona,
      shared: options.sharedWindow !== false,
      windowTitle: options.windowTitle,
      visible: options.visible,
      headless: options.headless,
      minimized: options.minimized,
      position: options.position,
      size: options.size,
      timeout: options.timeout
    };
  } else {
    // New style: requestSession(config)
    sessionConfig = {
      purpose: SessionPurpose[config.purpose?.toUpperCase() as keyof typeof SessionPurpose] || SessionPurpose.WORKSPACE,
      persona: config.persona || 'system',
      shared: config.shared !== false,
      windowTitle: config.windowTitle,
      visible: config.visible,
      headless: config.headless,
      minimized: config.minimized,
      position: config.position,
      size: config.size,
      timeout: config.timeout
    };
  }
  
  const session = await manager.requestSession(sessionConfig);
  
  // Return object compatible with old interface for git hook
  return {
    sessionId: session.id,
    purpose: session.purpose,
    aiPersona: session.persona,
    port: session.port,
    created: new Date().toISOString(),
    status: 'active',
    isSharedTab: session.isShared,
    browserPid: null,
    reuseExisting: true,
    
    // Add clean methods
    execute: (script: string) => session.execute(script),
    screenshot: (filename?: string) => session.screenshot(filename),
    waitForReady: (timeout?: number) => session.waitForReady(timeout),
    close: () => session.close()
  };
}