/**
 * Console Daemon - Universal Console Management
 * 
 * Handles console logging, interception, and routing across browser/server contexts.
 * Follows symmetric daemon architecture principles.
 */

import { DaemonBase } from '../../../shared/DaemonBase';
import { JTAGContext, JTAGMessage, JTAGPayload, JTAGMessageFactory } from '../../../shared/JTAGTypes';
import { JTAGRouter } from '../../../shared/JTAGRouter';
import { SystemEvents } from '../../../shared/events/SystemEvents';
import { TransportEvents } from '../../../transports/TransportEvents';
import { ConsoleEvents } from '../ConsoleEvents';

// Console-specific payload
export class ConsolePayload extends JTAGPayload {
  level: 'log' | 'info' | 'warn' | 'error' | 'debug';
  component: string;
  message: string;
  timestamp: string;
  context: 'browser' | 'server';
  data?: unknown; // Keep optional but use unknown instead of any
  stack?: string;

  constructor(data: Partial<ConsolePayload>) {
    super();
    this.level = data.level || 'log';
    this.component = data.component || 'UNKNOWN';
    this.message = data.message || '';
    this.timestamp = data.timestamp || new Date().toISOString();
    this.context = data.context || 'server';
    this.data = data.data;
    this.stack = data.stack;
  }
}

export interface ConsoleFilter {
  excludePatterns: string[];
  includeComponents?: string[];
  minLevel?: 'log' | 'info' | 'warn' | 'error' | 'debug';
}

/**
 * Universal Console Handler - Symmetric daemon following router pattern
 */
export abstract class ConsoleDaemon extends DaemonBase {
  public readonly subpath: string = 'console';
  protected intercepting = false;
  protected originalConsole: {
    log: typeof console.log;
    info: typeof console.info;
    warn: typeof console.warn;
    error: typeof console.error;
    debug: typeof console.debug;
  } = {
    log: console.log.bind(console),
    info: console.info.bind(console),
    warn: console.warn.bind(console),
    error: console.error.bind(console),
    debug: console.debug ? console.debug.bind(console) : console.log.bind(console)
  };
  private filters: ConsoleFilter = { excludePatterns: [] };
  private logBuffer: ConsolePayload[] = [];
  private maxBufferSize = 1000;
  private jtagSystemReady = false;
  private drainInterval?: NodeJS.Timeout;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('console-daemon', context, router);
  }

  /**
   * Initialize console daemon - attach to console and listen for system ready
   */
  protected async initialize(): Promise<void> {
    // Attach to console immediately (before JTAG system is ready)
    this.setupConsoleInterception();
    
    // Listen for JTAG system ready event
    this.listenForJTAGReady();
    
    // Use original console to avoid recursion
    this.originalConsole.log(`🎧 ${this.toString()}: Console daemon initialized`);
  }

  /**
   * Listen for JTAG system ready event to start draining queue
   */
  private listenForJTAGReady(): void {
    // Router is guaranteed by constructor - no need to check
    const eventSystem = this.router.eventSystem;
    
    // Listen for daemons loaded event - happens immediately after all daemon initialization
    eventSystem.on(SystemEvents.DAEMONS_LOADED, (data: any) => {
      if (!this.jtagSystemReady) {
        // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`🔌 ${this.toString()}: Daemons loaded event received, starting queue drain`);
        this.jtagSystemReady = true;
        this.startQueueDrain();
      }
    });
    
    // Also wait for JTAG system ready event as backup - TYPE-SAFE & MODULAR!
    eventSystem.waitFor(SystemEvents.READY, 10000)
      .then(() => {
        if (!this.jtagSystemReady) {
          // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`🚀 ${this.toString()}: JTAG system ready event received, starting queue drain`);
          this.jtagSystemReady = true;
          this.startQueueDrain();
        }
      })
      .catch((error: Error) => {
        this.originalConsole.warn(`⚠️ ${this.toString()}: JTAG ready timeout, falling back to transport check`);
        this.fallbackToTransportCheck();
      });

    // Also listen for transport ready events - TYPE-SAFE & MODULAR!
    eventSystem.on(TransportEvents.READY, (message: any) => {
      if (!this.jtagSystemReady) {
        // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`🔗 ${this.toString()}: Transport ready event received, starting queue drain`);
        this.jtagSystemReady = true;
        this.startQueueDrain();
      }
    });
  }

  /**
   * Fallback to transport checking (legacy approach)
   */
  private fallbackToTransportCheck(): void {
    const checkReady = () => {
      // Router is guaranteed by constructor
      const transport = (this.router as any).crossContextTransport;
      if (transport?.isConnected()) {
        // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`🔗 ${this.toString()}: Transport connected, starting queue drain`);
        this.jtagSystemReady = true;
        this.startQueueDrain();
        return;
      }
      
      // Check again in 1 second
      setTimeout(checkReady, 1000);
    };
    
    // Start checking after 2 seconds (allow system initialization)
    setTimeout(checkReady, 2000);
  }

  /**
   * Start draining the console queue to server (browser only)
   */
  private startQueueDrain(): void {
    if (this.context.environment === 'server') {
      // Server daemon is already in the right place, no draining needed
      // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`📍 ${this.toString()}: Server daemon - no queue draining needed`);
      return;
    }

    // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`🌊 ${this.toString()}: Starting queue drain - ${this.logBuffer.length} messages`);
    
    // Emit queue drain start event - TYPE-SAFE & MODULAR!
    // Router and eventSystem guaranteed by constructor
    this.router.eventSystem.emit(ConsoleEvents.QUEUE_DRAIN_START, {
      queueSize: this.logBuffer.length,
      environment: this.context.environment as 'browser' | 'server'
    });
    
    // Drain existing buffer immediately
    this.drainQueue();
    
    // Start periodic drain for new messages
    this.drainInterval = setInterval(() => {
      this.drainQueue();
    }, 500); // Drain every 500ms
  }

  /**
   * Drain queued console messages to server (browser → server transport)
   */
  private async drainQueue(): Promise<void> {
    if (this.logBuffer.length === 0 || this.context.environment === 'server') {
      return;
    }

    const messagesToDrain = [...this.logBuffer];
    this.logBuffer = []; // Clear buffer

    // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`🌊 ${this.toString()}: Draining ${messagesToDrain.length} messages to server`);

    for (const consolePayload of messagesToDrain) {
      try {
        // Router guaranteed by constructor
        const message = JTAGMessageFactory.createEvent(
          this.context,
          `${this.context.environment}/${this.subpath}`,
          `server/${this.subpath}`,
          consolePayload
        );
        await this.router.postMessage(message);
      } catch (error) {
        // If drain fails, put message back in buffer
        this.logBuffer.unshift(consolePayload);
        // SILENCED: Internal logging causes infinite loops - this.originalConsole.warn(`⚠️ ${this.toString()}: Failed to drain message, re-queued`);
        break; // Stop draining on first failure
      }
    }
  }


  /**
   * Handle incoming messages (from MessageSubscriber interface)
   */
  async handleMessage(message: JTAGMessage): Promise<any> {
    try {
      const consolePayload = message.payload as ConsolePayload;
      
      // Apply filters
      if (this.shouldFilterMessage(consolePayload)) {
        return { success: true, filtered: true };
      }

      // Add to buffer
      this.addToBuffer(consolePayload);
      
      // Process the console message (context-agnostic)
      await this.processConsolePayload(consolePayload);

      return { 
        success: true, 
        processed: true, 
        context: this.context.environment,
        level: consolePayload.level 
      };

    } catch (error: any) {
      this.originalConsole.error(`❌ ${this.toString()}: Error processing message:`, error.message);
      return { success: false, error: error.message };
    }
  }

  /**
   * Setup console interception using modern best practices
   * Clean pattern that avoids recursion and maintains readability
   */
  protected setupConsoleInterception(): void {
    // Store original methods before any override (critical for avoiding recursion)
    const originalLog = console.log.bind(console);
    const originalInfo = console.info.bind(console);
    const originalWarn = console.warn.bind(console);
    const originalError = console.error.bind(console);
    const originalDebug = console.debug ? console.debug.bind(console) : originalLog;

    // Update our stored references
    this.originalConsole = {
      log: originalLog,
      info: originalInfo,
      warn: originalWarn,
      error: originalError,
      debug: originalDebug
    };

    // Log initialization using stored original (no override active yet)
    originalLog(`🎧 ${this.toString()}: Console daemon initializing...`);

    // Create interception wrapper - clean and simple
    const createInterceptor = (level: 'log' | 'info' | 'warn' | 'error' | 'debug', originalMethod: Function) => {
      return (...args: any[]) => {
        // Always call original first to maintain expected console behavior
        originalMethod(...args);
        
        // Process through daemon (with recursion guard)
        if (!this.intercepting) {
          this.intercepting = true;
          try {
            this.processConsoleCall(level, args);
          } catch (error) {
            // Use original error method to avoid any recursion
            originalError('ConsoleDaemon processing failed:', error);
          } finally {
            this.intercepting = false;
          }
        }
      };
    };

    // Apply interceptors
    console.log = createInterceptor('log', originalLog);
    console.info = createInterceptor('info', originalInfo);
    console.warn = createInterceptor('warn', originalWarn);
    console.error = createInterceptor('error', originalError);  
    console.debug = createInterceptor('debug', originalDebug);

    // Confirm setup using stored original
    originalLog(`✅ ${this.toString()}: Console interception enabled`);
  }

  /**
   * Process raw console call arguments into ConsolePayload
   * Shared logic for parsing and creating payload
   */
  protected processConsoleCall(level: ConsolePayload['level'], args: any[]): void {
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');

    // Skip only internal daemon messages that cause infinite loops - be surgical!
    const skipPatterns = [
      // Console daemon self-reference (critical for preventing loops)
      'ConsoleDaemon', '🎧 ConsoleDaemon',
      
      // Message routing operations (critical for preventing loops)
      '📨 JTAGRouter', 'Routing message to server/console', 'Routing locally to server/console',
      '📥 JTAGMessageQueue', 'Queued message', 'Delivered queued message',
      '📤 WebSocket Client: Sending message to server',
      '📨 WebSocket Server: Received message from client',
      
      // Transport internal operations (prevent loops)
      'Transport Factory', 'Message handler connected',
      
      // Health/ping operations (prevent noise)
      'HealthManager', 'Ping successful', 'Connection established',
      
      // Event system internal operations (prevent loops)
      '📡 JTAGEventSystem: Emitting', 'JTAGEventSystem: Processing'
    ];
    
    if (skipPatterns.some(pattern => message.includes(pattern))) {
      //this.originalConsole.log(message);
      return;
    }

    const consolePayload = new ConsolePayload({
      level,
      component: this.extractComponent(message),
      message,
      timestamp: new Date().toISOString(),
      context: this.context.environment as 'browser' | 'server',
      stack: level === 'error' ? new Error().stack : undefined
    });

    // Only add to buffer if logBuffer exists (daemon fully initialized)
    if (this.logBuffer) {
      this.addToBuffer(consolePayload);
    }
    
    // Process the console message
    if (this.logBuffer) {
      this.processConsolePayload(consolePayload).catch(error => {
        this.originalConsole.error('ConsoleDaemon: Error processing console call:', error);
      });
    }
  }

  /**
   * Process console payload after creation
   * Environment-specific: browser stores locally + sends to server, server writes to files
   */
  protected abstract processConsolePayload(consolePayload: ConsolePayload): Promise<void>;


  protected extractComponent(message: string): string {
    // Try to extract component from message patterns
    const patterns = [
      /^\[([^\]]+)\]/,  // [ComponentName]
      /^(\w+):/,        // ComponentName:
      /^🎯\s*(\w+)/,    // 🎯 ComponentName
      /^📝\s*(\w+)/,    // 📝 ComponentName
    ];

    for (const pattern of patterns) {
      const match = message.match(pattern);
      if (match) {
        return match[1];
      }
    }

    return this.context.environment as 'browser' | 'server' === 'browser' ? 'BROWSER_CONSOLE' : 'SERVER_CONSOLE';
  }







  private shouldFilterMessage(consolePayload: ConsolePayload): boolean {
    // Check exclude patterns
    for (const pattern of this.filters.excludePatterns) {
      if (consolePayload.message.includes(pattern)) {
        return true;
      }
    }

    // Check include components (if specified)
    if (this.filters.includeComponents && 
        !this.filters.includeComponents.includes(consolePayload.component)) {
      return true;
    }

    return false;
  }

  private addToBuffer(consolePayload: ConsolePayload): void {
    this.logBuffer.push(consolePayload);
    
    // Maintain buffer size
    if (this.logBuffer.length > this.maxBufferSize) {
      this.logBuffer.shift();
    }
  }


  /**
   * Configure console filters
   */
  setFilters(filters: Partial<ConsoleFilter>): void {
    this.filters = { ...this.filters, ...filters };
    this.originalConsole.log(`🔧 ${this.toString()}: Filters updated`, this.filters);
  }

  /**
   * Get recent console messages from buffer
   */
  getRecentLogs(count: number = 50): ConsolePayload[] {
    return this.logBuffer.slice(-count);
  }

  /**
   * Clear the log buffer
   */
  clearBuffer(): void {
    this.logBuffer = [];
    this.originalConsole.log(`🧹 ${this.toString()}: Buffer cleared`);
  }

  /**
   * Restore original console (for cleanup)
   */
  restoreConsole(): void {
    if (typeof console !== 'undefined' && this.originalConsole.log) {
      console.log = this.originalConsole.log;
      console.info = this.originalConsole.info;
      console.warn = this.originalConsole.warn;
      console.error = this.originalConsole.error;
      console.debug = this.originalConsole.debug;
      
      this.originalConsole.log(`🔄 ${this.toString()}: Console restored`);
    }
  }
}