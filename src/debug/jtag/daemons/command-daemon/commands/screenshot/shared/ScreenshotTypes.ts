/**
 * Screenshot Command - Shared Types
 * 
 * Common types and interfaces used by both browser and server screenshot implementations.
 */

import { CommandParams, JTAGPayload } from '../../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';

/**
 * Screenshot Command Parameters - extends CommandParams
 */
export class ScreenshotParams extends CommandParams {
  filename: string;
  selector?: string;
  options?: ScreenshotOptions;

  constructor(filename: string, selector?: string, options?: ScreenshotOptions) {
    super();
    this.filename = filename;
    this.selector = selector;
    this.options = options;
  }
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
  html2canvasOptions?: any;
}

/**
 * Screenshot Result
 */
export class ScreenshotResult extends JTAGPayload {
  success: boolean;
  filepath: string;
  filename: string;
  context: JTAGContext['environment'];
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
    this.context = data.context ?? 'server';
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
  width: number;
  height: number;
  size: number;
  selector?: string;
  format?: string;
  captureTime?: number; // Time taken to capture in ms
}