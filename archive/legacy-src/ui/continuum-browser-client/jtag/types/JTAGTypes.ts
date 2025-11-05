/**
 * JTAG System Types - Strongly Typed Debugging Interfaces
 * 
 * These types can be shared across client and server for JTAG debugging operations.
 */

// Base probe interfaces
export interface JTAGProbeResult<T = any> {
  readonly success: boolean;
  readonly data: T;
  readonly timestamp: number;
  readonly category: string;
  readonly error?: string;
}

export interface JTAGProbeOptions {
  readonly category?: string;
  readonly tags?: readonly string[];
  readonly throttle?: number;
  readonly autoLog?: boolean;
}

// Widget analysis types
export interface WidgetAnalysis {
  readonly name: string;
  readonly exists: boolean;
  readonly hasShadowRoot: boolean;
  readonly shadowContent: string;
  readonly isRendered: boolean;
  readonly hasStyles: boolean;
  readonly styleCount: number;
  readonly renderTime?: number;
  readonly errors?: readonly string[];
}

export interface ShadowDOMAnalysis {
  readonly selector: string;
  readonly hasShadowRoot: boolean;
  readonly shadowHTML: string | null;
  readonly shadowLength: number;
  readonly childCount: number;
  readonly styles: readonly CSSStyleAnalysis[];
  readonly hasContent: boolean;
}

export interface CSSStyleAnalysis {
  readonly length: number;
  readonly rules: number;
  readonly source?: 'inline' | 'external' | 'injected';
}

// Custom elements analysis
export interface CustomElementsAnalysis {
  readonly supported: boolean;
  readonly sidebarRegistered: 'function' | 'undefined';
  readonly chatRegistered: 'function' | 'undefined';
  readonly registrySize: number;
  readonly definedElements: readonly CustomElementDefinition[];
}

export interface CustomElementDefinition {
  readonly name: string;
  readonly defined: boolean;
  readonly instances: number;
  readonly constructor?: string;
}

// Styling analysis
export interface StyleAnalysis {
  readonly globalStylesheets: number;
  readonly widgetStyles: readonly WidgetStyleAnalysis[];
  readonly totalCSSSize: number;
}

export interface WidgetStyleAnalysis {
  readonly widget: string;
  readonly styleElements: number;
  readonly totalCSS: number;
  readonly computedDisplay: string;
  readonly computedVisibility: string;
  readonly hasBaseCSS: boolean;
  readonly hasWidgetCSS: boolean;
}

// Performance analysis
export interface PerformanceAnalysis {
  readonly memory: MemoryAnalysis | null;
  readonly timing: TimingAnalysis;
  readonly resources: ResourceAnalysis;
  readonly widgets: WidgetPerformanceAnalysis;
}

export interface MemoryAnalysis {
  readonly used: number; // MB
  readonly total: number; // MB
  readonly limit: number; // MB
  readonly percentage: number;
}

export interface TimingAnalysis {
  readonly loadComplete: number;
  readonly domElements: number;
  readonly widgets: number;
  readonly renderTime?: number;
}

export interface ResourceAnalysis {
  readonly scripts: number;
  readonly stylesheets: number;
  readonly images: number;
  readonly fetchRequests?: number;
}

export interface WidgetPerformanceAnalysis {
  readonly totalWidgets: number;
  readonly renderedWidgets: number;
  readonly averageRenderTime?: number;
  readonly memoryPerWidget?: number;
}

// Network and API analysis
export interface NetworkAnalysis {
  readonly online: boolean;
  readonly continuum: ContinuumAPIAnalysis;
  readonly websocket: WebSocketAnalysis;
  readonly latency?: number;
}

export interface ContinuumAPIAnalysis {
  readonly available: boolean;
  readonly state?: string;
  readonly sessionId?: string;
  readonly clientId?: string;
  readonly methods: readonly string[];
  readonly version?: string;
}

export interface WebSocketAnalysis {
  readonly supported: boolean;
  readonly connected?: boolean;
  readonly url?: string;
  readonly readyState?: number;
  readonly protocol?: string;
}

// Health check analysis
export interface HealthAnalysis {
  readonly overall: 'healthy' | 'issues-detected' | 'critical';
  readonly issues: readonly string[];
  readonly warnings: readonly string[];
  readonly summary: HealthSummary;
  readonly score: number; // 0-100
}

export interface HealthSummary {
  readonly widgets: number;
  readonly rendered: number;
  readonly customElementsWorking: boolean;
  readonly memoryUsage: number | 'unknown';
  readonly cssLoaded: boolean;
  readonly apiConnected: boolean;
}

// Custom execution
export interface ExecutionResult {
  readonly result?: any;
  readonly error?: string;
  readonly code: string;
  readonly executionTime: number;
  readonly memoryBefore?: number;
  readonly memoryAfter?: number;
}

// Observer system types
export interface JTAGObservation {
  readonly id: string;
  readonly timestamp: number;
  readonly category: string;
  readonly trigger: string;
  readonly data: any;
  readonly severity: 'info' | 'warn' | 'error' | 'critical';
  readonly sessionId?: string;
}

export interface JTAGTrigger {
  readonly name: string;
  readonly selector?: string;
  readonly event?: string;
  readonly condition: () => boolean;
  readonly action: () => JTAGObservation | null;
  readonly throttle?: number;
  readonly enabled?: boolean;
}

export interface WidgetState {
  readonly name: string;
  readonly element: HTMLElement;
  readonly hasShadowRoot: boolean;
  readonly shadowContent: string;
  readonly isRendered: boolean;
  readonly lastRenderTime?: number;
  readonly errors: readonly string[];
  readonly lifecycle: WidgetLifecycleState;
}

export interface WidgetLifecycleState {
  readonly constructed: boolean;
  readonly connected: boolean;
  readonly rendered: boolean;
  readonly styled: boolean;
  readonly interactive: boolean;
}

// Batch operations
export interface JTAGBatchResult {
  readonly [method: string]: JTAGProbeResult;
}

export type JTAGProbeMethod = 
  | 'widgets'
  | 'shadowDOM'
  | 'customElements'
  | 'styles'
  | 'performance'
  | 'network'
  | 'health'
  | 'execute';

// Watch operations
export interface JTAGWatchOptions extends JTAGProbeOptions {
  readonly interval?: number;
  readonly maxDuration?: number;
  readonly onChange?: (result: JTAGProbeResult) => void;
}

export type JTAGWatchStopFunction = () => void;

// Error types
export interface JTAGError {
  readonly type: 'probe-error' | 'observer-error' | 'trigger-error' | 'execution-error';
  readonly message: string;
  readonly stack?: string;
  readonly context?: Record<string, any>;
  readonly timestamp: number;
}

// Configuration types
export interface JTAGConfig {
  readonly autoStart?: boolean;
  readonly defaultThrottle?: number;
  readonly maxObservations?: number;
  readonly enablePerformanceMonitoring?: boolean;
  readonly logLevel?: 'debug' | 'info' | 'warn' | 'error';
}

// Utility types for type safety
export type JTAGProbeFunction<T> = (options?: JTAGProbeOptions) => JTAGProbeResult<T>;

// Export type maps for runtime type checking
export const JTAG_PROBE_METHODS: readonly JTAGProbeMethod[] = [
  'widgets',
  'shadowDOM', 
  'customElements',
  'styles',
  'performance',
  'network',
  'health',
  'execute'
] as const;

export const JTAG_SEVERITY_LEVELS = ['info', 'warn', 'error', 'critical'] as const;
export const JTAG_HEALTH_STATES = ['healthy', 'issues-detected', 'critical'] as const;