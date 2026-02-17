/**
 * Screenshot Command - Shared Types
 * 
 * Common types and interfaces used by both browser and server screenshot implementations.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { MediaItem } from '@system/data/entities/ChatMessageEntity';

export type ResultType = 'file' | 'bytes' | 'both';
export type ScreenshotFormat = 'png' | 'jpeg' | 'webp';
export type ScreenshotDestination = 'file' | 'bytes' | 'both';

/**
 * Screenshot Command Parameters - Enhanced with advanced features
 */
export interface ScreenshotParams extends CommandParams {
  filename?: string;
  selector?: string;
  querySelector?: string; // Modern querySelector API
  elementName?: string;   // For debugging/logging
  iframeSelector?: string; // Capture content inside an iframe (same-origin only)
  viewportOnly?: boolean; // Capture only visible viewport (not full scroll height)
  options?: ScreenshotOptions;
  
  // Advanced targeting and cropping
  cropX?: number;
  cropY?: number; 
  cropWidth?: number;
  cropHeight?: number;
  
  // Scaling and resolution
  width?: number;
  height?: number;
  scale?: number;
  quality?: number;
  maxFileSize?: number;
  
  // Output control
  format?: ScreenshotFormat;
  destination?: ScreenshotDestination;
  resultType: ResultType;
  
  // Internal properties
  dataUrl?: string;
  metadata?: ScreenshotMetadata;
}

/**
 * Factory function for creating ScreenshotParams
 */
export const createScreenshotParams = (
  context: JTAGContext,
  sessionId: UUID,
  resultType: ResultType = 'file',
  data: Omit<Partial<ScreenshotParams>, 'context' | 'sessionId' | 'resultType' | 'userId'>
): ScreenshotParams => createPayload(context, sessionId, { userId: SYSTEM_SCOPES.SYSTEM, resultType, ...data });

/**
 * HTML2Canvas Configuration Options
 */
export interface Html2CanvasOptions {
  width?: number;
  height?: number;
  scrollX?: number;
  scrollY?: number;
  useCORS?: boolean;
  scale?: number;
  backgroundColor?: string;
  allowTaint?: boolean;
  foreignObjectRendering?: boolean;
  ignoreElements?: (element: Element) => boolean;
  imageTimeout?: number;
  logging?: boolean;
  onclone?: (clonedDoc: Document, element: HTMLElement) => void;
  proxy?: string;
  removeContainer?: boolean;
  windowWidth?: number;
  windowHeight?: number;
}

/**
 * HTML2Canvas Result Canvas Interface
 * Extends HTMLCanvasElement interface for proper type compatibility
 */
export interface Html2CanvasCanvas extends HTMLCanvasElement {
  width: number;
  height: number;
  toDataURL(type?: string, quality?: number): string;
}


/**
 * Screenshot Options
 */
export interface ScreenshotOptions {
  width?: number;
  height?: number;
  fullPage?: boolean;
  quality?: number;
  format?: 'png' | 'jpeg' | 'webp';
  delay?: number;
  scale?: number;
  backgroundColor?: string;
  
  // Capture strategy options
  directCapture?: boolean;    // Use direct element capture instead of body crop
  preserveShadows?: boolean;  // Enable shadow-preserving capture strategy
  
  // Multi-resolution options
  resolutions?: ScreenshotResolution[];  // Generate multiple resolution variants
  presets?: ScreenshotPreset[];          // Use predefined resolution presets
  
  html2canvasOptions?: Html2CanvasOptions;
}

/**
 * Screenshot Resolution Configuration
 */
export interface ScreenshotResolution {
  width: number;
  height: number;
  scale?: number;
  suffix?: string;  // Filename suffix (e.g., 'mobile', 'desktop', '4k')
}

/**
 * Common Screenshot Presets  
 */
export type ScreenshotPreset = 
  | 'mobile'      // 375x667 (iPhone)
  | 'tablet'      // 768x1024 (iPad)  
  | 'desktop'     // 1920x1080 (FHD)
  | '4k'          // 3840x2160 (4K)
  | 'ultrawide'   // 3440x1440 (21:9)
  | 'thumbnail'   // 300x200 (Preview)
  | 'social'      // 1200x630 (Social media);

/**
 * Resolution Preset Definitions
 */
export const RESOLUTION_PRESETS: Record<ScreenshotPreset, ScreenshotResolution> = {
  mobile: { width: 375, height: 667, suffix: 'mobile' },
  tablet: { width: 768, height: 1024, suffix: 'tablet' },
  desktop: { width: 1920, height: 1080, suffix: 'desktop' },
  '4k': { width: 3840, height: 2160, suffix: '4k' },
  ultrawide: { width: 3440, height: 1440, suffix: 'ultrawide' },
  thumbnail: { width: 300, height: 200, suffix: 'thumb', scale: 0.25 },
  social: { width: 1200, height: 630, suffix: 'social' }
};

/**
 * Utility function to expand presets into resolution configurations
 */
export function expandPresets(presets: ScreenshotPreset[]): ScreenshotResolution[] {
  return presets.map(preset => RESOLUTION_PRESETS[preset]);
}

/**
 * Generate filename with resolution suffix
 */
export function generateFilenameWithResolution(
  baseFilename: string, 
  resolution: ScreenshotResolution
): string {
  const name = baseFilename.replace(/\.[^.]+$/, ''); // Remove extension
  const ext = baseFilename.match(/\.[^.]+$/)?.[0] || '.png';
  return resolution.suffix ? `${name}-${resolution.suffix}${ext}` : baseFilename;
}

/**
 * Screenshot Result - interface extending CommandResult
 */
export interface ScreenshotResult extends CommandResult {
  success: boolean;
  filepath: string;
  filename: string;
  timestamp: string;
  options?: ScreenshotOptions;
  error?: JTAGError;
  metadata?: ScreenshotMetadata;
  dataUrl?: string; // Browser-captured image data
  bytes?: Uint8Array; // Raw image bytes for 'bytes' or 'both' destinations
  media?: MediaItem; // Structured media item for AI cognition
}

/**
 * Factory function for creating ScreenshotResult with defaults
 */
export const createScreenshotResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ScreenshotResult>, 'context' | 'sessionId'>
): ScreenshotResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: false,
  filepath: '',
  filename: '',
  timestamp: new Date().toISOString(),
  ...data
});

/**
 * Smart screenshot-specific inheritance from params
 * Auto-inherits: filename, options, metadata, dataUrl from params
 * Only specify what changed: success, filepath, error
 */
export const createScreenshotResultFromParams = (
  params: ScreenshotParams, 
  differences: Omit<Partial<ScreenshotResult>, 'context' | 'sessionId' | 'filename' | 'options' | 'metadata' | 'dataUrl'>
): ScreenshotResult => transformPayload(params, {
  success: false,
  filepath: '',
  filename: params.filename ?? `screenshot-${Date.now()}.png`,
  timestamp: new Date().toISOString(),
  options: params.options,
  metadata: params.metadata,
  dataUrl: params.dataUrl,
  ...differences
});

/**
 * Screenshot Command Response - specific to screenshot operations
 */
export interface ScreenshotResponse extends ScreenshotResult {
  filename: string;
  filepath: string;
  size: number;
}

/**
 * Factory for creating ScreenshotResponse
 */
export const createScreenshotResponse = (
  filename: string,
  filepath: string,
  size: number,
  context: JTAGContext,
  executionTime: number | undefined,
  sessionId: UUID
): ScreenshotResponse => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: true,
  timestamp: new Date().toISOString(),
  filepath,
  filename,
  options: undefined,
  metadata: {
    size,
    globalPath: filepath
  },
  // ScreenshotResponse specific fields - size already included above
  size
});

/**
 * Screenshot Metadata - Enhanced with coordinate and scaling info
 */
export interface ScreenshotMetadata {
  // Original capture dimensions
  originalWidth?: number;
  originalHeight?: number;
  
  // Final output dimensions 
  width?: number;
  height?: number;
  
  // File and processing info
  size?: number;
  fileSizeBytes?: number;
  selector?: string;
  elementName?: string;
  format?: string;
  captureTime?: number; // Time taken to capture in ms
  globalPath?: string; // Server-side file path
  
  // Coordinate and scaling metadata
  scale?: number;
  quality?: number;
  cropped?: boolean;
  cropCoordinates?: { x: number; y: number; width: number; height: number };
  compressed?: boolean; // If quality was reduced for file size
  
  // Multi-resolution metadata
  multiResolution?: boolean;
  resolutionCount?: number;
  successfulCaptures?: number;
}

/**
 * Screenshot — Type-safe command executor
 *
 * Usage:
 *   import { Screenshot } from '@commands/interface/screenshot/shared/ScreenshotTypes';
 *   const result = await Screenshot.execute({ querySelector: 'body', resultType: 'file' });
 */
export const Screenshot = {
  execute(params: CommandInput<ScreenshotParams>): Promise<ScreenshotResult> {
    return Commands.execute<ScreenshotParams, ScreenshotResult>('screenshot', params as Partial<ScreenshotParams>);
  },
  commandName: 'screenshot' as const,
} as const;