/**
 * DevTools Protocol and Adapter Types
 */

export interface DevToolsTarget {
  id: string;
  type: 'page' | 'service_worker' | 'background_page';
  title: string;
  url: string;
  webSocketDebuggerUrl?: string;
  devtoolsFrontendUrl?: string;
}

export interface ConsoleMessage {
  level: 'log' | 'info' | 'warn' | 'error' | 'debug';
  text: string;
  timestamp: number;
  source: 'console' | 'network' | 'security' | 'other';
  args?: any[];
  stackTrace?: {
    callFrames: Array<{
      functionName: string;
      scriptId: string;
      url: string;
      lineNumber: number;
      columnNumber: number;
    }>;
  };
}

export interface NetworkRequest {
  requestId: string;
  url: string;
  method: string;
  headers: Record<string, string>;
  timestamp: number;
  status?: number;
  statusText?: string;
  responseHeaders?: Record<string, string>;
  failed?: boolean;
  errorReason?: string;
}

export interface PerformanceMetrics {
  scriptDuration: number;
  layoutDuration: number;
  recalcStyleDuration: number;
  timestamp: number;
  memoryUsage?: {
    usedJSHeapSize: number;
    totalJSHeapSize: number;
    jsHeapSizeLimit: number;
  };
}

export interface DevToolsCapabilities {
  supportsConsole: boolean;
  supportsNetwork: boolean;
  supportsPerformance: boolean;
  supportsScreenshot: boolean;
  supportsProfiling: boolean;
  supportsCodeCoverage: boolean;
  supportsSecurityAnalysis: boolean;
}

export interface IDevToolsAdapter {
  readonly protocol: string; // 'cdp', 'firefox-remote', 'safari-inspector'
  readonly capabilities: DevToolsCapabilities;
  
  connect(debugUrl: string, targetId?: string): Promise<void>;
  disconnect(): Promise<void>;
  isConnected(): boolean;
  getTargets(): Promise<DevToolsTarget[]>;
  
  // Core DevTools operations
  evaluateScript(expression: string, targetId?: string): Promise<any>;
  navigate(url: string, targetId?: string): Promise<void>;
  reload(targetId?: string, ignoreCache?: boolean): Promise<void>;
  screenshot(targetId?: string, options?: ScreenshotOptions): Promise<Buffer>;
  
  // Monitoring capabilities
  enableConsole(callback: (message: ConsoleMessage) => void, targetId?: string): Promise<void>;
  disableConsole(targetId?: string): Promise<void>;
  enableNetwork(callback: (request: NetworkRequest) => void, targetId?: string): Promise<void>;
  disableNetwork(targetId?: string): Promise<void>;
  enablePerformance(callback: (metrics: PerformanceMetrics) => void, targetId?: string): Promise<void>;
  disablePerformance(targetId?: string): Promise<void>;
  
  // Advanced features
  getSecurityState(targetId?: string): Promise<SecurityState>;
  startCoverage(targetId?: string): Promise<void>;
  stopCoverage(targetId?: string): Promise<CoverageResult>;
  startProfiling(targetId?: string): Promise<void>;
  stopProfiling(targetId?: string): Promise<ProfilingResult>;
}

export interface ScreenshotOptions {
  format?: 'png' | 'jpeg';
  quality?: number;
  fullPage?: boolean;
}

export interface SecurityState {
  securityState: 'secure' | 'info' | 'insecure' | 'insecure-broken';
  explanations: Array<{
    securityState: string;
    title: string;
    summary: string;
    description: string;
  }>;
}

export interface CoverageResult {
  result: Array<{
    scriptId: string;
    url: string;
    functions: Array<{
      functionName: string;
      ranges: Array<{
        startOffset: number;
        endOffset: number;
        count: number;
      }>;
    }>;
  }>;
}

export interface ProfilingResult {
  profile: any; // Browser-specific profile format
}