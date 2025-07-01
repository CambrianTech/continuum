/**
 * Shared Browser Types - Used across multiple daemons and modules
 * 
 * Natural location for browser-related types that need to be shared
 * between browser-manager, renderer, command system, etc.
 */

// TODO: Create browser type modules as needed:
// - browser-core.ts (core browser interfaces)
// - browser-management.ts (browser instance management)
// - browser-session.ts (session-specific browser state)
// - devtools.ts (DevTools Protocol types)

export interface BrowserInstance {
  id: string;
  pid: number;
  debugUrl: string;
}

export interface DevToolsTarget {
  id: string;
  type: string;
  title: string;
  url: string;
  webSocketDebuggerUrl: string;
}