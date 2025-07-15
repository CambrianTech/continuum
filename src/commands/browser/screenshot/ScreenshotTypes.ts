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

// Strongly typed screenshot parameters with AI-friendly features
export interface ScreenshotParams {
  selector?: string;
  filename?: string;
  format?: ScreenshotFormat;
  quality?: number;
  animation?: ScreenshotAnimation;
  destination?: ScreenshotDestination;
  subdirectory?: string;
  
  // AI-friendly features
  width?: number;           // Target width (scales down if needed)
  height?: number;          // Target height (scales down if needed)
  scale?: number;           // Scale factor (0.1-2.0)
  cropX?: number;           // Crop starting X coordinate
  cropY?: number;           // Crop starting Y coordinate
  cropWidth?: number;       // Crop width
  cropHeight?: number;      // Crop height
  elementName?: string;     // Human-readable element name for AI context
  querySelector?: string;   // CSS selector for element targeting (maps to selector)
  maxFileSize?: number;     // Maximum file size in bytes (auto-compress)
}

export interface ScreenshotClientRequest {
  selector: string;
  filename: string;
  format: ScreenshotFormat;
  quality: number;
  animation: ScreenshotAnimation;
  destination: ScreenshotDestination;
  
  // AI-friendly features
  width?: number | undefined;
  height?: number | undefined;
  scale?: number | undefined;
  cropX?: number | undefined;
  cropY?: number | undefined;
  cropWidth?: number | undefined;
  cropHeight?: number | undefined;
  elementName?: string | undefined;
  querySelector?: string | undefined;
  maxFileSize?: number | undefined;
}

export interface ScreenshotClientResponse {
  imageData: string;
  filename: string;
  selector: string;
  format: ScreenshotFormat;
  width: number;
  height: number;
  
  // AI-friendly metadata
  elementName?: string;
  originalWidth?: number;
  originalHeight?: number;
  scale?: number;
  cropped?: boolean;
  compressed?: boolean;
  fileSizeBytes?: number;
}

export interface ScreenshotResult {
  success: boolean;
  data?: ScreenshotClientResponse & {
    dataUrl: string;
    saved: boolean;
    filePath: string | null;
    fullPath?: string | undefined;        // Full absolute path for AI reference
    relativePath?: string | undefined;    // Relative path from project root
    bytes?: Uint8Array | undefined;       // Raw image bytes if destination includes bytes
  };
  error?: string;
  timestamp: string;
  processor: string;
}