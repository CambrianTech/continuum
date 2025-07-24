/**
 * Screenshot Command - Shared Types
 * 
 * Common types and interfaces used by both browser and server screenshot implementations.
 */

import { CommandParams, CommandResult } from '../../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';

/**
 * Screenshot Command Parameters - extends CommandParams
 */
export class ScreenshotParams extends CommandParams {
  filename?: string;
  selector?: string;
  options?: ScreenshotOptions;
  
  // Additional properties for parametric behavior
  dataUrl?: string;
  returnToSource?: boolean;
  returnFormat?: 'file' | 'bytes' | 'download';
  crop?: { x: number; y: number; width: number; height: number };
  metadata?: ScreenshotMetadata;

  constructor(data: Partial<ScreenshotParams> = {}) {
    super();
    Object.assign(this, data);
  }
}

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
 * Screenshot Result - extends CommandResult
 */
export class ScreenshotResult extends CommandResult {
  success: boolean;
  filepath: string;
  filename: string;
  environment: JTAGContext['environment'];
  timestamp: string;
  options?: ScreenshotOptions;
  error?: string;
  metadata?: ScreenshotMetadata;
  dataUrl?: string; // Browser-captured image data

  constructor(data: Partial<ScreenshotResult>) {
    super();
    this.success = data.success ?? false;
    this.filepath = data.filepath ?? '';
    this.filename = data.filename ?? '';
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
    this.options = data.options;
    this.error = data.error;
    this.metadata = data.metadata;
    this.dataUrl = data.dataUrl;
  }
}

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