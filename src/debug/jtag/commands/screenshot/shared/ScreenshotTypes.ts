/**
 * Screenshot Command - Shared Types
 * 
 * Common types and interfaces used by both browser and server screenshot implementations.
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

export type ResultType = 'file' | 'bytes';

/**
 * Screenshot Command Parameters - interface extending CommandParams
 */
export interface ScreenshotParams extends CommandParams {
  filename?: string;
  selector?: string;
  options?: ScreenshotOptions;
  
  // Additional properties for parametric behavior
  dataUrl?: string;
  resultType: ResultType;
  crop?: { x: number; y: number; width: number; height: number };
  metadata?: ScreenshotMetadata;
}

/**
 * Factory function for creating ScreenshotParams
 */
export const createScreenshotParams = (
  context: JTAGContext,
  sessionId: UUID,
  resultType: ResultType = 'file',
  data: Omit<Partial<ScreenshotParams>, 'context' | 'sessionId' | 'resultType' >
): ScreenshotParams => createPayload(context, sessionId, { resultType, ...data });

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
 */
export interface Html2CanvasCanvas {
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
  html2canvasOptions?: Html2CanvasOptions;
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
}

/**
 * Factory function for creating ScreenshotResult with defaults
 */
export const createScreenshotResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ScreenshotResult>, 'context' | 'sessionId'>
): ScreenshotResult => createPayload(context, sessionId, {
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
  path: string;
  size: number;
}

/**
 * Factory for creating ScreenshotResponse
 */
export const createScreenshotResponse = (
  filename: string,
  path: string,
  size: number,
  context: JTAGContext,
  executionTime: number | undefined,
  sessionId: UUID
): ScreenshotResponse => createPayload(context, sessionId, {
  success: true,
  timestamp: new Date().toISOString(),
  filepath: path,
  filename,
  options: undefined,
  metadata: {
    size,
    globalPath: path
  },
  // ScreenshotResponse specific fields
  path,
  size
});

/**
 * Screenshot Metadata
 */
export interface ScreenshotMetadata {
  width?: number;
  height?: number;
  size?: number;
  selector?: string;
  format?: string;
  captureTime?: number; // Time taken to capture in ms
  globalPath?: string; // Server-side file path
}