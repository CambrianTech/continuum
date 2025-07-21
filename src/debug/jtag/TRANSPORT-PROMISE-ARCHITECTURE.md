# JTAG Universal Command Transport - Promise/Event Architecture

## 🚀 **Vision: Universal Command Bus with Promise/Event Flow**

JTAG becomes the foundational transport layer that any system can use for bidirectional promise/event communication:

```
CLI Command → Server → Transport → Browser → Transport → Server → Promise Resolution
./continuum screenshot → JTAG Transport → Browser execution → File creation → CLI response
```

## 🏗️ **Promise-Aware Transport Architecture**

### **Bidirectional Promise Flow**
```typescript
// Server initiates, browser executes, server resolves
const screenshot = await jtag.screenshot('debug.png'); 
// → WebSocket request to browser
// → Browser captures screenshot  
// → WebSocket response to server
// → Server saves file
// → Promise resolves with file path
```

### **Event-Driven Communication**
```typescript
// Real-time event streaming
jtag.on('screenshot-progress', (data) => {
  console.log(`Screenshot ${data.percent}% complete`);
});

jtag.on('screenshot-complete', (result) => {
  console.log(`Screenshot saved: ${result.filepath}`);
});
```

## 🔧 **Core Transport Implementation**

### **Universal Transport Message Protocol**
```typescript
interface TransportMessage {
  id: string;                           // Unique message identifier
  type: 'request' | 'response' | 'event' | 'error';
  operation: 'screenshot' | 'exec' | 'log' | 'info';
  payload: any;                         // Operation-specific data
  replyTo?: string;                     // For promise resolution
  timestamp: string;                    // ISO timestamp
  source: 'browser' | 'server' | 'cli' | 'external';
  target?: 'browser' | 'server' | 'all';
  priority?: 'low' | 'normal' | 'high' | 'critical';
}
```

### **Promise-Aware Transport Layer**
```typescript
class JTAGCommandTransport {
  private pendingPromises = new Map<string, {resolve: Function, reject: Function, timeout: NodeJS.Timeout}>();
  private eventListeners = new Map<string, Set<Function>>();
  private router: JTAGRouter;

  constructor(router: JTAGRouter) {
    this.router = router;
    this.setupMessageHandling();
  }

  /**
   * Send request and return promise that resolves when response received
   */
  async request(operation: string, payload: any, options?: {timeout?: number}): Promise<any> {
    const id = `req_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;
    const timeout = options?.timeout || 30000;

    return new Promise((resolve, reject) => {
      // Set up timeout
      const timeoutHandle = setTimeout(() => {
        this.pendingPromises.delete(id);
        reject(new Error(`Request timeout after ${timeout}ms: ${operation}`));
      }, timeout);

      // Store promise handlers
      this.pendingPromises.set(id, {resolve, reject, timeout: timeoutHandle});

      // Send request through router
      const message: TransportMessage = {
        id,
        type: 'request',
        operation,
        payload,
        timestamp: new Date().toISOString(),
        source: typeof window === 'undefined' ? 'server' : 'browser',
        target: typeof window === 'undefined' ? 'browser' : 'server'
      };

      this.router.routeMessage(message).catch(error => {
        clearTimeout(timeoutHandle);
        this.pendingPromises.delete(id);
        reject(error);
      });
    });
  }

  /**
   * Send response to a request
   */
  async respond(requestId: string, payload: any, error?: string): Promise<void> {
    const message: TransportMessage = {
      id: `res_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
      type: error ? 'error' : 'response',
      operation: 'response',
      payload: error ? {error} : payload,
      replyTo: requestId,
      timestamp: new Date().toISOString(),
      source: typeof window === 'undefined' ? 'server' : 'browser'
    };

    await this.router.routeMessage(message);
  }

  /**
   * Emit event to all subscribers
   */
  async emit(eventName: string, data: any): Promise<void> {
    const message: TransportMessage = {
      id: `evt_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
      type: 'event',
      operation: eventName,
      payload: data,
      timestamp: new Date().toISOString(),
      source: typeof window === 'undefined' ? 'server' : 'browser',
      target: 'all'
    };

    await this.router.routeMessage(message);
    
    // Also emit locally
    this.emitLocal(eventName, data);
  }

  /**
   * Subscribe to events
   */
  on(eventName: string, callback: (data: any) => void): void {
    if (!this.eventListeners.has(eventName)) {
      this.eventListeners.set(eventName, new Set());
    }
    this.eventListeners.get(eventName)!.add(callback);
  }

  /**
   * Unsubscribe from events
   */
  off(eventName: string, callback: (data: any) => void): void {
    const listeners = this.eventListeners.get(eventName);
    if (listeners) {
      listeners.delete(callback);
    }
  }

  private setupMessageHandling(): void {
    // Handle incoming responses and events
    this.router.subscribe('*', (message: TransportMessage) => {
      if (message.type === 'response' || message.type === 'error') {
        this.handleResponse(message);
      } else if (message.type === 'event') {
        this.handleEvent(message);
      } else if (message.type === 'request') {
        this.handleRequest(message);
      }
    });
  }

  private handleResponse(message: TransportMessage): void {
    const pending = this.pendingPromises.get(message.replyTo!);
    if (pending) {
      clearTimeout(pending.timeout);
      this.pendingPromises.delete(message.replyTo!);
      
      if (message.type === 'error') {
        pending.reject(new Error(message.payload.error));
      } else {
        pending.resolve(message.payload);
      }
    }
  }

  private handleEvent(message: TransportMessage): void {
    this.emitLocal(message.operation, message.payload);
  }

  private handleRequest(message: TransportMessage): void {
    this.emitLocal(`request:${message.operation}`, {
      id: message.id,
      payload: message.payload,
      respond: (data: any, error?: string) => this.respond(message.id, data, error)
    });
  }

  private emitLocal(eventName: string, data: any): void {
    const listeners = this.eventListeners.get(eventName);
    if (listeners) {
      listeners.forEach(callback => {
        try {
          callback(data);
        } catch (error) {
          console.error(`Event listener error for ${eventName}:`, error);
        }
      });
    }
  }
}
```

## 🎯 **Continuum Command Integration**

### **Universal Command Transport**
```typescript
// In JTAG index.ts - Universal API
export class JTAGUniversalAPI {
  private transport: JTAGCommandTransport;

  constructor() {
    this.transport = new JTAGCommandTransport(jtagRouter);
    this.setupCommandHandlers();
  }

  // Screenshot with promise resolution
  async screenshot(filename: string, options?: ScreenshotOptions): Promise<ScreenshotResult> {
    return await this.transport.request('screenshot', {filename, ...options});
  }

  // Code execution with promise resolution  
  async exec(code: string, options?: ExecOptions): Promise<ExecResult> {
    return await this.transport.request('exec', {code, ...options});
  }

  // System info with promise resolution
  async info(): Promise<SystemInfo> {
    return await this.transport.request('info', {});
  }

  // Event subscription
  on(eventName: string, callback: (data: any) => void): void {
    this.transport.on(eventName, callback);
  }

  // Event emission
  async emit(eventName: string, data: any): Promise<void> {
    await this.transport.emit(eventName, data);
  }

  private setupCommandHandlers(): void {
    // Server-side command handlers
    if (typeof window === 'undefined') {
      this.transport.on('request:screenshot', async ({id, payload, respond}) => {
        try {
          const result = await this.handleScreenshotRequest(payload);
          respond(result);
        } catch (error) {
          respond(null, error.message);
        }
      });

      this.transport.on('request:exec', async ({id, payload, respond}) => {
        try {
          const result = await this.handleExecRequest(payload);
          respond(result);
        } catch (error) {
          respond(null, error.message);
        }
      });
    }
    
    // Browser-side command handlers
    if (typeof window !== 'undefined') {
      this.transport.on('request:screenshot', async ({id, payload, respond}) => {
        try {
          const result = await this.handleBrowserScreenshot(payload);
          respond(result);
        } catch (error) {
          respond(null, error.message);
        }
      });
    }
  }

  private async handleScreenshotRequest(payload: any): Promise<ScreenshotResult> {
    // Server orchestrates browser screenshot
    this.transport.emit('screenshot-progress', {percent: 0, status: 'initiating'});
    
    // Request browser to capture
    const imageData = await this.transport.request('browser-capture', payload);
    
    this.transport.emit('screenshot-progress', {percent: 50, status: 'processing'});
    
    // Save to file system
    const filepath = await this.saveScreenshot(imageData, payload.filename);
    
    this.transport.emit('screenshot-progress', {percent: 100, status: 'complete'});
    this.transport.emit('screenshot-complete', {filepath, size: imageData.length});
    
    return {
      success: true,
      filepath,
      size: imageData.length,
      timestamp: new Date().toISOString()
    };
  }

  private async handleBrowserScreenshot(payload: any): Promise<any> {
    // Browser captures screenshot and returns data
    // Implementation would use html2canvas or similar
    return {
      imageData: 'base64-encoded-image-data',
      width: 1200,
      height: 800
    };
  }

  private async saveScreenshot(imageData: any, filename: string): Promise<string> {
    // Server saves screenshot to file system
    const filepath = path.join(jtagConfig.screenshotDirectory, filename);
    // Save logic here
    return filepath;
  }

  private async handleExecRequest(payload: any): Promise<ExecResult> {
    // Execute code and return result
    try {
      const result = eval(payload.code);
      return {
        success: true,
        result,
        executionTime: Date.now(),
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }
}

// Export universal API
export const jtag = new JTAGUniversalAPI();
```

### **Continuum Command Integration**
```typescript
// In ScreenshotCommand.ts
import { jtag } from '../debug/jtag';

export class ScreenshotCommand {
  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    // Subscribe to progress events
    jtag.on('screenshot-progress', (data) => {
      console.log(`📸 Screenshot ${data.percent}% complete: ${data.status}`);
    });

    // Execute screenshot via JTAG transport
    try {
      const result = await jtag.screenshot(params.filename, {
        selector: params.selector,
        width: params.width,
        height: params.height
      });

      console.log(`✅ Screenshot saved: ${result.filepath}`);
      return result;
    } catch (error) {
      console.error(`❌ Screenshot failed: ${error.message}`);
      throw error;
    }
  }
}
```

## 🧪 **Comprehensive Test Architecture**

### **Test Structure**
```
tests/
├── piece-1-basic-connection/
│   ├── websocket-server-startup.test.ts
│   ├── browser-client-loading.test.ts
│   ├── connection-establishment.test.ts
│   └── connection-events.test.ts
├── piece-2-simple-message-transport/
│   ├── console-log-flow.test.ts
│   ├── websocket-message-routing.test.ts
│   ├── file-creation-verification.test.ts
│   └── promise-resolution.test.ts
├── piece-3-transport-backends/
│   ├── file-transport.test.ts
│   ├── event-transport.test.ts
│   ├── http-transport.test.ts
│   └── backend-isolation.test.ts
├── piece-4-error-handling/
│   ├── transport-failures.test.ts
│   ├── router-fallback.test.ts
│   ├── promise-rejection.test.ts
│   └── network-recovery.test.ts
├── piece-5-performance/
│   ├── high-frequency-messages.test.ts
│   ├── concurrent-connections.test.ts
│   ├── backpressure-handling.test.ts
│   └── message-queuing.test.ts
└── piece-6-end-to-end/
    ├── screenshot-flow.test.ts
    ├── code-execution.test.ts
    ├── command-integration.test.ts
    └── complex-scenarios.test.ts
```

### **Piece 1: Basic Connection Tests**
```typescript
// tests/piece-1-basic-connection/connection-establishment.test.ts
import puppeteer from 'puppeteer';
import { JTAGServer } from '../../server/JTAGServer';

describe('Piece 1: Basic Connection', () => {
  let jtagServer: JTAGServer;
  let browser: any;
  let page: any;

  beforeAll(async () => {
    // Start JTAG WebSocket server
    jtagServer = new JTAGServer({port: 9001});
    await jtagServer.start();
    
    // Launch browser
    browser = await puppeteer.launch({headless: false, devtools: true});
    page = await browser.newPage();
  });

  afterAll(async () => {
    await browser?.close();
    await jtagServer?.stop();
  });

  test('WebSocket server starts on port 9001', async () => {
    const response = await fetch('http://localhost:9001/health');
    expect(response.ok).toBe(true);
  });

  test('Browser loads /jtag.js successfully', async () => {
    const response = await page.goto('http://localhost:9002/jtag.js');
    expect(response.status()).toBe(200);
    expect(response.headers()['content-type']).toContain('javascript');
  });

  test('WebSocket connection established', async () => {
    await page.goto('http://localhost:9002');
    
    // Wait for JTAG to initialize
    await page.waitForFunction(() => window.jtag && window.jtag.connected);
    
    const connected = await page.evaluate(() => window.jtag.connected);
    expect(connected).toBe(true);
  });

  test('Connection events fire correctly', async () => {
    const events = [];
    
    await page.evaluateOnNewDocument(() => {
      window.jtagEvents = [];
      window.addEventListener('jtag:connecting', (e) => window.jtagEvents.push('connecting'));
      window.addEventListener('jtag:ready', (e) => window.jtagEvents.push('ready'));
    });
    
    await page.goto('http://localhost:9002');
    await page.waitForFunction(() => window.jtagEvents?.includes('ready'));
    
    const events = await page.evaluate(() => window.jtagEvents);
    expect(events).toContain('connecting');
    expect(events).toContain('ready');
  });
});
```

### **Piece 2: Simple Message Transport Tests**
```typescript
// tests/piece-2-simple-message-transport/console-log-flow.test.ts
import puppeteer from 'puppeteer';
import * as fs from 'fs';
import * as path from 'path';

describe('Piece 2: Simple Message Transport', () => {
  test('Browser console.log → WebSocket → Router → File Transport', async () => {
    const browser = await puppeteer.launch();
    const page = await browser.newPage();
    
    await page.goto('http://localhost:9002');
    await page.waitForFunction(() => window.jtag?.connected);
    
    // Send console.log from browser
    await page.evaluate(() => {
      console.log('Test message from browser');
    });
    
    // Wait for file creation
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Verify file was created
    const logPath = path.join('.continuum/jtag/logs/browser.log.txt');
    expect(fs.existsSync(logPath)).toBe(true);
    
    const content = fs.readFileSync(logPath, 'utf8');
    expect(content).toContain('Test message from browser');
    
    await browser.close();
  });

  test('Promise resolves correctly in browser', async () => {
    const browser = await puppeteer.launch();
    const page = await browser.newPage();
    
    await page.goto('http://localhost:9002');
    await page.waitForFunction(() => window.jtag?.connected);
    
    // Test promise-based operation
    const result = await page.evaluate(async () => {
      const response = await window.jtag.info();
      return response;
    });
    
    expect(result.success).toBe(true);
    expect(result.timestamp).toBeDefined();
    
    await browser.close();
  });
});
```

### **Piece 6: End-to-End Screenshot Flow**
```typescript
// tests/piece-6-end-to-end/screenshot-flow.test.ts
describe('Piece 6: End-to-End Screenshot Flow', () => {
  test('Complete screenshot flow: CLI → Server → Browser → Server → File', async () => {
    const browser = await puppeteer.launch();
    const page = await browser.newPage();
    
    await page.goto('http://localhost:9002');
    await page.waitForFunction(() => window.jtag?.connected);
    
    // Track events
    const events = [];
    await page.evaluate(() => {
      window.screenshotEvents = [];
      window.jtag.on('screenshot-progress', (data) => {
        window.screenshotEvents.push(`progress:${data.percent}`);
      });
      window.jtag.on('screenshot-complete', (data) => {
        window.screenshotEvents.push(`complete:${data.filepath}`);
      });
    });
    
    // Execute screenshot from server side
    const result = await jtag.screenshot('test-flow.png', {
      selector: 'body',
      width: 1200,
      height: 800
    });
    
    // Verify result
    expect(result.success).toBe(true);
    expect(result.filepath).toContain('test-flow.png');
    expect(fs.existsSync(result.filepath)).toBe(true);
    
    // Verify events fired in browser
    const events = await page.evaluate(() => window.screenshotEvents);
    expect(events).toContain('progress:0');
    expect(events).toContain('progress:50');
    expect(events).toContain('progress:100');
    expect(events.some(e => e.startsWith('complete:'))).toBe(true);
    
    await browser.close();
  });
});
```

## 📊 **Test Success Criteria**

### **Measurable Success Criteria**
- **Connection Time**: WebSocket connection < 500ms
- **Message Latency**: Browser → Server < 50ms
- **File Creation**: Log files appear within 100ms
- **Promise Resolution**: Async operations complete within timeout
- **Event Delivery**: All events fire in correct order
- **Error Recovery**: Failed transports recover within 5 seconds
- **Throughput**: Handle 100+ messages/second
- **Memory Usage**: < 50MB for transport layer

### **Clear Failure Diagnostics**
- **Connection Failures**: WebSocket handshake errors
- **Message Loss**: Missing log entries or responses
- **Promise Timeouts**: Operations exceeding expected duration
- **Event Missing**: Expected events not firing
- **File System Errors**: Failed file creation or incorrect content
- **Memory Leaks**: Increasing memory usage over time
- **Performance Degradation**: Throughput below threshold

This architecture provides the foundation for true bidirectional promise/event communication that can power the entire Continuum command system through a universal transport layer.