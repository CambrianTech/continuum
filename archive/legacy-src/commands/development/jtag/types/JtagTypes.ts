/**
 * JTAG Shared Types - Strongly typed interfaces for AI debugging
 * ============================================================
 * Shared across browser, server, and CLI for type safety
 */

export interface JtagProbeRequest {
  readonly type: 'widgets' | 'network' | 'health' | 'probe' | 'screenshot';
  readonly params?: Record<string, unknown>;
  readonly timeout?: number;
  readonly sessionId?: string;
}

export interface JtagProbeResponse<T = unknown> {
  readonly success: boolean;
  readonly data?: T;
  readonly error?: string;
  readonly timestamp: number;
  readonly executionTime: number;
}

export interface WidgetAnalysis {
  readonly tagName: string;
  readonly hasShadowRoot: boolean;
  readonly shadowContentLength: number;
  readonly visible: boolean;
  readonly hasContent: boolean;
  readonly boundingRect: {
    readonly width: number;
    readonly height: number;
    readonly x: number;
    readonly y: number;
  };
  readonly classList: readonly string[];
}

export interface WidgetSummary {
  readonly total: number;
  readonly working: number;
  readonly empty: number;
  readonly visible: number;
  readonly broken: number;
}

export interface NetworkHealth {
  readonly connection: {
    readonly effectiveType: string;
    readonly downlink: number;
    readonly rtt: number;
  } | null;
  readonly websocket: {
    readonly connected: boolean;
    readonly sessionId: string | null;
    readonly readyState: number;
    readonly bufferedAmount: number;
  } | null;
  readonly performance: {
    readonly memory: {
      readonly used: number;
      readonly total: number;
    } | null;
    readonly resources: number;
    readonly navigationTime: number;
  };
}

export interface SystemHealth {
  readonly widgets: number;
  readonly customElements: number;
  readonly errors: number;
  readonly connected: boolean;
  readonly sessionId: string;
  readonly memory: number;
  readonly timestamp: string;
  readonly score: number;
}

export interface JtagCommandOptions {
  readonly screenshot?: boolean;
  readonly follow?: boolean;
  readonly level?: 'error' | 'warn' | 'info' | 'log' | 'debug';
  readonly selector?: string;
  readonly timeout?: number;
}

export interface JtagModuleInterface {
  analyze(): Promise<JtagProbeResponse>;
  getHealthScore(): Promise<number>;
  cleanup(): Promise<void>;
}

// CLI integration types
export interface JtagCLICommand {
  readonly name: string;
  readonly description: string;
  readonly options: readonly string[];
  execute(args: readonly string[], options: JtagCommandOptions): Promise<void>;
}

// Cross-module communication (JTAG can call other modules)
export interface ContinuumCommandExecutor {
  execute<T = unknown>(command: string, params?: Record<string, unknown>): Promise<JtagProbeResponse<T>>;
  executeTypescript<T = unknown>(code: string): Promise<JtagProbeResponse<T>>;
  screenshot(options?: { selector?: string }): Promise<JtagProbeResponse>;
}