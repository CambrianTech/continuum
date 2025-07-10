/**
 * JTAG Shared Types - Cross-Platform Debugging Interfaces
 * 
 * Strongly typed interfaces for JTAG operations that work across:
 * - Browser client
 * - Server daemons  
 * - CLI commands
 * - WebSocket communications
 * - API endpoints
 */

// Base interfaces for cross-platform compatibility
export interface JTAGProbeRequest {
  readonly method: JTAGProbeMethod;
  readonly options?: JTAGProbeOptions;
  readonly sessionId?: string;
  readonly clientId?: string;
  readonly timestamp: number;
}

export interface JTAGProbeResponse<T = any> {
  readonly success: boolean;
  readonly data: T;
  readonly timestamp: number;
  readonly category: string;
  readonly executionTime: number;
  readonly error?: JTAGError;
  readonly metadata?: JTAGProbeMetadata;
}

export interface JTAGProbeMetadata {
  readonly platform: 'browser' | 'server' | 'cli';
  readonly sessionId?: string;
  readonly clientId?: string;
  readonly userAgent?: string;
  readonly version?: string;
}

export interface JTAGProbeOptions {
  readonly category?: string;
  readonly tags?: readonly string[];
  readonly throttle?: number;
  readonly autoLog?: boolean;
  readonly selector?: string; // For DOM-specific probes
  readonly depth?: number; // For nested analysis
  readonly includeMetrics?: boolean;
}

// Probe method types (shared across platforms)
export type JTAGProbeMethod = 
  | 'widgets'
  | 'shadowDOM'
  | 'customElements'
  | 'styles'
  | 'performance'
  | 'network'
  | 'health'
  | 'execute'
  | 'system'
  | 'session'
  | 'logs';

// Widget analysis (cross-platform serializable)
export interface WidgetAnalysisData {
  readonly widgets: readonly WidgetState[];
  readonly summary: WidgetSummary;
  readonly issues: readonly WidgetIssue[];
}

export interface WidgetState {
  readonly name: string;
  readonly tagName: string;
  readonly exists: boolean;
  readonly hasShadowRoot: boolean;
  readonly shadowContentLength: number;
  readonly shadowContentPreview: string; // First 200 chars
  readonly isRendered: boolean;
  readonly hasStyles: boolean;
  readonly styleCount: number;
  readonly lifecycle: WidgetLifecycleState;
  readonly performance: WidgetPerformanceData;
  readonly errors: readonly string[];
}

export interface WidgetLifecycleState {
  readonly constructed: boolean;
  readonly connected: boolean;
  readonly rendered: boolean;
  readonly styled: boolean;
  readonly interactive: boolean;
  readonly timestamp: number;
}

export interface WidgetPerformanceData {
  readonly renderTime?: number | undefined;
  readonly memoryUsage?: number | undefined;
  readonly cssLoadTime?: number | undefined;
  readonly domComplexity: number;
}

export interface WidgetSummary {
  readonly total: number;
  readonly rendered: number;
  readonly broken: number;
  readonly empty: number;
  readonly performance: 'good' | 'fair' | 'poor';
}

export interface WidgetIssue {
  readonly widget: string;
  readonly type: 'empty-shadow' | 'missing-styles' | 'render-error' | 'performance';
  readonly severity: 'info' | 'warn' | 'error' | 'critical';
  readonly message: string;
  readonly suggestion?: string;
}

// Shadow DOM analysis (cross-platform)
export interface ShadowDOMAnalysisData {
  readonly elements: readonly ShadowDOMElement[];
  readonly summary: ShadowDOMSummary;
}

export interface ShadowDOMElement {
  readonly selector: string;
  readonly tagName: string;
  readonly hasShadowRoot: boolean;
  readonly shadowHTML: string | null;
  readonly shadowLength: number;
  readonly childCount: number;
  readonly hasContent: boolean;
  readonly styles: readonly ShadowDOMStyle[];
}

export interface ShadowDOMStyle {
  readonly source: 'inline' | 'external' | 'injected';
  readonly length: number;
  readonly rules: number;
  readonly isBaseCSS: boolean;
}

export interface ShadowDOMSummary {
  readonly totalElements: number;
  readonly withShadowRoot: number;
  readonly withContent: number;
  readonly totalStyles: number;
}

// Custom elements analysis
export interface CustomElementsAnalysisData {
  readonly supported: boolean;
  readonly registry: readonly CustomElementInfo[];
  readonly instances: readonly CustomElementInstance[];
  readonly summary: CustomElementsSummary;
}

export interface CustomElementInfo {
  readonly name: string;
  readonly defined: boolean;
  readonly constructor: string;
  readonly observedAttributes?: readonly string[];
}

export interface CustomElementInstance {
  readonly name: string;
  readonly count: number;
  readonly rendered: number;
  readonly broken: number;
}

export interface CustomElementsSummary {
  readonly totalDefinitions: number;
  readonly totalInstances: number;
  readonly workingInstances: number;
  readonly registrationHealth: 'good' | 'partial' | 'broken';
}

// Performance analysis (cross-platform)
export interface PerformanceAnalysisData {
  readonly memory: MemoryMetrics | null;
  readonly timing: TimingMetrics;
  readonly resources: ResourceMetrics;
  readonly widgets: WidgetPerformanceMetrics;
  readonly overall: PerformanceScore;
}

export interface MemoryMetrics {
  readonly used: number; // MB
  readonly total: number; // MB  
  readonly limit: number; // MB
  readonly percentage: number;
  readonly trend?: 'stable' | 'increasing' | 'decreasing';
}

export interface TimingMetrics {
  readonly loadComplete: number;
  readonly firstPaint?: number;
  readonly firstContentfulPaint?: number;
  readonly domComplete: number;
  readonly renderTime: number;
}

export interface ResourceMetrics {
  readonly scripts: number;
  readonly stylesheets: number;
  readonly images: number;
  readonly totalSize: number; // bytes
  readonly loadTime: number;
}

export interface WidgetPerformanceMetrics {
  readonly totalWidgets: number;
  readonly renderedWidgets: number;
  readonly averageRenderTime: number;
  readonly slowestWidget?: string;
  readonly memoryPerWidget: number;
}

export interface PerformanceScore {
  readonly score: number; // 0-100
  readonly grade: 'A' | 'B' | 'C' | 'D' | 'F';
  readonly issues: readonly string[];
  readonly recommendations: readonly string[];
}

// Network and API analysis
export interface NetworkAnalysisData {
  readonly online: boolean;
  readonly continuum: ContinuumAPIStatus;
  readonly websocket: WebSocketStatus;
  readonly latency: NetworkLatency;
  readonly connectivity: ConnectivityHealth;
}

export interface ContinuumAPIStatus {
  readonly available: boolean;
  readonly state: string;
  readonly sessionId?: string | undefined;
  readonly clientId?: string | undefined;
  readonly methods: readonly string[];
  readonly version?: string | undefined;
  readonly lastHeartbeat?: number | undefined;
}

export interface WebSocketStatus {
  readonly supported: boolean;
  readonly connected: boolean;
  readonly url?: string;
  readonly readyState: number;
  readonly protocol?: string;
  readonly messagesReceived: number;
  readonly messagesSent: number;
}

export interface NetworkLatency {
  readonly ping?: number | undefined;
  readonly apiResponseTime?: number | undefined;
  readonly websocketLatency?: number | undefined;
}

export interface ConnectivityHealth {
  readonly status: 'excellent' | 'good' | 'fair' | 'poor' | 'offline';
  readonly issues: readonly string[];
}

// Health check (comprehensive cross-platform)
export interface HealthAnalysisData {
  readonly overall: HealthStatus;
  readonly components: readonly ComponentHealth[];
  readonly issues: readonly HealthIssue[];
  readonly warnings: readonly HealthWarning[];
  readonly summary: HealthSummary;
  readonly score: number; // 0-100
  readonly recommendations: readonly string[];
}

export type HealthStatus = 'healthy' | 'issues-detected' | 'critical' | 'unknown';

export interface ComponentHealth {
  readonly name: string;
  readonly status: HealthStatus;
  readonly message?: string;
  readonly metrics?: Record<string, number>;
}

export interface HealthIssue {
  readonly component: string;
  readonly type: string;
  readonly severity: 'info' | 'warn' | 'error' | 'critical';
  readonly message: string;
  readonly suggestion?: string | undefined;
  readonly fixable: boolean;
}

export interface HealthWarning {
  readonly component: string;
  readonly message: string;
  readonly impact: 'low' | 'medium' | 'high';
}

export interface HealthSummary {
  readonly widgets: ComponentHealthSummary;
  readonly performance: ComponentHealthSummary;
  readonly network: ComponentHealthSummary;
  readonly memory: ComponentHealthSummary;
}

export interface ComponentHealthSummary {
  readonly status: HealthStatus;
  readonly score: number;
  readonly details?: string;
}

// Custom execution (cross-platform)
export interface ExecutionRequest {
  readonly code: string;
  readonly context?: 'browser' | 'server' | 'both';
  readonly timeout?: number;
  readonly returnType?: 'json' | 'string' | 'auto';
}

export interface ExecutionResult {
  readonly success: boolean;
  readonly result?: any;
  readonly error?: string | undefined;
  readonly code: string;
  readonly executionTime: number;
  readonly memoryBefore?: number | undefined;
  readonly memoryAfter?: number | undefined;
  readonly context: 'browser' | 'server' | 'cli';
}

// Observer system (for real-time monitoring)
export interface JTAGObservation {
  readonly id: string;
  readonly timestamp: number;
  readonly category: string;
  readonly trigger: string;
  readonly data: any;
  readonly severity: 'info' | 'warn' | 'error' | 'critical';
  readonly sessionId?: string;
  readonly platform: 'browser' | 'server' | 'cli';
}

export interface JTAGTriggerConfig {
  readonly name: string;
  readonly enabled: boolean;
  readonly selector?: string;
  readonly event?: string;
  readonly throttle?: number;
  readonly condition: string; // Serializable condition as string
  readonly action: string; // Serializable action as string
}

// Batch operations
export interface JTAGBatchRequest {
  readonly methods: readonly JTAGProbeMethod[];
  readonly options?: JTAGProbeOptions;
  readonly parallel?: boolean;
}

export interface JTAGBatchResponse {
  readonly results: Record<JTAGProbeMethod, JTAGProbeResponse>;
  readonly executionTime: number;
  readonly errors: readonly JTAGError[];
}

// Error handling (cross-platform)
export interface JTAGError {
  readonly type: 'probe-error' | 'observer-error' | 'trigger-error' | 'execution-error' | 'network-error';
  readonly message: string;
  readonly stack?: string | undefined;
  readonly context?: Record<string, any>;
  readonly timestamp: number;
  readonly platform: 'browser' | 'server' | 'cli';
}

// Configuration (cross-platform)
export interface JTAGConfig {
  readonly platform: 'browser' | 'server' | 'cli';
  readonly autoStart?: boolean;
  readonly defaultThrottle?: number;
  readonly maxObservations?: number;
  readonly enablePerformanceMonitoring?: boolean;
  readonly logLevel?: 'debug' | 'info' | 'warn' | 'error';
  readonly enabledProbes?: readonly JTAGProbeMethod[];
  readonly enabledTriggers?: readonly string[];
}

// API endpoints (for server integration)
export interface JTAGAPIEndpoint {
  readonly path: string;
  readonly method: 'GET' | 'POST' | 'PUT' | 'DELETE';
  readonly probeMethod: JTAGProbeMethod;
  readonly description: string;
}

// Constants for validation
export const JTAG_PROBE_METHODS: readonly JTAGProbeMethod[] = [
  'widgets',
  'shadowDOM',
  'customElements', 
  'styles',
  'performance',
  'network',
  'health',
  'execute',
  'system',
  'session',
  'logs'
] as const;

export const JTAG_SEVERITY_LEVELS = ['info', 'warn', 'error', 'critical'] as const;
export const JTAG_HEALTH_STATES: readonly HealthStatus[] = ['healthy', 'issues-detected', 'critical', 'unknown'] as const;
export const JTAG_PLATFORMS = ['browser', 'server', 'cli'] as const;

// Type guards for runtime validation
export function isJTAGProbeMethod(value: string): value is JTAGProbeMethod {
  return JTAG_PROBE_METHODS.includes(value as JTAGProbeMethod);
}

export function isHealthStatus(value: string): value is HealthStatus {
  return JTAG_HEALTH_STATES.includes(value as HealthStatus);
}

export function isJTAGPlatform(value: string): value is 'browser' | 'server' | 'cli' {
  return JTAG_PLATFORMS.includes(value as any);
}