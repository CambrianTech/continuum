/**
 * Media Resize Command - Server Implementation
 *
 * Image resizing via sharp library with model-aware dimension calculation.
 * Enables PersonaUsers to receive appropriately-sized images based on their
 * model's context window capacity.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { MediaResizeParams, MediaResizeResult } from '../shared/MediaResizeTypes';
import { createMediaResizeResult } from '../shared/MediaResizeTypes';
import { getContextWindow } from '../../../../system/shared/ModelContextWindows';
import * as fs from 'fs';
import * as path from 'path';

// Import sharp dynamically to handle environments where it's not available
let sharp: typeof import('sharp') | null = null;
try {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  sharp = require('sharp');
} catch (error) {
  console.warn('⚠️  Sharp library not available - image resizing disabled');
}

export class MediaResizeServerCommand extends CommandBase<MediaResizeParams, MediaResizeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('media/resize', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<MediaResizeResult> {
    const resizeParams = params as MediaResizeParams;
    const startTime = Date.now();

    // Check if sharp is available
    if (!sharp) {
      return createMediaResizeResult(resizeParams, {
        success: false,
        error: 'Sharp library not installed. Run: npm install sharp'
      });
    }

    // Validate input file exists
    if (!fs.existsSync(resizeParams.inputPath)) {
      return createMediaResizeResult(resizeParams, {
        success: false,
        error: `Input file not found: ${resizeParams.inputPath}`
      });
    }

    try {
      // Get original image metadata
      const inputImage = sharp(resizeParams.inputPath);
      const metadata = await inputImage.metadata();

      if (!metadata.width || !metadata.height) {
        return createMediaResizeResult(resizeParams, {
          success: false,
          error: 'Unable to read image dimensions'
        });
      }

      const originalDimensions = {
        width: metadata.width,
        height: metadata.height
      };

      // Calculate target dimensions
      const targetDimensions = this.calculateTargetDimensions(
        resizeParams,
        originalDimensions
      );

      // Generate output path if not provided
      const outputPath = resizeParams.outputPath ?? this.generateOutputPath(
        resizeParams.inputPath,
        targetDimensions
      );

      // Ensure output directory exists
      const outputDir = path.dirname(outputPath);
      fs.mkdirSync(outputDir, { recursive: true });

      // Resize image
      let resizeOperation = sharp(resizeParams.inputPath)
        .resize({
          width: targetDimensions.width,
          height: targetDimensions.height,
          fit: resizeParams.fit ?? 'inside',
          withoutEnlargement: true  // Never enlarge images
        });

      // Apply quality setting
      const format = path.extname(outputPath).substring(1).toLowerCase();
      if (format === 'jpg' || format === 'jpeg') {
        resizeOperation = resizeOperation.jpeg({
          quality: resizeParams.quality ?? 80
        });
      } else if (format === 'png') {
        resizeOperation = resizeOperation.png({
          quality: resizeParams.quality ?? 80
        });
      } else if (format === 'webp') {
        resizeOperation = resizeOperation.webp({
          quality: resizeParams.quality ?? 80
        });
      }

      // Write to file
      await resizeOperation.toFile(outputPath);

      // Get output file stats
      const stats = fs.statSync(outputPath);

      // Read base64 if requested
      let base64: string | undefined;
      let estimatedTokens: number | undefined;
      if (resizeParams.returnBase64) {
        const imageBuffer = fs.readFileSync(outputPath);
        base64 = imageBuffer.toString('base64');
        // Rough estimate: ~4 characters per token
        estimatedTokens = Math.ceil(base64.length / 4);
      }

      const processingTime = Date.now() - startTime;

      // Calculate context window percentage if model provided
      let contextWindowPercentage: number | undefined;
      let modelContextWindow: number | undefined;
      if (resizeParams.modelName && estimatedTokens) {
        modelContextWindow = getContextWindow(resizeParams.modelName);
        contextWindowPercentage = (estimatedTokens / modelContextWindow) * 100;
      }

      console.log(`✅ MEDIA RESIZE: ${originalDimensions.width}x${originalDimensions.height} → ${targetDimensions.width}x${targetDimensions.height} (${processingTime}ms, ${stats.size} bytes)`);

      return createMediaResizeResult(resizeParams, {
        success: true,
        outputPath,
        originalDimensions,
        resizedDimensions: targetDimensions,
        fileSize: stats.size,
        processingTime,
        base64,
        estimatedTokens,
        modelContextWindow,
        contextWindowPercentage
      });

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ MEDIA RESIZE: Failed -`, errorMessage);

      return createMediaResizeResult(resizeParams, {
        success: false,
        error: `Image resize failed: ${errorMessage}`
      });
    }
  }

  /**
   * Calculate target dimensions based on params
   */
  private calculateTargetDimensions(
    params: MediaResizeParams,
    originalDimensions: { width: number; height: number }
  ): { width: number; height: number } {
    // Priority 1: Explicit width and height
    if (params.width && params.height) {
      return { width: params.width, height: params.height };
    }

    // Priority 2: Model-aware sizing
    if (params.modelName) {
      return this.calculateModelAwareDimensions(
        params.modelName,
        params.targetPercentage ?? 0.12,  // Default: 12% of context window
        originalDimensions
      );
    }

    // Priority 3: Max width/height constraints
    if (params.maxWidth || params.maxHeight) {
      return this.calculateMaxConstrainedDimensions(
        originalDimensions,
        params.maxWidth,
        params.maxHeight
      );
    }

    // Priority 4: Single dimension (maintain aspect ratio)
    if (params.width) {
      const aspectRatio = originalDimensions.height / originalDimensions.width;
      return {
        width: params.width,
        height: Math.round(params.width * aspectRatio)
      };
    }

    if (params.height) {
      const aspectRatio = originalDimensions.width / originalDimensions.height;
      return {
        width: Math.round(params.height * aspectRatio),
        height: params.height
      };
    }

    // Fallback: Return original dimensions
    return originalDimensions;
  }

  /**
   * Calculate dimensions based on model's context window capacity
   */
  private calculateModelAwareDimensions(
    modelName: string,
    targetPercentage: number,
    originalDimensions: { width: number; height: number }
  ): { width: number; height: number } {
    const contextWindow = getContextWindow(modelName);

    // Calculate target tokens for image (with safety margin)
    const safetyMargin = 0.9;  // Use 90% of target to be conservative
    const targetTokens = Math.floor(contextWindow * targetPercentage * safetyMargin);

    // Rough estimate: ~4 characters per token, base64 encoding adds ~33% overhead
    const targetBase64Length = targetTokens * 4;
    const targetImageBytes = Math.floor(targetBase64Length / 1.33);

    // Estimate bytes per pixel based on image format (assume JPEG compression ~0.5 bytes/pixel)
    const bytesPerPixel = 0.5;
    const targetPixels = Math.floor(targetImageBytes / bytesPerPixel);

    // Calculate dimensions maintaining aspect ratio
    const aspectRatio = originalDimensions.width / originalDimensions.height;
    const targetHeight = Math.sqrt(targetPixels / aspectRatio);
    const targetWidth = targetHeight * aspectRatio;

    // Round to reasonable dimensions
    const width = Math.round(targetWidth);
    const height = Math.round(targetHeight);

    // Don't upscale
    if (width > originalDimensions.width || height > originalDimensions.height) {
      return originalDimensions;
    }

    console.log(`🤖 Model-aware sizing: ${modelName} (${contextWindow} tokens) → ${width}x${height} (~${targetTokens} tokens, ${targetPercentage * 100}% of context)`);

    return { width, height };
  }

  /**
   * Calculate dimensions constrained by max width/height
   */
  private calculateMaxConstrainedDimensions(
    originalDimensions: { width: number; height: number },
    maxWidth?: number,
    maxHeight?: number
  ): { width: number; height: number } {
    let width = originalDimensions.width;
    let height = originalDimensions.height;

    // Apply max width constraint
    if (maxWidth && width > maxWidth) {
      const ratio = maxWidth / width;
      width = maxWidth;
      height = Math.round(height * ratio);
    }

    // Apply max height constraint
    if (maxHeight && height > maxHeight) {
      const ratio = maxHeight / height;
      height = maxHeight;
      width = Math.round(width * ratio);
    }

    return { width, height };
  }

  /**
   * Generate output path based on input and target dimensions
   */
  private generateOutputPath(
    inputPath: string,
    targetDimensions: { width: number; height: number }
  ): string {
    const parsedPath = path.parse(inputPath);
    const suffix = `-${targetDimensions.width}x${targetDimensions.height}`;
    return path.join(
      parsedPath.dir,
      `${parsedPath.name}${suffix}${parsedPath.ext}`
    );
  }
}
