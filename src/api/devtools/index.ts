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
export async function requestSession(purpose: string, persona: string = 'system', options: any = {}): Promise<any> {
  const { getSessionManager } = await import('./SessionManager.js');
  const { SessionPurpose } = await import('./interfaces.js');
  
  const manager = getSessionManager();
  
  const config = {
    purpose: SessionPurpose[purpose.toUpperCase() as keyof typeof SessionPurpose] || SessionPurpose.WORKSPACE,
    persona,
    shared: options.sharedWindow !== false,
    windowTitle: options.windowTitle,
    timeout: options.timeout
  };
  
  const session = await manager.requestSession(config);
  
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