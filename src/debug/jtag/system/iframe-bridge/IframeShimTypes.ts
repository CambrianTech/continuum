/**
 * Iframe JTAG Shim - Shared Types
 *
 * Communication protocol between parent window and injected JTAG shim.
 * Enables remote control of proxied pages (screenshot, click, type, etc.)
 */

// ═══════════════════════════════════════════════════════════════════════════
// REQUEST/RESPONSE PROTOCOL
// ═══════════════════════════════════════════════════════════════════════════

export interface ShimRequest<T = unknown> {
  type: 'jtag-shim-request';
  command: ShimCommand;
  params: T;
  requestId: string;
}

export interface ShimResponse<T = unknown> {
  type: 'jtag-shim-response';
  requestId: string;
  result: ShimResult<T>;
}

export interface ShimResult<T = unknown> {
  success: boolean;
  data?: T;
  error?: { message: string; code?: string };
}

export interface ShimReadyEvent {
  type: 'jtag-shim-ready';
  version: string;
  url: string;
}

// ═══════════════════════════════════════════════════════════════════════════
// COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

export type ShimCommand =
  | 'ping'
  | 'screenshot'
  | 'click'
  | 'type'
  | 'scroll'
  | 'query'
  | 'queryAll'
  | 'getValue'
  | 'setValue'
  | 'focus'
  | 'blur'
  | 'hover'
  | 'waitFor'
  | 'pageInfo'
  | 'evaluate';

// ═══════════════════════════════════════════════════════════════════════════
// COMMAND PARAMS
// ═══════════════════════════════════════════════════════════════════════════

export interface PingParams {
  // No params needed
}

export interface ScreenshotParams {
  selector?: string;
  scale?: number;
  format?: 'png' | 'jpeg' | 'webp';
  quality?: number;
  backgroundColor?: string;
  viewportOnly?: boolean;  // Capture only visible area instead of full page
}

export interface ClickParams {
  selector: string;
  button?: 'left' | 'right' | 'middle';
  clickCount?: number;
  delay?: number;
}

export interface TypeParams {
  selector: string;
  text: string;
  delay?: number;       // Delay between keystrokes
  clear?: boolean;      // Clear existing value first
}

export interface ScrollParams {
  selector?: string;    // Scroll element into view
  x?: number;           // Or scroll to absolute position
  y?: number;
  behavior?: 'auto' | 'smooth';
}

export interface QueryParams {
  selector: string;
  attributes?: string[];  // Which attributes to return
  includeText?: boolean;
  includeHtml?: boolean;
  includeBounds?: boolean;
}

export interface QueryAllParams extends QueryParams {
  limit?: number;
}

export interface GetValueParams {
  selector: string;
}

export interface SetValueParams {
  selector: string;
  value: string;
}

export interface FocusParams {
  selector: string;
}

export interface HoverParams {
  selector: string;
}

export interface WaitForParams {
  selector: string;
  timeout?: number;
  visible?: boolean;
}

export interface PageInfoParams {
  // No params needed
}

export interface EvaluateParams {
  script: string;       // JavaScript to evaluate
  args?: unknown[];     // Arguments to pass
}

// ═══════════════════════════════════════════════════════════════════════════
// COMMAND RESULTS
// ═══════════════════════════════════════════════════════════════════════════

export interface PingResult {
  version: string;
  url: string;
}

export interface ScreenshotResult {
  dataUrl: string;
  metadata: {
    width: number;
    height: number;
    format: string;
    quality: number;
    selector: string;
    captureTime: number;
  };
}

export interface ClickResult {
  clicked: boolean;
  elementTag?: string;
}

export interface TypeResult {
  typed: boolean;
  finalValue?: string;
}

export interface ScrollResult {
  scrolled: boolean;
  scrollX: number;
  scrollY: number;
}

export interface QueryResult {
  found: boolean;
  tag?: string;
  id?: string;
  className?: string;
  attributes?: Record<string, string>;
  text?: string;
  html?: string;
  bounds?: DOMRect;
}

export interface QueryAllResult {
  count: number;
  elements: QueryResult[];
}

export interface GetValueResult {
  value: string;
  type: string;  // input type or tag
}

export interface SetValueResult {
  set: boolean;
  previousValue?: string;
}

export interface PageInfoResult {
  url: string;
  title: string;
  scrollX: number;
  scrollY: number;
  scrollWidth: number;
  scrollHeight: number;
  viewportWidth: number;
  viewportHeight: number;
}

export interface EvaluateResult {
  returnValue: unknown;
}

// ═══════════════════════════════════════════════════════════════════════════
// TYPE MAPPING (for type-safe command execution)
// ═══════════════════════════════════════════════════════════════════════════

export interface ShimCommandMap {
  ping: { params: PingParams; result: PingResult };
  screenshot: { params: ScreenshotParams; result: ScreenshotResult };
  click: { params: ClickParams; result: ClickResult };
  type: { params: TypeParams; result: TypeResult };
  scroll: { params: ScrollParams; result: ScrollResult };
  query: { params: QueryParams; result: QueryResult };
  queryAll: { params: QueryAllParams; result: QueryAllResult };
  getValue: { params: GetValueParams; result: GetValueResult };
  setValue: { params: SetValueParams; result: SetValueResult };
  focus: { params: FocusParams; result: void };
  blur: { params: FocusParams; result: void };
  hover: { params: HoverParams; result: void };
  waitFor: { params: WaitForParams; result: QueryResult };
  pageInfo: { params: PageInfoParams; result: PageInfoResult };
  evaluate: { params: EvaluateParams; result: EvaluateResult };
}
