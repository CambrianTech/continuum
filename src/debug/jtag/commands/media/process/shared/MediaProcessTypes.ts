/**
 * Media Process Command Types
 *
 * Comprehensive video/audio/image processing via ffmpeg and other media tools.
 * Supports speed adjustment, format conversion, trimming, audio manipulation, and more.
 */

import type { JTAGPayload } from '../../../../system/core/types/JTAGTypes';

/**
 * Media format types supported by ffmpeg
 */
export type MediaFormat = 'mp4' | 'webm' | 'mov' | 'avi' | 'mkv' | 'gif' | 'mp3' | 'wav' | 'flac' | 'aac';

/**
 * Quality presets for output
 */
export type QualityPreset = 'low' | 'medium' | 'high' | 'source';

/**
 * Codec options for video
 */
export type VideoCodec = 'h264' | 'h265' | 'vp9' | 'av1' | 'copy';

/**
 * Codec options for audio
 */
export type AudioCodec = 'aac' | 'mp3' | 'opus' | 'flac' | 'copy';

/**
 * Parameters for media processing
 */
export interface MediaProcessParams extends JTAGPayload {
  /** Input file path (required) */
  inputPath: string;

  /** Output file path (auto-generated if not provided) */
  outputPath?: string;

  // ===== SPEED OPERATIONS =====
  /** Speed multiplier (0.25 = 4x faster, 2.0 = 2x slower) */
  speed?: number;

  // ===== AUDIO OPERATIONS =====
  /** Strip audio from video */
  stripAudio?: boolean;

  /** Extract audio to separate file */
  extractAudio?: boolean;

  /** Path to audio file to add/replace */
  audioPath?: string;

  /** Adjust audio volume (0.5 = 50%, 2.0 = 200%) */
  volume?: number;

  // ===== FORMAT/QUALITY =====
  /** Output format (detected from outputPath if not specified) */
  format?: MediaFormat;

  /** Quality preset */
  quality?: QualityPreset;

  /** Video codec (default: h264 for compatibility) */
  videoCodec?: VideoCodec;

  /** Audio codec (default: aac for compatibility) */
  audioCodec?: AudioCodec;

  /** Bitrate for video (e.g., "1M", "500K") */
  videoBitrate?: string;

  /** Bitrate for audio (e.g., "128K", "192K") */
  audioBitrate?: string;

  // ===== RESOLUTION/SIZE =====
  /** Resize video to specific dimensions */
  resolution?: {
    width: number;
    height: number;
  };

  /** Scale video by percentage (0.5 = 50%, 2.0 = 200%) */
  scale?: number;

  // ===== EDITING =====
  /** Trim video to specific time range (seconds) */
  trim?: {
    start: number;
    end: number;
  };

  /** Trim video to specific time range (HH:MM:SS format) */
  trimTime?: {
    start: string;
    end: string;
  };

  /** Rotate video (90, 180, 270 degrees clockwise) */
  rotate?: 90 | 180 | 270;

  /** Flip video horizontally */
  flipHorizontal?: boolean;

  /** Flip video vertically */
  flipVertical?: boolean;

  // ===== GIF-SPECIFIC =====
  /** Frames per second for GIF output (default: 10) */
  gifFps?: number;

  /** Maximum width for GIF (keeps aspect ratio) */
  gifMaxWidth?: number;

  // ===== ADVANCED =====
  /** Custom ffmpeg arguments (for advanced users) */
  customArgs?: string[];

  /** Check if dependency is installed without processing */
  checkDependency?: boolean;

  /** Install missing dependencies (requires user confirmation) */
  installDependencies?: boolean;

  /** Emit progress events during processing */
  emitProgress?: boolean;
}

/**
 * Result of media processing
 */
export interface MediaProcessResult extends JTAGPayload {
  success: boolean;

  /** Output file path */
  outputPath?: string;

  /** Duration of processed media (seconds) */
  duration?: number;

  /** File size of output (bytes) */
  fileSize?: number;

  /** Processing time (milliseconds) */
  processingTime?: number;

  /** Dependency check result */
  dependencyInstalled?: boolean;

  /** Error message if processing failed */
  error?: string;

  /** Warning messages (non-fatal issues) */
  warnings?: string[];

  /** Detailed ffmpeg output (for debugging) */
  ffmpegOutput?: string;
}

/**
 * Progress event payload for long-running operations
 */
export interface MediaProcessProgressEvent extends JTAGPayload {
  /** Input file path */
  inputPath: string;

  /** Current frame being processed */
  currentFrame?: number;

  /** Total frames to process */
  totalFrames?: number;

  /** Percentage complete (0-100) */
  percentage?: number;

  /** Processing speed (fps) */
  speed?: number;

  /** Estimated time remaining (seconds) */
  estimatedTimeRemaining?: number;
}

/**
 * Create MediaProcessResult from params and outcome
 */
export function createMediaProcessResult(
  params: MediaProcessParams,
  outcome: Partial<MediaProcessResult>
): MediaProcessResult {
  return {
    success: outcome.success ?? false,
    outputPath: outcome.outputPath,
    duration: outcome.duration,
    fileSize: outcome.fileSize,
    processingTime: outcome.processingTime,
    dependencyInstalled: outcome.dependencyInstalled,
    error: outcome.error,
    warnings: outcome.warnings,
    ffmpegOutput: outcome.ffmpegOutput
  };
}

/**
 * Generate output path based on input and parameters
 */
export function generateOutputPath(
  inputPath: string,
  params: MediaProcessParams,
  outputDir: string
): string {
  const path = require('path');

  // Extract filename without extension
  const basename = path.basename(inputPath, path.extname(inputPath));

  // Determine output format
  let extension = path.extname(inputPath).substring(1);
  if (params.format) {
    extension = params.format;
  } else if (params.extractAudio) {
    extension = 'mp3';
  }

  // Build descriptive suffix
  const suffixes: string[] = [];
  if (params.speed) {
    const speedMultiplier = 1 / params.speed;
    suffixes.push(`${speedMultiplier.toFixed(1)}x`);
  }
  if (params.stripAudio) {
    suffixes.push('noaudio');
  }
  if (params.trim || params.trimTime) {
    suffixes.push('trimmed');
  }
  if (params.resolution) {
    suffixes.push(`${params.resolution.width}x${params.resolution.height}`);
  }
  if (params.rotate) {
    suffixes.push(`rot${params.rotate}`);
  }

  const suffix = suffixes.length > 0 ? `-${suffixes.join('-')}` : '-processed';

  return path.join(outputDir, `${basename}${suffix}.${extension}`);
}
