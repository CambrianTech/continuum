/**
 * Screenshot Types - Strong Type Definitions
 * =========================================
 * Shared strong types for screenshot functionality
 */

// Use union types instead of enums for better TypeScript patterns
export type ScreenshotFormat = 'png' | 'jpeg' | 'jpg' | 'webp';
export type ScreenshotDestination = 'file' | 'bytes' | 'both';
export type ScreenshotAnimation = 'none' | 'visible' | 'animated';

export interface StrongScreenshotParams {
  readonly selector?: string;
  readonly filename?: string;
  readonly subdirectory?: string;
  readonly namePrefix?: string;
  readonly scale?: number;
  readonly format?: ScreenshotFormat;
  readonly destination?: ScreenshotDestination;
  readonly animation?: ScreenshotAnimation;
  readonly quality?: number;
  readonly fullPage?: boolean;
  readonly manual?: boolean;
  readonly source?: string;
  readonly roi?: boolean;
}

export interface ScreenshotResult {
  readonly success: boolean;
  readonly data?: {
    readonly imageData?: string;
    readonly filename?: string;
    readonly selector: string;
    readonly format: ScreenshotFormat;
    readonly width: number;
    readonly height: number;
    readonly fileSize?: number;
  };
  readonly error?: string;
  readonly metadata: {
    readonly userAgent: string;
    readonly timestamp: number;
    readonly executionTime: number;
    readonly source: string;
  };
}

/**
 * Type guards for runtime validation
 */
export class ScreenshotTypeGuards {
  static isValidFormat(format: unknown): format is ScreenshotFormat {
    return typeof format === 'string' && 
           ['png', 'jpeg', 'jpg', 'webp'].includes(format);
  }

  static isValidDestination(dest: unknown): dest is ScreenshotDestination {
    return typeof dest === 'string' && 
           ['file', 'bytes', 'both'].includes(dest);
  }

  static isValidAnimation(anim: unknown): anim is ScreenshotAnimation {
    return typeof anim === 'string' && 
           ['none', 'visible', 'animated'].includes(anim);
  }

  static validateAndConvertParams(params: Record<string, unknown>): StrongScreenshotParams {
    // Build object with conditional properties - works with readonly
    return {
      ...(typeof params.selector === 'string' && { selector: params.selector }),
      ...(typeof params.filename === 'string' && { filename: params.filename }),
      ...(typeof params.subdirectory === 'string' && { subdirectory: params.subdirectory }),
      ...(typeof params.namePrefix === 'string' && { namePrefix: params.namePrefix }),
      ...(typeof params.scale === 'number' && { scale: params.scale }),
      ...(this.isValidFormat(params.format) && { format: params.format }),
      ...(this.isValidDestination(params.destination) && { destination: params.destination }),
      ...(this.isValidAnimation(params.animation) && { animation: params.animation }),
      ...(typeof params.quality === 'number' && { quality: params.quality }),
      ...(typeof params.fullPage === 'boolean' && { fullPage: params.fullPage }),
      ...(typeof params.manual === 'boolean' && { manual: params.manual }),
      ...(typeof params.source === 'string' && { source: params.source }),
      ...(typeof params.roi === 'boolean' && { roi: params.roi })
    };
  }
}