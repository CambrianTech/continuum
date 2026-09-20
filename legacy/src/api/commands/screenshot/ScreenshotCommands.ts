/**
 * Screenshot Commands API - Public Interface Types
 * 
 * Consumer-first API for screenshot and screen capture operations.
 */

// Screenshot capture parameters
export interface ScreenshotParams {
  // Element targeting
  querySelector?: string;
  element?: Element; // For browser-side direct element reference
  
  // Capture area
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  
  // Output options
  filename?: string;
  format?: 'png' | 'jpeg' | 'webp';
  quality?: number; // 0-100 for lossy formats
  
  // Capture behavior
  fullPage?: boolean;
  clipToViewport?: boolean;
  deviceScaleFactor?: number;
  
  // Wait conditions
  delay?: number; // ms to wait before capture
  waitForSelector?: string;
  waitForFunction?: string; // JavaScript condition
  
  // Processing options
  crop?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  resize?: {
    width: number;
    height: number;
    fit?: 'cover' | 'contain' | 'fill' | 'inside' | 'outside';
  };
  
  // Metadata
  metadata?: Record<string, any>;
}

export interface ScreenshotCoordinates {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ScreenshotMetadata {
  filename: string;
  format: string;
  fileSize: number;
  dimensions: {
    width: number;
    height: number;
  };
  devicePixelRatio: number;
  viewport: {
    width: number;
    height: number;
  };
  timestamp: string;
  captureArea: ScreenshotCoordinates;
  selector?: string;
  url?: string;
}

export interface ScreenshotResult {
  success: boolean;
  metadata?: ScreenshotMetadata;
  filepath?: string;
  base64?: string; // If requested
  error?: string;
}

// Screen recording parameters (future)
export interface ScreenRecordingParams {
  querySelector?: string;
  duration?: number; // seconds
  fps?: number;
  format?: 'webm' | 'mp4' | 'gif';
  filename?: string;
  quality?: 'low' | 'medium' | 'high';
}

export interface ScreenRecordingResult {
  success: boolean;
  filepath?: string;
  duration?: number;
  fileSize?: number;
  error?: string;
}

// Element information retrieval
export interface GetElementInfoParams {
  querySelector: string;
  includeStyles?: boolean;
  includeComputedStyles?: boolean;
  includeChildren?: boolean;
}

export interface ElementInfo {
  selector: string;
  tagName: string;
  id?: string;
  className?: string;
  textContent?: string;
  attributes: Record<string, string>;
  boundingRect: ScreenshotCoordinates;
  styles?: Record<string, string>;
  computedStyles?: Record<string, string>;
  children?: ElementInfo[];
  isVisible: boolean;
  isInViewport: boolean;
}

export interface GetElementInfoResult {
  success: boolean;
  element?: ElementInfo;
  elements?: ElementInfo[]; // If selector matches multiple
  error?: string;
}

// Page information and screenshot preparation
export interface GetPageInfoParams {
  includeViewport?: boolean;
  includeScrollPosition?: boolean;
  includeElements?: string[]; // selectors to analyze
}

export interface PageInfo {
  url: string;
  title: string;
  viewport: {
    width: number;
    height: number;
  };
  document: {
    width: number;
    height: number;
  };
  scrollPosition: {
    x: number;
    y: number;
  };
  devicePixelRatio: number;
  elements?: Record<string, ElementInfo>;
  readyState: 'loading' | 'interactive' | 'complete';
}

export interface GetPageInfoResult {
  success: boolean;
  pageInfo?: PageInfo;
  error?: string;
}

// Batch screenshot operations
export interface BatchScreenshotParams {
  screenshots: ScreenshotParams[];
  outputDirectory?: string;
  concurrent?: boolean;
  maxConcurrent?: number;
}

export interface BatchScreenshotResult {
  success: boolean;
  results: ScreenshotResult[];
  completed: number;
  failed: number;
  totalTime?: number;
  error?: string;
}

// Screenshot comparison (advanced)
export interface CompareScreenshotsParams {
  baseline: string; // filepath or base64
  current: string; // filepath or base64  
  threshold?: number; // 0-1 difference threshold
  highlightDifferences?: boolean;
  outputDiffImage?: string;
}

export interface ScreenshotComparison {
  identical: boolean;
  difference: number; // 0-1 percentage
  diffPixels: number;
  totalPixels: number;
  diffImagePath?: string;
}

export interface CompareScreenshotsResult {
  success: boolean;
  comparison?: ScreenshotComparison;
  error?: string;
}

// Export all screenshot command types
export type ScreenshotCommandParams = 
  | ScreenshotParams
  | ScreenRecordingParams
  | GetElementInfoParams
  | GetPageInfoParams
  | BatchScreenshotParams
  | CompareScreenshotsParams;

export type ScreenshotCommandResult = 
  | ScreenshotResult
  | ScreenRecordingResult
  | GetElementInfoResult
  | GetPageInfoResult
  | BatchScreenshotResult
  | CompareScreenshotsResult;