/**
 * JTAG Universal Auto-Initialization
 * 
 * This module auto-detects environment and wires JTAG appropriately:
 * - Continuum integration: Connects to Continuum daemon system
 * - Standalone: Connects to standalone JTAG server
 * - Zero configuration required from developer
 */

// NOTE: When bundled for external use, this will use standalone JTAG types
// When used inside Continuum, this can import from shared modules

// Environment Detection - JTAG has no knowledge of external systems
interface JTAGEnvironment {
  endpoint: string;
  transport: 'websocket' | 'http' | 'auto';
}

function detectEnvironment(): JTAGEnvironment {
  // JTAG is purely standalone - always connects to its own server
  return {
    endpoint: `ws://localhost:9001`,            // JTAG server
    transport: 'websocket'                      // Direct WebSocket connection
  };
}

// Auto-Configuration - Pure JTAG standalone
function createAutoConfig(): any {
  const env = detectEnvironment();
  
  console.log(`🔌 JTAG: Initializing standalone debugging system`);
  
  return {
    context: 'browser',
    jtagPort: 9001,
    enableRemoteLogging: true,
    enableConsoleOutput: true,
    maxBufferSize: 1000,
    transport: {
      type: env.transport,
      fallback: 'http',
      retryAttempts: 3,
      retryDelay: 1000
    }
  };
}

// Console Interception
function interceptConsole() {
  const originalLog = console.log;
  const originalError = console.error;
  const originalWarn = console.warn;
  
  let intercepting = false; // Prevent recursive loops

  console.log = (...args: any[]) => {
    originalLog(...args);
    if (!intercepting && window.jtag) {
      intercepting = true;
      try {
        const message = args.join(' ');
        // Skip ALL JTAG log messages to prevent loops
        if (!message.includes('JTAG:') && !message.includes('📝') && !message.includes('🌐') && !message.includes('✅') && !message.includes('🔌') && !message.includes('📨')) {
          window.jtag.log('BROWSER_CONSOLE', message);
        }
      } finally {
        intercepting = false;
      }
    }
  };

  console.error = (...args: any[]) => {
    originalError(...args);
    if (!intercepting && window.jtag) {
      intercepting = true;
      try {
        const message = args.join(' ');
        if (!message.includes('❌ JTAG:')) {
          window.jtag.critical('BROWSER_CONSOLE', message);
        }
      } finally {
        intercepting = false;
      }
    }
  };

  console.warn = (...args: any[]) => {
    originalWarn(...args);
    if (!intercepting && window.jtag) {
      intercepting = true;
      try {
        const message = args.join(' ');
        if (!message.includes('⚠️ JTAG:')) {
          window.jtag.log('BROWSER_CONSOLE', `[WARN] ${message}`);
        }
      } finally {
        intercepting = false;
      }
    }
  };
}

// JTAG Client Interface - Pure standalone implementation
class JTAGBrowserClient {
  private config: any;
  private connected = false;
  private websocket: WebSocket | null = null;

  constructor(config: any) {
    this.config = config;
  }

  async autoConnect(): Promise<void> {
    try {
      console.log('🔌 JTAG: Starting autoConnect - config:', this.config);
      console.log('🔌 JTAG: Connecting directly to WebSocket server...');
      console.log('🔌 JTAG: Will try endpoint:', `ws://localhost:${this.config.jtagPort}`);
      
      // Connect directly to WebSocket - NO HTTP routing bullshit
      const wsEndpoint = `ws://localhost:${this.config.jtagPort}`;
      this.websocket = new WebSocket(wsEndpoint);
      
      const connectResult = await new Promise<any>((resolve, reject) => {
        this.websocket!.onopen = () => {
          console.log('✅ JTAG: WebSocket connected directly');
          this.connected = true;
          resolve({ success: true, transport: 'websocket' });
        };
        
        this.websocket!.onerror = (error) => {
          console.error('❌ JTAG: WebSocket connection failed:', error);
          reject(error);
        };
        
        this.websocket!.onmessage = (event) => {
          console.log('📨 JTAG: WebSocket message received:', event.data);
        };
        
        setTimeout(() => reject(new Error('WebSocket connection timeout')), 5000);
      });
      console.log('🔌 JTAG: Connect result:', connectResult);
      
      // Test logging immediately over WebSocket
      console.log('🧪 JTAG: Testing log message over WebSocket...');
      this.log('BROWSER_INIT', 'Browser client connected successfully via WebSocket');
      
      // Emit ready event for app integration
      window.dispatchEvent(new CustomEvent('jtag:ready', {
        detail: {
          endpoint: wsEndpoint,
          config: this.config
        }
      }));
      
    } catch (error: any) {
      console.error('❌ JTAG: WebSocket connection failed:', error);
      const errorDetails = error instanceof Error ? {
        name: error.name,
        message: error.message,
        stack: error.stack
      } : { message: String(error) };
      console.error('❌ JTAG: Error details:', errorDetails);
      window.dispatchEvent(new CustomEvent('jtag:error', {
        detail: { error: error instanceof Error ? error.message : String(error) }
      }));
    }
  }

  // Core JTAG API - Standalone implementation
  getUUID() {
    return {
      uuid: 'browser_' + Date.now().toString(36),
      context: 'browser',
      timestamp: new Date().toISOString()
    };
  }

  // Transport router method - routes messages through available transports
  private async routeMessage(message: any): Promise<any> {
    try {
      // Use demo server port (9002) for API routing, not JTAG port (9001)
      const url = `http://localhost:9002/api/route`;
      console.log('🌐 JTAG: Making fetch request to:', url);
      console.log('🌐 JTAG: Request payload:', message);
      
      // Use fetch as fallback transport for browser clients
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(message)
      });
      
      console.log('🌐 JTAG: Response status:', response.status, response.statusText);
      
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      
      const result = await response.json();
      console.log('🌐 JTAG: Response data:', result);
      return result;
    } catch (error) {
      console.error('❌ JTAG Transport routing failed:', error);
      return { success: false, error: error instanceof Error ? error.message : String(error) };
    }
  }

  log(component: string, message: string, data?: any): void {
    const entry = {
      timestamp: new Date().toISOString(),
      context: 'browser',
      component,
      message,
      data,
      type: 'log'
    };
    
    console.log('📝 JTAG: Sending log over WebSocket:', entry);
    
    if (this.websocket && this.websocket.readyState === WebSocket.OPEN) {
      this.websocket.send(JSON.stringify({ type: 'log', payload: entry }));
      console.log('✅ JTAG: Log sent via WebSocket');
    } else {
      console.error('❌ JTAG: WebSocket not connected, cannot send log');
    }
  }

  critical(component: string, message: string, data?: any): void {
    const entry = {
      timestamp: new Date().toISOString(),
      context: 'browser',
      component,
      message,
      data,
      type: 'critical'
    };
    
    if (this.websocket && this.websocket.readyState === WebSocket.OPEN) {
      this.websocket.send(JSON.stringify({ type: 'log', payload: entry }));
    }
  }

  async exec(code: string): Promise<any> {
    try {
      const result = eval(code);
      return {
        success: true,
        result,
        context: 'browser',
        timestamp: new Date().toISOString()
      };
    } catch (error: any) {
      return {
        success: false,
        error: error.message,
        context: 'browser',
        timestamp: new Date().toISOString()
      };
    }
  }

  async screenshot(filename: string, _options?: any): Promise<any> {
    // Browser screenshot would use html2canvas or similar
    return {
      success: true,
      filename,
      context: 'browser',
      timestamp: new Date().toISOString(),
      message: 'Browser screenshot functionality would be implemented here'
    };
  }

  async connect(_params?: any): Promise<any> {
    return {
      healthy: true,
      transport: {
        type: 'websocket',
        state: this.connected ? 'connected' : 'disconnected',
        endpoint: `ws://localhost:${this.config.jtagPort}`
      }
    };
  }
}

// Global Declaration
declare global {
  interface Window {
    jtag: JTAGBrowserClient;
    JTAG_CONFIG?: any;
  }
}

// Create and export a client factory function for proper module usage
export async function createJTAGClient(customConfig?: any): Promise<JTAGBrowserClient> {
  console.log('🚀 JTAG: Creating client...');
  
  // Use provided config or auto-detect
  const config = customConfig || window.JTAG_CONFIG || createAutoConfig();
  console.log('⚙️ JTAG: Using config:', config);
  
  // Create client instance
  const client = new JTAGBrowserClient(config);
  console.log('🎯 JTAG: Client instance created');
  
  // Intercept console
  interceptConsole();
  console.log('🎧 JTAG: Console intercepted');
  
  // Auto-connect the client
  await client.autoConnect();
  
  // Store globally for convenience (optional)
  window.jtag = client;
  
  console.log('✅ JTAG: Client ready');
  return client;
}

// Shared instance for module imports
let sharedInstance: JTAGBrowserClient | null = null;

export class JTAGClient {
  static get shared(): JTAGBrowserClient {
    if (!sharedInstance) {
      // Wait for auto-init to create the instance
      sharedInstance = (window as any).jtag || null;
    }
    return sharedInstance!;
  }
  
  static waitForReady(): Promise<JTAGBrowserClient> {
    return new Promise((resolve) => {
      if (sharedInstance || (window as any).jtag) {
        resolve(JTAGClient.shared);
      } else {
        window.addEventListener('jtag:ready', () => {
          resolve(JTAGClient.shared);
        });
      }
    });
  }
}

export { JTAGBrowserClient, detectEnvironment, createAutoConfig };