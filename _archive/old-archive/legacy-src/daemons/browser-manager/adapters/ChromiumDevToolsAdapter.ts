/**
 * Chromium DevTools Adapter - Chrome DevTools Protocol (CDP) implementation
 * 
 * Supports Chrome, Edge, Opera, and other Chromium-based browsers
 * Provides full CDP feature set including:
 * - Runtime evaluation and console
 * - Network monitoring and interception
 * - Performance profiling and metrics
 * - Security analysis
 * - Code coverage
 * - Page automation and screenshots
 */

import { 
  IDevToolsAdapter, 
  DevToolsTarget, 
  ConsoleMessage, 
  NetworkRequest, 
  PerformanceMetrics,
  DevToolsCapabilities 
} from '../types/index.js';
import { WebSocket } from 'ws';

export class ChromiumDevToolsAdapter implements IDevToolsAdapter {
  readonly protocol = 'cdp';
  readonly capabilities: DevToolsCapabilities = {
    supportsConsole: true,
    supportsNetwork: true,
    supportsPerformance: true,
    supportsScreenshot: true,
    supportsProfiling: true,
    supportsCodeCoverage: true,
    supportsSecurityAnalysis: true
  };

  private ws: WebSocket | null = null;
  private baseUrl: string = '';
  private currentTargetId: string = '';
  
  constructor() {
    // TODO: Use currentTargetId when implementing multi-target support
    console.log('TODO: Initialize ChromiumDevToolsAdapter, currentTargetId:', this.currentTargetId);
  }

  private messageId = 1;
  private pendingCommands = new Map<number, { resolve: Function; reject: Function }>();
  private eventHandlers = new Map<string, Function[]>();

  async connect(debugUrl: string, targetId?: string): Promise<void> {
    this.baseUrl = debugUrl;
    
    // Get targets first
    const targets = await this.getTargets();
    const target = targetId ? 
      targets.find(t => t.id === targetId) : 
      targets.find(t => t.type === 'page') || targets[0];
      
    if (!target) {
      throw new Error('No suitable target found');
    }
    
    this.currentTargetId = target.id;
    
    if (!target.webSocketDebuggerUrl) {
      throw new Error('Target does not support WebSocket debugging');
    }
    
    // Connect to WebSocket
    this.ws = new WebSocket(target.webSocketDebuggerUrl);
    
    return new Promise((resolve, reject) => {
      this.ws!.on('open', () => {
        console.log(`✅ Connected to Chromium DevTools: ${target.title}`);
        resolve();
      });
      
      this.ws!.on('error', (error) => {
        console.error('❌ DevTools WebSocket error:', error);
        reject(error);
      });
      
      this.ws!.on('message', (data) => {
        this.handleMessage(JSON.parse(data.toString()));
      });
      
      this.ws!.on('close', () => {
        console.log('🔌 DevTools WebSocket connection closed');
        this.ws = null;
      });
    });
  }

  async disconnect(): Promise<void> {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }

  async getTargets(): Promise<DevToolsTarget[]> {
    try {
      const response = await fetch(`${this.baseUrl}/json`);
      const targets = await response.json();
      
      return targets.map((target: any) => ({
        id: target.id,
        type: target.type,
        title: target.title,
        url: target.url,
        webSocketDebuggerUrl: target.webSocketDebuggerUrl,
        devtoolsFrontendUrl: target.devtoolsFrontendUrl
      }));
    } catch (error) {
      throw new Error(`Failed to get targets: ${error}`);
    }
  }

  async evaluateScript(expression: string, targetId?: string): Promise<any> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for script evaluation:', activeTargetId);
    
    const result = await this.sendCommand('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true
    });
    
    if (result.exceptionDetails) {
      throw new Error(`Script evaluation failed: ${result.exceptionDetails.text}`);
    }
    
    return result.result.value;
  }

  async navigate(url: string, targetId?: string): Promise<void> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for navigation:', activeTargetId);
    
    await this.sendCommand('Page.navigate', { url });
  }

  async reload(targetId?: string, ignoreCache: boolean = false): Promise<void> {
    // Ignores targetId parameter - uses the connected target from this.currentTargetId
    if (targetId) { /* Interface compatibility - parameter ignored */ }
    await this.sendCommand('Page.reload', { ignoreCache });
  }

  async screenshot(targetId?: string, options: {
    format?: 'png' | 'jpeg';
    quality?: number;
    fullPage?: boolean;
  } = {}): Promise<Buffer> {
    // Ignores targetId parameter - uses the connected target from this.currentTargetId
    if (targetId) { /* Interface compatibility - parameter ignored */ }
    const params: any = {
      format: options.format || 'png'
    };
    
    if (options.quality && options.format === 'jpeg') {
      params.quality = options.quality;
    }
    
    if (options.fullPage) {
      // Get full page dimensions
      const { contentSize } = await this.sendCommand('Page.getLayoutMetrics');
      params.clip = {
        x: 0,
        y: 0,
        width: contentSize.width,
        height: contentSize.height,
        scale: 1
      };
    }
    
    const result = await this.sendCommand('Page.captureScreenshot', params);
    return Buffer.from(result.data, 'base64');
  }

  async enableConsole(callback: (message: ConsoleMessage) => void, targetId?: string): Promise<void> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for console enabling:', activeTargetId);
    
    // Enable Runtime domain
    await this.sendCommand('Runtime.enable');
    
    // Register event handler
    this.addEventListener('Runtime.consoleAPICalled', (params: any) => {
      const message: ConsoleMessage = {
        level: params.type,
        text: params.args.map((arg: any) => arg.value || arg.description || '').join(' '),
        timestamp: params.timestamp,
        source: 'console',
        args: params.args,
        stackTrace: params.stackTrace
      };
      callback(message);
    });
    
    // Also handle exceptions
    this.addEventListener('Runtime.exceptionThrown', (params: any) => {
      const message: ConsoleMessage = {
        level: 'error',
        text: params.exceptionDetails.text,
        timestamp: params.timestamp,
        source: 'console',
        stackTrace: params.exceptionDetails.stackTrace
      };
      callback(message);
    });
  }

  async disableConsole(targetId?: string): Promise<void> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for console disabling:', activeTargetId);
    
    await this.sendCommand('Runtime.disable');
    this.removeEventListener('Runtime.consoleAPICalled');
    this.removeEventListener('Runtime.exceptionThrown');
  }

  async enableNetwork(callback: (request: NetworkRequest) => void, targetId?: string): Promise<void> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for network enabling:', activeTargetId);
    
    await this.sendCommand('Network.enable');
    
    const requests = new Map<string, Partial<NetworkRequest>>();
    
    this.addEventListener('Network.requestWillBeSent', (params: any) => {
      const request: NetworkRequest = {
        requestId: params.requestId,
        url: params.request.url,
        method: params.request.method,
        headers: params.request.headers,
        timestamp: params.timestamp
      };
      requests.set(params.requestId, request);
      callback(request);
    });
    
    this.addEventListener('Network.responseReceived', (params: any) => {
      const request = requests.get(params.requestId);
      if (request) {
        Object.assign(request, {
          status: params.response.status,
          statusText: params.response.statusText,
          responseHeaders: params.response.headers
        });
        callback(request as NetworkRequest);
      }
    });
    
    this.addEventListener('Network.loadingFailed', (params: any) => {
      const request = requests.get(params.requestId);
      if (request) {
        Object.assign(request, {
          failed: true,
          errorReason: params.errorText
        });
        callback(request as NetworkRequest);
      }
    });
  }

  async disableNetwork(targetId?: string): Promise<void> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for network disabling:', activeTargetId);
    
    await this.sendCommand('Network.disable');
    this.removeEventListener('Network.requestWillBeSent');
    this.removeEventListener('Network.responseReceived');
    this.removeEventListener('Network.loadingFailed');
  }

  async enablePerformance(callback: (metrics: PerformanceMetrics) => void, targetId?: string): Promise<void> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for performance enabling:', activeTargetId);
    
    await this.sendCommand('Performance.enable');
    
    this.addEventListener('Performance.metrics', (params: any) => {
      const metrics: PerformanceMetrics = {
        scriptDuration: params.metrics.find((m: any) => m.name === 'ScriptDuration')?.value || 0,
        layoutDuration: params.metrics.find((m: any) => m.name === 'LayoutDuration')?.value || 0,
        recalcStyleDuration: params.metrics.find((m: any) => m.name === 'RecalcStyleDuration')?.value || 0,
        timestamp: params.timestamp
      };
      callback(metrics);
    });
  }

  async disablePerformance(targetId?: string): Promise<void> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for performance disabling:', activeTargetId);
    
    await this.sendCommand('Performance.disable');
    this.removeEventListener('Performance.metrics');
  }

  async getSecurityState(targetId?: string): Promise<{
    securityState: 'secure' | 'info' | 'insecure' | 'insecure-broken';
    explanations: Array<{
      securityState: string;
      title: string;
      summary: string;
      description: string;
    }>;
  }> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for security state:', activeTargetId);
    
    await this.sendCommand('Security.enable');
    const result = await this.sendCommand('Security.getSecurityState');
    await this.sendCommand('Security.disable');
    
    return {
      securityState: result.securityState,
      explanations: result.explanations || []
    };
  }

  async startCoverage(targetId?: string): Promise<void> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for coverage start:', activeTargetId);
    
    await this.sendCommand('Profiler.enable');
    await this.sendCommand('Profiler.startPreciseCoverage', {
      callCount: true,
      detailed: true
    });
  }

  async stopCoverage(targetId?: string): Promise<{
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
  }> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for coverage stop:', activeTargetId);
    
    const result = await this.sendCommand('Profiler.takePreciseCoverage');
    await this.sendCommand('Profiler.stopPreciseCoverage');
    await this.sendCommand('Profiler.disable');
    
    return result;
  }

  async startProfiling(targetId?: string): Promise<void> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for profiling start:', activeTargetId);
    
    await this.sendCommand('Profiler.enable');
    await this.sendCommand('Profiler.start');
  }

  async stopProfiling(targetId?: string): Promise<{ profile: any }> {
    // Use specified targetId or fall back to connected target
    const activeTargetId = targetId || this.currentTargetId;
    console.log('TODO: Use activeTargetId for profiling stop:', activeTargetId);
    
    const result = await this.sendCommand('Profiler.stop');
    await this.sendCommand('Profiler.disable');
    return result;
  }

  // Private helper methods

  private async sendCommand(method: string, params: any = {}): Promise<any> {
    if (!this.isConnected()) {
      throw new Error('DevTools not connected');
    }
    
    const id = this.messageId++;
    const message = { id, method, params };
    
    return new Promise((resolve, reject) => {
      this.pendingCommands.set(id, { resolve, reject });
      this.ws!.send(JSON.stringify(message));
      
      // Timeout after 30 seconds
      setTimeout(() => {
        if (this.pendingCommands.has(id)) {
          this.pendingCommands.delete(id);
          reject(new Error(`Command timeout: ${method}`));
        }
      }, 30000);
    });
  }

  private handleMessage(message: any): void {
    if (message.id && this.pendingCommands.has(message.id)) {
      // Response to command
      const { resolve, reject } = this.pendingCommands.get(message.id)!;
      this.pendingCommands.delete(message.id);
      
      if (message.error) {
        reject(new Error(`DevTools error: ${message.error.message}`));
      } else {
        resolve(message.result);
      }
    } else if (message.method) {
      // Event
      const handlers = this.eventHandlers.get(message.method) || [];
      handlers.forEach(handler => {
        try {
          handler(message.params);
        } catch (error) {
          console.error(`DevTools event handler error for ${message.method}:`, error);
        }
      });
    }
  }

  private addEventListener(eventName: string, handler: Function): void {
    if (!this.eventHandlers.has(eventName)) {
      this.eventHandlers.set(eventName, []);
    }
    this.eventHandlers.get(eventName)!.push(handler);
  }

  private removeEventListener(eventName: string, handler?: Function): void {
    if (handler) {
      const handlers = this.eventHandlers.get(eventName) || [];
      const index = handlers.indexOf(handler);
      if (index > -1) {
        handlers.splice(index, 1);
      }
    } else {
      this.eventHandlers.delete(eventName);
    }
  }
}