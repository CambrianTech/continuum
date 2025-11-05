/**
 * Browser Session and State Management
 */

import { BrowserType, BrowserStatus } from './browser-core.js';

export interface BrowserSession {
  sessionId: string;
  browserType: BrowserType;
  browserPid?: number;
  debugPort?: number;
  devToolsUrl?: string;
  status: BrowserStatus;
  createdAt: Date;
  lastActivity: Date;
  tabs: TabInfo[];
  currentUrl?: string;
  metadata: SessionMetadata;
}

export interface TabInfo {
  id: string;
  url: string;
  title: string;
  type: 'page' | 'service_worker' | 'background_page';
  webSocketDebuggerUrl?: string;
  devtoolsFrontendUrl?: string;
}

export interface SessionMetadata {
  userAgent?: string;
  viewport?: { width: number; height: number };
  preferences?: Record<string, any>;
  extensions?: string[];
  bookmarks?: string[];
  cookies?: any[];
}