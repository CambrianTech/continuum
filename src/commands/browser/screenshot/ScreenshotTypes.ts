/**
 * Shared types for Screenshot command - used by both server and client
 */

export enum ScreenshotFormat {
  PNG = 'png',
  JPG = 'jpg', 
  JPEG = 'jpeg',
  WEBP = 'webp'
}

export enum ScreenshotDestination {
  FILE = 'file',           // Save to file, return filename
  BYTES = 'bytes',         // Return raw image data
  BOTH = 'both'           // Save to file AND return bytes
}

export enum ScreenshotAnimation {
  NONE = 'none',           // No UI feedback
  VISIBLE = 'visible',     // Show ROI highlighting
  ANIMATED = 'animated'    // Animate ROI highlighting
}

// Strongly typed screenshot parameters
export interface ScreenshotParams {
  selector?: string;
  filename?: string;
  format?: ScreenshotFormat;
  quality?: number;
  animation?: ScreenshotAnimation;
  destination?: ScreenshotDestination;
  subdirectory?: string;
}

export interface ScreenshotClientRequest {
  selector: string;
  filename: string;
  format: ScreenshotFormat;
  quality: number;
  animation: ScreenshotAnimation;
  destination: ScreenshotDestination;
}

export interface ScreenshotClientResponse {
  imageData: string;
  filename: string;
  selector: string;
  format: ScreenshotFormat;
  width: number;
  height: number;
}

export interface ScreenshotResult {
  success: boolean;
  data?: ScreenshotClientResponse & {
    dataUrl: string;
    saved: boolean;
    filePath: string | null;
  };
  error?: string;
  timestamp: string;
  processor: string;
}