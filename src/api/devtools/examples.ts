/**
 * Examples of the new pluggable browser system
 * Sessions can pick browser, visibility, and tab behavior
 */

import { getSessionManager } from './SessionManager.js';
import { SessionPurpose, BrowserType } from './interfaces.js';

export async function demonstrateBrowserSelection() {
  const sessionManager = getSessionManager();

  // Example 1: Auto-detect best browser, hidden execution for git hooks
  const gitSession = await sessionManager.requestSession({
    purpose: SessionPurpose.GIT_VERIFICATION,
    persona: 'system',
    browser: 'auto',           // Let system pick best available
    visible: false,            // Run completely hidden
    shared: true              // Reuse existing browser if possible
  });

  // Example 2: Force Chrome with specific positioning for development
  const devSession = await sessionManager.requestSession({
    purpose: SessionPurpose.WORKSPACE,
    persona: 'developer',
    browser: BrowserType.CHROME,    // Force Chrome
    visible: true,
    position: { x: 100, y: 100 },
    size: { width: 1200, height: 800 },
    windowTitle: 'Continuum Dev Workspace'
  });

  // Example 3: Headless Firefox for automated testing
  const testSession = await sessionManager.requestSession({
    purpose: SessionPurpose.TESTING,
    persona: 'test-runner',
    browser: BrowserType.FIREFOX,   // Force Firefox
    headless: true,                 // Pure headless mode
    additionalBrowserArgs: ['--disable-gpu', '--no-sandbox']
  });

  // Example 4: Opera GX minimized for background monitoring
  const monitorSession = await sessionManager.requestSession({
    purpose: SessionPurpose.DEBUGGING,
    persona: 'monitor',
    browser: BrowserType.OPERA_GX,  // Force Opera GX
    minimized: true,                // Start minimized
    shared: false                   // Dedicated browser instance
  });

  // Example 5: User preference with fallback
  try {
    const userSession = await sessionManager.requestSession({
      purpose: SessionPurpose.WORKSPACE,
      persona: 'user',
      browser: BrowserType.SAFARI,    // User prefers Safari
      visible: true
    });
  } catch (error) {
    // Safari not available, system will auto-fallback to best available
    console.log('Safari not available, using fallback browser');
  }

  return {
    gitSession,
    devSession, 
    testSession,
    monitorSession
  };
}

// Usage examples for different scenarios
export const browserExamples = {
  // Git hook - completely invisible
  gitHook: {
    browser: 'auto' as const,
    visible: false,
    shared: true
  },

  // Development - visible Chrome with positioning
  development: {
    browser: BrowserType.CHROME,
    visible: true,
    position: { x: 100, y: 100 },
    size: { width: 1200, height: 800 }
  },

  // Testing - headless Firefox
  testing: {
    browser: BrowserType.FIREFOX,
    headless: true,
    additionalBrowserArgs: ['--disable-gpu']
  },

  // Gaming setup - Opera GX
  gaming: {
    browser: BrowserType.OPERA_GX,
    visible: true,
    minimized: false
  }
};