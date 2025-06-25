/**
 * DevTools API TypeScript Definitions
 * ==================================
 * 
 * Comprehensive type definitions for the DevTools API system.
 * Provides strong typing, interfaces, enums, and class definitions
 * for robust object-oriented development.
 */

// ========================================
// CORE ENUMS
// ========================================

export enum SessionPurpose {
  GIT_VERIFICATION = 'git_verification',
  WORKSPACE = 'workspace', 
  TESTING = 'testing',
  AUTOMATION = 'automation',
  PRESENTATION = 'presentation',
  DEBUGGING = 'debugging',
  DEVELOPMENT = 'development'
}

export enum SessionState {
  INITIALIZING = 'initializing',
  STARTING = 'starting',
  ACTIVE = 'active',
  PAUSED = 'paused',
  CLOSING = 'closing',
  CLOSED = 'closed',
  ERROR = 'error'
}

export enum WindowState {
  NORMAL = 'normal',
  MINIMIZED = 'minimized',
  MAXIMIZED = 'maximized',
  FULLSCREEN = 'fullscreen',
  HIDDEN = 'hidden'
}

export enum BrowserEngine {
  OPERA_GX = 'opera_gx',
  CHROME = 'chrome',
  CHROMIUM = 'chromium',
  EDGE = 'edge',
  FIREFOX = 'firefox'
}

export enum ScreenshotFormat {
  PNG = 'png',
  JPEG = 'jpeg',
  WEBP = 'webp'
}

export enum LogLevel {
  DEBUG = 'debug',
  INFO = 'info',
  WARNING = 'warning',
  ERROR = 'error',
  CRITICAL = 'critical'
}

export enum EventType {
  SESSION_CREATED = 'session:created',
  SESSION_STARTED = 'session:started',
  SESSION_STOPPED = 'session:stopped',
  TAB_CREATED = 'tab:created',
  TAB_CLOSED = 'tab:closed',
  CONSOLE_MESSAGE = 'console:message',
  EXCEPTION_THROWN = 'exception:thrown',
  SCREENSHOT_CAPTURED = 'screenshot:captured'
}

// ========================================
// CORE INTERFACES
// ========================================

export interface IWindowProperties {
  readonly width: number;
  readonly height: number;
  readonly x?: number;
  readonly y?: number;
  readonly state: WindowState;
  readonly alwaysOnTop: boolean;
  readonly resizable: boolean;
  readonly title: string;
  readonly opacity: number;
}

export interface IDisplayOptions {
  readonly headless: boolean;
  readonly visible: boolean;
  readonly kiosk: boolean;
  readonly displayId?: string;
  readonly multiMonitor: boolean;
}

export interface IBrowserOptions {
  readonly engine: BrowserEngine;
  readonly debugging: boolean;
  readonly webSecurity: boolean;
  readonly extensions: boolean;
  readonly plugins: boolean;
  readonly javascript: boolean;
  readonly images: boolean;
  readonly notifications: boolean;
  readonly userAgent?: string;
  readonly preferences: Record<string, any>;
}

export interface ISessionIsolation {
  readonly userDataDir?: string;
  readonly incognito: boolean;
  readonly sharedSession: boolean;
  readonly persistData: boolean;
  readonly profileName?: string;
  readonly cookies: Record<string, string>;
  readonly localStorage: Record<string, any>;
}

export interface IAutomationFeatures {
  readonly autoClose: boolean;
  readonly timeout: number;
  readonly autoRestart: boolean;
  readonly captureErrors: boolean;
  readonly captureConsole: boolean;
  readonly captureNetwork: boolean;
  readonly startUrl: string;
  readonly preloadScripts: string[];
}

export interface ISessionConfiguration {
  readonly purpose: SessionPurpose;
  readonly aiPersona: string;
  readonly window: IWindowProperties;
  readonly display: IDisplayOptions;
  readonly browser: IBrowserOptions;
  readonly isolation: ISessionIsolation;
  readonly automation: IAutomationFeatures;
}

export interface ISessionMetadata {
  readonly sessionId: string;
  readonly created: Date;
  readonly port: number;
  readonly windowName: string;
  readonly artifactPath?: string;
  readonly tabId?: string;
}

export interface IDevToolsSession {
  readonly config: ISessionConfiguration;
  readonly metadata: ISessionMetadata;
  readonly state: SessionState;
  readonly isSharedTab: boolean;
  readonly browserPid?: number;
}

export interface ITabInformation {
  readonly id: string;
  readonly url: string;
  readonly title: string;
  readonly description: string;
  readonly type: string;
  readonly webSocketDebuggerUrl: string;
  readonly devtoolsFrontendUrl: string;
}

export interface IConsoleMessage {
  readonly type: string;
  readonly level: LogLevel;
  readonly timestamp: Date;
  readonly args: any[];
  readonly sessionId: string;
  readonly source: 'console' | 'exception';
  readonly stackTrace?: any;
}

export interface IScreenshotOptions {
  readonly format: ScreenshotFormat;
  readonly quality: number;
  readonly fullPage: boolean;
  readonly element?: string;
  readonly clip?: IClipRegion;
  readonly sessionArtifact: boolean;
  readonly createLatestSymlink: boolean;
}

export interface IClipRegion {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly scale?: number;
}

export interface IScreenshotResult {
  readonly success: boolean;
  readonly path?: string;
  readonly filename?: string;
  readonly method?: string;
  readonly session?: string;
  readonly error?: string;
}

// ========================================
// ABSTRACT BASE CLASSES
// ========================================

export abstract class BaseSessionComponent {
  protected readonly sessionId: string;
  protected readonly port: number;
  protected connected: boolean = false;
  
  constructor(sessionId: string, port: number) {
    this.sessionId = sessionId;
    this.port = port;
  }
  
  abstract connect(): Promise<void>;
  abstract disconnect(): Promise<void>;
  abstract isConnected(): boolean;
  abstract getHealthInfo(): any;
}

export abstract class BaseConfigurable<T> {
  protected config: T;
  
  constructor(initialConfig: T) {
    this.config = initialConfig;
  }
  
  abstract validate(): { valid: boolean; errors: string[] };
  abstract clone(): this;
  abstract toJSON(): any;
  
  getConfig(): Readonly<T> {
    return this.config;
  }
}

// ========================================
// EVENT SYSTEM
// ========================================

export interface IEventEmitter {
  on(event: EventType, listener: (...args: any[]) => void): void;
  off(event: EventType, listener: (...args: any[]) => void): void;
  emit(event: EventType, ...args: any[]): void;
}

export interface IDevToolsEvent {
  readonly type: EventType;
  readonly timestamp: Date;
  readonly sessionId: string;
  readonly data: any;
}

// ========================================
// SESSION MANAGER INTERFACES
// ========================================

export interface ISessionCoordinator {
  requestSession(purpose: SessionPurpose, aiPersona: string, config?: Partial<ISessionConfiguration>): Promise<IDevToolsSession>;
  closeSession(sessionKey: string): Promise<void>;
  getSessionSummary(): ISessionSummary;
  emergencyShutdown(): Promise<void>;
}

export interface ISessionSummary {
  readonly totalSessions: number;
  readonly byPurpose: Record<SessionPurpose, number>;
  readonly byPersona: Record<string, number>;
  readonly activePorts: number[];
  readonly sessions: ISessionInfo[];
}

export interface ISessionInfo {
  readonly id: string;
  readonly purpose: SessionPurpose;
  readonly persona: string;
  readonly port: number;
  readonly state: SessionState;
  readonly created: Date;
}

// ========================================
// TAB MANAGEMENT INTERFACES
// ========================================

export interface ITabManager extends BaseSessionComponent {
  executeJavaScript(expression: string): Promise<any>;
  navigate(url: string): Promise<void>;
  takeScreenshot(options?: Partial<IScreenshotOptions>): Promise<string>;
  clickElement(selector: string): Promise<boolean>;
  typeInElement(selector: string, text: string): Promise<boolean>;
  waitForElement(selector: string, timeout?: number): Promise<boolean>;
  getTitle(): Promise<string>;
  getURL(): Promise<string>;
  getState(): ITabState;
}

export interface ITabState {
  readonly tabId: string;
  readonly connected: boolean;
  readonly url?: string;
  readonly title?: string;
  readonly loadingState: 'unknown' | 'loading' | 'domReady' | 'loaded';
}

// ========================================
// PROTOCOL CLIENT INTERFACES
// ========================================

export interface IProtocolClient extends BaseSessionComponent {
  sendCommand(method: string, params?: any, targetTabId?: string): Promise<any>;
  subscribe(eventMethod: string, callback: (params: any) => void): void;
  unsubscribe(eventMethod: string, callback: (params: any) => void): void;
  executeJavaScript(expression: string, tabId?: string): Promise<any>;
  createTab(url?: string): Promise<ITabInformation>;
  navigate(url: string, tabId?: string): Promise<any>;
  takeScreenshot(options?: Partial<IScreenshotOptions>, tabId?: string): Promise<string>;
  enableDomain(domain: string, tabId?: string): Promise<any>;
  disableDomain(domain: string, tabId?: string): Promise<any>;
  getBrowserVersion(): Promise<any>;
  listTabs(): Promise<ITabInformation[]>;
}

// ========================================
// CONSOLE FORWARDER INTERFACES
// ========================================

export interface IConsoleForwarder extends BaseSessionComponent {
  start(): Promise<void>;
  stop(): Promise<void>;
  onMessage(callback: (message: IConsoleMessage) => void): void;
  offMessage(callback: (message: IConsoleMessage) => void): void;
  addFilter(filterFunction: (message: IConsoleMessage) => boolean): void;
  removeFilter(filterFunction: (message: IConsoleMessage) => boolean): void;
  getRecentMessages(count?: number): IConsoleMessage[];
  getMessagesByLevel(level: LogLevel, count?: number): IConsoleMessage[];
  getStats(): IConsoleStats;
  clearBuffer(): void;
}

export interface IConsoleStats {
  readonly totalMessages: number;
  readonly errorMessages: number;
  readonly warningMessages: number;
  readonly infoMessages: number;
  readonly debugMessages: number;
  readonly bufferSize: number;
  readonly connected: boolean;
  readonly sessionId: string;
}

// ========================================
// SCREENSHOT CAPTURE INTERFACES
// ========================================

export interface IScreenshotCapture {
  captureScreenshot(session: IDevToolsSession, filename?: string, options?: Partial<IScreenshotOptions>): Promise<IScreenshotResult>;
  captureElement(session: IDevToolsSession, selector: string, filename?: string, options?: Partial<IScreenshotOptions>): Promise<IScreenshotResult>;
  captureFullPage(session: IDevToolsSession, filename?: string, options?: Partial<IScreenshotOptions>): Promise<IScreenshotResult>;
  getRecentScreenshots(session: IDevToolsSession, limit?: number): Promise<IScreenshotInfo[]>;
}

export interface IScreenshotInfo {
  readonly filename: string;
  readonly path: string;
  readonly created: Date;
  readonly size: number;
  readonly location: 'artifact' | 'global';
}

// ========================================
// MAIN API INTERFACE
// ========================================

export interface IDevToolsAPI extends IEventEmitter {
  // Session Management
  requestSession(purpose: SessionPurpose, aiPersona?: string, config?: Partial<ISessionConfiguration>): Promise<IDevToolsSession>;
  closeSession(sessionKey: string): Promise<void>;
  getSessionSummary(): ISessionSummary;
  
  // Tab Management
  createTab(sessionId: string, url?: string): Promise<{ tabInfo: ITabInformation; tabManager: ITabManager }>;
  getTabManager(tabId: string): ITabManager | undefined;
  closeTab(tabId: string): Promise<void>;
  
  // Browser Automation
  executeJavaScript(sessionId: string, script: string, tabId?: string): Promise<any>;
  takeScreenshot(sessionId: string, filename?: string, options?: Partial<IScreenshotOptions>): Promise<IScreenshotResult>;
  navigateTab(sessionId: string, url: string, tabId?: string): Promise<any>;
  
  // Console Monitoring
  getConsoleForwarder(sessionId: string): IConsoleForwarder | undefined;
  onConsoleMessage(sessionId: string, callback: (message: IConsoleMessage) => void): void;
  getRecentConsoleMessages(sessionId: string, count?: number): IConsoleMessage[];
  
  // Protocol Access
  getProtocolClient(sessionId: string): IProtocolClient | undefined;
  sendProtocolCommand(sessionId: string, method: string, params?: any, tabId?: string): Promise<any>;
  
  // Utility
  emergencyShutdown(): Promise<void>;
  healthCheck(): Promise<IHealthCheckResult>;
}

export interface IHealthCheckResult {
  readonly coordinator: boolean;
  readonly activeSessions: number;
  readonly activeClients: number;
  readonly activeForwarders: number;
  readonly activeTabs: number;
  readonly errors: string[];
}

// ========================================
// CONFIGURATION BUILDER INTERFACES
// ========================================

export interface ISessionConfigBuilder {
  setPurpose(purpose: SessionPurpose): this;
  setAIPersona(persona: string): this;
  setWindow(properties: Partial<IWindowProperties>): this;
  setDisplay(options: Partial<IDisplayOptions>): this;
  setBrowser(options: Partial<IBrowserOptions>): this;
  setIsolation(options: Partial<ISessionIsolation>): this;
  setAutomation(features: Partial<IAutomationFeatures>): this;
  
  // Presets
  development(): this;
  production(): this;
  testing(): this;
  debugging(): this;
  kiosk(): this;
  shared(): this;
  
  build(): ISessionConfiguration;
}

// ========================================
// ERROR TYPES
// ========================================

export class DevToolsError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly sessionId?: string
  ) {
    super(message);
    this.name = 'DevToolsError';
  }
}

export class SessionError extends DevToolsError {
  constructor(message: string, sessionId: string) {
    super(message, 'SESSION_ERROR', sessionId);
    this.name = 'SessionError';
  }
}

export class ProtocolError extends DevToolsError {
  constructor(message: string, sessionId?: string) {
    super(message, 'PROTOCOL_ERROR', sessionId);
    this.name = 'ProtocolError';
  }
}

export class ConfigurationError extends DevToolsError {
  constructor(message: string) {
    super(message, 'CONFIGURATION_ERROR');
    this.name = 'ConfigurationError';
  }
}

// ========================================
// UTILITY TYPES
// ========================================

export type SessionKey = string;
export type TabId = string;
export type PortNumber = number;
export type FilePath = string;
export type URL = string;
export type JSONSerializable = string | number | boolean | null | undefined | JSONSerializable[] | { [key: string]: JSONSerializable };

// Generic result type for async operations
export interface IResult<T, E = Error> {
  readonly success: boolean;
  readonly data?: T;
  readonly error?: E;
}

// Configuration override type for flexible configuration
export type ConfigurationOverride<T> = {
  [K in keyof T]?: T[K] extends object ? Partial<T[K]> : T[K];
};

// Event listener type
export type EventListener<T = any> = (data: T) => void;

// Async event listener type  
export type AsyncEventListener<T = any> = (data: T) => Promise<void>;

export default IDevToolsAPI;