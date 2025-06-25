/**
 * Clean TypeScript interfaces for DevTools session management
 * Event-driven architecture with proper async/await patterns
 */

export enum SessionPurpose {
  GIT_VERIFICATION = 'git_verification',
  WORKSPACE = 'workspace',
  TESTING = 'testing',
  DEBUGGING = 'debugging'
}

export enum SessionStatus {
  CREATING = 'creating',
  LOADING = 'loading', 
  READY = 'ready',
  ACTIVE = 'active',
  ERROR = 'error',
  CLOSED = 'closed'
}

export interface SessionConfig {
  purpose: SessionPurpose;
  persona: string;
  shared?: boolean;
  windowTitle?: string;
  timeout?: number;
}

export interface SessionEvents {
  'status-changed': (status: SessionStatus) => void;
  'ready': () => void;
  'error': (error: Error) => void;
  'closed': () => void;
  'console-log': (message: any) => void;
}

export interface ISession {
  readonly id: string;
  readonly purpose: SessionPurpose;
  readonly persona: string;
  readonly port: number;
  readonly status: SessionStatus;
  readonly isShared: boolean;
  
  // Event-driven methods
  waitForReady(timeout?: number): Promise<void>;
  execute(script: string): Promise<any>;
  screenshot(filename?: string): Promise<string>;
  close(): Promise<void>;
  
  // Event handling
  on<K extends keyof SessionEvents>(event: K, listener: SessionEvents[K]): void;
  off<K extends keyof SessionEvents>(event: K, listener: SessionEvents[K]): void;
  emit<K extends keyof SessionEvents>(event: K, ...args: Parameters<SessionEvents[K]>): void;
}

export interface ISessionManager {
  requestSession(config: SessionConfig): Promise<ISession>;
  findSession(purpose: SessionPurpose, persona: string): ISession | null;
  listSessions(): ISession[];
  closeAllSessions(): Promise<void>;
}

export interface IBrowserCoordinator {
  findExistingBrowser(): Promise<BrowserInfo | null>;
  launchBrowser(config: BrowserConfig): Promise<BrowserInfo>;
  createTabInBrowser(browser: BrowserInfo, url: string): Promise<TabInfo>;
  closeBrowser(browser: BrowserInfo): Promise<void>;
}

export interface BrowserInfo {
  port: number;
  pid?: number;
  shared: boolean;
  tabs: TabInfo[];
}

export interface TabInfo {
  id: string;
  url: string;
  title: string;
  ready: boolean;
}

export interface BrowserConfig {
  port: number;
  userDataDir: string;
  initialUrl: string;
  windowTitle: string;
  shared: boolean;
}

export interface DevToolsResponse<T = any> {
  id?: number;
  result?: {
    value?: T;
    type?: string;
  };
  error?: {
    message: string;
    code: number;
  };
}