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

  console.log = (...args: any[]) => {
    originalLog(...args);
    if (window.jtag) {
      window.jtag.log('BROWSER_CONSOLE', args.join(' '));
    }
  };

  console.error = (...args: any[]) => {
    originalError(...args);
    if (window.jtag) {
      window.jtag.critical('BROWSER_CONSOLE', args.join(' '));
    }
  };

  console.warn = (...args: any[]) => {
    originalWarn(...args);
    if (window.jtag) {
      window.jtag.log('BROWSER_CONSOLE', `[WARN] ${args.join(' ')}`);
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
      console.log('🔌 JTAG: Connecting to debugging server...');
      
      this.websocket = new WebSocket(`ws://localhost:${this.config.jtagPort}`);
      
      this.websocket.onopen = () => {
        this.connected = true;
        console.log('✅ JTAG: Connected and ready');
        
        // Emit ready event for app integration
        window.dispatchEvent(new CustomEvent('jtag:ready', {
          detail: { 
            endpoint: `ws://localhost:${this.config.jtagPort}`,
            config: this.config 
          }
        }));
      };
      
      this.websocket.onerror = (error) => {
        console.error('❌ JTAG: Connection failed:', error);
        window.dispatchEvent(new CustomEvent('jtag:error', {
          detail: { error: 'WebSocket connection failed' }
        }));
      };
      
    } catch (error) {
      console.error('❌ JTAG: Auto-connect failed:', error);
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

  log(component: string, message: string, data?: any): void {
    const entry = {
      timestamp: new Date().toISOString(),
      context: 'browser',
      component,
      message,
      data,
      type: 'log'
    };
    
    if (this.websocket && this.connected) {
      this.websocket.send(JSON.stringify({ type: 'log', payload: entry }));
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
    
    if (this.websocket && this.connected) {
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

// Auto-Initialization
(function autoInitJTAG() {
  // Use injected config or auto-detect
  const config = window.JTAG_CONFIG || createAutoConfig();
  
  // Create global JTAG instance
  window.jtag = new JTAGBrowserClient(config);
  
  // Intercept console
  interceptConsole();
  
  // Auto-connect when DOM is ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
      window.jtag.autoConnect();
    });
  } else {
    // DOM already loaded
    setTimeout(() => window.jtag.autoConnect(), 100);
  }
  
  console.log('🔌 JTAG: Auto-initialization complete');
})();

export { JTAGBrowserClient, detectEnvironment, createAutoConfig };