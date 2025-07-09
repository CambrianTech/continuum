/**
 * Widget System Types - Strongly Typed Widget Architecture
 * Complete type definitions for modular widget system
 */

// Core widget interfaces
export interface WidgetConfig {
  readonly name: string;
  readonly version: string;
  readonly type: 'ui' | 'data' | 'control' | 'display';
  readonly dependencies?: string[];
  readonly permissions?: WidgetPermission[];
}

export interface WidgetPermission {
  readonly type: 'api' | 'storage' | 'network' | 'file';
  readonly resource: string;
  readonly level: 'read' | 'write' | 'execute';
}

export interface WidgetAssets {
  readonly css: string[];
  readonly html: string[];
  readonly js: string[];
  readonly ts: string[];
  readonly other?: string[];
}

export interface WidgetManifest {
  readonly config: WidgetConfig;
  readonly assets: WidgetAssets;
  readonly basePath: string;
  readonly compiled?: boolean;
}

// Widget lifecycle interfaces
export interface WidgetLifecycle {
  initialize(): Promise<void>;
  render(): Promise<void>;
  update(data?: unknown): Promise<void>;
  cleanup(): Promise<void>;
}

export interface WidgetState {
  readonly connected: boolean;
  readonly initialized: boolean;
  readonly collapsed: boolean;
  readonly error?: string;
  readonly data?: Record<string, unknown>;
}

// Widget communication interfaces
export interface WidgetMessage {
  readonly type: string;
  readonly payload: unknown;
  readonly source: string;
  readonly timestamp: string;
}

export interface WidgetCommand {
  readonly command: string;
  readonly params: Record<string, unknown>;
  readonly requestId: string;
  readonly widgetId: string;
}

export interface WidgetResponse {
  readonly success: boolean;
  readonly data?: unknown;
  readonly error?: string;
  readonly requestId: string;
}

// Widget registry interfaces
export interface WidgetRegistryEntry {
  readonly manifest: WidgetManifest;
  readonly instance?: BaseWidgetInstance;
  readonly registered: Date;
  readonly active: boolean;
}

export interface BaseWidgetInstance {
  readonly config: WidgetConfig;
  readonly state: WidgetState;
  readonly element: HTMLElement;
  
  // Core lifecycle methods
  initialize(): Promise<void>;
  render(): Promise<void>;
  update(data?: unknown): Promise<void>;
  cleanup(): Promise<void>;
  
  // Communication methods
  sendMessage(message: WidgetMessage): Promise<void>;
  executeCommand(command: WidgetCommand): Promise<WidgetResponse>;
  
  // Event handling
  addEventListener(type: string, handler: (event: Event) => void): void;
  removeEventListener(type: string, handler: (event: Event) => void): void;
}

// Widget factory interfaces
export interface WidgetFactory {
  readonly name: string;
  readonly supportedTypes: string[];
  
  createWidget(manifest: WidgetManifest): Promise<BaseWidgetInstance>;
  validateManifest(manifest: WidgetManifest): boolean;
  compileAssets(assets: WidgetAssets): Promise<WidgetAssets>;
}

// Widget renderer interfaces
export interface WidgetRenderer {
  renderWidget(widget: BaseWidgetInstance): Promise<string>;
  renderError(error: Error, widgetId: string): Promise<string>;
  compileTypeScript(source: string, options?: CompileOptions): Promise<string>;
}

export interface CompileOptions {
  readonly target?: 'es5' | 'es2015' | 'es2020';
  readonly module?: 'commonjs' | 'es2015' | 'es2020';
  readonly strict?: boolean;
  readonly sourceMaps?: boolean;
}

// Widget discovery interfaces
export interface WidgetDiscovery {
  discoverWidgets(searchPath: string): Promise<WidgetManifest[]>;
  validateWidget(manifest: WidgetManifest): Promise<boolean>;
  loadWidgetAssets(manifest: WidgetManifest): Promise<WidgetAssets>;
}

// Widget events
export interface WidgetEvent {
  readonly type: 'widget:created' | 'widget:updated' | 'widget:destroyed' | 'widget:error';
  readonly widgetId: string;
  readonly timestamp: Date;
  readonly data?: unknown;
}

export interface WidgetEventHandler {
  (event: WidgetEvent): void | Promise<void>;
}

// Widget system configuration
export interface WidgetSystemConfig {
  readonly baseUrl: string;
  readonly assetsPath: string;
  readonly compilationEnabled: boolean;
  readonly cacheEnabled: boolean;
  readonly developmentMode: boolean;
  readonly maxWidgets: number;
  readonly permissions: WidgetPermission[];
}

// Error types
export class WidgetError extends Error {
  constructor(
    message: string,
    public readonly widgetId: string,
    public readonly code: string,
    public readonly cause?: Error
  ) {
    super(message);
    this.name = 'WidgetError';
  }
}

export class WidgetCompilationError extends WidgetError {
  constructor(
    message: string,
    widgetId: string,
    public readonly file: string,
    public readonly line?: number,
    public readonly column?: number
  ) {
    super(message, widgetId, 'COMPILATION_ERROR');
    this.name = 'WidgetCompilationError';
  }
}

export class WidgetAssetError extends WidgetError {
  constructor(
    message: string,
    widgetId: string,
    public readonly assetPath: string,
    public readonly assetType: string
  ) {
    super(message, widgetId, 'ASSET_ERROR');
    this.name = 'WidgetAssetError';
  }
}

// Utility types
export type WidgetEventType = 'widget:created' | 'widget:updated' | 'widget:destroyed' | 'widget:error';
export type WidgetAssetType = 'css' | 'html' | 'js' | 'ts' | 'other';
export type WidgetStateType = 'initializing' | 'ready' | 'error' | 'destroyed';

// Type guards
export function isWidgetManifest(obj: unknown): obj is WidgetManifest {
  return typeof obj === 'object' && 
         obj !== null && 
         'config' in obj && 
         'assets' in obj && 
         'basePath' in obj;
}

export function isWidgetInstance(obj: unknown): obj is BaseWidgetInstance {
  return typeof obj === 'object' && 
         obj !== null && 
         'config' in obj && 
         'state' in obj && 
         'element' in obj;
}

export function isWidgetCommand(obj: unknown): obj is WidgetCommand {
  return typeof obj === 'object' && 
         obj !== null && 
         'command' in obj && 
         'params' in obj && 
         'requestId' in obj && 
         'widgetId' in obj;
}

export function isWidgetResponse(obj: unknown): obj is WidgetResponse {
  return typeof obj === 'object' && 
         obj !== null && 
         'success' in obj && 
         'requestId' in obj;
}