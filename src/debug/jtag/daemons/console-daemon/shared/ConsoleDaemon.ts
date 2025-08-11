// ISSUES: 2 open, last updated 2025-07-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * 🔧 IMPROVEMENTS:
 * - [ ] Issue #1: ConsolePayload.data still uses unknown type (line 28)
 * - [ ] Issue #2: Many silenced logs suggest recursion complexity could be simplified
 */

/**
 * Console Daemon - Universal Console Management
 * 
 * Sophisticated console interception system providing universal logging across
 * browser/server contexts with intelligent buffering, filtering, and transport
 * coordination. Core component of the symmetric daemon architecture.
 * 
 * CORE ARCHITECTURE:
 * - Console interception with recursion protection
 * - Cross-context message queuing and draining
 * - JTAG system readiness coordination
 * - Configurable filtering and buffer management
 * - Event-driven queue processing
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Console interception and message formatting
 * - Integration tests: Cross-context log transport reliability
 * - Performance tests: High-frequency logging scenarios
 * - Recursion tests: Prevention of infinite logging loops
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Extends DaemonBase for consistent lifecycle management
 * - Original console methods preserved to prevent recursion
 * - Queue drain waits for JTAG system readiness
 * - Filter patterns prevent internal system log loops
 * - Event correlation enables distributed debugging
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import { type JTAGPayload, JTAGMessageFactory, createPayload } from '../../../system/core/types/JTAGTypes';
import { type UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { SYSTEM_EVENTS } from '../../../system/events';
import { TRANSPORT_EVENTS } from '@system/transports/shared/TransportEvents';
import { SYSTEM_SCOPES, globalSessionContext } from '../../../system/core/types/SystemScopes';
import { CONSOLE_EVENTS } from '../ConsoleEvents';
import { JTAG_ENDPOINTS } from '../../../system/core/router/shared/JTAGEndpoints';
import { type ConsoleResponse, createConsoleSuccessResponse, createConsoleErrorResponse } from '../../../system/core/types/ResponseTypes';
import type { TimerHandle } from '../../../system/core/types/CrossPlatformTypes';
import type { LogLevel } from './LogLevels';


// Console-specific payload
export interface ConsolePayload extends JTAGPayload {
  level: LogLevel;
  component: string;
  message: string;
  timestamp: string;
  data?: unknown; // Keep optional but use unknown instead of any
  stack?: string;
}

export const createConsolePayload = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    level?: LogLevel;
    component?: string;
    message?: string;
    timestamp?: string;
    data?: unknown;
    stack?: string;
  }
): ConsolePayload => createPayload(context, sessionId, {
  level: data.level ?? 'log',
  component: data.component ?? 'UNKNOWN',
  message: data.message ?? '',
  timestamp: data.timestamp ?? new Date().toISOString(),
  data: data.data,
  stack: data.stack,
  ...data
});

export interface ConsoleFilter {
  excludePatterns: string[];
  includeComponents?: string[];
  minLevel?: LogLevel;
}

/**
 * Universal Console Handler - Symmetric daemon following router pattern
 */
export abstract class ConsoleDaemon extends DaemonBase {
  public readonly subpath: string = 'console';
  protected intercepting = false;
  private temporarySessionId: UUID | null = null;
  private sessionIdProvider?: () => UUID;
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
  private drainInterval?: TimerHandle;

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
    const eventSystem = this.router.eventManager.events;
    
    // Listen for daemons loaded event - happens immediately after all daemon initialization
    eventSystem.on(SYSTEM_EVENTS.DAEMONS_LOADED, () => {
      if (!this.jtagSystemReady) {
        // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`🔌 ${this.toString()}: Daemons loaded event received, starting queue drain`);
        this.jtagSystemReady = true;
        this.startQueueDrain();
      }
    });
    
    // Also wait for JTAG system ready event as backup - TYPE-SAFE & MODULAR!
    eventSystem.waitFor?.(SYSTEM_EVENTS.READY, 10000)
      ?.then(() => {
        if (!this.jtagSystemReady) {
          // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`🚀 ${this.toString()}: JTAG system ready event received, starting queue drain`);
          this.jtagSystemReady = true;
          this.startQueueDrain();
        }
      })
      .catch(() => {
        this.originalConsole.warn(`⚠️ ${this.toString()}: JTAG ready timeout`);
      });

    // Also listen for transport ready events - TYPE-SAFE & MODULAR!
    eventSystem.on(TRANSPORT_EVENTS.READY, () => {
      if (!this.jtagSystemReady) {
        // SILENCED: Internal logging causes infinite loops - this.originalConsole.log(`🔗 ${this.toString()}: Transport ready event received, starting queue drain`);
        this.jtagSystemReady = true;
        this.startQueueDrain();
      }
    });
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
    this.router.eventManager.events.emit(CONSOLE_EVENTS.QUEUE_DRAIN_START, {
      queueSize: this.logBuffer.length,
      environment: this.context.environment
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
        // Router guaranteed by constructor - use type-safe endpoints
        const message = JTAGMessageFactory.createEvent(
          this.context,
          this.context.environment === 'browser' ? JTAG_ENDPOINTS.CONSOLE.BROWSER : JTAG_ENDPOINTS.CONSOLE.SERVER,
          JTAG_ENDPOINTS.CONSOLE.SERVER,
          consolePayload
        );
        await this.router.postMessage(message);
      } catch  {
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
  async handleMessage(message: JTAGMessage): Promise<ConsoleResponse> {
    try {
      const consolePayload = message.payload as ConsolePayload;
      
      // Apply filters
      if (this.shouldFilterMessage(consolePayload)) {
        return createConsoleSuccessResponse({ filtered: true }, this.context, message.payload.sessionId);
      }

      // Add to buffer
      this.addToBuffer(consolePayload);
      
      // Process the console message (context-agnostic)
      await this.processConsolePayload(consolePayload);

      return createConsoleSuccessResponse({ 
        processed: true, 
        level: consolePayload.level 
      }, this.context, message.payload.sessionId);

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.originalConsole.error(`❌ ${this.toString()}: Error processing message:`, errorMessage);
      return createConsoleErrorResponse(errorMessage, this.context, message.payload.sessionId);
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

    // Update our stored references using Levels type for keys
    this.originalConsole = {
      log: originalLog,
      info: originalInfo,
      warn: originalWarn,
      error: originalError,
      debug: originalDebug
    } as Record<LogLevel, typeof originalLog>;

    // Log initialization using stored original (no override active yet)
    originalLog(`🎧 ${this.toString()}: Console daemon initializing...`);

    // Create interception wrapper - clean and simple
    
    const createInterceptor = (
      level: LogLevel,
      originalMethod: (...args: unknown[]) => void
    ): (...args: unknown[]) => void => {
      return (...args: unknown[]): void => {
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
  protected processConsoleCall(level: LogLevel, args: unknown[]): void {
    const message = args.map(arg => 
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');

    // Skip only internal daemon messages that cause infinite loops - be surgical!
    const skipPatterns = [
      // Console daemon self-reference (critical for preventing loops)
      'ConsoleDaemon', '🎧 ConsoleDaemon',
      
      // Message routing operations (critical for preventing loops) - COMPREHENSIVE
      'JTAGRouter[', 'JTAGMessageQueue[', 'Routing message', 'Routing locally',
      'Queued message', 'Delivered queued message', 'Using hierarchical routing',
      'Sending response', 'Response sent', 'Skipping flush - connection unhealthy',
      
      // WebSocket operations (prevent loops) - EXPANDED
      '📤 WebSocket Server', '📨 WebSocket Server', '📤 WebSocket Client',
      'Broadcasting message to', 'clients', 'Message sent to', 'Received message from client',
      
      // CommandDaemon operations (prevent noise)
      '📨 CommandDaemonServer', 'Handling message to server/commands',
      
      // Transport internal operations (prevent loops) - COMPREHENSIVE
      'Transport Factory', 'Message handler connected',
      'websocket-client:', 'websocket-server:', 'Sending message to server',
      'Received message from client', 'WebSocket transport', 'HTTP transport',
      
      // Health/ping operations (prevent noise)
      'HealthManager', 'Ping successful', 'Connection established',
      
      // Event system internal operations (prevent loops)
      '📡 JTAGEventSystem: Emitting', 'JTAGEventSystem: Processing'
    ];
    
    if (skipPatterns.some(pattern => message.includes(pattern))) {
      //this.originalConsole.log(message);
      return;
    }

    // Use session context: global session > temporary session > provider session
    const globalSessionId = globalSessionContext.getCurrentSessionId();
    const effectiveSessionId = globalSessionId ?? this.temporarySessionId ?? this.getCurrentSessionId();
    
    const consolePayload = createConsolePayload(this.context, effectiveSessionId, {
      level,
      component: this.extractComponent(message),
      message,
      timestamp: new Date().toISOString(),
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

    return this.context.environment === 'browser' ? 'BROWSER_CONSOLE' : 'SERVER_CONSOLE';
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
   * Set the session ID provider (typically from client)
   */
  setSessionIdProvider(provider: () => UUID): void {
    this.sessionIdProvider = provider;
    this.originalConsole.log(`🏷️ ${this.toString()}: Session provider updated`);
  }

  /**
   * Temporarily set session ID for console logging during specific operations
   * Used by server to log session-specific operations to correct directories
   */
  setTemporarySessionId(sessionId: UUID | null): void {
    this.temporarySessionId = sessionId;
  }

  /**
   * Execute function with temporary session context for dual logging
   */
  async withSessionContext<T>(sessionId: UUID, fn: () => Promise<T>): Promise<T> {
    const previousSessionId = this.temporarySessionId;
    this.temporarySessionId = sessionId;
    try {
      return await fn();
    } finally {
      this.temporarySessionId = previousSessionId;
    }
  }

  /**
   * Get the current session ID
   */
  getCurrentSessionId(): UUID {
    // Use provider if available, otherwise fall back to system session
    return this.sessionIdProvider?.() ?? '00000000-0000-0000-0000-000000000000' as UUID;
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