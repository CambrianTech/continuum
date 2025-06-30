/**
 * DevTools Adapter Interface - Universal DevTools control abstraction
 * 
 * Provides consistent interface across different browser DevTools protocols:
 * - Chrome DevTools Protocol (CDP) for Chrome/Edge/Opera
 * - Firefox Remote Protocol for Firefox
 * - Safari Remote Inspector for Safari
 * 
 * Each adapter handles the browser-specific protocol details while exposing
 * a common interface for automation and debugging tasks.
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
  
  /**
   * Connect to DevTools for given target
   */
  connect(debugUrl: string, targetId?: string): Promise<void>;
  
  /**
   * Disconnect from DevTools
   */
  disconnect(): Promise<void>;
  
  /**
   * Check if adapter is connected and ready
   */
  isConnected(): boolean;
  
  /**
   * Get available targets (tabs, workers, etc.)
   */
  getTargets(): Promise<DevToolsTarget[]>;
  
  /**
   * Execute JavaScript in target context
   */
  evaluateScript(expression: string, targetId?: string): Promise<any>;
  
  /**
   * Navigate target to URL
   */
  navigate(url: string, targetId?: string): Promise<void>;
  
  /**
   * Reload target page
   */
  reload(targetId?: string, ignoreCache?: boolean): Promise<void>;
  
  /**
   * Take screenshot of target
   */
  screenshot(targetId?: string, options?: {
    format?: 'png' | 'jpeg';
    quality?: number;
    fullPage?: boolean;
  }): Promise<Buffer>;
  
  /**
   * Console monitoring
   */
  enableConsole(callback: (message: ConsoleMessage) => void, targetId?: string): Promise<void>;
  disableConsole(targetId?: string): Promise<void>;
  
  /**
   * Network monitoring  
   */
  enableNetwork(callback: (request: NetworkRequest) => void, targetId?: string): Promise<void>;
  disableNetwork(targetId?: string): Promise<void>;
  
  /**
   * Performance monitoring
   */
  enablePerformance(callback: (metrics: PerformanceMetrics) => void, targetId?: string): Promise<void>;
  disablePerformance(targetId?: string): Promise<void>;
  
  /**
   * Security analysis
   */
  getSecurityState(targetId?: string): Promise<{
    securityState: 'secure' | 'info' | 'insecure' | 'insecure-broken';
    explanations: Array<{
      securityState: string;
      title: string;
      summary: string;
      description: string;
    }>;
  }>;
  
  /**
   * Code coverage analysis
   */
  startCoverage(targetId?: string): Promise<void>;
  stopCoverage(targetId?: string): Promise<{
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
  }>;
  
  /**
   * Profiling support
   */
  startProfiling(targetId?: string): Promise<void>;
  stopProfiling(targetId?: string): Promise<{
    profile: any; // Browser-specific profile format
  }>;
}